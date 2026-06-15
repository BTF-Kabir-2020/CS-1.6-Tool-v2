/// Game window discovery and geometry for CS 1.6 / Half-Life.
use std::sync::Mutex;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowW, GetClassNameW, GetClientRect, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible,
};

use windows::core::PCWSTR;

/// Axis-aligned rectangle describing the game window's client or window area.
///
/// All coordinates are in screen pixels relative to the top-left corner of the monitor.
///
/// مستطیل محوری که ناحیه کلاینت یا پنجره بازی را توصیف می‌کند.
#[derive(Debug, Clone, Copy, Default)]
pub struct GameRect {
    /// Horizontal offset from the screen's top-left corner.
    pub x: i32,
    /// Vertical offset from the screen's top-left corner.
    pub y: i32,
    /// Width of the rectangle in pixels.
    pub width: i32,
    /// Height of the rectangle in pixels.
    pub height: i32,
}

/// Cached result of a successful window lookup, keyed by PID.
struct Cache {
    /// PID whose window handle is cached.
    pid: u32,
    /// Cached window handle (or `None` if lookup previously failed).
    hwnd: Option<isize>,
    /// Timestamp of when the handle was cached; `None` means never cached.
    at: Option<Instant>,
}

/// Global window-handle cache protected by a mutex.
///
/// Avoids redundant `EnumWindows` calls on every tick.
static CACHE: Mutex<Cache> = Mutex::new(Cache {
    pid: 0,
    hwnd: None,
    at: None,
});

/// Invalidate the global window-handle cache.
///
/// Call this after the game restarts or switches full-screen mode so the next
/// lookup performs a fresh enumeration.
///
/// باطل‌سازی کش هندل پنجره سراسری. پس از ری‌استارت بازی فراخوانی شود.
pub fn invalidate_window_cache() {
    if let Ok(mut c) = CACHE.lock() {
        *c = Cache {
            pid: 0,
            hwnd: None,
            at: None,
        };
    }
}

/// Find the game window handle for a given PID, using the global cache.
///
/// The cached entry lives for 800 ms before being refreshed. If the cached
/// handle is invalid the lookup is re-executed immediately.
///
/// پیدا کردن هندل پنجره بازی برای شناسه پروسه داده‌شده، با استفاده از کش سراسری.
pub fn find_game_window(pid: u32) -> Option<isize> {
    if pid == 0 {
        return None;
    }

    /// Cache time-to-live — refreshed every 800 ms.
    const TTL: Duration = Duration::from_millis(800);

    if let Ok(mut cache) = CACHE.lock() {
        if cache.pid == pid {
            if let Some(hwnd) = cache.hwnd {
                if cache.at.map(|t| t.elapsed() < TTL).unwrap_or(false) && is_valid(hwnd) {
                    return Some(hwnd);
                }
            }
        }
        let hwnd = find_uncached(pid);
        cache.pid = pid;
        cache.hwnd = hwnd;
        cache.at = Some(Instant::now());
        return hwnd;
    }
    find_uncached(pid)
}

/// Return `true` if the given window handle points to a still-existing window.
fn is_valid(hwnd: isize) -> bool {
    hwnd != 0 && unsafe { IsWindow(HWND(hwnd as *mut _)) }.as_bool()
}

/// Locate the game window without consulting the cache.
///
/// Tries, in order:
/// 1. `Valve001` window class (standard for CS 1.6).
/// 2. Known title strings (`Counter-Strike`, `Half-Life`, `Condition Zero`).
/// 3. Fallback: the largest visible window belonging to the PID.
fn find_uncached(pid: u32) -> Option<isize> {
    if let Some(h) = by_class(pid, "Valve001") {
        return Some(h);
    }
    for title in &["Counter-Strike", "Half-Life", "Condition Zero"] {
        if let Some(h) = by_title(pid, title) {
            return Some(h);
        }
    }
    largest_visible(pid)
}

/// Get the game window's rectangle, preferring the client area.
///
/// Falls back to the full window rectangle if the client rect has zero dimensions.
///
/// مستطیل پنجره بازی را برمی‌گرداند. ابتدا ناحیه کلاینت و در صورت عدم اعتبار، کل پنجره.
pub fn get_game_rect(hwnd: isize) -> Option<GameRect> {
    if let Some(r) = client_rect(hwnd) {
        if r.width > 0 && r.height > 0 {
            return Some(r);
        }
    }
    window_rect(hwnd)
}

