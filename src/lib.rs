//! [EN] CS 1.6 Tool v2 — complete rewrite in Rust.
//! [FA] ابزار Counter-Strike 1.6 نسخه 2 — بازنویسی کامل با Rust.
//!
//! ## [EN] Differences from v1
//! - **Local Player**: reads money/hp/armor from game entity (known offsets)
//! - **Re-resolve every tick**: clip/reserve/money chains are re-resolved each frame
//! - **Fallback priority**: entity → hw chain → client direct
//! - Modular and testable architecture
//!
//! ## [FA] تفاوت با v1
//! - **Local Player**: خواندن money/hp/armor از entity بازیکن (offsetهای شناخته‌شده)
//! - **Re-resolve هر tick**: chainهای clip/reserve/money هر فریم دوباره resolve می‌شوند
//! - **اولویت fallback**: entity → hw chain → client direct
//! - معماری ماژولار و تست‌پذیر

/// [EN] Configuration module — TOML-based config parsing, validation, and defaults.
/// [FA] ماژول پیکربندی — پارس، اعتبارسنجی و پیش‌فرض‌های پیکربندی مبتنی بر TOML.
pub mod config;

/// [EN] Debug console module — periodic memory address and state debugging output.
/// [FA] ماژول کنسول دیباگ — خروجی دوره‌ای دیباگ آدرس‌های حافظه و وضعیت.
pub mod debug;

/// [EN] Error types module — centralized error handling with `thiserror`.
/// [FA] ماژول انواع خطا — مدیریت خطای متمرکز با `thiserror`.
pub mod error;

/// [EN] Game engine module — memory reading, state tracking, and game interaction logic.
/// [FA] ماژول موتور بازی — خواندن حافظه، ردیابی وضعیت، و منطق تعامل با بازی.
pub mod game;

/// [EN] Overlay module — transparent window overlay for displaying game stats.
/// [FA] ماژول overlay — پنجره شفاف برای نمایش آمار بازی.
pub mod overlay;

/// [EN] Windows API wrapper module — process enumeration, module finding, and memory R/W.
/// [FA] ماژول پوشش API ویندوز — پیمایش پروسس، یافتن ماژول، و خواندن/نوشتن حافظه.
pub mod win;

/// [EN] Re-export of `AppConfig` for convenient access.
/// [FA] بازصادرات `AppConfig` برای دسترسی راحت‌تر.
pub use config::AppConfig;

/// [EN] Re-export of `AppError` for convenient access.
/// [FA] بازصادرات `AppError` برای دسترسی راحت‌تر.
pub use error::AppError;
