use anyhow::anyhow;
use kafer_core::{DebugEvent, DebugEventKind, Debugger};

fn main() -> anyhow::Result<()> {
    let program: Vec<String> = std::env::args().collect();
    if program.len() < 2 {
        Err(anyhow!("No program to execute found!"))?;
    }
    let mut debugger = Debugger::run(&program[1], &program[2..])?;
    println!("Debugger is running now.");
    let mut buffer = String::new();
    'debugger: loop {
        let mut event = debugger.pull_event()?;
        handle_event(&event)?;
        loop {
            let ip = event.instruction_pointer();
            let symbol_name = event.look_up_symbol(ip);
            if let Some(name) = symbol_name {
                println!("[kafer] {name} ({ip:#0x})");
            } else {
                println!("[kafer] {ip:#0x}");
            }
            buffer.clear();
            std::io::stdin().read_line(&mut buffer)?;
            let cmd: Vec<&str> = buffer.trim().split(' ').collect();
            match &cmd[..] {
                &["reg"] => {
                    event.registers().print();
                }
                &["s"] => {
                    event.step_into()?;
                    break;
                }
                &["n" | "c" | ""] => {
                    break;
                }
                &["q"] => {
                    break 'debugger;
                }
                &["read", addr] if parse_addr(addr, &event).is_some() => {
                    let value = event.read_memory(parse_addr(addr, &event).unwrap())?;
                    for byte in value {
                        print!("{byte:02x} ");
                    }
                    println!();
                }
                &["listmodules"] => {
                    for name in event.parent.module_names() {
                        println!("Module {name}");
                    }
                }
                &["k"] => {
                    for (frame_number, stack_frame) in event.stack_frames().iter().enumerate() {
                        // TODO: Hide CONTEXT or AlignedContext type from public
                        // interface!
                        let context = stack_frame.context;
                        if let Some(sym) = event.look_up_symbol(context.Rip) {
                            println!("{:02X} 0x{:016X} {}", frame_number, context.Rsp, sym);
                        } else {
                            println!(
                                "{:02X} 0x{:016X} 0x{:X}",
                                frame_number, context.Rsp, context.Rip
                            );
                        }
                    }
                }
                &["d" | "u", addr] if parse_addr(addr, &event).is_some() => {
                    let addr = parse_addr(addr, &event).unwrap();
                    for instruction in event.disassemble_at(addr, 8)? {
                        println!("{instruction}");
                    }
                }
                &["bp"] => {
                    for bp in event.breakpoints() {
                        match event.look_up_symbol(bp.addr) {
                            Some(name) => {
                                println!("Breakpoint#{} in {name} ({:#x})", 0, bp.addr);
                            }
                            None => {
                                println!("Breakpoint#{} at ({:#x})", 0, bp.addr);
                            }
                        }
                    }
                }
                &["clbp", index] if parse_usize(index).is_some() => {
                    let index = parse_addr(index, &event).unwrap();
                    event.clear_breakpoint(index);
                }
                &["bp", addr] if parse_addr(addr, &event).is_some() => {
                    let address = parse_addr(addr, &event).unwrap();
                    match event.add_breakpoint(address) {
                        Some(id) => println!("[kafer] Added breakpoint#{id}"),
                        None => println!("[kafer] Failed to add breakpoint. No space left, delete a prior breakpoint."),
                    }
                }
                err => {
                    println!("`{}` is no valid command!", err.join(" "));
                }
            }
        }
        if !event.kind.should_continue() {
            break;
        }
    }
    Ok(())
}

fn handle_event(event: &DebugEvent) -> anyhow::Result<()> {
    match &event.kind {
        DebugEventKind::Unknown => (),
        DebugEventKind::Exception(exception) => {
            if let Some(bp) = exception.breakpoint {
                println!("[kafer] Breakpoint #{bp} was hit.");
            } else {
                println!(
                    "[kafer] Exception {:?} was thrown. Is this the first chance? {:?}",
                    exception.code, exception.is_first_chance
                );
            }
        }
        DebugEventKind::CreateThread => (),
        DebugEventKind::CreateProcess(name) => {
            println!("[kafer] Loaded dll {name}.");
        }
        DebugEventKind::ExitThread => (),
        DebugEventKind::ExitProcess => {
            println!("[kafer] Exited process!");
        }
        DebugEventKind::LoadDll(name) => {
            println!("[kafer] Loaded dll {name}.");
        }
        DebugEventKind::UnloadDll => (),
        DebugEventKind::OutputDebugString(text) => {
            println!("[kafer] DebugOut: {text}");
        }
        DebugEventKind::RipEvent => (),
    }
    Ok(())
}

fn parse_addr(addr: &str, event: &DebugEvent) -> Option<usize> {
    match addr.split_once('!') {
        None => {
            if let Some(register) = addr.strip_prefix('@') {
                event.registers().get_by_name(register).map(|u| u as _)
            } else {
                parse_usize(addr)
            }
        }
        Some((module_name, function_name)) => event
            .resolve_symbol(module_name, function_name)
            .map(|u| u as _),
    }
}

fn parse_usize(addr: &str) -> Option<usize> {
    match addr.strip_prefix("0x") {
        Some(hex) => usize::from_str_radix(hex, 16),
        None => addr.parse(),
    }
    .ok()
}
