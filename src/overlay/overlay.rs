//! Overlay شفاف Win32 — LWA_COLORKEY magenta.

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

const COLORKEY: u32 = 0x00_FF_00_FF;
const CLASS: &str = "CS16ToolV2Overlay";

enum Cmd {
    Visible(bool),
    Shutdown,
}

struct Line {
    text: String,
    color: u32,
}

pub struct OverlayHandle {
    tx: Sender<Cmd>,
    visible: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl OverlayHandle {
    pub fn spawn(
        pid: Arc<AtomicU32>,
        process: String,
        cfg: OverlayConfig,
        state: Arc<RwLock<GameState>>,
    ) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let visible = Arc::new(AtomicBool::new(true));
        let vis = Arc::clone(&visible);
        let join = thread::spawn(move || run(pid, process, cfg, rx, state, vis));
        Self {
            tx,
            visible,
            join: Some(join),
        }
    }

    pub fn toggle(&self) {
        let v = !self.visible.load(Ordering::SeqCst);
        self.set_visible(v);
    }

    pub fn set_visible(&self, v: bool) {
        self.visible.store(v, Ordering::SeqCst);
        let _ = self.tx.send(Cmd::Visible(v));
    }
}

impl Drop for OverlayHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Shutdown);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

struct PaintCtx {
    state: Arc<RwLock<GameState>>,
    cfg: OverlayConfig,
    visible: bool,
    game_found: bool,
}

fn run(
    pid_arc: Arc<AtomicU32>,
    process: String,
    cfg: OverlayConfig,
    rx: Receiver<Cmd>,
    state: Arc<RwLock<GameState>>,
    visible: Arc<AtomicBool>,
) {
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

        if !visible.load(Ordering::SeqCst) {
            thread::sleep(std::time::Duration::from_millis(50));
            continue;
        }

        let mut pid = pid_arc.load(Ordering::SeqCst);
        if pid == 0 {
            pid = find_pid(&process).unwrap_or(0);
        }
        ctx.write().game_found = pid != 0;

        if pid != 0 {
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
                sync_pos(hwnd, r);
            }
        }

        repaint(hwnd);

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

        thread::sleep(std::time::Duration::from_millis(16));
    }
}

fn repaint(hwnd: HWND) {
    unsafe {
        let _ = InvalidateRect(hwnd, None, false);
    }
}

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

std::thread_local! {
    static CTX: std::cell::RefCell<Option<Arc<RwLock<PaintCtx>>>> =
        const { std::cell::RefCell::new(None) };
}

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
            hbrBackground: CreateSolidBrush(COLORREF(COLORKEY)),
            style: CS_HREDRAW | CS_VREDRAW,
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);

        CTX.with(|c| *c.borrow_mut() = Some(Arc::clone(&ctx)));

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

        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(COLORKEY), 0, LWA_COLORKEY);
        let _ = ShowWindow(hwnd, SW_HIDE);
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
        Some(hwnd)
    }
}

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
        let font: HFONT = CreateFontW(
            ctx.cfg.font_size,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            1,
            0,
            0,
            5,
            0,
            PCWSTR(fname.as_ptr()),
        );
        let old = SelectObject(hdc, HGDIOBJ(font.0));

        let total_h = lines.len() as i32 * spacing;
        let mut max_w = 0i32;
        for line in &lines {
            let w = wide(&line.text);
            let slice = &w[..w.len().saturating_sub(1)];
            let mut sz = windows::Win32::Foundation::SIZE::default();
            let _ = GetTextExtentPoint32W(hdc, slice, &mut sz);
            max_w = max_w.max(sz.cx);
        }

        let (bx, by) = match pos {
            OverlayPosition::TopLeft => (margin, margin),
            OverlayPosition::TopRight => (cw - max_w - margin, margin),
            OverlayPosition::BottomLeft => (margin, ch - total_h - margin),
            OverlayPosition::BottomRight => (cw - max_w - margin, ch - total_h - margin),
        };
        let x = bx + ctx.cfg.offset_x;
        let mut y = by + ctx.cfg.offset_y;

        for line in &lines {
            let w = wide(&line.text);
            let slice = &w[..w.len().saturating_sub(1)];
            let _ = SetTextColor(hdc, COLORREF(0x000000));
            let _ = TextOutW(hdc, x + 1, y + 1, slice);
            let _ = SetTextColor(hdc, COLORREF(line.color));
            let _ = TextOutW(hdc, x, y, slice);
            y += spacing;
        }

        let _ = SelectObject(hdc, old);
        let _ = DeleteObject(HGDIOBJ(font.0));
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
