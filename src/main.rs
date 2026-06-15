//! CS 1.6 Tool v2 — entry point.

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

#[derive(Parser)]
#[command(name = "cs16-tool", about = "CS 1.6 external memory tool v2")]
struct Cli {
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,
    #[arg(long)]
    read_only: bool,
    #[arg(long)]
    no_overlay: bool,
    #[arg(long)]
    debug: bool,
}

fn main() {
    if let Err(e) = run() {
        error!("{e}");
        eprintln!("\nخطا: {e}");
        pause();
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("cs16_tool_v2=info".parse()?))
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let mut config = AppConfig::load(&cli.config)?;

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

    let running = Arc::new(AtomicBool::new(true));
    let state = Arc::new(RwLock::new(GameState::default()));
    let pid_store = Arc::new(AtomicU32::new(0));
    let debug_on = Arc::new(AtomicBool::new(config.debug_console.enabled));

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

    let r_run = Arc::clone(&running);
    let r_cfg = config.clone();
    let r_state = Arc::clone(&state);
    let r_pid = Arc::clone(&pid_store);
    let r_dbg = Arc::clone(&debug_on);
    let mem = thread::spawn(move || memory_loop(r_run, r_cfg, r_state, r_pid, r_dbg));

    println!("کلیدها: Q/End=خروج | Insert=Overlay | F7=Debug");
    println!();

    while running.load(Ordering::SeqCst) {
        if key(0x51) || key(0x23) {
            break;
        }
        if let Some(ref ov) = overlay {
            if key(0x2D) {
                ov.toggle();
                thread::sleep(Duration::from_millis(200));
            }
        }
        if key(0x76) {
            let v = !debug_on.load(Ordering::SeqCst);
            debug_on.store(v, Ordering::SeqCst);
            info!("Debug console: {}", if v { "ON" } else { "OFF" });
            thread::sleep(Duration::from_millis(200));
        }

        if !debug_on.load(Ordering::SeqCst) {
            let line = state.read().format_status(&cs16_tool_v2::game::StatusDisplay::all());
            print!("\r{line}   ");
            let _ = std::io::stdout().flush();
        }
        thread::sleep(Duration::from_millis(50));
    }

    running.store(false, Ordering::SeqCst);
    let _ = mem.join();
    drop(overlay);
    println!("\n\nخروج.");
    Ok(())
}

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
                    if fails == 1 || fails % 20 == 0 {
                        warn!("اتصال... ({e})");
                    }
                    thread::sleep(Duration::from_millis(config.timing.connect_retry_ms));
                    continue;
                }
            }
        }

        if let Some(ref mut eng) = engine {
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

            if debug_on.load(Ordering::SeqCst) {
                dbg.config.enabled = true;
                dbg.maybe_print(&eng.debug_snapshot(&snap));
            }
        }

        thread::sleep(Duration::from_millis(config.timing.memory_loop_ms));
    }
}

fn key(vk: i32) -> bool {
    unsafe { (GetAsyncKeyState(vk) as u16) & 0x8000 != 0 }
}

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
        cfg.targets.money,
        cfg.targets.clip,
        cfg.targets.reserve,
    );
    println!("╚══════════════════════════════════════════╝");
}

fn on(v: bool) -> &'static str {
    if v { "ON " } else { "OFF" }
}

fn pause() {
    println!("Enter...");
    let mut s = String::new();
    let _ = std::io::stdin().read_line(&mut s);
}
