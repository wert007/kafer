use std::{fmt::Debug, io::Cursor};

use crate::code_view::{self, CodeViewString};

macro_rules! bytes_to_primitive {
    ($buffer:expr, u16) => {{
        let buffer = $buffer;
        u16::from_le_bytes([buffer[0], buffer[1]])
    }};
    ($buffer:expr, u32) => {{
        let buffer = $buffer;
        u32::from_le_bytes([buffer[0], buffer[1], buffer[2], buffer[3]])
    }};
}

pub struct Parser<'a> {
    buffer: &'a [u8],
    position: usize,
}

impl<'a> Parser<'a> {
    pub fn new(buffer: &'a [u8]) -> Self {
        Self {
            buffer,
            position: 0,
        }
    }

    pub fn read_u32(&mut self) -> u32 {
        let result = bytes_to_primitive!(&self.buffer[self.position..], u32);
        self.position += 4;
        result
    }

    pub fn read_u16(&mut self) -> u16 {
        let result = bytes_to_primitive!(&self.buffer[self.position..], u16);
        self.position += 2;
        result
    }

    pub(crate) fn skip(&mut self, offset: usize) {
        self.position += offset;
        self.position = self.position.min(self.buffer.len());
    }

    pub(crate) fn remaining(&self) -> usize {
        self.buffer.len() - self.position
    }

    pub(crate) fn read_path_buf_with_length_trimmed(
        &mut self,
        length: usize,
    ) -> std::path::PathBuf {
        self.read_string_with_length_trimmed(length).into()
    }

    pub fn read_string_with_length(&mut self, length: usize) -> &'a str {
        std::str::from_utf8(self.read_bytes(length)).unwrap()
    }

    pub fn read_string_with_length_trimmed(&mut self, length: usize) -> &'a str {
        self.read_string_with_length(length).trim_end_matches('\0')
    }

    pub(crate) fn read_bytes(&mut self, length: usize) -> &'a [u8] {
        let result = &self.buffer[self.position..][..length];
        self.position += length;
        result
    }

    pub(crate) fn try_parse<T: code_view::RecordEntry + Debug + binrw::BinRead>(
        &mut self,
    ) -> Option<T>
    where
        for<'b> <T as binrw::BinRead>::Args<'b>: Default,
    {
        let pos = self.position;
        let record_size = self.read_u16();
        let record_type = self.read_u16();
        self.position = pos;
        if !T::is_valid_record_type(record_type) {
            return None;
        }
        let bytes = self.read_bytes(record_size as usize + 2);
        Some(<T>::read_le(&mut Cursor::new(bytes)).unwrap())
    }

    pub(crate) fn peek(&self) -> Self {
        Self {
            buffer: self.buffer,
            position: self.position,
        }
    }
}
