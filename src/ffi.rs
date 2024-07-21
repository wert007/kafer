use std::fmt::Display;

use windows::{
    core::{Param, PCWSTR, PWSTR},
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::Diagnostics::Debug::{CONTEXT, CONTEXT_ALL_X86, M128A, XSAVE_FORMAT},
    },
};

#[repr(align(16))]
#[derive(Clone, Copy)]
pub struct AlignedContext(pub(super) CONTEXT);

const fn zero_context() -> CONTEXT {
    let ctx: CONTEXT = CONTEXT {
        P1Home: 0,
        P2Home: 0,
        P3Home: 0,
        P4Home: 0,
        P5Home: 0,
        P6Home: 0,
        ContextFlags: windows::Win32::System::Diagnostics::Debug::CONTEXT_FLAGS(0),
        MxCsr: 0,
        SegCs: 0,
        SegDs: 0,
        SegEs: 0,
        SegFs: 0,
        SegGs: 0,
        SegSs: 0,
        EFlags: 0,
        Dr0: 0,
        Dr1: 0,
        Dr2: 0,
        Dr3: 0,
        Dr6: 0,
        Dr7: 0,
        Rax: 0,
        Rcx: 0,
        Rdx: 0,
        Rbx: 0,
        Rsp: 0,
        Rbp: 0,
        Rsi: 0,
        Rdi: 0,
        R8: 0,
        R9: 0,
        R10: 0,
        R11: 0,
        R12: 0,
        R13: 0,
        R14: 0,
        R15: 0,
        Rip: 0,
        Anonymous: windows::Win32::System::Diagnostics::Debug::CONTEXT_0 {
            FltSave: XSAVE_FORMAT {
                ControlWord: 0,
                StatusWord: 0,
                TagWord: 0,
                Reserved1: 0,
                ErrorOpcode: 0,
                ErrorOffset: 0,
                ErrorSelector: 0,
                Reserved2: 0,
                DataOffset: 0,
                DataSelector: 0,
                Reserved3: 0,
                MxCsr: 0,
                MxCsr_Mask: 0,
                FloatRegisters: [M128A { Low: 0, High: 0 }; 8],
                XmmRegisters: [M128A { Low: 0, High: 0 }; 16],
                Reserved4: [0; 96],
            },
        },
        VectorRegister: [M128A { Low: 0, High: 0 }; 26],
        VectorControl: 0,
        DebugControl: 0,
        LastBranchToRip: 0,
        LastBranchFromRip: 0,
        LastExceptionToRip: 0,
        LastExceptionFromRip: 0,
    };
    ctx
}

impl AlignedContext {
    pub const ALL: AlignedContext = AlignedContext(CONTEXT {
        ContextFlags: CONTEXT_ALL_X86,
        ..zero_context()
    });

    pub(crate) fn as_ptr(&self) -> *const CONTEXT {
        &self.0 as _
    }
}

impl std::ops::Deref for AlignedContext {
    type Target = CONTEXT;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for AlignedContext {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Clone)]
pub struct AutoClosedHandle(pub HANDLE);

impl std::ops::Deref for AutoClosedHandle {
    type Target = HANDLE;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for AutoClosedHandle {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl windows::core::IntoParam<HANDLE> for &AutoClosedHandle {
    fn into_param(self) -> Param<HANDLE> {
        Param::Borrowed(self.0)
    }
}

impl Drop for AutoClosedHandle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0).unwrap();
        }
    }
}

pub struct WideString {
    buffer: Vec<u16>,
}

impl From<String> for WideString {
    fn from(val: String) -> Self {
        let input = val.as_bytes();
        let mut buffer = Vec::new();
        let mut input_pos = 0;
        while let Some(mut code_point) = decode_utf8_char(input, &mut input_pos) {
            if code_point <= 0xffff {
                buffer.push(code_point as _);
            } else {
                code_point -= 0x10000;
                buffer.push(0xd800 + (code_point >> 10) as u16);
                buffer.push(0xdc00 + (code_point & 0x3ff) as u16);
            }
        }
        WideString { buffer }
    }
}

impl windows::core::IntoParam<PCWSTR> for &WideString {
    fn into_param(self) -> Param<PCWSTR> {
        Param::Borrowed(PCWSTR::from_raw(self.buffer.as_ptr()))
    }
}

impl windows::core::IntoParam<PWSTR> for &mut WideString {
    fn into_param(self) -> Param<PWSTR> {
        Param::Borrowed(PWSTR::from_raw(self.buffer.as_mut_ptr()))
    }
}

impl Display for WideString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        String::from_utf16(&self.buffer)
            .expect("Right now it is impossible to be invalid utf8 here!")
            .fmt(f)
    }
}

impl WideString {
    pub fn as_pwstr(&mut self) -> PWSTR {
        PWSTR::from_raw(self.buffer.as_mut_ptr())
    }
}

// fn utf16_len(bytes: &[u8]) -> usize {
//     let mut pos = 0;
//     let mut len = 0;
//     while let Some((code_point, new_pos)) = decode_utf8_char(bytes, pos) {
//         pos = new_pos;
//         len += if code_point <= 0xffff { 1 } else { 2 };
//     }
//     len
// }

fn decode_utf8_char(bytes: &[u8], pos: &mut usize) -> Option<u32> {
    if bytes.len() == *pos {
        return None;
    }
    let ch = bytes[*pos] as u32;
    *pos += 1;
    if ch <= 0x7f {
        return Some(ch);
    }
    if (ch & 0xe0) == 0xc0 {
        if bytes.len() - *pos < 1 {
            return None;
        }
        let ch2 = bytes[*pos] as u32;
        *pos += 1;
        if (ch2 & 0xc0) != 0x80 {
            return None;
        }
        let result: u32 = ((ch & 0x1f) << 6) | (ch2 & 0x3f);
        if result <= 0x7f {
            return None;
        }
        return Some(result);
    }
    if (ch & 0xf0) == 0xe0 {
        if bytes.len() - *pos < 2 {
            return None;
        }
        let ch2 = bytes[*pos] as u32;
        *pos += 1;
        let ch3 = bytes[*pos] as u32;
        *pos += 1;
        if (ch2 & 0xc0) != 0x80 || (ch3 & 0xc0) != 0x80 {
            return None;
        }
        let result = ((ch & 0x0f) << 12) | ((ch2 & 0x3f) << 6) | (ch3 & 0x3f);
        if result <= 0x7ff || (0xd800..=0xdfff).contains(&result) {
            return None;
        }
        return Some(result);
    }
    if (ch & 0xf8) == 0xf0 {
        if bytes.len() - *pos < 3 {
            return None;
        }
        let ch2 = bytes[*pos] as u32;
        *pos += 1;
        let ch3 = bytes[*pos] as u32;
        *pos += 1;
        let ch4 = bytes[*pos] as u32;
        *pos += 1;
        if (ch2 & 0xc0) != 0x80 || (ch3 & 0xc0) != 0x80 || (ch4 & 0xc0) != 0x80 {
            return None;
        }
        let result =
            ((ch & 0x07) << 18) | ((ch2 & 0x3f) << 12) | ((ch3 & 0x3f) << 6) | (ch4 & 0x3f);
        if result <= 0xffff || 0x10ffff < result {
            return None;
        }
        return Some(result);
    }
    None
}
