/// Cross-process memory read/write and pointer-chain resolution.
use std::mem::size_of;

use windows::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};

use crate::error::MemoryError;
use crate::win::process::ProcessHandle;

/// Resolve a multi-level pointer chain starting from `base_ptr`.
///
/// Each element in `offsets` is added to the value read at the current address
/// before following the next pointer. This is the standard pattern for navigating
/// CS 1.6's static → dynamic pointer structures.
///
/// زنجیره اشاره‌گر چندسطحی را از `base_ptr` حل می‌کند. هر عنصر در `offsets`
/// پس از خواندن مقدار فعلی به آدرس بعدی اضافه می‌شود.
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

    // Walk the chain: read pointer → add offset → repeat.
    // پیمایش زنجیره: خواندن اشاره‌گر → اضافه کردن آفست → تکرار.
    for (step, &offset) in offsets.iter().enumerate() {
        let next = reader
            .read_u32(addr)
            .map_err(|_| MemoryError::ChainBroken {
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

/// Safe(ish) wrapper for reading another process's memory.
///
/// Uses `ReadProcessMemory` under the hood. The lifetime `'a` ties the reader
/// to a [`ProcessHandle`] so it cannot outlive the handle it reads from.
///
/// این ساختار یک انتزاع امن دور خواندن حافظه پروسه دیگر است.
pub struct MemoryReader<'a> {
    /// Raw Win32 process handle used for reading.
    handle: windows::Win32::Foundation::HANDLE,
    /// Marker tying this reader to the lifetime of a `ProcessHandle`.
    _marker: std::marker::PhantomData<&'a ProcessHandle>,
}

impl<'a> MemoryReader<'a> {
    /// Create a new reader from a borrowed [`ProcessHandle`].
    pub fn new(process: &'a ProcessHandle) -> Self {
        Self {
            handle: process.raw(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Read `size_of::<T>()` bytes from `address` and interpret as type `T`.
    ///
    /// Returns [`MemoryError::InvalidAddress`] for null addresses and
    /// [`MemoryError::ReadFailed`] if the underlying `ReadProcessMemory` call fails.
    ///
    /// خواندن بایت‌هایی به اندازه `size_of::<T>()` از `address` و تفسیر آن به عنوان نوع `T`.
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
        // SAFETY: ReadProcessMemory succeeded and wrote size_of::<T>() bytes.
        Ok(unsafe { buf.assume_init() })
    }

    /// Convenience wrapper — read a 32-bit signed integer.
    pub fn read_i32(&self, address: u32) -> Result<i32, MemoryError> {
        self.read(address)
    }

    /// Convenience wrapper — read a 32-bit unsigned integer.
    pub fn read_u32(&self, address: u32) -> Result<u32, MemoryError> {
        self.read(address)
    }

    /// Convenience wrapper — read a 32-bit float.
    pub fn read_f32(&self, address: u32) -> Result<f32, MemoryError> {
        self.read(address)
    }
}

/// Safe(ish) wrapper for writing to another process's memory.
///
/// Uses `WriteProcessMemory` under the hood. The lifetime `'a` ties the writer
/// to a [`ProcessHandle`] so it cannot outlive the handle it writes to.
///
/// این ساختار یک انتزاع امن دور نوشتن حافظه پروسه دیگر است.
pub struct MemoryWriter<'a> {
    /// Raw Win32 process handle used for writing.
    handle: windows::Win32::Foundation::HANDLE,
    /// Marker tying this writer to the lifetime of a `ProcessHandle`.
    _marker: std::marker::PhantomData<&'a ProcessHandle>,
}

impl<'a> MemoryWriter<'a> {
    /// Create a new writer from a borrowed [`ProcessHandle`].
    pub fn new(process: &'a ProcessHandle) -> Self {
        Self {
            handle: process.raw(),
            _marker: std::marker::PhantomData,
        }
    }

    /// Write `size_of::<T>()` bytes of `value` to `address`.
    ///
    /// Returns [`MemoryError::InvalidAddress`] for null addresses and
    /// [`MemoryError::WriteFailed`] if the underlying `WriteProcessMemory` call fails.
    ///
    /// نوشتن بایت‌هایی به اندازه `size_of::<T>()` از `value` در `address`.
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

    /// Convenience wrapper — write a 32-bit signed integer.
    pub fn write_i32(&self, address: u32, value: i32) -> Result<(), MemoryError> {
        self.write(address, value)
    }

    /// Convenience wrapper — write a 32-bit float.
    pub fn write_f32(&self, address: u32, value: f32) -> Result<(), MemoryError> {
        self.write(address, value)
    }
}
