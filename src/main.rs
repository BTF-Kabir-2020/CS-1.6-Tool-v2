//! [EN] CS 1.6 Tool v2 — main entry point for the external memory tool.
//! [FA] ابزار حافظه خارجی Counter-Strike 1.6 نسخه 2 — نقطه ورود اصلی برنامه.

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use clap::Parser;
use parking_lot::RwLock;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

use cs16_tool_v2::config::AppConfig;
use cs16_tool_v2::debug::DebugConsole;
use cs16_tool_v2::game::{connect, GameEngine, GameState};
use cs16_tool_v2::overlay::OverlayHandle;
use cs16_tool_v2::win::{find_pid, invalidate_window_cache};

/// [EN] Command-line argument structure parsed by clap.
/// [FA] ساختار آرگومان‌های خط فرمان که توسط clap پارس می‌شود.
#[derive(Parser)]
#[command(name = "cs16-tool", about = "CS 1.6 external memory tool v2")]
struct Cli {
    /// [EN] Path to the configuration file (default: config.toml).
    /// [FA] مسیر فایل پیکربندی (پیش‌فرض: config.toml).
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,
    /// [EN] Enable read-only mode (disables memory writes).
    /// [FA] فعال کردن حالت فقط-خواندن (غیرفعال کردن نوشتن حافظه).
    #[arg(long)]
    read_only: bool,
    /// [EN] Disable the overlay display.
    /// [FA] غیرفعال کردن نمایش overlay.
    #[arg(long)]
    no_overlay: bool,
    /// [EN] Enable debug console output.
    /// [FA] فعال کردن خروجی کنسول دیباگ.
    #[arg(long)]
    debug: bool,
}

/// [EN] Program entry point — prints disclaimer, initializes logging, and runs the main loop.
/// [FA] نقطه ورود برنامه — چاپ سلب مسئولیت، راه‌اندازی لاگینگ، و اجرای حلقه اصلی.
fn main() {
    println!("╔══════════════════════════════════════════════════════════╗");
    println!("║  DISCLAIMER: Educational use only. No warranty.         ║");
    println!("║  Author is NOT responsible for any damage or misuse.    ║");
    println!("║  Commercial use is strictly prohibited.                 ║");
    println!("║  See LICENSE for full terms.                            ║");
    println!("╚══════════════════════════════════════════════════════════╝");
    println!();
    if let Err(e) = run() {
        error!("{e}");
        eprintln!("\nخطا: {e}");
        pause();
    }
}

/// [EN] Main run function — sets up tracing, parses CLI args, spawns threads, and enters the hotkey loop.
/// [FA] تابع اجرای اصلی — راه‌اندازی tracing، پارس آرگومان‌های CLI، ایجاد thread‌ها، و ورود به حلقه کلیدهای میانبر.
fn run() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing subscriber with environment filter
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("cs16_tool_v2=info".parse()?))
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let mut config = AppConfig::load(&cli.config)?;

    // Apply CLI overrides to configuration
    if cli.read_only {
        config.features.write_enabled = false;
    }
    if cli.no_overlay {
        config.features.overlay_enabled = false;
    }
    if cli.debug {
        config.features.debug_addresses = true;
        config.debug_console.enabled = true;
    }

    banner(&config);

    // Shared state for inter-thread communication
    let running = Arc::new(AtomicBool::new(true));
    let state = Arc::new(RwLock::new(GameState::default()));
    let pid_store = Arc::new(AtomicU32::new(0));
    let debug_on = Arc::new(AtomicBool::new(config.debug_console.enabled));

    // Spawn overlay thread if enabled
    let overlay = if config.features.overlay_enabled {
        Some(OverlayHandle::spawn(
            Arc::clone(&pid_store),
            config.process.name.clone(),
            config.overlay.clone(),
            Arc::clone(&state),
        ))
    } else {
        None
    };

    // Spawn the memory reading/writing loop on a dedicated thread
    let r_run = Arc::clone(&running);
    let r_cfg = config.clone();
    let r_state = Arc::clone(&state);
    let r_pid = Arc::clone(&pid_store);
    let r_dbg = Arc::clone(&debug_on);
    let mem = thread::spawn(move || memory_loop(r_run, r_cfg, r_state, r_pid, r_dbg));

    println!("کلیدها: Q/End=خروج | Insert=Overlay | F7=Debug");
    println!();

    // Main hotkey polling loop
    while running.load(Ordering::SeqCst) {
        // Q (0x51) or End (0x23) → exit
        if unsafe { key(0x51) } || unsafe { key(0x23) } {
            break;
        }
        // Insert (0x2D) → toggle overlay visibility
        if let Some(ref ov) = overlay {
            if unsafe { key(0x2D) } {
                ov.toggle();
                thread::sleep(Duration::from_millis(200));
            }
        }
        // F7 (0x76) → toggle debug console
        if unsafe { key(0x76) } {
            let v = !debug_on.load(Ordering::SeqCst);
            debug_on.store(v, Ordering::SeqCst);
            info!("Debug console: {}", if v { "ON" } else { "OFF" });
            thread::sleep(Duration::from_millis(200));
        }

        // Print status line to terminal (when debug is off)
        if !debug_on.load(Ordering::SeqCst) {
            let line = state
                .read()
                .format_status(&cs16_tool_v2::game::StatusDisplay::all());
            print!("\r{line}   ");
            let _ = std::io::stdout().flush();
        }
        thread::sleep(Duration::from_millis(50));
    }

    // Signal threads to stop and wait for cleanup
    running.store(false, Ordering::SeqCst);
    let _ = mem.join();
    drop(overlay);
    println!("\n\nخروج.");
    Ok(())
}

