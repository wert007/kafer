use std::ffi::c_void;

use windows::Win32::{Foundation::HANDLE, System::Diagnostics::Debug::ReadProcessMemory};

use crate::error::{Error, WindowsError, WindowsFunction};

#[allow(dead_code)]
pub trait MemorySource {
    /// Read up to "len" bytes, and return Option<u8> to represent what bytes
    /// are available in the range
    fn read_memory(&self, address: u64, len: usize) -> Result<Vec<Option<u8>>, Error>;
    /// Read up to "len" bytes, and stop at the first failure
    fn read_raw_memory(&self, address: u64, len: usize) -> Result<Vec<u8>, Error>;

    fn read_memory_array<T: Sized + Default>(
        &self,
        address: u64,
        max_count: usize,
    ) -> Result<Vec<T>, Error> {
        let element_size = ::core::mem::size_of::<T>();
        let max_bytes = max_count * element_size;
        let raw_bytes = self.read_raw_memory(address, max_bytes)?;
        let mut data: Vec<T> = Vec::with_capacity(max_count);
        let mut offset: usize = 0;
        while offset + element_size <= raw_bytes.len() {
            let mut item: T = T::default();
            let dst = &mut item as *mut T as *mut u8;
            let src = &raw_bytes[offset] as *const u8;
            unsafe { std::ptr::copy_nonoverlapping(src, dst, element_size) };
            data.push(item);
            offset += element_size;
        }

        Ok(data)
    }

    fn read_memory_full_array<T: Sized + Default>(
        &self,
        address: u64,
        count: usize,
    ) -> Result<Vec<T>, Error> {
        let result = self.read_memory_array(address, count)?;
        if result.len() == count {
            Ok(result)
        } else {
            Err(Error::MemorySourceNotEnoughData)
        }
    }

    fn read_memory_data<T: Sized + Default + Copy>(&self, address: u64) -> Result<T, Error> {
        let data = self.read_memory_array::<T>(address, 1)?;
        Ok(data[0])
    }

    fn read_memory_string(
        &self,
        address: u64,
        max_count: usize,
        is_wide: bool,
    ) -> Result<String, Error> {
        let result: String = if is_wide {
            let mut words = self.read_memory_array::<u16>(address, max_count)?;
            let null_pos = words.iter().position(|&v| v == 0);
            if let Some(null_pos) = null_pos {
                words.truncate(null_pos);
            }
            String::from_utf16_lossy(&words)
        } else {
            let mut bytes = self.read_memory_array::<u8>(address, max_count)?;
            let null_pos = bytes.iter().position(|&v| v == 0);
            if let Some(null_pos) = null_pos {
                bytes.truncate(null_pos);
            }
            String::from_utf8(bytes).unwrap()
        };
        Ok(result)
    }

    fn read_memory_string_indirect(
        &self,
        address: u64,
        max_count: usize,
        is_wide: bool,
    ) -> Result<String, Error> {
        let string_address = self.read_memory_data::<u64>(address)?;
        self.read_memory_string(string_address, max_count, is_wide)
    }
}

pub struct ProcessMemoryReader {
    handle: HANDLE,
}

impl ProcessMemoryReader {
    pub fn from_process_handle(handle: HANDLE) -> Self {
        Self { handle }
    }
}

impl MemorySource for ProcessMemoryReader {
    fn read_memory(&self, address: u64, len: usize) -> Result<Vec<Option<u8>>, Error> {
        let mut buffer: Vec<u8> = vec![0; len];
        let mut data: Vec<Option<u8>> = vec![None; len];
        let mut offset: usize = 0;

        while offset < len {
            let mut bytes_read: usize = 0;
            let len_left = len - offset;
            let cur_address = address + (offset as u64);

            unsafe {
                ReadProcessMemory(
                    self.handle,
                    cur_address as *const c_void,
                    buffer.as_mut_ptr() as *mut c_void,
                    len_left,
                    Some(&mut bytes_read as *mut usize),
                )
                .map_err(|e| WindowsError::new(WindowsFunction::ReadProcessMemory, e))?
            };

            for (index, value) in buffer.iter().copied().enumerate().take(bytes_read) {
                data[offset + index] = Some(value);
            }

            if bytes_read > 0 {
                offset += bytes_read;
            } else {
                offset += 1;
            }
        }

        Ok(data)
    }

    fn read_raw_memory(&self, address: u64, len: usize) -> Result<Vec<u8>, Error> {
        let mut buffer: Vec<u8> = vec![0; len];
        let mut bytes_read: usize = 0;

        if unsafe {
            ReadProcessMemory(
                self.handle,
                address as *const c_void,
                buffer.as_mut_ptr() as *mut c_void,
                len,
                Some(&mut bytes_read as *mut usize),
            )
        }
        .is_err()
        {
            bytes_read = 0;
        }

        buffer.truncate(bytes_read);

        Ok(buffer)
    }
}
