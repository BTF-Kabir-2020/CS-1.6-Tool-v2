use std::io::{self, Write};

use crate::config::DebugConsoleConfig;
use crate::game::DebugSnapshot;

pub struct DebugConsole {
    pub config: DebugConsoleConfig,
    tick: u64,
    last: std::time::Instant,
}

impl DebugConsole {
    pub fn new(config: DebugConsoleConfig) -> Self {
        Self {
            config,
            tick: 0,
            last: std::time::Instant::now()
                .checked_sub(std::time::Duration::from_secs(10))
                .unwrap_or_else(std::time::Instant::now),
        }
    }

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

fn opt(valid: bool, v: i32) -> String {
    if valid {
        v.to_string()
    } else {
        "--".into()
    }
}

fn yn(v: bool) -> &'static str {
    if v { "YES" } else { "NO" }
}
