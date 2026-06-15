/// Process enumeration and handle management via Win32 ToolHelp snapshots.
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, Process32FirstW, Process32NextW,
    MODULEENTRY32W, PROCESSENTRY32W, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::{
    GetExitCodeProcess, OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_OPERATION,
    PROCESS_VM_READ, PROCESS_VM_WRITE,
};

use crate::config::ModulesConfig;
use crate::error::MemoryError;

/// Exit code constant indicating a process is still running.
const STILL_ACTIVE: u32 = 259;

/// RAII wrapper around a Win32 process handle.
///
/// Holds the `HANDLE` and its associated PID. Provides methods for
/// querying module information and automatically closes the handle on drop.
///
/// این ساختار یک بسته RAII دور هندل پروسه ویندوز است و هنگام drop خودکار بسته می‌شود.
pub struct ProcessHandle {
    /// Raw Win32 process handle.
    handle: HANDLE,
    /// Process ID associated with this handle.
    pid: u32,
}

// SAFETY: HANDLE is a raw pointer-like value; the caller must ensure the process
// is not accessed from multiple threads without synchronisation where required.
// ایمنی: هندل خام است؛ فراخوان باید اطمینان دهد همگام‌سازی لازم رعایت شده.
unsafe impl Send for ProcessHandle {}
unsafe impl Sync for ProcessHandle {}

impl ProcessHandle {
    /// Attach to a running process by name.
    ///
    /// Opens the process with VM read/write/query permissions.
    /// Returns [`MemoryError::ProcessNotFound`] if the process cannot be located,
    /// or [`MemoryError::OpenProcessFailed`] if the handle cannot be obtained.
    ///
    /// اتصال به پروسه در حال اجرایی بر اساس نام. در صورت عدم یافتن یا عدم باز شدن خطا برمی‌گرداند.
    pub fn attach(name: &str) -> Result<Self, MemoryError> {
        let pid = find_pid(name).ok_or_else(|| MemoryError::ProcessNotFound {
            name: name.to_string(),
        })?;

        let handle = unsafe {
            OpenProcess(
                PROCESS_VM_READ
                    | PROCESS_VM_WRITE
                    | PROCESS_VM_OPERATION
                    | PROCESS_QUERY_INFORMATION,
                false,
                pid,
            )
        }
        .map_err(|_| MemoryError::OpenProcessFailed { pid })?;

        if handle.is_invalid() {
            return Err(MemoryError::OpenProcessFailed { pid });
        }

        Ok(Self { handle, pid })
    }

    /// Return the process ID (PID).
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Return the raw Win32 `HANDLE` for low-level API calls.
    pub fn raw(&self) -> HANDLE {
        self.handle
    }

    /// Return the base address of the named module loaded in this process.
    ///
    /// Searches the process's module list for `module` and returns its load address.
    /// Returns [`MemoryError::ModuleNotFound`] if the module is not present.
    ///
    /// آدرس پایه ماژول مشخص‌شده در این پروسه را برمی‌گرداند.
    pub fn module_base(&self, module: &str) -> Result<u32, MemoryError> {
        module_info(self.pid, module)
            .map(|(base, _)| base)
            .ok_or_else(|| MemoryError::ModuleNotFound {
                name: module.to_string(),
            })
    }

    /// Return the size (in bytes) of the named module loaded in this process.
    ///
    /// حجم (به بایت) ماژول مشخص‌شده در این پروسه را برمی‌گرداند.
    pub fn module_size(&self, module: &str) -> Result<u32, MemoryError> {
        module_info(self.pid, module)
            .map(|(_, size)| size)
            .ok_or_else(|| MemoryError::ModuleNotFound {
                name: module.to_string(),
            })
    }
}

impl Drop for ProcessHandle {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            let _ = unsafe { CloseHandle(self.handle) };
        }
    }
}

/// Find the PID of a process by executable name (case-insensitive).
///
/// Takes a snapshot of all running processes and iterates until a match is found.
/// Returns `None` if no matching process exists.
///
/// پیدا کردن شناسه پروسه بر اساس نام اجرایی (بدون حساسیت به حروف).
pub fn find_pid(name: &str) -> Option<u32> {
    let wide = to_wide(name);
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.ok()?;
    if snap.is_invalid() {
        return None;
    }
    // RAII guard ensures the snapshot handle is closed on all exit paths.
    // نگهبان RAII اطمینان می‌دهد هندل اسنپ‌شات در همه مسیرهای خروج بسته شود.
    let _guard = HandleGuard(snap);

    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };

    if unsafe { Process32FirstW(snap, &mut entry) }.is_err() {
        return None;
    }

    // Walk the process list until we find a matching executable name or exhaust the list.
    // پیمایش لیست پروسه‌ها تا یافتن نام اجرایی مطابق یا اتمام لیست.
    loop {
        if wide_eq(&entry.szExeFile, &wide) {
            return Some(entry.th32ProcessID);
        }
        if unsafe { Process32NextW(snap, &mut entry) }.is_err() {
            break;
        }
    }
    None
}

