use std::{
    fmt::Debug,
    os::{
        raw::c_void,
        windows::{ffi::OsStringExt, io::FromRawHandle},
    },
};

use registers::Registers;
use windows::Win32::{
    Foundation::{
        DBG_CONTINUE, DBG_EXCEPTION_NOT_HANDLED, EXCEPTION_ACCESS_VIOLATION,
        EXCEPTION_ARRAY_BOUNDS_EXCEEDED, EXCEPTION_BREAKPOINT, EXCEPTION_DATATYPE_MISALIGNMENT,
        EXCEPTION_FLT_DENORMAL_OPERAND, EXCEPTION_FLT_DIVIDE_BY_ZERO, EXCEPTION_FLT_INEXACT_RESULT,
        EXCEPTION_FLT_INVALID_OPERATION, EXCEPTION_FLT_OVERFLOW, EXCEPTION_FLT_STACK_CHECK,
        EXCEPTION_FLT_UNDERFLOW, EXCEPTION_ILLEGAL_INSTRUCTION, EXCEPTION_INT_DIVIDE_BY_ZERO,
        EXCEPTION_INT_OVERFLOW, EXCEPTION_INVALID_DISPOSITION, EXCEPTION_IN_PAGE_ERROR,
        EXCEPTION_NONCONTINUABLE_EXCEPTION, EXCEPTION_PRIV_INSTRUCTION, EXCEPTION_SINGLE_STEP,
        EXCEPTION_STACK_OVERFLOW, NTSTATUS,
    },
    Storage::FileSystem::{GetFinalPathNameByHandleW, GETFINALPATHNAMEBYHANDLE_FLAGS},
    System::{
        Diagnostics::Debug::{
            ContinueDebugEvent, SetThreadContext, CREATE_PROCESS_DEBUG_INFO,
            CREATE_THREAD_DEBUG_INFO, DEBUG_EVENT, EXCEPTION_DEBUG_INFO, LOAD_DLL_DEBUG_INFO,
            OUTPUT_DEBUG_STRING_INFO,
        },
        Threading::GetThreadId,
    },
};

use crate::{
    breakpoints::BreakpointManager,
    error::{Error, WindowsError, WindowsFunction},
    ffi::{AlignedContext, AutoClosedHandle},
    memory::{MemorySource, ProcessMemoryReader},
    processes::Process,
    stack::StackFrame,
    Debugger,
};

mod registers;

#[derive(Debug, Clone, Copy)]
pub struct ExceptionEventKind {
    expect_step_exception: bool,
    pub is_first_chance: bool,
    pub code: ExceptionCode,
    pub breakpoint: Option<u32>,
}

#[derive(Debug, Clone)]
pub enum DebugEventKind {
    Unknown,
    Exception(ExceptionEventKind),
    CreateThread,
    CreateProcess(String),
    ExitThread,
    ExitProcess,
    LoadDll(String),
    UnloadDll,
    OutputDebugString(String),
    RipEvent,
}

impl DebugEventKind {
    pub fn should_continue(&self) -> bool {
        !matches!(self, Self::ExitProcess)
    }

