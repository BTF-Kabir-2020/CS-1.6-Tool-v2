use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("پیکربندی: {0}")]
    Config(String),

    #[error("حافظه: {0}")]
    Memory(#[from] MemoryError),

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("پروسس '{name}' پیدا نشد")]
    ProcessNotFound { name: String },

    #[error("OpenProcess برای PID={pid} ناموفق — Run as Administrator")]
    OpenProcessFailed { pid: u32 },

    #[error("ماژول '{name}' لود نشده — وارد match شو")]
    ModuleNotFound { name: String },

    #[error("خواندن در {address:#x} ناموفق")]
    ReadFailed { address: u32 },

    #[error("نوشتن در {address:#x} ناموفق")]
    WriteFailed { address: u32 },

    #[error("chain در گام {step} شکست (آدرس {address:#x})")]
    ChainBroken { step: usize, address: u32 },

    #[error("آدرس نامعتبر: {address:#x}")]
    InvalidAddress { address: u32 },
}
