use std::mem::size_of;

use windows::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};

use crate::error::MemoryError;
use crate::win::process::ProcessHandle;

/// Resolve pointer chain — هر tick صدا زده می‌شود.
pub fn resolve_chain(
    process: &ProcessHandle,
    base_ptr: u32,
    offsets: &[u32],
) -> Result<u32, MemoryError> {
    if base_ptr == 0 {
        return Err(MemoryError::InvalidAddress { address: 0 });
    }

    let reader = MemoryReader::new(process);
    let mut addr = base_ptr;

    for (step, &offset) in offsets.iter().enumerate() {
        let next = reader.read_u32(addr).map_err(|_| MemoryError::ChainBroken {
            step,
            address: addr,
        })?;
        if next == 0 {
            return Err(MemoryError::ChainBroken {
                step,
                address: addr,
            });
        }
        addr = next.wrapping_add(offset);
    }

    Ok(addr)
}

pub struct MemoryReader<'a> {
    handle: windows::Win32::Foundation::HANDLE,
    _marker: std::marker::PhantomData<&'a ProcessHandle>,
}

impl<'a> MemoryReader<'a> {
    pub fn new(process: &'a ProcessHandle) -> Self {
        Self {
            handle: process.raw(),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn read<T: Copy>(&self, address: u32) -> Result<T, MemoryError> {
        if address == 0 {
            return Err(MemoryError::InvalidAddress { address });
        }
        let mut buf = std::mem::MaybeUninit::<T>::uninit();
        let ok = unsafe {
            ReadProcessMemory(
                self.handle,
                address as *const _,
                buf.as_mut_ptr() as *mut _,
                size_of::<T>(),
                None,
            )
        }
        .is_ok();
        if !ok {
            return Err(MemoryError::ReadFailed { address });
        }
        Ok(unsafe { buf.assume_init() })
    }

    pub fn read_i32(&self, address: u32) -> Result<i32, MemoryError> {
        self.read(address)
    }

    pub fn read_u32(&self, address: u32) -> Result<u32, MemoryError> {
        self.read(address)
    }

    pub fn read_f32(&self, address: u32) -> Result<f32, MemoryError> {
        self.read(address)
    }
}

pub struct MemoryWriter<'a> {
    handle: windows::Win32::Foundation::HANDLE,
    _marker: std::marker::PhantomData<&'a ProcessHandle>,
}

impl<'a> MemoryWriter<'a> {
    pub fn new(process: &'a ProcessHandle) -> Self {
        Self {
            handle: process.raw(),
            _marker: std::marker::PhantomData,
        }
    }

    pub fn write<T: Copy>(&self, address: u32, value: T) -> Result<(), MemoryError> {
        if address == 0 {
            return Err(MemoryError::InvalidAddress { address });
        }
        let ok = unsafe {
            WriteProcessMemory(
                self.handle,
                address as *const _,
                &value as *const T as *const _,
                size_of::<T>(),
                None,
            )
        }
        .is_ok();
        if !ok {
            return Err(MemoryError::WriteFailed { address });
        }
        Ok(())
    }

    pub fn write_i32(&self, address: u32, value: i32) -> Result<(), MemoryError> {
        self.write(address, value)
    }

    pub fn write_f32(&self, address: u32, value: f32) -> Result<(), MemoryError> {
        self.write(address, value)
    }
}
