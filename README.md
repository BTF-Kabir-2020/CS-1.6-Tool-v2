<div dir="rtl" align="right">

# CS 1.6 Tool v2 (Rust)

بازنویسی کامل پروژه `cs16-rust` — با معماری تمیزتر و روش‌های متعدد خواندن حافظه.

---

## شروع سریع

```powershell
# PowerShell را Run as Administrator باز کن
cd cs16-tool-v2
cargo run --release
```

1. CS 1.6 را باز کن
2. **وارد match شو** (New Game / سرور)
3. overlay و کنسول باید پول/ammo را نشان دهند

---

## ویژگی‌ها

| بخش | وضعیت | توضیح |
|-----|--------|--------|
| اتصال External به `hl.exe` | ✅ | OpenProcess + Toolhelp32 |
| خواندن/نوشتن حافظه | ✅ | ReadProcessMemory / WriteProcessMemory |
| پول (hw + client + entity) | ✅ | ۳ استراتژی fallback |
| Clip / Reserve | ✅ | چند chain + pick_best |
| HP / Armor | ⚙️ | offset دستی در config |
| Game Overlay | ✅ | Win32 GDI شفاف |
| Position (XYZ) | ⚙️ | ۶ استراتژی discovery |
| Reconnect خودکار | ✅ | قطع اتصال / تغییر base |
| sw.dll fallback | ✅ | Software renderer |

---

## Offsetها

**منبع اصلی:** `1_cs16` / `2_cs16` — همان chainهای تست‌شده شما.

| فیلد | RVA / Chain |
|------|-------------|
| Reserve | `hw+0x7AF55C` → `74,5C0,A4,5E8` |
| Money hw | `hw+0x6E92AC` → `1CC,320,4,7C,21C` |
| Money client | `client+0x1213C4` |
| Clip ×3 | همان `CLIP_CHAINS` در C++ |

Targets: **15000 / 15 / 20** — مثل `DESIRED_*` در C++.

---

## روش خواندن پول (اولویت)

1. `hw.dll + RVA` → Local Player → `+0xE4` (money)
2. Pointer chain در `[chains.money_hw]`
3. `client.dll + direct_rva` fallback
4. `entity + money_offset` از local player

---

## ساختار پروژه

```
src/
├── main.rs           # Entry point — ۳ thread
├── config.rs         # AppConfig + parse helpers
├── error.rs          # AppError + MemoryError
├── win/
│   ├── process.rs    # ProcessHandle, find_pid
│   ├── memory.rs     # MemoryReader/Writer, resolve_chain
│   └── window.rs     # find_game_window, get_game_rect
├── game/
│   ├── engine.rs     # GameEngine — منطق اصلی
│   ├── state.rs      # GameState, format_status
│   ├── local_player.rs # Local Player discovery
│   └── position.rs   # Position (vec3) discovery
├── overlay/
│   └── overlay.rs    # Win32 GDI overlay
└── debug/
    └── mod.rs        # DebugConsole
```

**مدل thread:**
- **Memory Thread** — هر N ms: read/write + GameState
- **Overlay Thread** — ~60 FPS، GDI روی پنجره بازی
- **Main Thread** — کلیدها + console status

---

## کلیدها

| کلید | عمل |
|------|-----|
| Q / End | خروج |
| Insert | نمایش/مخفی overlay |
| F7 | toggle debug console |

---

## CLI

```powershell
# حالت عادی
cargo run --release

# فقط خواندن (بدون نوشتن)
cargo run --release -- --read-only

# بدون overlay
cargo run --release -- --no-overlay

# دیباگ کامل
cargo run --release -- --debug

# config سفارشی
cargo run --release -- -c my.toml
```

---

## config.toml

