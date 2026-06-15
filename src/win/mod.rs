/// Windows platform abstraction layer — لایه انتزاع پلتفرم ویندوز
///
/// Provides safe(ish) wrappers around Win32 APIs for process inspection,
/// memory read/write, and game-window discovery in a CS 1.6 context.
///
/// ارائه‌بندی‌های امن(تر) دور API‌های ویندوز برای بازرسی پروسه،
/// خواندن/نوشتن حافظه و یافتن پنجره بازی در بافت CS 1.6.
pub mod memory;
pub mod process;
pub mod window;

/// Re-exports for convenient single-path imports.
pub use memory::{resolve_chain, MemoryReader, MemoryWriter};
pub use process::{engine_base, find_pid, is_alive, ProcessHandle};
pub use window::{find_game_window, get_game_rect, invalidate_window_cache, GameRect};
