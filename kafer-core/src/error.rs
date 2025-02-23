use std::fmt::Display;

use thiserror::Error;

#[derive(Debug)]
pub enum WindowsFunction {
    CreateProcessW,
    CloseHandle,
    WaitForDebugEventEx,
    ContinueDebugEvent,
    OpenThread,
    GetThreadContext,
    SetThreadContext,
    ReadProcessMemory,
}

#[derive(Debug)]
pub struct WindowsError {
    source: WindowsFunction,
    error: windows::core::Error,
}
impl WindowsError {
    pub fn new(source: WindowsFunction, error: windows::core::Error) -> Self {
        Self { source, error }
    }
}

impl Display for WindowsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowsError")
            .field("source", &self.source)
            .field("error", &self.error)
            .finish()
    }
}

impl std::error::Error for WindowsError {}

#[derive(Debug, Error)]
pub enum Error {
    #[error("WindowsError failed. {0:#?}")]
    WindowsError(#[from] WindowsError),
    #[error("MemorySource could not supply enough data.")]
    MemorySourceNotEnoughData,
    #[error("Did not find a module named `{0}`.")]
    UnknownModuleName(String),
    #[error("Add a real error message here!.")]
    Todo,
    #[error("Error in pdb2. {0}")]
    Pdb2(#[from] pdb2::Error),
}