/// Check whether a process with the given PID is still alive.
///
/// Opens the process, retrieves its exit code, and checks for `STILL_ACTIVE`.
/// Returns `false` for PID 0 or if the process cannot be opened.
///
/// بررسی اینکه آیا پروسه با شناسه داده‌شده هنوز زنده است یا خیر.
pub fn is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) };
    let Ok(handle) = handle else {
        return false;
    };
    if handle.is_invalid() {
        return false;
    }
    let mut code = 0u32;
    let ok = unsafe { GetExitCodeProcess(handle, &mut code) }.is_ok();
    let _ = unsafe { CloseHandle(handle) };
    ok && code == STILL_ACTIVE
}

/// Resolve the engine (hw.dll / sw.dll) base address for the given process.
///
/// First tries the Half-Life (`hw`) module; if that fails and a Steam (`sw`) module
/// name is configured, falls back to it. Returns [`MemoryError::ModuleNotFound`]
/// if neither module is present.
///
/// آدرس پایه موتور بازی (hw.dll / sw.dll) را برای پروسه مشخص‌شده حل می‌کند.
pub fn engine_base(process: &ProcessHandle, modules: &ModulesConfig) -> Result<u32, MemoryError> {
    if let Ok(base) = process.module_base(&modules.hw) {
        return Ok(base);
    }
    if !modules.sw.is_empty() {
        if let Ok(base) = process.module_base(&modules.sw) {
            tracing::info!("{} نیست — از {} استفاده می‌شود", modules.hw, modules.sw);
            return Ok(base);
        }
    }
    Err(MemoryError::ModuleNotFound {
        name: format!("{} / {}", modules.hw, modules.sw),
    })
}

/// Internal helper — retrieve `(base_address, size)` for a loaded module by name.
///
/// Uses a ToolHelp module snapshot; returns `None` if the module is not found.
fn module_info(pid: u32, name: &str) -> Option<(u32, u32)> {
    let wide = to_wide(name);
    let flags = TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32;
    let snap = unsafe { CreateToolhelp32Snapshot(flags, pid) }.ok()?;
    if snap.is_invalid() {
        return None;
    }
    let _guard = HandleGuard(snap);

    let mut entry = MODULEENTRY32W {
        dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32,
        ..Default::default()
    };

    if unsafe { Module32FirstW(snap, &mut entry) }.is_err() {
        return None;
    }

    // Linear scan through loaded modules.
    loop {
        if wide_eq(&entry.szModule, &wide) {
            return Some((entry.modBaseAddr as u32, entry.modBaseSize));
        }
        if unsafe { Module32NextW(snap, &mut entry) }.is_err() {
            break;
        }
    }
    None
}

/// Convert a Rust `&str` to a null-terminated UTF-16 vector (Windows `LPCWSTR` format).
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Compare two null-terminated wide-character buffers case-insensitively.
///
/// Splits at the first null and compares element-wise using ASCII lowercasing.
fn wide_eq(buf: &[u16], target: &[u16]) -> bool {
    let a = buf.split(|&c| c == 0).next().unwrap_or(&[]);
    let b = target.split(|&c| c == 0).next().unwrap_or(&[]);
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .all(|(x, y)| x.to_ascii_lowercase() == y.to_ascii_lowercase())
}

/// Extension trait for ASCII-only lowercase conversion on `u16` values.
trait AsciiLower {
    fn to_ascii_lowercase(self) -> u16;
}
impl AsciiLower for u16 {
    fn to_ascii_lowercase(self) -> u16 {
        // ASCII uppercase range: A (65) – Z (90)
        if (65..=90).contains(&self) {
            self + 32
        } else {
            self
        }
    }
}

/// RAII guard that closes a Win32 `HANDLE` on drop.
///
/// Prevents handle leaks when early-returning from functions that open snapshots.
struct HandleGuard(HANDLE);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}
