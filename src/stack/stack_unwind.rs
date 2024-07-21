use crate::{error::Error, ffi::AlignedContext, memory::MemorySource};

const UWOP_PUSH_NONVOL: u8 = 0; /* info == register number */
const UWOP_ALLOC_LARGE: u8 = 1; /* no info, alloc size in next 2 slots */
const UWOP_ALLOC_SMALL: u8 = 2; /* info == size of allocation / 8 - 1 */
const UWOP_SET_FPREG: u8 = 3; /* no info, FP = RSP + UNWIND_INFO.FPRegOffset*16 */
const UWOP_SAVE_NONVOL: u8 = 4; /* info == register number, offset in next slot */
const UWOP_SAVE_NONVOL_FAR: u8 = 5; /* info == register number, offset in next 2 slots */
const UWOP_SAVE_XMM128: u8 = 8; /* info == XMM reg number, offset in next slot */
const UWOP_SAVE_XMM128_FAR: u8 = 9; /* info == XMM reg number, offset in next 2 slots */

// These represent the logical operations, so large/small and far/near are merged
#[derive(Debug, Clone, Copy)]
pub enum UnwindOp {
    PushNonVolatile {
        reg: Register,
    },
    Alloc {
        size: u32,
    },
    SetFpreg {
        frame_register: Register,
        frame_offset: u16,
    },
    SaveNonVolatile {
        reg: Register,
        offset: u32,
    },
    SaveXmm128 {
        reg: Register,
        offset: u32,
    },
    #[allow(dead_code)]
    PushMachFrame {
        error_code: bool,
    },
}

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum Register {
    Rax = 0,
    Rcx,
    Rdx,
    Rbx,
    Rsp,
    Rbp,
    Rsi,
    Rdi,
    R8,
    R9,
    R10,
    R11,
    R12,
    R13,
    R14,
    R15,
}
impl Register {
    fn get_mut<'a>(&self, context: &'a mut AlignedContext) -> &'a mut u64 {
        match self {
            Register::Rax => &mut context.Rax,
            Register::Rcx => &mut context.Rcx,
            Register::Rdx => &mut context.Rdx,
            Register::Rbx => &mut context.Rbx,
            Register::Rsp => &mut context.Rsp,
            Register::Rbp => &mut context.Rbp,
            Register::Rsi => &mut context.Rsi,
            Register::Rdi => &mut context.Rdi,
            Register::R8 => &mut context.R8,
            Register::R9 => &mut context.R9,
            Register::R10 => &mut context.R10,
            Register::R11 => &mut context.R11,
            Register::R12 => &mut context.R12,
            Register::R13 => &mut context.R13,
            Register::R14 => &mut context.R14,
            Register::R15 => &mut context.R15,
        }
    }

    fn get(&self, context: AlignedContext) -> u64 {
        match self {
            Register::Rax => context.Rax,
            Register::Rcx => context.Rcx,
            Register::Rdx => context.Rdx,
            Register::Rbx => context.Rbx,
            Register::Rsp => context.Rsp,
            Register::Rbp => context.Rbp,
            Register::Rsi => context.Rsi,
            Register::Rdi => context.Rdi,
            Register::R8 => context.R8,
            Register::R9 => context.R9,
            Register::R10 => context.R10,
            Register::R11 => context.R11,
            Register::R12 => context.R12,
            Register::R13 => context.R13,
            Register::R14 => context.R14,
            Register::R15 => context.R15,
        }
    }
}

impl TryFrom<u8> for Register {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => Self::Rax,
            1 => Self::Rcx,
            2 => Self::Rdx,
            3 => Self::Rbx,
            4 => Self::Rsp,
            5 => Self::Rbp,
            6 => Self::Rsi,
            7 => Self::Rdi,
            8 => Self::R8,
            9 => Self::R9,
            10 => Self::R10,
            11 => Self::R11,
            12 => Self::R12,
            13 => Self::R13,
            14 => Self::R14,
            15 => Self::R15,
            err => return Err(err),
        })
    }
}

// Does not directly correspond to UNWIND_CODE
#[derive(Debug, Clone, Copy)]
pub struct UnwindCode {
    code_offset: u8,
    op: UnwindOp,
}

// fn get_op_register<'a>(context: &'a mut AlignedContext, reg: u8) -> &'a mut u64 {
//     match reg {
//         0 => &mut context.Rax,
//         1 => &mut context.Rcx,
//         2 => &mut context.Rdx,
//         3 => &mut context.Rbx,
//         4 => &mut context.Rsp,
//         5 => &mut context.Rbp,
//         6 => &mut context.Rsi,
//         7 => &mut context.Rdi,
//         8 => &mut context.R8,
//         9 => &mut context.R9,
//         10 => &mut context.R10,
//         11 => &mut context.R11,
//         12 => &mut context.R12,
//         13 => &mut context.R13,
//         14 => &mut context.R14,
//         15 => &mut context.R15,
//         _ => panic!("Bad register given to get_op_register()"),
//     }
// }

