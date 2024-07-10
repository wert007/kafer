use std::borrow::Cow;

use crate::ffi::AlignedContext;

macro_rules! r {
    ($name:literal, $value:expr) => {
        Register {
            name: $name.into(),
            value: $value,
        }
    };
}

pub struct Registers<'a> {
    registers: Vec<Register<'a>>,
}

impl Registers<'static> {
    pub fn from_context(ctx: &AlignedContext) -> Registers<'static> {
        Self {
            registers: vec![
                r! {"rax", ctx.Rax},
                r! {"rbx", ctx.Rbx},
                r! {"rcx", ctx.Rcx},
                r! {"rdx", ctx.Rdx},
                r! {"rsi", ctx.Rsi},
                r! {"rdi", ctx.Rdi},
                r! {"rip", ctx.Rip},
                r! {"rsp", ctx.Rsp},
                r! {"rbp", ctx.Rbp},
                r! {"r8", ctx.R8},
                r! {"r9", ctx.R9},
                r! {"r10", ctx.R10},
                r! {"r11", ctx.R11},
                r! {"r12", ctx.R12},
                r! {"r13", ctx.R13},
                r! {"r14", ctx.R14},
                r! {"r15", ctx.R15},
                r! {"eflags", ctx.EFlags as _},
            ],
        }
    }

    pub fn print(&self) {
        for line in self.registers.chunks(3) {
            for reg in line {
                print!("{:03}={:#018x} ", reg.name, reg.value);
            }
            println!();
        }
    }
}

pub struct Register<'a> {
    name: Cow<'a, str>,
    value: u64,
}
