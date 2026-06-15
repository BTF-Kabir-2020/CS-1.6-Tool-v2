/// [EN] Transparent Win32 overlay using LWA_COLORKEY magenta for rendering game status HUD.
/// [FA] پنجره شفاف Win32 با LWA_COLORKEY برای رندر HUD وضعیت بازی.
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use crossbeam_channel::{Receiver, Sender};
use parking_lot::RwLock;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint, FillRect,
    GetTextExtentPoint32W, InvalidateRect, SelectObject, SetBkMode, SetTextColor, TextOutW, HFONT,
    HGDIOBJ, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, LoadCursorW, PostQuitMessage,
    RegisterClassW, SetLayeredWindowAttributes, SetWindowPos, ShowWindow, TranslateMessage,
    CS_HREDRAW, CS_VREDRAW, HWND_TOPMOST, IDC_ARROW, LWA_COLORKEY, MSG, SWP_NOACTIVATE, SWP_NOMOVE,
    SWP_NOSIZE, SWP_SHOWWINDOW, SW_HIDE, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_DESTROY,
    WM_PAINT, WM_QUIT, WNDCLASSW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

use crate::config::{parse_color, OverlayConfig, OverlayPosition};
use crate::game::{GameState, StatusDisplay};
use crate::win::{find_game_window, find_pid, get_game_rect, GameRect};

/// [EN] Magenta color key (0x00FF00FF) — any pixel with this color becomes transparent.
/// [FA] کلید رنگ ارغوانی (0x00FF00FF) — هر پیکسل با این رنگ شفاف می‌شود.
const COLORKEY: u32 = 0x00_FF_00_FF;

/// [EN] Window class name registered with Win32 for the overlay window.
/// [FA] نام کلاس پنجره ثبت‌شده در Win32 برای پنجره overlay.
const CLASS: &str = "CS16ToolV2Overlay";

/// [EN] Commands sent from the main thread to the overlay thread.
/// [FA] دستورات ارسال‌شده از رشته اصلی به رشته overlay.
enum Cmd {
    /// [EN] Toggle overlay visibility on or off.
    /// [FA] روشن/خاموش کردن نمایانی overlay.
    Visible(bool),
    /// [EN] Signal the overlay thread to shut down and destroy the window.
    /// [FA] ارسال سیگنال به رشته overlay برای خاموش شدن و تخریب پنجره.
    Shutdown,
}

/// [EN] A single line of text with its display color, used for multi-line HUD rendering.
/// [FA] یک خط متن با رنگ نمایشی آن، برای رندر چندخطی HUD استفاده می‌شود.
struct Line {
    /// [EN] The text content of the line.
    /// [FA] محتوای متنی خط.
    text: String,
    /// [EN] RGB color value for this line (0x00RRGGBB).
    /// [FA] مقدار رنگ RGB برای این خط (0x00RRGGBB).
    color: u32,
}

/// [EN] Handle to a running overlay thread — allows toggling visibility and clean shutdown.
/// [FA] دسته رشته overlay در حال اجرا — امکان تغییر نمایانی و خاموشی تمیز را فراهم می‌کند.
pub struct OverlayHandle {
    /// [EN] Channel sender for sending commands to the overlay thread.
    /// [FA] فرستنده کانال برای ارسال دستورات به رشته overlay.
    tx: Sender<Cmd>,
    /// [EN] Shared atomic flag reflecting the current visibility state.
    /// [FA] پرچم اتمی اشتراکی که وضعیت نمایانی فعلی را بازتاب می‌دهد.
    visible: Arc<AtomicBool>,
    /// [EN] Join handle for the overlay thread, taken on drop to join.
    /// [FA] دسته پیوستن رشته overlay، هنگام drop برای پیوستن برداشته می‌شود.
    join: Option<JoinHandle<()>>,
}

