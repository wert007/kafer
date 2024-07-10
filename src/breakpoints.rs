use windows::Win32::System::{
    Diagnostics::Debug::{GetThreadContext, SetThreadContext},
    Threading::{OpenThread, THREAD_GET_CONTEXT, THREAD_SET_CONTEXT},
};

use crate::{
    error::{Error, WindowsError, WindowsFunction},
    ffi::{AlignedContext, AutoClosedHandle},
    processes::Process,
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Breakpoint {
    pub addr: u64,
    id: usize,
}

pub struct BreakpointManager {
    breakpoints: [Option<Breakpoint>; 4],
}

impl BreakpointManager {
    pub fn new() -> BreakpointManager {
        BreakpointManager {
            breakpoints: [Default::default(); 4],
        }
    }

    // fn get_free_id(&self) -> u32 {
    //     for i in 0..4 {
    //         if self.breakpoints.iter().find(|&x| x.id == i).is_none() {
    //             return i;
    //         }
    //     }
    //     panic!("Too many breakpoints!")
    // }

    pub fn add_breakpoint(&mut self, addr: u64) -> Option<usize> {
        for (id, bp) in self
            .breakpoints
            .iter_mut()
            .enumerate()
            .filter(|(_, bp)| bp.is_none())
        {
            *bp = Some(Breakpoint { addr, id });
            return Some(id);
        }
        None
    }

    pub fn list_breakpoints(&self) -> Vec<Breakpoint> {
        self.breakpoints.iter().copied().filter_map(|b| b).collect()
    }

    pub fn clear_breakpoint(&mut self, id: usize) {
        self.breakpoints[id] = None;
    }

    pub fn was_breakpoint_hit(&self, thread_context: &AlignedContext) -> Option<u32> {
        for idx in 0..self.breakpoints.len() {
            if (thread_context.Dr6 << idx) != 0 {
                return Some(idx as u32);
            }
        }
        None
    }

    pub fn apply_breakpoints(
        &mut self,
        process: &mut Process,
        resume_thread_id: u32,
    ) -> Result<(), Error> {
        for thread_id in process.threads() {
            let mut ctx = AlignedContext::ALL;
            let thread = AutoClosedHandle(unsafe {
                OpenThread(THREAD_GET_CONTEXT | THREAD_SET_CONTEXT, false, *thread_id)
                    .map_err(|error| WindowsError::new(WindowsFunction::OpenThread, error))?
            });
            unsafe {
                GetThreadContext(thread.0, &mut ctx.0)
                    .map_err(|error| WindowsError::new(WindowsFunction::GetThreadContext, error))?
            };

            // Currently there is a limit of 4 breakpoints, since we are using hardware breakpoints.
            for (idx, bp) in self.breakpoints.iter().enumerate() {
                match bp {
                    Some(bp) => {
                        match idx {
                            0 => ctx.Dr0 = bp.addr,
                            1 => ctx.Dr1 = bp.addr,
                            2 => ctx.Dr2 = bp.addr,
                            3 => ctx.Dr3 = bp.addr,
                            _ => unreachable!("Only 4 breakpoints possible right now!"),
                        }
                        let pattern = !(0b1111u64 << (idx as u64 * 4 + 16));
                        ctx.Dr7 = ctx.Dr7 & pattern;
                        // Enable breakpoint.
                        let pattern = 1u64 << (idx as u64 * 2);
                        ctx.Dr7 = ctx.Dr7 | pattern;
                    }
                    None => {
                        // Disable breakpoint.
                        let pattern = !(1u64 << (idx as u64 * 2));
                        ctx.Dr7 = ctx.Dr7 & pattern;
                    }
                }
            }

            // This prevents the current thread from hitting a breakpoint on the current instruction
            if *thread_id == resume_thread_id {
                ctx.EFlags = ctx.EFlags | (1 << 16);
            }
            unsafe {
                SetThreadContext(&thread, ctx.as_ptr())
                    .map_err(|error| WindowsError::new(WindowsFunction::SetThreadContext, error))?
            };
        }
        Ok(())
    }
}
