# CS 1.6 Tool v2 Architecture / معماری CS 1.6 Tool v2

**Last updated / آخرین به‌روزرسانی:** June 2026 / ژوئن ۲۰۲۶

---

## Table of Contents / فهرست

1. [Overview / نمای کلی](#1-overview--نمای-کلی)
2. [Folder Structure / ساختار پوشه](#2-folder-structure--ساختار-پوشه)
3. [Thread Model / مدل thread](#3-thread-model--مدل-thread)
4. [Module win/ — Memory Access / ماژول win/ — دسترسی به حافظه](#4-module-win--memory-access--ماژول-win--دسترسی-به-حافظه)
5. [Module game/ — Game Engine / ماژول game/ — موتور بازی](#5-module-game--game-engine--ماژول-game--موتور-بازی)
6. [Module overlay/ — Render on Game Window / ماژول overlay/ — رندر روی پنجره بازی](#6-module-overlay--render-on-game-window--ماژول-overlay--رندر-روی-پنجره-بازی)
7. [Module debug/ — Debug Console / ماژول debug/ — کنسول دیباگ](#7-module-debug--debug-console--ماژول-debug--کنسول-دیباگ)
8. [Module config/ — Configuration / ماژول config/ — پیکربندی](#8-module-config--configuration--ماژول-config--پیکربندی)
9. [Data Flow / جریان داده](#9-data-flow--جریان-داده)
10. [State Management / مدیریت state](#10-state-management--مدیریت-state)
11. [resolve chain — Algorithm / الگوریتم resolve chain](#11-resolve-chain--الگوریتم)
12. [Local Player Discovery](#12-local-player-discovery)
13. [Position Discovery](#13-position-discovery)
14. [Overlay Rendering](#14-overlay-rendering)
15. [Window Management](#15-window-management)
16. [Error Handling / مدیریت خطا](#16-error-handling--مدیریت-خطا)
17. [Dependencies / وابستگی‌ها](#17-dependencies--وابستگی‌ها)
18. [Memory Safety — Safe vs Unsafe / ایمنی حافظه](#18-memory-safety--safe-vs-unsafe--ایمنی-حافظه)

---

## 1. Overview / نمای کلی

External Memory Tool for Counter-Strike 1.6 (GoldSrc Engine) — complete rewrite in Rust.
<br>ابزار External Memory برای Counter-Strike 1.6 (GoldSrc Engine) — بازنویسی کامل با Rust.

**Key features / ویژگی‌های کلیدی:**
- Read/write `hl.exe` memory from outside (External) / خواندن/نوشتن حافظه `hl.exe` از بیرون
- Multiple address resolution strategies (Entity, Chain, Direct) / چندین استراتژی resolve آدرس
- Transparent overlay on game window (GDI + LWA_COLORKEY) / Overlay شفاف روی پنجره بازی
- Auto reconnect / Reconnect خودکار
- hw.dll and sw.dll (Software Renderer) support / پشتیبانی از hw.dll و sw.dll

---

## 2. Folder Structure / ساختار پوشه

```
cs16-tool-v2/
├── Cargo.toml              # Dependencies & profile / وابستگی‌ها و profile
├── config.toml             # User configuration / پیکربندی کاربر
├── README.md               # Quick start / شروع سریع
├── ARCHITECTURE.md         # This file / این فایل
└── src/
    ├── main.rs             # Entry point — thread orchestration / نقطه ورود — هماهنگی thread
    ├── lib.rs              # Public modules / ماژول‌های pub
    ├── config.rs           # AppConfig + parse helpers
    ├── error.rs            # AppError + MemoryError
    ├── bin/
    │   └── dump.rs         # Second CLI: dump addresses / CLI دوم: dump آدرس‌ها
    ├── win/
    │   ├── mod.rs          # Re-exports
    │   ├── process.rs      # ProcessHandle, find_pid, module_base
    │   ├── memory.rs       # MemoryReader, MemoryWriter, resolve_chain
    │   └── window.rs       # find_game_window, get_game_rect
    ├── game/
    │   ├── mod.rs          # Re-exports
    │   ├── engine.rs       # GameEngine — main tick logic / منطق اصلی tick
    │   ├── state.rs        # GameState, StatusDisplay, DebugSnapshot
    │   ├── local_player.rs # Local Player discovery
    │   └── position.rs     # Position (vec3) discovery
    ├── overlay/
    │   ├── mod.rs          # Re-exports
    │   └── overlay.rs      # Win32 overlay window
    └── debug/
        └── mod.rs          # DebugConsole — ANSI terminal
```

---

## 3. Thread Model / مدل thread

The project uses **3 threads** / پروژه از **۳ thread** استفاده می‌کند:

```
┌──────────────────────────────────────────────────────────────┐
│  Main Thread                                                 │
│  • Keys: Q/End=exit, Insert=toggle overlay, F7=debug         │
│  • Print status to console (every 50ms) / چاپ status در کنسول │
│  • Lifecycle control / کنترل lifecycle                        │
├──────────────────────────────────────────────────────────────┤
│  Memory Thread (memory_loop)                                 │
│  • Every N ms: connect → tick → write GameState              │
│  • Auto reconnect / Reconnect خودکار                         │
│  • Resolve chains every tick / resolve chainها هر tick        │
├──────────────────────────────────────────────────────────────┤
│  Overlay Thread (overlay::run)                               │
│  • ~60 FPS: InvalidateRect → WM_PAINT → GDI draw            │
│  • Sync position with game window / sync موقعیت با پنجره بازی │
│  • PeekMessage loop                                          │
└──────────────────────────────────────────────────────────────┘
```

**Thread communication / ارتباط threadها:**
- `Arc<RwLock<GameState>>` — shared between memory and overlay / بین memory و overlay مشترک
- `Arc<AtomicU32>` — PID store
- `Arc<AtomicBool>` — running flag + debug_on flag
- `crossbeam_channel` — overlay commands (toggle visibility, shutdown) / دستورات overlay

---

## 4. Module win/ — Memory Access / ماژول win/ — دسترسی به حافظه

### win/process.rs

**`ProcessHandle`** — Process handle management / مدیریت handle باز به پروسس:
```rust
pub struct ProcessHandle {
    handle: HANDLE,
    pid: u32,
}
```

- `attach(name)` → OpenProcess with VM_READ | VM_WRITE | VM_OPERATION | QUERY_INFORMATION
- `module_base(name)` → Scan modules with CreateToolhelp32Snapshot
- Drop → CloseHandle (RAII)

**`find_pid(name)`** → Scan processes with PROCESSENTRY32W

**`engine_base(process, modules)`** → hw.dll first, sw.dll (fallback)

### win/memory.rs

**`MemoryReader`** — Memory reading / خواندن حافظه:
```rust
pub fn read<T: Copy>(&self, address: u32) -> Result<T, MemoryError>
pub fn read_i32 / read_u32 / read_f32
```

**`MemoryWriter`** — Memory writing / نوشتن حافظه:
```rust
pub fn write<T: Copy>(&self, address: u32, value: T) -> Result<(), MemoryError>
pub fn write_i32 / write_f32
```

**`resolve_chain(process, base_ptr, offsets)`** → Pointer chain algorithm / الگوریتم pointer chain:
```
addr = base_ptr
for offset in offsets:
    addr = read_u32(addr) + offset
return addr
```

### win/window.rs

**`find_game_window(pid)`** — Find game HWND / پیدا کردن HWND بازی:
1. First class `Valve001` / ابتدا کلاس `Valve001`
2. Title: `Counter-Strike`, `Half-Life`, `Condition Zero`
3. Fallback: largest visible window / Fallback: بزرگ‌ترین پنجره visible

**Cache:** TTL 800ms to prevent frequent scanning / TTL 800ms برای جلوگیری از اسکن مکرر

**`get_game_rect(hwnd)`** → GetClientRect + ClientToScreen

---

## 5. Module game/ — Game Engine / ماژول game/ — موتور بازی

### game/engine.rs — GameEngine

Main logic. Each tick: / هسته اصلی برنامه. هر tick:

```
tick() → GameState
├── refresh_bases()          # Check hw_base / client_base change / بررسی تغییر base
├── refresh_money_hw_if_needed()
├── refresh_reserve_if_needed()
├── discover_local_player_if_needed()
├── read_vitals()            # HP + Armor
├── read_money()             # 3 strategies: hw chain → client direct → entity
├── read_clip()              # Multiple chains + pick_best
├── read_reserve()           # Single chain
├── read_position_values()   # 6 different strategies / ۶ استراتژی مختلف
├── read_view_aux()          # H / mouse
├── merge_cache()            # Cache values / cache مقدارها
├── check_ready()            # Are all fields OK? / آیا همه فیلدها OK هستند
└── return GameState
```

**Money read priority / اولویت read پول:**
1. `hw.dll + RVA` → pointer chain → money
2. `client.dll + direct_rva` → direct / مستقیم
3. `entity + money_offset` → from local player / از local player

**Clip read priority / اولویت read clip:**
1. All `[[chains.clip]]` chains → best valid value / بهترین مقدار معتبر
2. If none → reserve chain (clip[2] in some builds) / اگر هیچکدام → reserve chain

### game/state.rs

**`GameState`** — Instant game state / وضعیت لحظه‌ای بازی:
```rust
pub struct GameState {
    pub money: i32,
    pub clip: i32,
    pub reserve: i32,
    pub hp: f32,
    pub armor: f32,
    pub pos_x/y/z: f32,
    pub view_h/mx/my: f32,
    pub connected: bool,
    pub ready: bool,
    pub money_valid: bool,
    pub clip_valid: bool,
    pub reserve_valid: bool,
    pub hp_active: bool,
    pub armor_active: bool,
    pub position_active: bool,
    pub view_active: bool,
    pub player_alive: bool,
}
```

**`format_status()`** — Single formatter for console + overlay / formatter واحد برای کنسول + overlay:
- `connected=false` → `[Waiting for game...]` / `[منتظر بازی...]`
- `ready=false` → `[Reading...]` / `[در حال خواندن...]`
- Invalid field → `--` (not `0`) / فیلد نامعتبر → `--`

### game/local_player.rs

**`discover()`** — Find Local Player pointer / پیدا کردن Local Player pointer:
1. First config RVA
2. Known list (LP_RVA_HW / LP_RVA_CLIENT)
3. Module scan (0x100000..0x600000)

**Scoring:** HP range + armor valid + from_known_rva bonus

### game/position.rs

**6 strategies for reading coordinates / ۶ استراتژی برای خواندن مختصات:**

| # | Strategy | Description |
|---|----------|-------------|
| 1 | `locked_pos_player` | Previously locked address / آدرس قفل‌شده از قبل |
| 2 | `position_global_hw_rva` | Direct vec3 in hw.dll |
| 3 | `resolve_hw_local_player_position` | hw entity → pev → origin |
| 4 | `read_hw_entity_world_origin` | hw entity fallback |
| 5 | `discovered_*` | Previous discovery cache / کش discovery قبلی |
| 6 | `player + offset` | client entity |

**Filters / فیلترها:**
- `looks_like_coords()` — Map range / محدوده نقشه
- `looks_like_world_origin()` — Not view aux
- `looks_like_spawn_stub()` — Reject spawn point / رد کردن spawn point
- `is_usable_position()` — Combined filter / ترکیب همه فیلترها

---

## 6. Module overlay/ — Render on Game Window / ماژول overlay/ — رندر روی پنجره بازی

### overlay/overlay.rs

**OverlayHandle** — Overlay thread management / مدیریت overlay thread:
```rust
pub struct OverlayHandle {
    tx: Sender<Cmd>,
    visible: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}
```

**Window:** Win32 WS_POPUP + WS_EX_LAYERED + WS_EX_TRANSPARENT + WS_EX_TOPMOST

**Render:** GDI — CreateFontW + TextOutW + FillRect

**Repaint mechanism / مکانیسم repaint:**
```rust
fn repaint(hwnd: HWND) {
    unsafe { InvalidateRect(hwnd, None, false); }
}
```

**WM_PAINT handler:**
1. BeginPaint
2. FillRect with COLORKEY (0xFF00FF — transparent magenta)
3. build_lines() → Read GameState / خواندن GameState
4. draw() → Draw each line with color and shadow / رسم هر خط با رنگ و سایه
5. EndPaint

**Position sync / sync موقعیت:**
- Each frame: find_game_window → get_game_rect → SetWindowPos
- HWND_TOPMOST to stay on top of game / HWND_TOPMOST برای ماندن روی بازی

---

## 7. Module debug/ — Debug Console / ماژول debug/ — کنسول دیباگ

**DebugConsole** — ANSI terminal printing / چاپ ANSI در ترمینال:
- `\x1b[2J\x1b[H` → Clear screen / پاک کردن صفحه
- Info: tick, ready, money, clip, reserve, write_enabled, alive
- Configurable interval in config / Interval قابل تنظیم در config

---

## 8. Module config/ — Configuration / ماژول config/ — پیکربندی

**`AppConfig`** — Main structure / ساختار اصلی:
```rust
pub struct AppConfig {
    pub process: ProcessConfig,        // name = "hl.exe"
    pub modules: ModulesConfig,        // hw, sw, client
    pub targets: TargetsConfig,        // money=15000, clip=15, reserve=20
    pub features: FeaturesConfig,      // write/overlay/hp/armor/position
    pub timing: TimingConfig,          // memory_loop_ms, overlay_loop_ms
    pub entity: EntityConfig,          // local_player_rva, offsets
    pub overlay: OverlayConfig,        // font, colors, position
    pub chains: ChainsConfig,          // pointer chains
    pub clip_detection: ClipDetectionConfig, // min/max value
    pub hud_sync: HudSyncConfig,       // HUD sync
    pub debug_console: DebugConsoleConfig,
}
```

**`parse_hex_u32()`** → Convert "0x7AF55C" to u32

**`parse_offsets()`** → Convert Vec<String> to Vec<u32>

---

## 9. Data Flow / جریان داده

```
config.toml
    │
    ▼
AppConfig (parse once / یکبار parse)
    │
    ├──► GameEngine::open()
    │       ├── ProcessHandle::attach()
    │       ├── engine_base() → hw.dll / sw.dll
    │       ├── resolve_static_addresses()
    │       └── discover_local_player_if_needed()
    │
    ▼
memory_loop (every N ms / هر N ms)
    │
    ├──► eng.tick() → GameState
    │       ├── read money (3 strategies / ۳ استراتژی)
    │       ├── read clip (multiple chains / چند chain)
    │       ├── read reserve
    │       ├── read hp/armor
    │       ├── read position (6 strategies / ۶ استراتژی)
    │       └── merge_cache → GameState
    │
    ├──► *state.write() = snap
    │
    ▼
Overlay Thread (every 16ms / هر 16ms)
    │
    ├──► state.read() → GameState
    ├──► build_lines() → Vec<Line>
    └──► draw() → GDI TextOutW
```

---

## 10. State Management / مدیریت state

**`Arc<RwLock<GameState>>`** — Shared between memory + overlay / مشترک بین memory + overlay

- Memory thread: `*state.write() = snap.clone()`
- Overlay thread: `state.read()` → `build_lines()` → `draw()`
- Main thread: `state.read()` → `format_status()` → console

**`Cache`** in GameEngine:
```rust
struct Cache {
    money: i32, money_valid: bool,
    clip: i32, clip_valid: bool,
    reserve: i32, reserve_valid: bool,
}
```
- Only successful values are cached / فقط مقدارهای موفق cache می‌شوند
- Bad tick → previous value preserved / tick بد → مقدار قبلی حفظ می‌شود

**`display_ready`** → Once true, never false again / یکبار true شد، دیگر false نمی‌شود

---

## 11. resolve chain — Algorithm / الگوریتم

```rust
fn resolve_chain(process, base_ptr, offsets) -> Result<u32> {
    let mut addr = base_ptr;
    for (step, &offset) in offsets.iter().enumerate() {
        let next = read_u32(addr)?;  // Read pointer / خواندن pointer
        if next == 0 { return Err(ChainBroken); }
        addr = next + offset;        // Add offset / اضافه کردن offset
    }
    Ok(addr)
}
```

**Example / مثال:**
```
hw.dll + 0x6E92AC
  → [+0x1CC] → [+0x320] → [+0x4] → [+0x7C] → [+0x21C]
  → Money address
```

**Resolved every tick** — not once. This makes it work after base change / هر tick resolve می‌شود — نه یکبار. این باعث می‌شود بعد از تغییر base دوباره کار کند.

---

## 12. Local Player Discovery

**Goal / هدف:** Find current player entity address / پیدا کردن آدرس entity بازیکن فعلی

**Method / روش:**
1. config RVA → `hw.dll + rva` → read pointer → player entity
2. Known RVA list / لیست شناخته‌شده RVAها
3. Scan 0x100000..0x600000 with step=4

**HP verify:** `(0..=100).contains(&hp)` → If HP valid → correct player / اگر HP معتبر بود → player درست

**Scoring:**
- from_known_rva: +1000
- hp_off matches config: +500
- armor valid: +100
- hp value: +hp

---

## 13. Position Discovery

**Most complex part / پیچیده‌ترین بخش** — 6 strategies + multiple filters / ۶ استراتژی + فیلترهای متعدد

**Vec3 filters / فیلترهای vec3:**
- `looks_like_coords()` — Map range (<=16384) / محدوده نقشه
- `looks_like_world_origin()` — Not view aux, both horizontal axes meaningful
- `looks_like_view_aux()` — Pattern (164, 1, 140)
- `looks_like_spawn_stub()` — (0, 300, 0)
- `is_usable_position()` — Final combination / ترکیب نهایی

**Discovery live:**
1. collect_all_movement_snaps() — Initial sampling / نمونه‌گیری اول
2. sleep(wait_ms)
3. pick_best_mover() — Compare two samples / مقایسه دو نمونه
4. If failed → discover_by_changing_floats() / اگر نشد → discover_by_changing_floats()

---

## 14. Overlay Rendering

**Simple GDI render / رندر GDI ساده:**
1. `InvalidateRect` → Mark window as dirty / علامت‌گذاری پنجره به عنوان dirty
2. `PeekMessage` → Generate WM_PAINT / تولید WM_PAINT
3. `wnd_proc(WM_PAINT)`:
   - `BeginPaint` → HDC
   - `FillRect` with magenta (COLORKEY)
   - `CreateFontW` → Font / فونت
   - For each line: `SetTextColor` + `TextOutW` (shadow + main) / برای هر خط: سایه + اصلی
   - `DeleteObject(font)` → Cleanup / آزادسازی

**Features / ویژگی‌ها:**
- Transparent layer (LWA_COLORKEY) / لایه شفاف
- On top of all windows (WS_EX_TOPMOST) / روی همه پنجره‌ها
- No click impact (WS_EX_TRANSPARENT) / بدون تأثیر روی کلیک
- No taskbar (WS_EX_TOOLWINDOW) / بدون taskbar

---

## 15. Window Management

**`find_game_window(pid)`:**
1. Class `Valve001` (standard SDK) / کلاس `Valve001` (SDK استاندارد)
2. Title: Counter-Strike / Half-Life / Condition Zero
3. Largest visible window / بزرگ‌ترین پنجره visible

**Cache:** Mutex + TTL 800ms

**`get_game_rect(hwnd)`:**
1. GetClientRect → Client area dimensions / ابعاد client area
2. ClientToScreen → Screen coordinates / مختصات صفحه
3. Fallback: GetWindowRect

---

## 16. Error Handling / مدیریت خطا

**`AppError`** — High-level errors / خطاهای سطح بالا:
```rust
pub enum AppError {
    Config(String),
    Memory(MemoryError),
    Other(anyhow::Error),
}
```

**`MemoryError`** — Memory errors / خطاهای حافظه:
```rust
pub enum MemoryError {
    ProcessNotFound { name },
    OpenProcessFailed { pid },
    ModuleNotFound { name },
    ReadFailed { address },
    WriteFailed { address },
    ChainBroken { step, address },
    InvalidAddress { address },
}
```

**Reconnect logic / منطق Reconnect:**
- `should_reconnect()` → !is_alive || !engine_base || stale >= threshold
- Stale counter: +1 each tick without successful read / stale counter: هر tick بدون read موفق +1
- After reconnect: pid_store=0, invalidate_window_cache, state=default

---

## 17. Dependencies / وابستگی‌ها

| Crate | Version | Purpose / کاربرد |
|-------|---------|-----------------|
| `windows` | 0.58 | Win32 API (RPM/WPM, GDI, Window) |
| `clap` | 4 | CLI argument parsing |
| `parking_lot` | 0.12 | Faster RwLock than std / RwLock سریع‌تر از std |
| `crossbeam-channel` | 0.5 | Overlay thread communication / ارتباط thread overlay |
| `tracing` | 0.1 | Logging |
| `tracing-subscriber` | 0.3 | Console logging |
| `serde` | 1 | Deserialize config |
| `toml` | 0.8 | Parse TOML |
| `thiserror` | 1 | Error derive |
| `anyhow` | 1 | Error wrapping |

---

## 18. Memory Safety — Safe vs Unsafe / ایمنی حافظه

### Summary / خلاصه

The codebase is largely **unsafe**. Almost all memory and Win32 API operations are in `unsafe` blocks. Only Rust wrappers around them are safe.
<br>سورس کامل **unsafe** است. تقریباً همه عملیات حافظه و Win32 API داخل `unsafe` block هستند. فقط wrapperهای Rust دورشون safe هستند.

### Why unsafe is needed? / چرا unsafe لازم است؟

CS 1.6 Tool is an **External Memory Tool** — it reads/writes game process memory from outside. This is only possible through Win32 API, and all Win32 FFI functions are **unsafe** in Rust:
<br>CS 1.6 Tool یک **External Memory Tool** است — یعنی از بیرون پروسس بازی حافظه را می‌خواند/می‌نویسد. این کار فقط از طریق Win32 API امکان‌پذیر است و همه Win32 FFI functions در Rust **unsafe** هستند:

```rust
// unsafe because: FFI call to Windows kernel — no guarantee for valid handle/addr
// unsafe چون: FFI call به Windows kernel — تضمینی برای valid handle/addr نیست
ReadProcessMemory(handle, addr, buf, size, None)
WriteProcessMemory(handle, addr, buf, size, None)
OpenProcess(permissions, false, pid)
```

### Unsafe map in code / نقشه unsafe در کد

| File | Unsafe reason / علت | Example |
|------|---------------------|---------|
| `win/memory.rs` | FFI → RPM/WPM | `ReadProcessMemory`, `WriteProcessMemory` |
| `win/process.rs` | FFI → Process API | `OpenProcess`, `CreateToolhelp32Snapshot`, `Module32FirstW` |
| `win/window.rs` | FFI → Window API | `EnumWindows`, `FindWindowW`, `GetClientRect` |
| `overlay/overlay.rs` | FFI → Win32 Window + GDI | `CreateWindowExW`, `TextOutW`, `BeginPaint`, `PeekMessageW` |
| `game/engine.rs` | Indirect (via MemoryReader) / غیرمستقیم | — |
| `game/position.rs` | Indirect (via MemoryReader) / غیرمستقیم | — |
| `game/local_player.rs` | Indirect (via MemoryReader) / غیرمستقیم | — |

### What is safe? / چه بخش‌هایی safe هستند؟

| File | Why safe / چرا safe است |
|------|------------------------|
| `config.rs` | Pure Rust — serde + TOML parse |
| `error.rs` | Pure Rust — thiserror derive |
| `game/state.rs` | Pure Rust — data structures + format |
| /`debug/mod.rs` | Pure Rust — println + ANSI |
| `main.rs` | Safe — thread spawn + AtomicBool + channel |

### Safety pattern: Wrapper around unsafe / الگوی ایمنی

```rust
// unsafe in one place — wrapped in safe API
// unsafe در یک جا — wrap شده در safe API
pub fn read_i32(&self, address: u32) -> Result<i32, MemoryError> {
    if address == 0 {
        return Err(MemoryError::InvalidAddress { address });  // validation
    }
    // unsafe block — only here FFI is called / فقط اینجا FFI صدا زده می‌شود
    let mut buf = MaybeUninit::<T>::uninit();
    let ok = unsafe {
        ReadProcessMemory(self.handle, address as *const _, ...)
    }.is_ok();
    if !ok {
        return Err(MemoryError::ReadFailed { address });  // error handling
    }
    Ok(unsafe { buf.assume_init() })
}
```

**Advantage / مزیت:** `unsafe` is concentrated in one place. The rest of the code (engine, state, overlay logic) uses `read_i32()` safely.

### RAII for Handle

```rust
impl Drop for ProcessHandle {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            let _ = unsafe { CloseHandle(self.handle) };  // cleanup
        }
    }
}
```

Handle is automatically closed — no leak / handle خودکار بسته می‌شود — leak نمی‌دهد.

### Conclusion / نتیجه‌گیری

| Criteria / معیار | Status / وضعیت |
|------------------|----------------|
| Is all code safe? / آیا کل کد safe است؟ | **No / نه** |
| Is unsafe controlled? / آیا unsafe کنترل‌شده است؟ | **Yes** — only in win/ and overlay/ / فقط در win/ و overlay/ |
| Are wrappers safe? / آیا wrapper safe هستند؟ | **Yes** — validation + error handling |
| Memory leak? / آیا memory leak دارد؟ | **No / نه** — RAII for handles |
| Undefined behavior? / آیا undefined behavior دارد؟ | **No / نه** — unsafe is only FFI |

---

*This document complements `README.md` and `CHANGELOG.md`.*
<br>*این مستند مکمل `README.md` و `CHANGELOG.md` است.*