impl OverlayHandle {
    /// [EN] Spawn a new overlay thread that creates a transparent Win32 window and renders game HUD.
    /// [FA] یک رشته overlay جدید ایجاد می‌کند که پنجره شفاف Win32 می‌سازد و HUD بازی را رندر می‌کند.
    ///
    /// # Parameters
    /// - `pid`: Shared atomic PID of the game process to track.
    /// - `process`: Process name to search for if PID is zero.
    /// - `cfg`: Overlay display configuration (position, colors, font, etc.).
    /// - `state`: Shared game state that the overlay reads each frame.
    pub fn spawn(
        pid: Arc<AtomicU32>,
        process: String,
        cfg: OverlayConfig,
        state: Arc<RwLock<GameState>>,
    ) -> Self {
        // Create an unbounded channel for overlay commands (no back-pressure needed)
        let (tx, rx) = crossbeam_channel::unbounded();
        let visible = Arc::new(AtomicBool::new(true));
        let vis = Arc::clone(&visible);
        // Spawn the overlay thread with all shared state moved in
        let join = thread::spawn(move || run(pid, process, cfg, rx, state, vis));
        Self {
            tx,
            visible,
            join: Some(join),
        }
    }

    /// [EN] Toggle the overlay visibility (show ↔ hide).
    /// [FA] تغییر حالت نمایانی overlay (نمایش ↔ مخفی).
    pub fn toggle(&self) {
        let v = !self.visible.load(Ordering::SeqCst);
        self.set_visible(v);
    }

    /// [EN] Set the overlay visibility to the given value.
    /// [FA] تنظیم نمایانی overlay به مقدار داده‌شده.
    pub fn set_visible(&self, v: bool) {
        self.visible.store(v, Ordering::SeqCst);
        let _ = self.tx.send(Cmd::Visible(v));
    }
}

/// [EN] Clean shutdown: send Shutdown command and join the overlay thread.
/// [FA] خاموشی تمیز: ارسال دستور Shutdown و پیوستن رشته overlay.
impl Drop for OverlayHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Shutdown);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

/// [EN] Shared rendering context passed between the main loop and the window procedure.
/// [FA] زمینه رندر مشترک بین حلقه اصلی و رویه پنجره.
struct PaintCtx {
    /// [EN] Shared game state read by the render thread.
    /// [FA] وضعیت بازی مشترک که توسط رشته رندر خوانده می‌شود.
    state: Arc<RwLock<GameState>>,
    /// [EN] Overlay configuration (position, colors, font settings).
    /// [FA] تنظیمات overlay (موقعیت، رنگ‌ها، تنظیمات فونت).
    cfg: OverlayConfig,
    /// [EN] Whether the overlay is currently visible.
    /// [FA] آیا overlay در حال حاضر نمایان است.
    visible: bool,
    /// [EN] Whether the game process has been found and is running.
    /// [FA] آیا پروسه بازی پیدا شده و در حال اجراست.
    game_found: bool,
}

