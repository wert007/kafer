use std::iter;

use breakpoints::BreakpointManager;
use error::Error;
pub use events::{DebugEvent, DebugEventKind};
use ffi::{AlignedContext, AutoClosedHandle, WideString};
use memory::{MemorySource, ProcessMemoryReader};
use processes::Process;
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::CloseHandle,
        System::{
            Diagnostics::Debug::*,
            Threading::{
                CreateProcessW, OpenThread, CREATE_NEW_CONSOLE, DEBUG_ONLY_THIS_PROCESS, INFINITE,
                PROCESS_INFORMATION, STARTUPINFOEXW, STARTUPINFOW, THREAD_GET_CONTEXT,
                THREAD_SET_CONTEXT,
            },
        },
    },
};

use crate::error::{WindowsError, WindowsFunction};
mod breakpoints;
mod disassembler;
mod error;
mod events;
mod ffi;
mod memory;
mod processes;
mod stack;

#[allow(dead_code)]
pub struct Debugger {
    process_info: PROCESS_INFORMATION,
    command_line: WideString,
    process: Process,
    breakpoints: BreakpointManager,
}

impl Debugger {
    fn memory_reader(&self) -> ProcessMemoryReader {
        ProcessMemoryReader::from_process_handle(self.process_info.hProcess)
    }

    pub fn resolve_symbol(&self, module_name: &str, function_name: &str) -> Option<u64> {
        if let Some(module) = self.process.get_module_by_name(module_name) {
            if let Some(addr) = module.resolve_function(function_name) {
                Some(addr)
            } else {
                println!("No function {function_name} in module {module_name}");
                // Err(format!("Could not find {} in module {}", func_name, module_name))
                None
            }
        } else {
            println!("No module {module_name}");
            // Err(format!("Could not find module {}", module_name))
            None
        }
    }

    pub fn run(program: impl Into<String>, args: &[String]) -> Result<Self, Error> {
        let program = program.into();
        let startup_info = STARTUPINFOEXW {
            StartupInfo: STARTUPINFOW {
                cb: std::mem::size_of::<STARTUPINFOEXW>() as _,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut process_info = PROCESS_INFORMATION::default();
        // let mut command_line = unsafe { w!("cmd").as_wide() }.to_vec();
        let command_line = iter::once(&program)
            .chain(args)
            .fold(String::new(), |a, b| a + b + " ");
        let mut command_line: WideString = command_line.into();
        // let mut command_line = unsafe { w!("./return_42.exe").as_wide() }.to_vec();
        println!("Running `{}`", command_line);
        unsafe {
            loop {
                let result = CreateProcessW(
                    PCWSTR::null(),
                    command_line.as_pwstr(),
                    None,
                    None,
                    false,
                    DEBUG_ONLY_THIS_PROCESS | CREATE_NEW_CONSOLE,
                    None,
                    PCWSTR::null(),
                    &startup_info.StartupInfo,
                    &mut process_info,
                )
                .map_err(|e| WindowsError::new(WindowsFunction::CreateProcessW, e));
                if result.is_ok() {
                    break;
                }
            }
        }
        unsafe {
            CloseHandle(process_info.hThread)
                .map_err(|e| WindowsError::new(WindowsFunction::CloseHandle, e))?;
        }
        Ok(Self {
            process_info,
            command_line,
            process: Process::new(),
            breakpoints: BreakpointManager::new(),
        })
    }

    pub fn pull_event(&mut self) -> Result<DebugEvent, Error> {
        let mut debug_event = DEBUG_EVENT::default();
        unsafe {
            WaitForDebugEventEx(&mut debug_event, INFINITE)
                .map_err(|e| WindowsError::new(WindowsFunction::WaitForDebugEventEx, e))?;
        }

        let thread = unsafe {
            OpenThread(
                THREAD_GET_CONTEXT | THREAD_SET_CONTEXT,
                false,
                debug_event.dwThreadId,
            )
            .map_err(|e| WindowsError::new(WindowsFunction::OpenThread, e))?
        };
        let thread = AutoClosedHandle(thread);
        let mut ctx = AlignedContext::ALL;
        unsafe {
            GetThreadContext(&thread, &mut ctx.0)
                .map_err(|e| WindowsError::new(WindowsFunction::GetThreadContext, e))?
        };

        // debug_event.u.CreateProcessInfo;
        let kind = match debug_event.dwDebugEventCode {
            CREATE_PROCESS_DEBUG_EVENT => {
                let memory = self.memory_reader();
                DebugEventKind::create_process(
                    &mut self.process,
                    memory,
                    unsafe { debug_event.u.CreateProcessInfo },
                    &debug_event,
                )?
            }
            CREATE_THREAD_DEBUG_EVENT => {
                // TODO: Add Thread to process!
                DebugEventKind::create_thread(&mut self.process, unsafe {
                    debug_event.u.CreateThread
                })
            }
            EXCEPTION_DEBUG_EVENT => DebugEventKind::exception(
                unsafe { debug_event.u.Exception },
                &self.breakpoints,
                &ctx,
            ),
            EXIT_PROCESS_DEBUG_EVENT => DebugEventKind::ExitProcess,
            EXIT_THREAD_DEBUG_EVENT => DebugEventKind::ExitThread,
            LOAD_DLL_DEBUG_EVENT => {
                let memory = self.memory_reader();
                DebugEventKind::load_dll(&mut self.process, memory, unsafe {
                    debug_event.u.LoadDll
                })?
            }
            OUTPUT_DEBUG_STRING_EVENT => {
                DebugEventKind::output_debug_string(self.memory_reader(), unsafe {
                    debug_event.u.DebugString
                })?
            }
            RIP_EVENT => DebugEventKind::RipEvent,
            UNLOAD_DLL_DEBUG_EVENT => DebugEventKind::UnloadDll,
            _ => panic!("Unexpected debug event"),
        };

        Ok(DebugEvent::new(self, kind, debug_event, ctx, thread))
    }

    pub fn read_memory(&self, address: usize) -> Result<Vec<u8>, Error> {
        self.memory_reader().read_memory_array(address as _, 16)
    }

    pub fn look_up_symbol(&mut self, address: u64) -> Option<String> {
        self.process.address_to_name(address)
    }

    fn apply_breakpoints(&mut self, thread_id: u32) -> Result<(), Error> {
        self.breakpoints
            .apply_breakpoints(&mut self.process, thread_id)?;
        Ok(())
    }

    fn breakpoints(&self) -> Vec<breakpoints::Breakpoint> {
        self.breakpoints.list_breakpoints()
    }

    fn add_breakpoint(&mut self, address: usize) -> Option<usize> {
        self.breakpoints.add_breakpoint(address as _)
    }

    pub fn module_names(&self) -> Vec<String> {
        self.process.module_names()
    }

    fn clear_breakpoint(&mut self, index: usize) {
        self.breakpoints.clear_breakpoint(index as _);
    }
}

impl Drop for Debugger {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.process_info.hProcess).unwrap();
        }
    }
}
