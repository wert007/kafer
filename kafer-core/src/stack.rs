use ffi::{RUNTIME_FUNCTION, UNWIND_INFO};
use windows::Win32::System::Diagnostics::Debug::{
    IMAGE_DIRECTORY_ENTRY_EXCEPTION, UNW_FLAG_CHAININFO,
};

use crate::{ffi::AlignedContext, memory::MemorySource, processes::Process};

mod ffi;

// Splits an integer up that represents bitfields so that each field can be stored in a tuple. Specify the
// size of the fields from low bits to high bits. For instance, let (x, y, z) = split_up!(q => 3, 6, 7) will put the low 3 bits into x
macro_rules! split_up {
    ($value:expr => $($len:expr),+) => {
        {
            let mut _value = $value;
            // Use a tuple to collect the fields
            ( $(
                {
                    let field = _value & ((1 << $len) - 1); // Mask the value to get the field
                    _value >>= $len; // Shift the value for the next field
                    field
                }
            ),+ ) // The '+' sign indicates one or more repetitions
        }
    };
}

mod stack_unwind;

#[derive(Clone, Copy)]
pub struct StackFrame {
    pub context: AlignedContext,
}

impl StackFrame {
    pub fn new(context: AlignedContext) -> Self {
        Self { context }
    }

    pub fn find_parent(
        &self,
        process: &mut Process,
        memory_source: &impl MemorySource,
    ) -> Option<Self> {
        let module = process.get_module_by_address(self.context.Rip)?;
        let data_directory = module.get_data_directory(IMAGE_DIRECTORY_ENTRY_EXCEPTION)?;
        let count = data_directory.Size as usize / std::mem::size_of::<RUNTIME_FUNCTION>();
        let table_address = module.address + data_directory.VirtualAddress as u64;

        // Note: In a real debugger you might want to cache these.
        let functions: Vec<RUNTIME_FUNCTION> =
            memory_source.read_memory_array(table_address, count).ok()?;
        let rva = self.context.Rip - module.address;
        let function = find_runtime_function(rva as _, &functions);
        let Some(function) = function else {
            let mut context = self.context;
            context.Rip = memory_source.read_memory_data(context.Rsp).ok()?;
            context.Rsp += 8;
            return Some(StackFrame::new(context));
        };
        // We have unwind data!
        let info_addr = module.address + function.UnwindInfo as u64;
        let info: UNWIND_INFO = memory_source.read_memory_data(info_addr).ok()?;
        let (_version, flags) = split_up!(info.version_flags => 3, 5);
        if flags as u32 & UNW_FLAG_CHAININFO.0 == UNW_FLAG_CHAININFO.0 {
            todo!("Implement chained info!");
        }

        let (frame_register, frame_offset) = split_up!(info.frame_register_offset => 4, 4);
        let frame_offset = (frame_offset as u16) * 16;
        // The codes are UNWIND_CODE, but we'll have to break them up in different ways anyway based on the operation, so we might as well just
        // read them as u16 and then parse out the fields as needed.
        let codes = memory_source
            .read_memory_full_array::<u16>(info_addr + 4, info.count_of_codes as usize)
            .ok()?;
        let func_address = module.address + function.BeginAddress as u64;
        let unwind_ops =
            stack_unwind::parse_unwind_ops(&codes, frame_register, frame_offset).ok()?;
        let mut ctx = unwind_ops
            .into_iter()
            .try_fold(self.context, |c, op| {
                op.apply(c, func_address, memory_source)
            })
            .ok()?;
        ctx.Rip = memory_source.read_memory_data::<u64>(ctx.Rsp).ok()?;
        ctx.Rsp += 8;

        // TODO: There are other conditions that should be checked
        if ctx.Rip == 0 {
            return None;
        }
        Some(StackFrame::new(ctx))
    }
}

fn find_runtime_function(
    addr: u32,
    function_list: &[RUNTIME_FUNCTION],
) -> Option<&RUNTIME_FUNCTION> {
    let index = function_list.binary_search_by(|func| func.BeginAddress.cmp(&addr));

    match index {
        // Exact match
        Ok(pos) => function_list.get(pos),
        // Inexact match
        Err(pos) => {
            if pos > 0
                && function_list.get(pos - 1).map_or(false, |func| {
                    func.BeginAddress <= addr && addr < func.EndAddress
                })
            {
                function_list.get(pos - 1)
            } else if pos < function_list.len()
                && function_list.get(pos).map_or(false, |func| {
                    func.BeginAddress <= addr && addr < func.EndAddress
                })
            {
                function_list.get(pos)
            } else {
                None
            }
        }
    }
}
