/// [EN] Centralized error types for the application — uses `thiserror` for derive-based Display.
/// [FA] انواع خطای متمرکز برنامه — از `thiserror` برای Display مبتنی بر derive استفاده می‌کند.
use thiserror::Error;

/// [EN] Top-level application error — wraps config, memory, and generic errors.
/// [FA] خطای سطح بالای برنامه — خطا‌های پیکربندی، حافظه و عمومی را در بر می‌گیرد.
#[derive(Debug, Error)]
pub enum AppError {
    /// [EN] Configuration parsing or validation error.
    /// [FA] خطای پارس یا اعتبارسنجی پیکربندی.
    #[error("پیکربندی: {0}")]
    Config(String),

    /// [EN] Memory access error — auto-converted from `MemoryError` via `#[from]`.
    /// [FA] خطای دسترسی حافظه — به‌صورت خودکار از `MemoryError` از طریق `#[from]` تبدیل می‌شود.
    #[error("حافظه: {0}")]
    Memory(#[from] MemoryError),

    /// [EN] Catch-all for any other error type — wrapped via `anyhow`.
    /// [FA] جایگزین برای هر نوع خطای دیگر — از طریق `anyhow` در بر گرفته شده.
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

/// [EN] Memory-specific errors — covers process access, module loading, read/write, and chain resolution.
/// [FA] خطاهای مرتبط با حافظه — دسترسی پروسس، بارگذاری ماژول، خواندن/نوشتن و حل زنجیره.
#[derive(Debug, Error)]
pub enum MemoryError {
    /// [EN] Target process not found by name (e.g., "hl.exe" not running).
    /// [FA] پروسس هدف با نام یافت نشد (مثلاً "hl.exe" در حال اجرا نیست).
    #[error("پروسس '{name}' پیدا نشد")]
    ProcessNotFound { name: String },

    /// [EN] OpenProcess API call failed — usually means "Run as Administrator" is required.
    /// [FA] فراخوانی OpenProcess ناموفق بود — معمولاً به معنای نیاز به "Run as Administrator" است.
    #[error("OpenProcess برای PID={pid} ناموفق — Run as Administrator")]
    OpenProcessFailed { pid: u32 },

    /// [EN] DLL module not loaded in the target process (e.g., hw.dll not found).
    /// [FA] ماژول DLL در پروسس هدف بارگذاری نشده (مثلاً hw.dll یافت نشد).
    #[error("ماژول '{name}' لود نشده — وارد match شو")]
    ModuleNotFound { name: String },

    /// [EN] ReadProcessMemory failed at the given address.
    /// [FA] ReadProcessMemory در آدرس داده شده ناموفق بود.
    #[error("خواندن در {address:#x} ناموفق")]
    ReadFailed { address: u32 },

    /// [EN] WriteProcessMemory failed at the given address.
    /// [FA] WriteProcessMemory در آدرس داده شده ناموفق بود.
    #[error("نوشتن در {address:#x} ناموفق")]
    WriteFailed { address: u32 },

    /// [EN] Pointer chain resolution failed at the given step — null pointer or invalid dereference.
    /// [FA] حل زنجیره اشاره‌گر در گام داده شده ناموفق بود — اشاره‌گر null یا dereference نامعتبر.
    #[error("chain در گام {step} شکست (آدرس {address:#x})")]
    ChainBroken { step: usize, address: u32 },

    /// [EN] Null or invalid memory address was passed to a read/write operation.
    /// [FA] آدرس حافظه null یا نامعتبر به عملیات خواندن/نوشتن ارسال شد.
    #[error("آدرس نامعتبر: {address:#x}")]
    InvalidAddress { address: u32 },
}