    pub fn create_process(
        base_process: &mut Process,
        memory: ProcessMemoryReader,
        create_process_info: CREATE_PROCESS_DEBUG_INFO,
        debug_event: &DEBUG_EVENT,
    ) -> Result<DebugEventKind, Error> {
        let _file =
            unsafe { std::fs::File::from_raw_handle(create_process_info.hFile.0 as *mut c_void) };
        let exe_base = create_process_info.lpBaseOfImage as u64;
        let mut exe_name = vec![0u16; 260];
        let exe_name_len = unsafe {
            GetFinalPathNameByHandleW(
                create_process_info.hFile,
                &mut exe_name,
                GETFINALPATHNAMEBYHANDLE_FLAGS::default(),
            )
        } as usize;

        let exe_name = if exe_name_len != 0 {
            // This will be the full name, e.g. \\?\C:\git\HelloWorld\hello.exe
            // It might be useful to have the full name, but it's not available for all
            // modules in all cases.
            let full_path = std::ffi::OsString::from_wide(&exe_name[0..exe_name_len]);
            std::path::Path::new(&full_path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
        } else {
            None
        };
        base_process.add_thread(debug_event.dwThreadId);
        let module = base_process.add_module(exe_base, exe_name, memory)?;
        Ok(DebugEventKind::CreateProcess(module.name().into_owned()))
    }

    pub fn load_dll(
        process: &mut Process,
        memory: ProcessMemoryReader,
        load_dll: LOAD_DLL_DEBUG_INFO,
    ) -> Result<DebugEventKind, Error> {
        let dll_base: u64 = load_dll.lpBaseOfDll as u64;
        let dll_name = if load_dll.lpImageName.is_null() {
            None
        } else {
            let is_wide = load_dll.fUnicode != 0;
            memory
                .read_memory_string_indirect(load_dll.lpImageName as u64, 260, is_wide)
                .ok()
        };

        let module = process.add_module(dll_base, dll_name, memory)?;
        Ok(DebugEventKind::LoadDll(module.name().into_owned()))
    }

    pub fn exception(
        exception: EXCEPTION_DEBUG_INFO,
        breakpoint_manager: &BreakpointManager,
        ctx: &AlignedContext,
    ) -> DebugEventKind {
        let is_first_chance = exception.dwFirstChance != 0;
        let exception = exception.ExceptionRecord;
        let exception_code = ExceptionCode::try_from(exception.ExceptionCode).unwrap();
        let breakpoint = breakpoint_manager.was_breakpoint_hit(ctx);
        DebugEventKind::Exception(ExceptionEventKind {
            expect_step_exception: false,
            code: exception_code,
            is_first_chance,
            breakpoint,
        })
    }

    fn continue_status(&self) -> NTSTATUS {
        match self {
            Self::Exception(exception) => {
                if (exception.expect_step_exception && exception.code == ExceptionCode::SingleStep)
                    || exception.breakpoint.is_some()
                {
                    DBG_CONTINUE
                } else {
                    DBG_EXCEPTION_NOT_HANDLED
                }
            }
            _ => DBG_CONTINUE,
        }
    }

    pub(crate) fn output_debug_string(
        memory: ProcessMemoryReader,
        debug_string: OUTPUT_DEBUG_STRING_INFO,
    ) -> Result<DebugEventKind, Error> {
        let is_wide = debug_string.fUnicode != 0;
        let address = debug_string.lpDebugStringData.0 as u64;
        let len = debug_string.nDebugStringLength as usize;
        let debug_string = memory.read_memory_string(address, len, is_wide)?;
        Ok(DebugEventKind::OutputDebugString(debug_string))
    }

    pub(crate) fn create_thread(
        process: &mut Process,
        create_thread: CREATE_THREAD_DEBUG_INFO,
    ) -> DebugEventKind {
        let thread_handle = AutoClosedHandle(create_thread.hThread);
        let thread_id = unsafe { GetThreadId(&thread_handle) };
        process.add_thread(thread_id);
        DebugEventKind::CreateThread
    }
}

pub struct DebugEvent<'a> {
    pub parent: &'a mut Debugger,
    pub kind: DebugEventKind,
    pub(super) thread: AutoClosedHandle,
    pub(super) raw: DEBUG_EVENT,
    pub(super) ctx: AlignedContext,
    pub(super) continue_status: NTSTATUS,
}

impl<'a> DebugEvent<'a> {
    const TRAP_FLAG: u32 = 1 << 8;
    pub fn step_into(&mut self) -> Result<(), Error> {
        self.ctx.EFlags |= Self::TRAP_FLAG;
        unsafe {
            SetThreadContext(&self.thread, &self.ctx.0)
                .map_err(|e| WindowsError::new(WindowsFunction::SetThreadContext, e))?;
        }
        Ok(())
    }