/// [EN] Main overlay event loop: creates the window, processes commands, syncs position, and repaints.
/// [FA] حلقه رویداد اصلی overlay: پنجره را می‌سازد، دستورات را پردازش می‌کند، موقعیت را هماهنگ می‌کند و بازرندر می‌کند.
fn run(
    pid_arc: Arc<AtomicU32>,
    process: String,
    cfg: OverlayConfig,
    rx: Receiver<Cmd>,
    state: Arc<RwLock<GameState>>,
    visible: Arc<AtomicBool>,
) {
    // Shared paint context for the window procedure
    let ctx = Arc::new(RwLock::new(PaintCtx {
        state: Arc::clone(&state),
        cfg,
        visible: true,
        game_found: false,
    }));
    let hwnd = match create_window(Arc::clone(&ctx)) {
        Some(h) => h,
        None => {
            tracing::error!("overlay create failed");
            return;
        }
    };

    let mut msg = MSG::default();
    let mut last_rect: Option<GameRect> = None;
    let mut synced = false;

    loop {
        // Drain all pending commands from the channel (non-blocking)
        while let Ok(cmd) = rx.try_recv() {
            match cmd {
                Cmd::Shutdown => {
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                    return;
                }
                Cmd::Visible(v) => {
                    ctx.write().visible = v;
                    let _ = unsafe { ShowWindow(hwnd, if v { SW_SHOW } else { SW_HIDE }) };
                }
            }
        }

        // Sleep when hidden to avoid busy-looping
        if !visible.load(Ordering::SeqCst) {
            thread::sleep(std::time::Duration::from_millis(50));
            continue;
        }

        // Resolve the game PID — try shared atomic first, then search by name
        let mut pid = pid_arc.load(Ordering::SeqCst);
        if pid == 0 {
            pid = find_pid(&process).unwrap_or(0);
        }
        ctx.write().game_found = pid != 0;

        if pid != 0 {
            // Try to get the game window rect and sync overlay position
            if let Some(gh) = find_game_window(pid) {
                if let Some(r) = get_game_rect(gh) {
                    sync_pos(hwnd, r);
                    last_rect = Some(r);
                    if !synced {
                        synced = true;
                        let _ = unsafe { ShowWindow(hwnd, SW_SHOW) };
                        println!(
                            "Overlay sync: {}x{} @ ({}, {})",
                            r.width, r.height, r.x, r.y
                        );
                    }
                }
            } else if let Some(r) = last_rect {
                // Fall back to last known position if game window temporarily disappears
                sync_pos(hwnd, r);
            }
        }

        repaint(hwnd);

        // Process Win32 messages (non-blocking via PeekMessage)
        unsafe {
            while windows::Win32::UI::WindowsAndMessaging::PeekMessageW(
                &mut msg,
                None,
                0,
                0,
                windows::Win32::UI::WindowsAndMessaging::PM_REMOVE,
            )
            .as_bool()
            {
                if msg.message == WM_QUIT {
                    return;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }

        // Cap at ~60 FPS (16ms per frame)
        thread::sleep(std::time::Duration::from_millis(16));
    }
}

/// [EN] Trigger a repaint of the overlay window by invalidating the entire client area.
/// [FA] بازرندر پنجره overlay را با باطل کردن کل ناحیه کلاینت فراخوانی می‌کند.
fn repaint(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(hwnd, None, false);
    }
}

/// [EN] Sync the overlay window position and size to match the game window rect.
/// [FA] هماهنگ‌سازی موقعیت و اندازه پنجره overlay با مستطیل پنجره بازی.
fn sync_pos(hwnd: HWND, r: GameRect) {
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            r.x,
            r.y,
            r.width,
            r.height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}

// Thread-local storage for the paint context, accessible from the window procedure.
// حافظه محلی رشته برای زمینه رندر، قابل دسترسی از رویه پنجره.
std::thread_local! {
    static CTX: std::cell::RefCell<Option<Arc<RwLock<PaintCtx>>>> =
        const { std::cell::RefCell::new(None) };
}

/// [EN] Create the layered Win32 overlay window: registers the class, creates the HWND, and applies
/// the color-key transparency so that magenta pixels become invisible.
/// [FA] پنجره لایه‌ای Win32 overlay را می‌سازد: کلاس را ثبت می‌کند، HWND ایجاد می‌کند و
/// شفافیت کلید رنگ را اعمال می‌کند تا پیکسل‌های ارغوانی نامرئی شوند.
fn create_window(ctx: Arc<RwLock<PaintCtx>>) -> Option<HWND> {
    let class = wide(CLASS);
    unsafe {
        let inst = GetModuleHandleW(None).ok()?;
        let cursor = LoadCursorW(None, IDC_ARROW).ok()?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: HINSTANCE::from(inst),
            lpszClassName: PCWSTR(class.as_ptr()),
            hCursor: cursor,
            // Background brush uses the color-key so transparent areas are magenta
            hbrBackground: CreateSolidBrush(COLORREF(COLORKEY)),
            style: CS_HREDRAW | CS_VREDRAW,
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);

        // Store the paint context in thread-local storage for the wnd_proc callback
        CTX.with(|c| *c.borrow_mut() = Some(Arc::clone(&ctx)));

        // Extended style: layered (for color-key), transparent to input, always on top, no taskbar
        let ex = WINDOW_EX_STYLE(
            WS_EX_LAYERED.0 | WS_EX_TRANSPARENT.0 | WS_EX_TOPMOST.0 | WS_EX_TOOLWINDOW.0,
        );
        let hwnd = CreateWindowExW(
            ex,
            PCWSTR(class.as_ptr()),
            PCWSTR::null(),
            WINDOW_STYLE(WS_POPUP.0),
            0,
            0,
            800,
            600,
            None,
            None,
            HINSTANCE::from(inst),
            None,
        )
        .ok()?;

        // Apply the color-key: magenta pixels become fully transparent
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(COLORKEY), 0, LWA_COLORKEY);
        // Start hidden; shown after first position sync
        let _ = ShowWindow(hwnd, SW_HIDE);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        Some(hwnd)
    }
}