/// Get the client area of the window converted to screen coordinates.
///
/// Uses `GetClientRect` + `ClientToScreen` to obtain the drawable region
/// excluding title bar and borders.
fn client_rect(hwnd: isize) -> Option<GameRect> {
    let hwnd = HWND(hwnd as *mut _);
    let mut client = RECT::default();
    unsafe {
        if GetClientRect(hwnd, &mut client).is_err() {
            return None;
        }
        // Convert client-relative corners to screen coordinates.
        // تبدیل گوشه‌های نسبی کلاینت به مختصات صفحه.
        let mut tl = windows::Win32::Foundation::POINT {
            x: client.left,
            y: client.top,
        };
        let mut br = windows::Win32::Foundation::POINT {
            x: client.right,
            y: client.bottom,
        };
        let _ = ClientToScreen(hwnd, &mut tl);
        let _ = ClientToScreen(hwnd, &mut br);
        let w = br.x - tl.x;
        let h = br.y - tl.y;
        if w <= 0 || h <= 0 {
            return None;
        }
        Some(GameRect {
            x: tl.x,
            y: tl.y,
            width: w,
            height: h,
        })
    }
}

/// Get the full window rectangle (including title bar and borders).
fn window_rect(hwnd: isize) -> Option<GameRect> {
    let hwnd = HWND(hwnd as *mut _);
    let mut rect = RECT::default();
    unsafe {
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return None;
        }
        let w = rect.right - rect.left;
        let h = rect.bottom - rect.top;
        if w <= 0 || h <= 0 {
            return None;
        }
        Some(GameRect {
            x: rect.left,
            y: rect.top,
            width: w,
            height: h,
        })
    }
}

/// Find a visible window belonging to `pid` whose class name matches `class`.
///
/// Uses `EnumWindows` with a callback that checks process ownership,
/// visibility, and class name (case-insensitive).
fn by_class(pid: u32, class: &str) -> Option<isize> {
    struct S {
        pid: u32,
        class: String,
        found: Option<isize>,
    }

    unsafe extern "system" fn cb(
        hwnd: HWND,
        lparam: windows::Win32::Foundation::LPARAM,
    ) -> windows::Win32::Foundation::BOOL {
        use windows::Win32::Foundation::TRUE;
        let s = &mut *(lparam.0 as *mut S);
        let mut wp = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut wp)) };
        if wp != s.pid {
            return TRUE;
        }
        // Skip invisible or minimised windows.
        // پنجره‌های نامرئی یا کوچک‌شده را رد کن.
        if !unsafe { IsWindowVisible(hwnd) }.as_bool() || unsafe { IsIconic(hwnd) }.as_bool() {
            return TRUE;
        }
        let mut buf = [0u16; 64];
        let len = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
        let c = String::from_utf16_lossy(&buf[..len]);
        if c.eq_ignore_ascii_case(&s.class) {
            s.found = Some(hwnd.0 as isize);
            return windows::Win32::Foundation::FALSE;
        }
        TRUE
    }

    let mut s = S {
        pid,
        class: class.to_string(),
        found: None,
    };
    let _ = unsafe {
        EnumWindows(
            Some(cb),
            windows::Win32::Foundation::LPARAM(&mut s as *mut _ as isize),
        )
    };
    s.found
}

/// Find a visible window belonging to `pid` whose title exactly matches `title`.
///
/// Uses `FindWindowW` then verifies process ownership via `GetWindowThreadProcessId`.
fn by_title(pid: u32, title: &str) -> Option<isize> {
    let wide = title
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let hwnd = unsafe { FindWindowW(PCWSTR::null(), PCWSTR(wide.as_ptr())) }.ok()?;
    if hwnd.0.is_null() {
        return None;
    }
    let mut wp = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut wp)) };
    if wp == pid {
        Some(hwnd.0 as isize)
    } else {
        None
    }
}

/// Fallback: find the largest visible window belonging to `pid`.
///
/// Enumerates all windows and picks the one with the greatest client area.
/// Used when neither the standard class name nor known titles match.
///
/// پنجره مرئی بزرگ‌تر متعلق به `pid` را پیدا می‌کند. به عنوان جایگزین استفاده می‌شود.
fn largest_visible(pid: u32) -> Option<isize> {
    struct S {
        pid: u32,
        best: Option<isize>,
        area: i32,
    }

    unsafe extern "system" fn cb(
        hwnd: HWND,
        lparam: windows::Win32::Foundation::LPARAM,
    ) -> windows::Win32::Foundation::BOOL {
        use windows::Win32::Foundation::TRUE;
        let s = &mut *(lparam.0 as *mut S);
        let mut wp = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut wp)) };
        if wp != s.pid || !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            return TRUE;
        }
        if let Some(r) = get_game_rect(hwnd.0 as isize) {
            let area = r.width * r.height;
            if area > s.area {
                s.area = area;
                s.best = Some(hwnd.0 as isize);
            }
        }
        TRUE
    }

    let mut s = S {
        pid,
        best: None,
        area: 0,
    };
    let _ = unsafe {
        EnumWindows(
            Some(cb),
            windows::Win32::Foundation::LPARAM(&mut s as *mut _ as isize),
        )
    };
    s.best
}