impl UnwindCode {
    pub(crate) fn apply(
        &self,
        mut context: AlignedContext,
        func_address: u64,
        memory_source: &impl MemorySource,
    ) -> Result<AlignedContext, Error> {
        let func_offset = context.Rip - func_address;
        if self.code_offset as u64 > func_offset {
            return Ok(context);
        }
        match self.op {
            UnwindOp::Alloc { size } => {
                context.Rsp += size as u64;
            }
            UnwindOp::PushNonVolatile { reg } => {
                let addr = context.Rsp;
                let val = memory_source.read_memory_data::<u64>(addr)?;
                *reg.get_mut(&mut context) = val;
                context.Rsp += 8;
            }
            UnwindOp::SaveNonVolatile { reg, offset } => {
                let addr = context.Rsp + offset as u64;
                let val = memory_source.read_memory_data::<u64>(addr)?;
                *reg.get_mut(&mut context) = val;
            }
            UnwindOp::SetFpreg {
                frame_register,
                frame_offset,
            } => {
                context.Rsp = frame_register.get(context) - (frame_offset as u64);
            }
            _ => todo!("unwind op"),
        }
        Ok(context)
    }
}

pub enum UnwindCodeParseError {
    IncompleteOp(u8),
    UnknownOp(u8),
    InvalidRegister(u8),
}

pub fn parse_unwind_ops(
    code_slots: &[u16],
    frame_register: u8,
    frame_offset: u16,
) -> Result<Vec<UnwindCode>, UnwindCodeParseError> {
    let mut ops = Vec::<UnwindCode>::new();

    let mut i = 0;
    while i < code_slots.len() {
        let (code_offset, unwind_op, op_info) = split_up!(code_slots[i] => 8, 4, 4);
        let code_offset = code_offset as u8;
        let unwind_op = unwind_op as u8;
        let op_info = op_info as u8;
        match unwind_op {
            UWOP_PUSH_NONVOL => {
                ops.push(UnwindCode {
                    code_offset,
                    op: UnwindOp::PushNonVolatile {
                        reg: op_info
                            .try_into()
                            .map_err(UnwindCodeParseError::InvalidRegister)?,
                    },
                });
            }
            UWOP_ALLOC_LARGE if op_info == 0 => {
                if i + 1 >= code_slots.len() {
                    return Err(UnwindCodeParseError::IncompleteOp(UWOP_ALLOC_LARGE));
                }
                let size = (code_slots[i + 1] as u32) * 8;
                ops.push(UnwindCode {
                    code_offset,
                    op: UnwindOp::Alloc { size },
                });
                i += 1;
            }
            UWOP_ALLOC_LARGE if op_info == 1 => {
                if i + 2 >= code_slots.len() {
                    return Err(UnwindCodeParseError::IncompleteOp(UWOP_ALLOC_LARGE));
                }
                let size = code_slots[i + 1] as u32 + ((code_slots[i + 2] as u32) << 16);
                ops.push(UnwindCode {
                    code_offset,
                    op: UnwindOp::Alloc { size },
                });
                i += 2;
            }
            UWOP_ALLOC_SMALL => {
                let size = (op_info as u32) * 8 + 8;
                ops.push(UnwindCode {
                    code_offset,
                    op: UnwindOp::Alloc { size },
                });
            }
            UWOP_SET_FPREG => {
                ops.push(UnwindCode {
                    code_offset,
                    op: UnwindOp::SetFpreg {
                        frame_register: frame_register
                            .try_into()
                            .map_err(UnwindCodeParseError::InvalidRegister)?,
                        frame_offset,
                    },
                });
            }
            UWOP_SAVE_NONVOL => {
                if i + 1 >= code_slots.len() {
                    return Err(UnwindCodeParseError::IncompleteOp(UWOP_SAVE_NONVOL));
                }
                let offset = code_slots[i + 1] as u32;
                ops.push(UnwindCode {
                    code_offset,
                    op: UnwindOp::SaveNonVolatile {
                        reg: op_info
                            .try_into()
                            .map_err(UnwindCodeParseError::InvalidRegister)?,
                        offset,
                    },
                });
                i += 1;
            }
            UWOP_SAVE_NONVOL_FAR => {
                if i + 2 >= code_slots.len() {
                    return Err(UnwindCodeParseError::IncompleteOp(UWOP_SAVE_NONVOL_FAR));
                }
                let offset = code_slots[i + 1] as u32 + ((code_slots[i + 2] as u32) << 16);
                ops.push(UnwindCode {
                    code_offset,
                    op: UnwindOp::SaveNonVolatile {
                        reg: op_info
                            .try_into()
                            .map_err(UnwindCodeParseError::InvalidRegister)?,
                        offset,
                    },
                });
                i += 2;
            }
            UWOP_SAVE_XMM128 => {
                if i + 1 >= code_slots.len() {
                    return Err(UnwindCodeParseError::IncompleteOp(UWOP_SAVE_XMM128));
                }
                let offset = code_slots[i + 1] as u32;
                ops.push(UnwindCode {
                    code_offset,
                    op: UnwindOp::SaveXmm128 {
                        reg: op_info
                            .try_into()
                            .map_err(UnwindCodeParseError::InvalidRegister)?,
                        offset,
                    },
                });
                i += 1;
            }
            UWOP_SAVE_XMM128_FAR => {
                if i + 2 >= code_slots.len() {
                    return Err(UnwindCodeParseError::IncompleteOp(UWOP_SAVE_XMM128_FAR));
                }
                let offset = code_slots[i + 1] as u32 + ((code_slots[i + 2] as u32) << 16);
                ops.push(UnwindCode {
                    code_offset,
                    op: UnwindOp::SaveXmm128 {
                        reg: op_info
                            .try_into()
                            .map_err(UnwindCodeParseError::InvalidRegister)?,
                        offset,
                    },
                });
                i += 2;
            }
            err => return Err(UnwindCodeParseError::UnknownOp(err)),
        }
        i += 1;
    }

    Ok(ops)
}
