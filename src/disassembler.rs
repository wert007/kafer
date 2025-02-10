use std::fmt::Display;

use iced_x86::{Decoder, DecoderOptions, Formatter, NasmFormatter};

use crate::{error::Error, memory::MemorySource};

pub struct Instruction {
    raw: iced_x86::Instruction,
    bytes: [u8; 15],
    hexbytes_column_byte_length: usize,
}
impl Instruction {
    fn new(raw: iced_x86::Instruction, bytes: &[u8]) -> Self {
        Self {
            raw,
            bytes: std::array::from_fn(|i| bytes.get(i).copied().unwrap_or_default()),
            hexbytes_column_byte_length: 10,
        }
    }
}

impl Display for Instruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:016X} ", self.raw.ip())?;
        let instr_bytes = &self.bytes[..self.raw.len()];
        for b in instr_bytes.iter() {
            write!(f, "{:02X}", b)?;
        }
        if instr_bytes.len() < self.hexbytes_column_byte_length {
            for _ in 0..self.hexbytes_column_byte_length - instr_bytes.len() {
                write!(f, "  ")?;
            }
        }
        let mut output = String::new();
        let mut formatter = NasmFormatter::new();
        formatter.format(&self.raw, &mut output);

        write!(f, " {}", output)?;
        Ok(())
    }
}

pub(crate) fn disassemble(
    memory_source: impl MemorySource,
    addr: u64,
    line_count: usize,
) -> Result<Vec<Instruction>, Error> {
    let bytes = memory_source.read_raw_memory(addr, line_count * 15)?;
    if bytes.len() == 0 {
        return Err(Error::MemorySourceNotEnoughData);
    }

    let code_bitness = 64;
    let decoder = Decoder::with_ip(code_bitness, bytes.as_slice(), addr, DecoderOptions::NONE);
    Ok(decoder
        .into_iter()
        .take(line_count)
        .map(|i| {
            Instruction::new(
                i,
                &bytes[(i.ip() - addr) as usize..(i.next_ip() - addr) as usize],
            )
        })
        .collect())
}
