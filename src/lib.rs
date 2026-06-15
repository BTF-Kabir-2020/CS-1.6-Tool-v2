//! CS 1.6 Tool v2 — بازنویسی کامل با Rust.
//!
//! ## تفاوت با v1
//! - **Local Player**: خواندن money/hp/armor از entity بازیکن (offsetهای شناخته‌شده)
//! - **Re-resolve هر tick**: chainهای clip/reserve/money هر فریم دوباره resolve می‌شوند
//! - **اولویت fallback**: entity → hw chain → client direct
//! - معماری ماژولار و تست‌پذیر

pub mod config;
pub mod debug;
pub mod error;
pub mod game;
pub mod overlay;
pub mod win;

pub use config::AppConfig;
pub use error::AppError;