    pub fn registers(&self) -> Registers<'static> {
        Registers::from_context(&self.ctx)
    }

    pub(crate) fn new(
        parent: &'a mut Debugger,
        kind: DebugEventKind,
        debug_event: DEBUG_EVENT,
        ctx: AlignedContext,
        thread: AutoClosedHandle,
    ) -> Self {
        let continue_status = kind.continue_status();
        Self {
            parent,
            kind,
            raw: debug_event,
            ctx,
            thread,
            continue_status,
        }
    }

    pub fn instruction_pointer(&self) -> u64 {
        self.ctx.Rip
    }

    pub fn look_up_symbol(&mut self, address: u64) -> Option<String> {
        self.parent.look_up_symbol(address)
    }

    pub fn read_memory(&self, address: usize) -> Result<Vec<u8>, Error> {
        self.parent.read_memory(address)
    }

    pub fn thread_id(&self) -> u32 {
        self.raw.dwThreadId
    }

    pub fn breakpoints(&self) -> Vec<crate::breakpoints::Breakpoint> {
        self.parent.breakpoints()
    }

    pub fn add_breakpoint(&mut self, address: usize) -> Option<usize> {
        self.parent.add_breakpoint(address)
    }

    pub fn resolve_symbol(&self, module_name: &str, function_name: &str) -> Option<u64> {
        self.parent.resolve_symbol(module_name, function_name)
    }

    pub fn clear_breakpoint(&mut self, index: usize) {
        self.parent.clear_breakpoint(index);
    }

    pub fn stack_frames(&mut self) -> Vec<StackFrame> {
        let mut result = Vec::new();
        let mut current = StackFrame::new(self.ctx);
        result.push(current);
        let memory_reader = self.parent.memory_reader();
        while let Some(parent) = current.find_parent(&mut self.parent.process, &memory_reader) {
            result.push(parent);
            current = parent;
        }
        result
    }
}

impl Drop for DebugEvent<'_> {
    fn drop(&mut self) {
        if !self.kind.should_continue() {
            return;
        }
        self.parent.apply_breakpoints(self.thread_id()).unwrap();
        unsafe {
            ContinueDebugEvent(
                self.raw.dwProcessId,
                self.raw.dwThreadId,
                self.continue_status,
            )
            .unwrap();
        }
    }
}

impl Debug for DebugEvent<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DebugEvent")
            .field("kind", &self.kind)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionCode {
    AccessViolation,
    ArrayBoundsExceeded,
    Breakpoint,
    DatatypeMisalignment,
    FloatDenormalOperand,
    FloatDivideByZero,
    FloatInexactResult,
    FloatInvalidOperation,
    FloatOverflow,
    FloatStackCheck,
    FloatUnderflow,
    IllegalInstruction,
    InPageError,
    IntDivideByZero,
    IntOverflow,
    InvalidDisposition,
    NoncontinueableException,
    PrivateInstruction,
    SingleStep,
    StackOverflow,
}

impl TryFrom<NTSTATUS> for ExceptionCode {
    type Error = NTSTATUS;

    fn try_from(value: NTSTATUS) -> Result<Self, Self::Error> {
        Ok(match value {
            EXCEPTION_ACCESS_VIOLATION => Self::AccessViolation,
            EXCEPTION_ARRAY_BOUNDS_EXCEEDED => Self::ArrayBoundsExceeded,
            EXCEPTION_BREAKPOINT => Self::Breakpoint,
            EXCEPTION_DATATYPE_MISALIGNMENT => Self::DatatypeMisalignment,
            EXCEPTION_FLT_DENORMAL_OPERAND => Self::FloatDenormalOperand,
            EXCEPTION_FLT_DIVIDE_BY_ZERO => Self::FloatDivideByZero,
            EXCEPTION_FLT_INEXACT_RESULT => Self::FloatInexactResult,
            EXCEPTION_FLT_INVALID_OPERATION => Self::FloatInvalidOperation,
            EXCEPTION_FLT_OVERFLOW => Self::FloatOverflow,
            EXCEPTION_FLT_STACK_CHECK => Self::FloatStackCheck,
            EXCEPTION_FLT_UNDERFLOW => Self::FloatUnderflow,
            EXCEPTION_ILLEGAL_INSTRUCTION => Self::IllegalInstruction,
            EXCEPTION_IN_PAGE_ERROR => Self::InPageError,
            EXCEPTION_INT_DIVIDE_BY_ZERO => Self::IntDivideByZero,
            EXCEPTION_INT_OVERFLOW => Self::IntOverflow,
            EXCEPTION_INVALID_DISPOSITION => Self::InvalidDisposition,
            EXCEPTION_NONCONTINUABLE_EXCEPTION => Self::NoncontinueableException,
            EXCEPTION_PRIV_INSTRUCTION => Self::PrivateInstruction,
            EXCEPTION_SINGLE_STEP => Self::SingleStep,
            EXCEPTION_STACK_OVERFLOW => Self::StackOverflow,
            err => return Err(err),
        })
    }
}