```toml
[process]
name = "hl.exe"

[modules]
hw = "hw.dll"
sw = "sw.dll"
client = "client.dll"

[targets]
money = 15000
clip = 15
reserve = 20

[features]
write_enabled = true
overlay_enabled = true
money_enabled = true
clip_enabled = true
clip_write_enabled = false
reserve_enabled = true
reserve_write_enabled = false
hp_enabled = true
armor_enabled = true
position_enabled = true

[timing]
memory_loop_ms = 50
overlay_loop_ms = 16
connect_retry_ms = 250
stale_reconnect_ticks = 40

[entity]
enabled = true
local_player_rva = "0x32ABF4"
money_offset = "0xE4"
health_offset = "0xB74"
armor_offset = "0x10C"
health_type = "float"
armor_type = "float"
position_offset = "0x8"

[overlay]
offset_x = 12
offset_y = 12
font_size = 18
font_name = "Consolas"
font_bold = true
line_spacing = 22
position = "top-left"
margin = 12

[overlay.colors]
money = "0x00D7FF"
ammo = "0x00FF00"
hp = "0x4444FF"
armor = "0xFFAA00"
position = "0xFFFF88"
default = "0x00FF00"

[overlay.display]
show_money = true
show_ammo = true
show_hp = true
show_armor = true
show_position = true
show_view_aux = true

[clip_detection]
min_value = 0
max_value = 150
```

---

## وابستگی‌ها

| Crate | کاربرد |
|-------|--------|
| `windows` 0.58 | Win32 API |
| `clap` 4 | CLI |
| `parking_lot` 0.12 | RwLock |
| `crossbeam-channel` 0.5 | Channel |
| `tracing` | Logging |
| `serde` + `toml` | Config |
| `thiserror` + `anyhow` | Errors |

---

## ساخت

```powershell
# Release (توصیه شده)
cargo build --release

# Debug
cargo build

# تست
cargo test
```

---

## منابع اینترنتی

- [UnknownCheats — CS1.6 Finding Offsets](https://www.unknowncheats.me/forum/counterstrike-1-5-1-6-and-mods/125661-cs1-6-finding-offsets.html)
- [BLASTHACK CS 1.6 Dumper](https://www.blast.hk/threads/225183/)
- [ReadProcessMemory WinAPI](https://codingvision.net/c-read-write-another-process-memory)
- [ReClass.NET](https://github.com/ReClassNET/ReClass.NET)
- [x64dbg / x32dbg](https://x64dbg.com/)

---

## مستندات مرتبط

| فایل | محتوا |
|------|--------|
| `ARCHITECTURE.md` | معماری کامل + flow |
| [`../پیشرفت-CS16.md`](../پیشرفت-CS16.md) | گزارش پیشرفت |
| [`../آموزش-پیدا-کردن-Chain-و-Offset-از-صفر.md`](../آموزش-پیدا-کردن-Chain-و-Offset-از-صفر.md) | آموزش CE + ReClass + x32dbg |

---

---

## ⚠️ Disclaimer / سلب مسئولیت

> **EN:** This software is provided "AS IS" for **educational and research purposes only**.
> The author(s) are **NOT responsible** for any damage, data loss, hardware failure,
> legal consequences, or misuse of this software. Use at your own risk.
> Commercial use, redistribution for profit, and claiming authorship are strictly prohibited.
> See [LICENSE](LICENSE) for full terms.

> **FA:** این نرم‌افزار **فقط برای اهداف آموزشی و تحقیقاتی** ارائه شده است.
> نویسنده(گان) در قبال **هیچگونه خسارت، از بین رفتن داده، خرابی سخت‌افزار،
> پیامدهای قانونی یا سوءاستفاده** از این نرم‌افزار مسئولیتی ندارند.
> استفاده از این نرم‌افزار به عهده خود شماست.
> استفاده تجاری، توزیع برای کسب درآمد و ادعای نویسندگی کاملاً ممنوع است.
> شرایط کامل در [LICENSE](LICENSE) موجود است.

---

## License

**Non-Commercial License** — فقط آموزشی. مشاهده [LICENSE](LICENSE).

</div>
