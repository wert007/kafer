use std::{any, collections::HashMap, path::PathBuf};

use code_view::RecordEntry;
use parser::Parser;
use pdb2::StreamIndex;
#[cfg(target_endian = "little")]
mod code_view;
mod parser;
#[derive(Debug)]
pub struct DebugSymbolsCollection<'s, S> {
    files: HashMap<PathBuf, DebugSymbolsFromFile>,
    reader: pdb2::PDB<'s, S>,
}

impl DebugSymbolsCollection<'_, std::fs::File> {
    pub fn read_from_file(file: impl Into<PathBuf>) -> Result<Self, pdb2::Error> {
        let mut reader = pdb2::PDB::open(std::fs::File::open(file.into())?)?;
        let mut files = HashMap::new();
        let mut index = 0;
        loop {
            if reader.raw_stream(StreamIndex(index)).is_err() {
                if index == u16::MAX {
                    break;
                } else {
                    index += 1;
                    continue;
                }
            }
            if let Some(file) = read_symbols_for_file(&mut reader, StreamIndex(index))? {
                files.insert(file.file_path.clone(), file);
            }
            if index == u16::MAX {
                break;
            }
            index += 1;
        }
        Ok(Self { files, reader })
    }
}

fn read_symbols_for_file(
    reader: &mut pdb2::PDB<'_, std::fs::File>,
    i: StreamIndex,
) -> pdb2::Result<Option<DebugSymbolsFromFile>> {
    let Some(stream) = reader.raw_stream(i)? else {
        return Ok(None);
    };
    let mut parser = Parser::new(stream.as_slice());
    if parser.remaining() == 0 {
        return Ok(None);
    }
    let kind = parser.read_u32();
    let length = parser.read_u16();
    if kind != 4 {
        return Ok(None);
    }
    let _padding = parser.read_u16();
    let _sbz = parser.read_u32();
    let file_path = parser.read_path_buf_with_length_trimmed(length as usize - 6);
    let mut result = DebugSymbolsFromFile {
        stream_index: i,
        file_path,
    };
    result.read(reader)?;
    return Ok(Some(result));
}

#[derive(Debug)]
pub struct DebugSymbolsFromFile {
    stream_index: StreamIndex,
    file_path: PathBuf,
}

impl DebugSymbolsFromFile {
    pub fn read(&mut self, reader: &mut pdb2::PDB<'_, std::fs::File>) -> pdb2::Result<()> {
        let stream = reader
            .raw_stream(self.stream_index)?
            .expect("StreamIndex should be valid at this point!");
        let mut parser = Parser::new(&stream);
        parser.read_u32();
        let length = parser.read_u16();
        parser.skip(length as _);
        // parser.try_parse(0x113c);
        let _version: Option<code_view::CompileSym> = parser.try_parse::<code_view::CompileSym>();
        dbg!(_version.unwrap());
        loop {
            let mut peek = parser.peek();
            let length = peek.read_u16();
            let kind = peek.read_u16();
            match kind {
                0x6 => break,
                // 0x113c | 0x1116 => {}
                0x1107 => {
                    dbg!(parser.try_parse::<code_view::ConstantSymbol>().unwrap());
                }
                0x1124 => {
                    dbg!(parser.try_parse::<code_view::Namespace>().unwrap());
                }
                _ => {
                    parser.skip(length as usize - 2);
                    println!("next kind is {kind:#06x}");
                    continue;
                }
            };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_file() {
        let x = DebugSymbolsCollection::read_from_file("../a.pdb").unwrap();
        // dbg!(x);
        panic!();
    }
}