/// [EN] Win32 window procedure — handles WM_PAINT (draws HUD text) and WM_DESTROY (quits).
/// [FA] رویه پنجره Win32 — WM_PAINT (متن HUD را رندر می‌کند) و WM_QUIT (خارج می‌شود) را مدیریت می‌کند.
unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);
            if let Some(ctx) = CTX.with(|c| c.borrow().clone()) {
                let shared = ctx.read();
                if shared.visible {
                    // Fill the client area with the color-key to reset it
                    let mut rect = windows::Win32::Foundation::RECT::default();
                    let _ = windows::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rect);
                    let brush = CreateSolidBrush(COLORREF(COLORKEY));
                    let _ = FillRect(hdc, &rect, brush);
                    let _ = DeleteObject(HGDIOBJ(brush.0));
                    draw(hdc, &shared, rect.right, rect.bottom);
                }
            }
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

/// [EN] Build the list of text lines to display on the overlay based on the current game state.
/// Each line has text content and a color. Returns empty if nothing should be shown.
/// [FA] لیست خطوط متنی که بر اساس وضعیت فعلی بازی روی overlay نمایش داده می‌شوند را می‌سازد.
/// هر خط دارای محتوای متنی و رنگ است. اگر چیزی نباید نمایش داده شود، خالی برمی‌گرداند.
fn build_lines(ctx: &PaintCtx) -> Vec<Line> {
    let game = ctx.state.read();
    let disp = &ctx.cfg.display;
    let colors = &ctx.cfg.colors;
    let def = parse_color(&colors.default).unwrap_or(0x00FF00);

    if !game.connected {
        return vec![Line {
            text: game.waiting_message(ctx.game_found),
            color: def,
        }];
    }

    let has_any = game.money_valid || game.clip_valid || game.reserve_valid;
    if !game.ready && !has_any {
        return vec![Line {
            text: game.waiting_message(ctx.game_found),
            color: def,
        }];
    }

    let mut lines = Vec::new();
    let sd = StatusDisplay {
        show_money: disp.show_money,
        show_ammo: disp.show_ammo,
        show_hp: disp.show_hp,
        show_armor: disp.show_armor,
        show_position: disp.show_position,
        show_view_aux: disp.show_view_aux,
    };

    if disp.show_money && game.money_valid {
        lines.push(Line {
            text: format!("💰 {:>6}", game.money),
            color: parse_color(&colors.money).unwrap_or(0x00D7FF),
        });
    }
    if disp.show_ammo && (game.clip_valid || game.reserve_valid) {
        let c = if game.clip_valid {
            format!("{:>2}", game.clip)
        } else {
            "--".into()
        };
        let r = if game.reserve_valid {
            format!("{:>2}", game.reserve)
        } else {
            "--".into()
        };
        lines.push(Line {
            text: format!("🔫 {c}/{r}"),
            color: parse_color(&colors.ammo).unwrap_or(0x00FF00),
        });
    }
    if disp.show_hp && game.hp_active {
        lines.push(Line {
            text: format!("HP {:>3.0}", game.hp),
            color: parse_color(&colors.hp).unwrap_or(0x4444FF),
        });
    }
    if disp.show_armor && game.armor_active {
        lines.push(Line {
            text: format!("Armor {:>3.0}", game.armor),
            color: parse_color(&colors.armor).unwrap_or(0xFFAA00),
        });
    }
    if disp.show_position && game.position_active {
        lines.push(Line {
            text: format!(
                "📍 {:>7.0} {:>7.0} {:>5.0}",
                game.pos_x, game.pos_y, game.pos_z
            ),
            color: parse_color(&colors.position).unwrap_or(0xFFFF88),
        });
    }
    if disp.show_view_aux && game.view_active {
        lines.push(Line {
            text: format!(
                "📐 H:{:>5.0} M:{:>5.0}/{:<5.0}",
                game.view_h, game.view_mx, game.view_my
            ),
            color: parse_color(&colors.position).unwrap_or(0xFFFF88),
        });
    }

    if lines.is_empty() {
        lines.push(Line {
            text: game.format_status(&sd),
            color: def,
        });
    }
    lines
}

