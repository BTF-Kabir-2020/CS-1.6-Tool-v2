<div dir="rtl" align="right">

# Changelog — CS 1.6 Tool v2

---

## v2.0.0 — ژوئن ۲۰۲۶

### بازنویسی کامل با Rust

- بازنویسی از C++ به Rust
- معماری ماژولار (win/, game/, overlay/, debug/)
- ۳ thread: Memory, Overlay, Main

### ویژگی‌های جدید

- **Local Player Entity** — خواندن money/hp/armor از entity بازیکن
- **چند استراتژی resolve** — entity → hw chain → client direct
- **Re-resolve هر tick** — chainها هر فریم دوباره resolve می‌شوند
- **Reconnect خودکار** — قطع اتصال / تغییر base
- **sw.dll fallback** — Software renderer support
- **Overlay شفاف** — GDI + LWA_COLORKEY
- **Debug Console** — ANSI terminal با اطلاعات live
- **Config TOML** — پیکربندی انعطاف‌پذیر
- **CLI flags** — --read-only, --no-overlay, --debug

### رفع باگ‌ها

- **Overlay refresh** — جایگزینی `SendMessageW(WM_PAINT)` با `InvalidateRect`
  - مشکل: overlay بار اول نمایش داده می‌شد ولی بعد از آن آپدیت نمی‌شد
  - علت: `WM_PAINT` نباید با `SendMessage` ارسال شود (مستندات مایکروسافت)
  - فیکس: `InvalidateRect` → `PeekMessage` → `WM_PAINT` استاندارد
- **Cache state** — مقدار نامعتبر نمایش داده نمی‌شود (نه `0` بلکه `--`)
- **format_status واحد** — کنسول و overlay از یک formatter استفاده می‌کنند

---

## نقشه راه

- [ ] تأیید offsetها با CE + ReClass + x32dbg
- [ ] HP/Armor offset در config
- [ ] Weapon Guard برای infinite دقیق
- [ ] HUD sync بهبود یافته

---

</div>
