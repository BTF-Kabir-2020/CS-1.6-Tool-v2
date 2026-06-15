# Changelog — CS 1.6 Tool v2

---

## v2.0.0 — June 2026

### Complete Rewrite in Rust / بازنویسی کامل با Rust

- Rewritten from C++ to Rust / بازنویسی از C++ به Rust
- Modular architecture (win/, game/, overlay/, debug/) / معماری ماژولار
- 3 threads: Memory, Overlay, Main / ۳ thread: Memory, Overlay, Main

### New Features / ویژگی‌های جدید

- **Local Player Entity** — Read money/hp/armor from player entity / خواندن money/hp/armor از entity بازیکن
- **Multi-strategy resolve** — entity → hw chain → client direct / چند استراتژی resolve
- **Re-resolve every tick** — Chains are resolved each frame / chainها هر فریم دوباره resolve می‌شوند
- **Auto reconnect** — Disconnection / base change / Reconnect خودکار — قطع اتصال / تغییر base
- **sw.dll fallback** — Software renderer support / پشتیبانی از software renderer
- **Transparent overlay** — GDI + LWA_COLORKEY / Overlay شفاف
- **Debug Console** — ANSI terminal with live info / کنسول دیباگ با اطلاعات live
- **TOML Config** — Flexible configuration / پیکربندی انعطاف‌پذیر
- **CLI flags** — --read-only, --no-overlay, --debug

### Bug Fixes / رفع باگ‌ها

- **Overlay refresh** — Replaced `SendMessageW(WM_PAINT)` with `InvalidateRect`
  - Problem: overlay showed once but didn't update after / مشکل: overlay بار اول نمایش داده می‌شد ولی بعد از آن آپدیت نمی‌شد
  - Cause: `WM_PAINT` should not be sent with `SendMessage` (Microsoft docs) / علت: `WM_PAINT` نباید با `SendMessage` ارسال شود
  - Fix: `InvalidateRect` → `PeekMessage` → standard `WM_PAINT` / فیکس: `InvalidateRect` → `PeekMessage` → `WM_PAINT` استاندارد
- **Cache state** — Invalid values show `--` not `0` / مقدار نامعتبر نمایش داده نمی‌شود (نه `0` بلکه `--`)
- **Unified format_status** — Console and overlay use one formatter / کنسول و overlay از یک formatter استفاده می‌کنند

---

## Roadmap / نقشه راه

- [ ] Verify offsets with CE + ReClass + x32dbg / تأیید offsetها
- [ ] HP/Armor offset in config
- [ ] Weapon Guard for accurate infinite / Weapon Guard برای infinite دقیق
- [ ] Improved HUD sync / HUD sync بهبود یافته

---