/// [EN] Render text lines to the overlay HDC with a shadow effect (offset black text behind colored text).
/// Handles font creation, position calculation based on config, and text measurement.
/// [FA] خطوط متن را با افکت سایه (متن سیاه با آفست پشت متن رنگی) روی HDC overlay رندر می‌کند.
/// ایجاد فونت، محاسبه موقعیت بر اساس تنظیمات و اندازه‌گیری متن را مدیریت می‌کند.
fn draw(hdc: windows::Win32::Graphics::Gdi::HDC, ctx: &PaintCtx, cw: i32, ch: i32) {
    let lines = build_lines(ctx);
    if lines.is_empty() || cw <= 0 || ch <= 0 {
        return;
    }

    let pos = OverlayPosition::parse(&ctx.cfg.position).unwrap_or(OverlayPosition::TopLeft);
    let margin = ctx.cfg.margin;
    let spacing = ctx.cfg.line_spacing;
    let weight = if ctx.cfg.font_bold { 700 } else { 400 };

    unsafe {
        let _ = SetBkMode(hdc, TRANSPARENT);
        let fname = wide(&ctx.cfg.font_name);
        // Create the configured font
        let font: HFONT = CreateFontW(
            ctx.cfg.font_size,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            1, // DEFAULT_CHARSET
            0,
            0,
            5, // CLEARTYPE_QUALITY
            0,
            PCWSTR(fname.as_ptr()),
        );
        let old = SelectObject(hdc, HGDIOBJ(font.0));

        // Calculate total height and max width across all lines
        let total_h = lines.len() as i32 * spacing;
        let mut max_w = 0i32;
        for line in &lines {
            let w = wide(&line.text);
            // Exclude the null terminator from measurement
            let slice = &w[..w.len().saturating_sub(1)];
            let mut sz = windows::Win32::Foundation::SIZE::default();
            let _ = GetTextExtentPoint32W(hdc, slice, &mut sz);
            max_w = max_w.max(sz.cx);
        }

        // Determine base position based on the configured corner
        let (bx, by) = match pos {
            OverlayPosition::TopLeft => (margin, margin),
            OverlayPosition::TopRight => (cw - max_w - margin, margin),
            OverlayPosition::BottomLeft => (margin, ch - total_h - margin),
            OverlayPosition::BottomRight => (cw - max_w - margin, ch - total_h - margin),
        };
        let x = bx + ctx.cfg.offset_x;
        let mut y = by + ctx.cfg.offset_y;

        // Draw each line: shadow first (black, offset +1,+1), then colored text on top
        for line in &lines {
            let w = wide(&line.text);
            let slice = &w[..w.len().saturating_sub(1)];
            // Shadow: black text offset by 1 pixel for readability
            let _ = SetTextColor(hdc, COLORREF(0x000000));
            let _ = TextOutW(hdc, x + 1, y + 1, slice);
            // Main text: colored
            let _ = SetTextColor(hdc, COLORREF(line.color));
            let _ = TextOutW(hdc, x, y, slice);
            y += spacing;
        }

        // Restore the old font and delete the created one
        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(font.0));
    }
}

/// [EN] Convert a Rust &str to a null-terminated UTF-16 Vec<u16> for Win32 API calls.
/// [FA] یک &str رست را به Vec<u16> UTF-16 با پایان‌دهنده null برای فراخوانی‌های API Win32 تبدیل می‌کند.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