/// [EN] Background memory loop — connects to the game process, reads/writes memory each tick.
/// [FA] حلقه حافظه پس‌زمینه — اتصال به پروسس بازی، خواندن/نوشتن حافظه در هر tick.
fn memory_loop(
    running: Arc<AtomicBool>,
    config: AppConfig,
    state: Arc<RwLock<GameState>>,
    pid_store: Arc<AtomicU32>,
    debug_on: Arc<AtomicBool>,
) {
    let mut engine: Option<GameEngine> = None;
    let mut dbg = DebugConsole::new(config.debug_console.clone());
    let mut fails = 0u32;

    while running.load(Ordering::SeqCst) {
        // Attempt connection if not yet connected
        if engine.is_none() {
            if let Some(p) = find_pid(&config.process.name) {
                pid_store.store(p, Ordering::SeqCst);
            }
            match connect(&config) {
                Ok(eng) => {
                    fails = 0;
                    let p = eng.pid();
                    pid_store.store(p, Ordering::SeqCst);
                    info!("متصل به {} (PID={p})", config.process.name);

                    // Log resolved memory addresses when debug mode is on
                    if config.features.debug_addresses {
                        let r = eng.resolved();
                        info!("hwBase          = {:#x}", r.hw_base);
                        info!("hwAmmoAddr      = {:#x}", r.reserve_addr);
                        info!("hwMoneyAddr     = {:#x}", r.money_addr);
                        info!("clientMoneyAddr = {:#x}", eng.client_money_addr());
                        info!("clipAddr        = {:#x}", r.clip_addr);
                    }
                    engine = Some(eng);
                }
                Err(e) => {
                    fails = fails.saturating_add(1);
                    // Log connection failure on first attempt and every 20th retry
                    if fails == 1 || fails % 20 == 0 {
                        warn!("اتصال... ({e})");
                    }
                    thread::sleep(Duration::from_millis(config.timing.connect_retry_ms));
                    continue;
                }
            }
        }

        // Tick the engine and update shared state
        if let Some(ref mut eng) = engine {
            // Check if the game process has restarted or closed
            if eng.should_reconnect() {
                warn!("reconnect...");
                pid_store.store(0, Ordering::SeqCst);
                invalidate_window_cache();
                *state.write() = GameState::default();
                engine = None;
                thread::sleep(Duration::from_millis(config.timing.connect_retry_ms));
                continue;
            }

            let snap = eng.tick();
            *state.write() = snap.clone();

            // Print debug snapshot if debug console is active
            if debug_on.load(Ordering::SeqCst) {
                dbg.config.enabled = true;
                dbg.maybe_print(&eng.debug_snapshot(&snap));
            }
        }

        thread::sleep(Duration::from_millis(config.timing.memory_loop_ms));
    }
}

/// [EN] Checks if a virtual key is currently pressed using Win32 GetAsyncKeyState.
/// [FA] بررسی می‌کند آیا یک کلید مجازی در حال حاضر فشرده شده است با استفاده از GetAsyncKeyState ویندوز.
unsafe fn key(vk: i32) -> bool {
    (GetAsyncKeyState(vk) as u16) & 0x8000 != 0
}

/// [EN] Prints the application banner with current feature status.
/// [FA] چاپ بنر برنامه با وضعیت فعلی قابلیت‌ها.
fn banner(cfg: &AppConfig) {
    println!("╔══════════════════════════════════════════╗");
    println!("║     CS 1.6 Tool v2 — Rust Rewrite        ║");
    println!("╠══════════════════════════════════════════╣");
    println!(
        "║  Write: {}  Overlay: {}  Entity: {}    ║",
        on(cfg.features.write_enabled),
        on(cfg.features.overlay_enabled),
        on(cfg.entity.enabled),
    );
    println!(
        "║  💰{}  🔫{}/{}  ❤read  🛡read           ║",
        cfg.targets.money, cfg.targets.clip, cfg.targets.reserve,
    );
    println!("╚══════════════════════════════════════════╝");
}

/// [EN] Converts a boolean to a display string: "ON " for true, "OFF" for false.
/// [FA] تبدیل یک مقدار بولی به رشته نمایشی: "ON " برای درست، "OFF" برای نادرست.
fn on(v: bool) -> &'static str {
    if v {
        "ON "
    } else {
        "OFF"
    }
}

/// [EN] Pauses execution and waits for user to press Enter.
/// [FA] اجرای برنامه را متوقف کرده و منتظر فشردن Enter توسط کاربر می‌ماند.
fn pause() {
    println!("Enter...");
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s);
}
