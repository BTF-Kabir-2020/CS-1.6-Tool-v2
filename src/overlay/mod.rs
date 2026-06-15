//! [EN] Overlay module — transparent Win32 overlay for displaying game status on screen.
//! [FA] ماژول overlay — پنجره شفاف Win32 برای نمایش وضعیت بازی روی صفحه.

#[allow(clippy::module_inception)]
mod overlay;

/// [EN] Re-export the overlay handle for public use by the rest of the crate.
/// [FA] خروجی مجدد overlay handle برای استفاده عمومی بقیه کrate.
pub use overlay::OverlayHandle;
