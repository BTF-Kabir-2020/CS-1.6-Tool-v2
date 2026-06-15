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

const STILL_ACTIVE: u32 = 259;

pub struct ProcessHandle {
    handle: HANDLE,
    pid: u32,
}

unsafe impl Send for ProcessHandle {}
unsafe impl Sync for ProcessHandle {}

impl ProcessHandle {
    pub fn attach(name: &str) -> Result<Self, MemoryError> {
        let pid = find_pid(name).ok_or_else(|| MemoryError::ProcessNotFound {
            name: name.to_string(),
        })?;

        let handle = unsafe {
            OpenProcess(
                PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_VM_OPERATION | PROCESS_QUERY_INFORMATION,
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

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn raw(&self) -> HANDLE {
        self.handle
    }

    pub fn module_base(&self, module: &str) -> Result<u32, MemoryError> {
        module_info(self.pid, module)
            .map(|(base, _)| base)
            .ok_or_else(|| MemoryError::ModuleNotFound {
                name: module.to_string(),
            })
    }

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

pub fn find_pid(name: &str) -> Option<u32> {
    let wide = to_wide(name);
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.ok()?;
    if snap.is_invalid() {
        return None;
    }
    let _guard = HandleGuard(snap);

    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };

    if unsafe { Process32FirstW(snap, &mut entry) }.is_err() {
        return None;
    }

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

pub fn is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let handle = unsafe {
        OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)
    };
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

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

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

trait AsciiLower {
    fn to_ascii_lowercase(self) -> u16;
}
impl AsciiLower for u16 {
    fn to_ascii_lowercase(self) -> u16 {
        if (65..=90).contains(&self) {
            self + 32
        } else {
            self
        }
    }
}

struct HandleGuard(HANDLE);
impl Drop for HandleGuard {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }
}
