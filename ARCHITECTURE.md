<div dir="rtl" align="right">

# معماری CS 1.6 Tool v2

**آخرین به‌روزرسانی:** ژوئن ۲۰۲۶

---

## فهرست

1. [نمای کلی](#۱-نمای-کلی)
2. [ساختار پوشه](#۲-ساختار-پوشه)
3. [thread مدل](#۳-thread-مدل)
4. [ماژول win/ — دسترسی به حافظه](#۴-ماژول-win)
5. [ماژول game/ — موتور بازی](#۵-ماژول-game)
6. [ماژول overlay/ — رندر روی پنجره بازی](#۶-ماژول-overlay)
7. [ماژول debug/ — کنسول دیباگ](#۷-ماژول-debug)
8. [ماژول config/ — پیکربندی](#۸-ماژول-config)
9. [جریان داده](#۹-جریان-داده)
10. [مدیریت state](#۱۰-مدیریت-state)
11. [_resolve chain_ — الگوریتم](#۱۱-resolve-chain)
12. [Local Player Discovery](#۱۲-local-player-discovery)
13. [Position Discovery](#۱۳-position-discovery)
14. [Overlay Rendering](#۱۴-overlay-rendering)
15. [Window Management](#۱۵-window-management)
16. [Error Handling](#۱۶-error-handling)
17. [وابستگی‌ها](#۱۷-وابستگی‌ها)
18. [ایمنی حافظه — Safe vs Unsafe](#۱۸-ایمنی-حافظه)

---

## ۱. نمای کلی

ابزار External Memory برای Counter-Strike 1.6 (GoldSrc Engine) — بازنویسی کامل با Rust.

**ویژگی‌های کلیدی:**
- خواندن/نوشتن حافظه `hl.exe` از بیرون (External)
- چندین استراتژی resolve آدرس (Entity, Chain, Direct)
- Overlay شفاف روی پنجره بازی (GDI + LWA_COLORKEY)
- Reconnect خودکار
- پشتیبانی از hw.dll و sw.dll (Software Renderer)

---

## ۲. ساختار پوشه

```
cs16-tool-v2/
├── Cargo.toml              # وابستگی‌ها و profile
├── config.toml             # پیکربندی کاربر
├── README.md               # شروع سریع
├── ARCHITECTURE.md         # این فایل
└── src/
    ├── main.rs             # Entry point — thread orchestration
    ├── lib.rs              # ماژول‌های pub
    ├── config.rs           # AppConfig + parse helpers
    ├── error.rs            # AppError + MemoryError
    ├── bin/
    │   └── dump.rs         # CLI دوم: dump آدرس‌ها
    ├── win/
    │   ├── mod.rs          # Re-exports
    │   ├── process.rs      # ProcessHandle, find_pid, module_base
    │   ├── memory.rs       # MemoryReader, MemoryWriter, resolve_chain
    │   └── window.rs       # find_game_window, get_game_rect
    ├── game/
    │   ├── mod.rs          # Re-exports
    │   ├── engine.rs       # GameEngine — منطق اصلی tick
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

## ۳. thread مدل

پروژه از **۳ thread** استفاده می‌کند:

```
┌──────────────────────────────────────────────────────────────┐
│  Main Thread                                                 │
│  • کلیدها: Q/End=خروج, Insert=toggle overlay, F7=debug      │
│  • چاپ status در کنسول (هر 50ms)                            │
│  • کنترل lifecycle                                           │
├──────────────────────────────────────────────────────────────┤
│  Memory Thread (memory_loop)                                 │
│  • هر N ms: connect → tick → write GameState                 │
│  • Reconnect خودکار                                          │
│  • resolve chainها هر tick                                    │
├──────────────────────────────────────────────────────────────┤
│  Overlay Thread (overlay::run)                               │
│  • ~60 FPS: InvalidateRect → WM_PAINT → GDI draw            │
│  • sync موقعیت با پنجره بازی                                 │
│  • PeekMessage loop                                          │
└──────────────────────────────────────────────────────────────┘
```

**ارتباط threadها:**
- `Arc<RwLock<GameState>>` — بین memory و overlay مشترک
- `Arc<AtomicU32>` — PID store
- `Arc<AtomicBool>` — running flag + debug_on flag
- `crossbeam_channel` — دستورات overlay (toggle visibility, shutdown)

---

## ۴. ماژول win/

### win/process.rs

**`ProcessHandle`** — مدیریت handle باز به پروسس:
```rust
pub struct ProcessHandle {
    handle: HANDLE,
    pid: u32,
}
```

- `attach(name)` → OpenProcess با VM_READ | VM_WRITE | VM_OPERATION | QUERY_INFORMATION
- `module_base(name)` → اسکن ماژول‌ها با CreateToolhelp32Snapshot
- Drop → CloseHandle (RAII)

**`find_pid(name)`** → اسکن پروسس‌ها با PROCESSENTRY32W

**`engine_base(process, modules)`** → اول hw.dll، بعد sw.dll (fallback)

### win/memory.rs

**`MemoryReader`** — خواندن حافظه:
```rust
pub fn read<T: Copy>(&self, address: u32) -> Result<T, MemoryError>
pub fn read_i32 / read_u32 / read_f32
```

**`MemoryWriter`** — نوشتن حافظه:
```rust
pub fn write<T: Copy>(&self, address: u32, value: T) -> Result<(), MemoryError>
pub fn write_i32 / write_f32
```

**`resolve_chain(process, base_ptr, offsets)`** → الگوریتم pointer chain:
```
addr = base_ptr
for offset in offsets:
    addr = read_u32(addr) + offset
return addr
```

### win/window.rs

**`find_game_window(pid)`** → پیدا کردن HWND بازی:
1. ابتدا کلاس `Valve001`
2. عنوان: `Counter-Strike`, `Half-Life`, `Condition Zero`
3. Fallback: بزرگ‌ترین پنجره visible

**Cache:** TTL 800ms برای جلوگیری از اسکن مکرر

**`get_game_rect(hwnd)`** → GetClientRect + ClientToScreen

---

## ۵. ماژول game/

### game/engine.rs — GameEngine

هسته اصلی برنامه. هر tick:

```
tick() → GameState
├── refresh_bases()          # بررسی تغییر hw_base / client_base
├── refresh_money_hw_if_needed()
├── refresh_reserve_if_needed()
├── discover_local_player_if_needed()
├── read_vitals()            # HP + Armor
├── read_money()             # 3 استراتژی: hw chain → client direct → entity
├── read_clip()              # چند chain + pick_best
├── read_reserve()           # یک chain
├── read_position_values()   # 6 استراتژی مختلف
├── read_view_aux()          # H / mouse
├── merge_cache()            # cache مقدارها
├── check_ready()            # آیا همه فیلدها OK هستند
└── return GameState
```

**اولویت read پول:**
1. `hw.dll + RVA` → pointer chain → money
2. `client.dll + direct_rva` → مستقیم
3. `entity + money_offset` → از local player

**اولویت read clip:**
1. همه chainهای `[[chains.clip]]` → بهترین مقدار معتبر
2. اگر هیچکدام → reserve chain (clip[2] در some builds)

### game/state.rs

**`GameState`** — وضعیت لحظه‌ای بازی:
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

**`format_status()`** — formatter واحد برای کنسول + overlay:
- `connected=false` → `[منتظر بازی...]`
- `ready=false` → `[در حال خواندن...]`
- فیلد نامعتبر → `--` (نه `0`)

### game/local_player.rs

**`discover()`** — پیدا کردن Local Player pointer:
1. ابتدا config RVA
2. لیست شناخته‌شده (LP_RVA_HW / LP_RVA_CLIENT)
3. Scan محدود ماژول (0x100000..0x600000)

**Scoring:** HP range + armor valid + from_known_rva bonus

### game/position.rs

**۶ استراتژی برای خواندن مختصات:**

| # | استراتژی | توضیح |
|---|---------|-------|
| 1 | `locked_pos_player` | آدرس قفل‌شده از قبل |
| 2 | `position_global_hw_rva` | vec3 مستقیم در hw.dll |
| 3 | `resolve_hw_local_player_position` | hw entity → pev → origin |
| 4 | `read_hw_entity_world_origin` | hw entity fallback |
| 5 | `discovered_*` | کش discovery قبلی |
| 6 | `player + offset` | client entity |

**فیلترها:**
- `looks_like_coords()` — محدوده نقشه
- `looks_like_world_origin()` — نه view aux
- `looks_like_spawn_stub()` — رد کردن spawn point
- `is_usable_position()` — ترکیب همه فیلترها

---

## ۶. ماژول overlay/

### overlay/overlay.rs

**OverlayHandle** — مدیریت overlay thread:
```rust
pub struct OverlayHandle {
    tx: Sender<Cmd>,
    visible: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}
```

**پنجره:** Win32 WS_POPUP + WS_EX_LAYERED + WS_EX_TRANSPARENT + WS_EX_TOPMOST

**رندر:** GDI — CreateFontW + TextOutW + FillRect

**مکانیسم repaint:**
```rust
fn repaint(hwnd: HWND) {
    unsafe { InvalidateRect(hwnd, None, false); }
}
```

**WM_PAINT handler:**
1. BeginPaint
2. FillRect با COLORKEY (0xFF00FF — magenta شفاف)
3. build_lines() → خواندن GameState
4. draw() → رسم هر خط با رنگ و سایه
5. EndPaint

**位置 sync:**
- هر فریم: find_game_window → get_game_rect → SetWindowPos
- HWND_TOPMOST برای ماندن روی بازی

---

## ۷. ماژول debug/

**DebugConsole** — چاپ ANSI در ترمینال:
- `\x1b[2J\x1b[H` → پاک کردن صفحه
- اطلاعات: tick, ready, money, clip, reserve, write_enabled, alive
- Interval قابل تنظیم در config

---

## ۸. ماژول config/

**`AppConfig`** — ساختار اصلی:
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

**`parse_hex_u32()`** → تبدیل "0x7AF55C" به u32

**`parse_offsets()`** → تبدیل Vec<String> به Vec<u32>

---

## ۹. جریان داده

```
config.toml
    │
    ▼
AppConfig (parse once)
    │
    ├──► GameEngine::open()
    │       ├── ProcessHandle::attach()
    │       ├── engine_base() → hw.dll / sw.dll
    │       ├── resolve_static_addresses()
    │       └── discover_local_player_if_needed()
    │
    ▼
memory_loop (هر N ms)
    │
    ├──► eng.tick() → GameState
    │       ├── read money (3 استراتژی)
    │       ├── read clip (چند chain)
    │       ├── read reserve
    │       ├── read hp/armor
    │       ├── read position (6 استراتژی)
    │       └── merge_cache → GameState
    │
    ├──► *state.write() = snap
    │
    ▼
Overlay Thread (هر 16ms)
    │
    ├──► state.read() → GameState
    ├──► build_lines() → Vec<Line>
    └──► draw() → GDI TextOutW
```

---

## ۱۰. مدیریت state

**`Arc<RwLock<GameState>>`** — مشترک بین memory + overlay

- Memory thread: `*state.write() = snap.clone()`
- Overlay thread: `state.read()` → `build_lines()` → `draw()`
- Main thread: `state.read()` → `format_status()` → console

**`Cache`** در GameEngine:
```rust
struct Cache {
    money: i32, money_valid: bool,
    clip: i32, clip_valid: bool,
    reserve: i32, reserve_valid: bool,
}
```
- فقط مقدارهای موفق cache می‌شوند
- tick بد → مقدار قبلی حفظ می‌شود

**`display_ready`** → یکبار true شد، دیگر false نمی‌شود

---

## ۱۱. _resolve chain_ — الگوریتم

```rust
fn resolve_chain(process, base_ptr, offsets) -> Result<u32> {
    let mut addr = base_ptr;
    for (step, &offset) in offsets.iter().enumerate() {
        let next = read_u32(addr)?;  // خواندن pointer
        if next == 0 { return Err(ChainBroken); }
        addr = next + offset;        // اضافه کردن offset
    }
    Ok(addr)
}
```

**مثال:**
```
hw.dll + 0x6E92AC
  → [+0x1CC] → [+0x320] → [+0x4] → [+0x7C] → [+0x21C]
  → Money address
```

**هر tick resolve می‌شود** — نه یکبار. این باعث می‌شود بعد از تغییر base دوباره کار کند.

---

## ۱۲. Local Player Discovery

**هدف:** پیدا کردن آدرس entity بازیکن فعلی

**روش:**
1. config RVA → `hw.dll + rva` → read pointer → player entity
2. لیست شناخته‌شده RVAها
3. Scan 0x100000..0x600000 با step=4

**HP verify:** `(0..=100).contains(&hp)` → اگر HP معتبر بود → player درست

**Scoring:**
- from_known_rva: +1000
- hp_off مطابق config: +500
- armor valid: +100
- hp value: +hp

---

## ۱۳. Position Discovery

**پیچیده‌ترین بخش** — ۶ استراتژی + فیلترهای متعدد

**فیلترهای vec3:**
- `looks_like_coords()` — محدوده نقشه (<=16384)
- `looks_like_world_origin()` — نه view aux، هر دو محور افقی معنی‌دار
- `looks_like_view_aux()` — الگوی (164, 1, 140)
- `looks_like_spawn_stub()` — (0, 300, 0)
- `is_usable_position()` — ترکیب نهایی

**Discovery live:**
1. collect_all_movement_snaps() — نمونه‌گیری اول
2. sleep(wait_ms)
3. pick_best_mover() — مقایسه دو نمونه
4. اگر نشد → discover_by_changing_floats()

---

## ۱۴. Overlay Rendering

**رندر GDI ساده:**
1. `InvalidateRect` → علامت‌گذاری پنجره به عنوان dirty
2. `PeekMessage` → تولید WM_PAINT
3. `wnd_proc(WM_PAINT)`:
   - `BeginPaint` → HDC
   - `FillRect` با magenta (COLORKEY)
   - `CreateFontW` → فونت
   - برای هر خط: `SetTextColor` + `TextOutW` (سایه + اصلی)
   - `DeleteObject(font)` → آزادسازی

**ویژگی‌ها:**
- لایه شفاف (LWA_COLORKEY)
- روی همه پنجره‌ها (WS_EX_TOPMOST)
- بدون تأثیر روی کلیک (WS_EX_TRANSPARENT)
- بدون taskbar (WS_EX_TOOLWINDOW)

---

## ۱۵. Window Management

**`find_game_window(pid)`:**
1. کلاس `Valve001` (SDK استاندارد)
2. عنوان: Counter-Strike / Half-Life / Condition Zero
3. بزرگ‌ترین پنجره visible

**Cache:** Mutex + TTL 800ms

**`get_game_rect(hwnd)`:**
1. GetClientRect → ابعاد client area
2. ClientToScreen → مختصات صفحه
3. Fallback: GetWindowRect

---

## ۱۶. Error Handling

**`AppError`** — خطاهای سطح بالا:
```rust
pub enum AppError {
    Config(String),
    Memory(MemoryError),
    Other(anyhow::Error),
}
```

**`MemoryError`** — خطاهای حافظه:
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

**Reconnect logic:**
- `should_reconnect()` → !is_alive || !engine_base || stale >= threshold
- stale counter: هر tick بدون read موفق +1
- بعد از reconnect: pid_store=0, invalidate_window_cache, state=default

---

## ۱۷. وابستگی‌ها

| Crate | نسخه | کاربرد |
|-------|------|--------|
| `windows` | 0.58 | Win32 API (RPM/WPM, GDI, Window) |
| `clap` | 4 | CLI argument parsing |
| `parking_lot` | 0.12 | RwLock سریع‌تر از std |
| `crossbeam-channel` | 0.5 | ارتباط thread overlay |
| `tracing` | 0.1 | Logging |
| `tracing-subscriber` | 0.3 | Console logging |
| `serde` | 1 | Deserialize config |
| `toml` | 0.8 | Parse TOML |
| `thiserror` | 1 | Error derive |
| `anyhow` | 1 | Error wrapping |

---

## ۱۸. ایمنی حافظه — Safe vs Unsafe

### خلاصه

سورس کامل **unsafe** است. تقریباً همه عملیات حافظه و Win32 API داخل `unsafe` block هستند. فقط wrapperهای Rust دورشون safe هستند.

### چرا unsafe لازم است؟

CS 1.6 Tool یک **External Memory Tool** است — یعنی از بیرون پروسس بازی حافظه را می‌خواند/می‌نویسد. این کار فقط از طریق Win32 API امکان‌پذیر است و همه Win32 FFI functions در Rust **unsafe** هستند:

```rust
// unsafe چون: FFI call به Windows kernel — تضمینی برای valid handle/addr نیست
ReadProcessMemory(handle, addr, buf, size, None)
WriteProcessMemory(handle, addr, buf, size, None)
OpenProcess(permissions, false, pid)
```

### نقشه unsafe در کد

| فایل | unsafe علت | مثال |
|------|------------|------|
| `win/memory.rs` | FFI → RPM/WPM | `ReadProcessMemory`, `WriteProcessMemory` |
| `win/process.rs` | FFI → Process API | `OpenProcess`, `CreateToolhelp32Snapshot`, `Module32FirstW` |
| `win/window.rs` | FFI → Window API | `EnumWindows`, `FindWindowW`, `GetClientRect` |
| `overlay/overlay.rs` | FFI → Win32 Window + GDI | `CreateWindowExW`, `TextOutW`, `BeginPaint`, `PeekMessageW` |
| `game/engine.rs` | غیرمستقیم (از طریق MemoryReader) | — |
| `game/position.rs` | غیرمستقیم (از طریق MemoryReader) | — |
| `game/local_player.rs` | غیرمستقیم (از طریق MemoryReader) | — |

### چه بخش‌هایی safe هستند؟

| فایل | چرا safe است |
|------|-------------|
| `config.rs` | Pure Rust — serde + TOML parse |
| `error.rs` | Pure Rust — thiserror derive |
| `game/state.rs` | Pure Rust — data structures + format |
| `debug/mod.rs` | Pure Rust — println + ANSI |
| `main.rs` | Safe — thread spawn + AtomicBool + channel |

### الگوی ایمنی: Wrapper around unsafe

```rust
// unsafe در یک جا — wrap شده در safe API
pub fn read_i32(&self, address: u32) -> Result<i32, MemoryError> {
    if address == 0 {
        return Err(MemoryError::InvalidAddress { address });  // validation
    }
    // unsafe block — فقط اینجا FFI صدا زده می‌شود
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

**مزیت:** `unsafe` فقط در یک جا متمرکز است. بقیه کد (engine, state, overlay logic) از `read_i32()` safe استفاده می‌کند.

### RAII برای Handle

```rust
impl Drop for ProcessHandle {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            let _ = unsafe { CloseHandle(self.handle) };  // cleanup
        }
    }
}
```

handle خودکار بسته می‌شود — leak نمی‌دهد.

### نتیجه‌گیری

| معیار | وضعیت |
|-------|--------|
| آیا کل کد safe است؟ | **نه** |
| آیا unsafe کنترل‌شده است؟ | **بله** — فقط در win/ و overlay/ |
| آیا wrapper safe هستند؟ | **بله** — validation + error handling |
| آیا memory leak دارد؟ | **نه** — RAII برای handleها |
| آیا undefined behavior دارد؟ | **نه** — unsafe فقط FFI است |

---

*این مستند مکمل `README.md` و `پیشرفت-CS16.md` است.*

</div>
