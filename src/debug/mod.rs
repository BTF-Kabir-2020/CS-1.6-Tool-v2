/// [EN] Debug console module — renders a live debug panel to stdout with ANSI escape codes.
/// [FA] ماژول کنسول دیباگ — پنل دیباگ زنده با کدهای فرار ANSI در stdout رندر می‌شود.
use std::io::{self, Write};

use crate::config::DebugConsoleConfig;
use crate::game::DebugSnapshot;

/// [EN] Debug console that periodically prints game state snapshots to stdout.
/// [FA] کنسول دیباگ که به‌صورت دوره‌ای اسنپ‌شات وضعیت بازی را در stdout چاپ می‌کند.
pub struct DebugConsole {
    /// [EN] Configuration controlling display settings and update interval.
    /// [FA] تنظیمات کنترل نمایش و فاصله به‌روزرسانی.
    pub config: DebugConsoleConfig,
    /// [EN] Monotonically increasing tick counter for each render cycle.
    /// [FA] شمارنده تیک صعودی برای هر چرخه رندر.
    tick: u64,
    /// [EN] Timestamp of the last render to enforce the interval throttle.
    /// [FA] زمان‌سنج آخرین رندر برای اجرای محدوده فاصله.
    last: std::time::Instant,
}

impl DebugConsole {
    /// [EN] Create a new debug console with the given configuration.
    /// The initial last-render time is set 10 seconds in the past so the first render fires immediately.
    /// [FA] یک کنسول دیباگ جدید با تنظیمات داده‌شده می‌سازد.
    /// زمان آخرین رندر اولیه ۱۰ ثانیه در گذشته تنظیم می‌شود تا اولین رندر فوراً اجرا شود.
    pub fn new(config: DebugConsoleConfig) -> Self {
        Self {
            config,
            tick: 0,
            last: std::time::Instant::now()
                .checked_sub(std::time::Duration::from_secs(10))
                .unwrap_or_else(std::time::Instant::now),
        }
    }

    /// [EN] Conditionally render the debug snapshot if enough time has elapsed since the last render.
    /// [FA] در صورت گذشت زمان کافی از آخرین رندر، اسنپ‌شات دیباگ را به‌صورت شرطی رندر می‌کند.
    pub fn maybe_print(&mut self, snap: &DebugSnapshot) {
        if !self.config.enabled {
            return;
        }
        let interval = std::time::Duration::from_millis(self.config.interval_ms);
        if self.last.elapsed() < interval {
            return;
        }
        self.last = std::time::Instant::now();
        self.tick += 1;
        self.render(snap);
    }

    /// [EN] Render the debug panel to stdout: header, game state, and key bindings.
    /// Uses ANSI escape sequences for screen clearing and Unicode box-drawing characters.
    /// [FA] پنل دیباگ را در stdout رندر می‌کند: سرصفحه، وضعیت بازی و کلیدهای میانبر.
    /// از توالی‌های فرار ANSI برای پاک کردن صفحه و کاراکترهای جعبه‌کشی یونیکد استفاده می‌کند.
    fn render(&self, snap: &DebugSnapshot) {
        let mut out = String::new();
        if self.config.clear_screen {
            out.push_str("\x1b[2J\x1b[H");
        }
        out.push_str("╔══════════════════════════════════════════════════╗\n");
        out.push_str(&format!(
            "║  CS16 Tool v2 Debug  tick #{:<5}  ready: {:<5}     ║\n",
            self.tick,
            if snap.ready { "YES" } else { "NO" }
        ));
        out.push_str("╚══════════════════════════════════════════════════╝\n\n");

        if !snap.connected {
            out.push_str("[!] منتظر hl.exe — وارد match شو\n");
        } else if !snap.ready {
            out.push_str("[!] در حال resolve آدرس‌ها...\n");
        } else {
            out.push_str(&format!(
                "💰 {}  🔫 {}/{}  write={}  alive={}\n",
                opt(snap.money_valid, snap.money),
                opt(snap.clip_valid, snap.clip),
                opt(snap.reserve_valid, snap.reserve),
                yn(snap.write_enabled),
                yn(snap.player_alive),
            ));
            if !snap.money_source.is_empty() {
                out.push_str(&format!("money source: {}\n", snap.money_source));
            }
        }

        out.push_str("\n[F7] debug | [Insert] overlay | [Q] quit\n");
        print!("{out}");
        let _ = io::stdout().flush();
    }
}

/// [EN] Format an optional integer: show the value if valid, "--" otherwise.
/// [FA] فرمت یک عدد اختیاری: اگر معتبر باشد مقدار را نشان می‌دهد، وگرنه "--".
fn opt(valid: bool, v: i32) -> String {
    if valid {
        v.to_string()
    } else {
        "--".into()
    }
}

/// [EN] Convert a boolean to "YES" or "NO" string.
/// [FA] تبدیل یک بولین به رشته "YES" یا "NO".
fn yn(v: bool) -> &'static str {
    if v {
        "YES"
    } else {
        "NO"
    }
}
