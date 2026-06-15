# Rust Syntax Guide / راهنمای نحو Rust

This file explains Rust syntax used throughout the CS 1.6 Tool v2 codebase.
<br>این فایل نحو Rust استفاده شده در پروژه CS 1.6 Tool v2 را توضیح می‌دهد.

---

## Table of Contents / فهرست

1. [`&` — References / ارجاعات](#1----references--ارجاعات)
2. [`&mut` — Mutable References / ارجاعات تغییرپذیر](#2---mut----mutable-references--ارجاعات-تغییرپذیر)
3. [`*` — Dereference / آدرس‌گشایی](#3----dereference--آدرس‌گشایی)
4. [`mut` — Mutable Variables / متغیرهای تغییرپذیر](#4----mutable-variables--متغیرهای-تغییرپذیر)
5. [`clone()` — Deep Copy / کپی عمیق](#5----clone----deep-copy--کپی-عمیق)
6. [`_` — Wildcard / کاراکتر جایگزین](#6----_----wildcard--کاراکتر-جایگزین)
7. [`Option<T>` and `Result<T, E>`](#7-optiont-and-resultt-e)
8. [`Arc` and `RwLock` — Thread Safety / ایمنی Thread](#8----arc-and-rwlock----thread-safety--ایمنی-thread)
9. [`unsafe` — Unsafe Code / کد ناامن](#9----unsafe----unsafe-code--کد-ناامن)
10. [`move` — Closure Capture / گرفتن closure](#10----move----closure-capture--گرفتن-closure)
11. [`impl` and `trait` / پیاده‌سازی و ویژگی](#11----impl-and-trait--پیاده‌سازی-و-ویژگی)
12. [`derive` — Auto Traits / ویژگی‌های خودکار](#12----derive----auto-traits--ویژگی‌های-خودکار)
13. [`PhantomData` — Type-Level Marker / نشانگر سطح نوع](#13----phantomdata----type-level-marker--نشانگر-سطح-نوع)

---

## 1. `&` — References / ارجاعات

`&` creates a **shared reference** — it lets you read a value without owning it.
<br>`&` یک **ارجاع مشترک** ایجاد می‌کند — به شما اجازه خواندن مقدار بدون مالکیت آن را می‌دهد.

```rust
fn greet(name: &str) {        // name is a borrowed &str — not owned
    println!("Hello, {name}");
}

let name = String::from("Ali");
greet(&name);                  // pass a reference — name is still valid after
```

**In this project:** `&ProcessHandle`, `&MemoryReader`, `&AppConfig` — all borrow data without taking ownership.
<br>**در این پروژه:** `&ProcessHandle`، `&MemoryReader`، `&AppConfig` — همه داده را قرض می‌گیرند بدون اینکه مالکیت را بگیرند.

---

## 2. `&mut` — Mutable References / ارجاعات تغییرپذیر

`&mut` creates a **mutable reference** — you can both read AND modify the value.
<br>`&mut` یک **ارجاع تغییرپذیر** ایجاد می‌کند — می‌توانید مقدار را هم بخوانید و هم تغییر دهید.

```rust
fn double(x: &mut i32) {      // x is a mutable reference
    *x *= 2;                   // modify the original value through the reference
}

let mut num = 5;
double(&mut num);              // num is now 10
```

**Rule:** You can have EITHER one `&mut` OR many `&` at the same time — never both.
<br>**قانون:** در هر زمان می‌توانید یکی `&mut` یا چندین `&` داشته باشید — هرگز هر دو.

---

## 3. `*` — Dereference / آدرس‌گشایی

`*` accesses the value behind a reference or pointer.
<br>`*` به مقدار پشت ارجاع یا اشاره‌گر دسترسی پیدا می‌کند.

```rust
let x = 42;
let r = &x;          // r is a reference to x
println!("{}", *r);   // dereference r to get the value 42
```

**`*const` / `*mut` in FFI:** Raw pointers used in Win32 API calls — these are inherently unsafe.
<br>**`*const` / `*mut` در FFI:** اشاره‌گرهای خام استفاده شده در فراخوانی‌های Win32 API — اینها ذاتاً ناامن هستند.

---

## 4. `mut` — Mutable Variables / متغیرهای تغییرپذیر

By default, Rust variables are **immutable** (cannot be reassigned). Add `mut` to allow changes.
<br>به‌طور پیش‌فرض، متغیرهای Rust **غیرقابل تغییر** هستند. `mut` اضافه کنید تا تغییرات مجاز شوند.

```rust
let x = 5;          // immutable — cannot change
// x = 10;          // ERROR!

let mut y = 5;      // mutable — can change
y = 10;             // OK!
```

---

## 5. `clone()` — Deep Copy / کپی عمیق

`clone()` creates a **deep copy** of a value. Required when you need an independent copy.
<br>`clone()` یک **کپی عمیق** از مقدار ایجاد می‌کند. وقتی به کپی مستقل نیاز دارید لازم است.

```rust
let a = String::from("hello");
let b = a.clone();   // b is a completely independent copy
// Now both a and b are valid — modifying one doesn't affect the other
```

**In this project:** `state.write() = snap.clone()` — copies GameState so the overlay thread gets its own snapshot.
<br>**در این پروژه:** `state.write() = snap.clone()` — GameState را کپی می‌کند تا thread overlay اسنپ‌شات خودش را داشته باشد.

---

## 6. `_` — Wildcard / کاراکتر جایگزین

`_` has multiple uses:

| Context | Meaning |
|---------|---------|
| `let _ = expr;` | Evaluate but intentionally ignore the result |
| `fn foo(_: i32)` | Accept parameter but don't name it |
| `let (a, _) = (1, 2);` | Destructure but skip this element |
| `_marker: PhantomData` | Field name (convention for unused markers) |

<br>

```rust
let _ = unsafe { CloseHandle(handle) };  // We don't care if CloseHandle fails
let (x, _) = (1, 2);                     // x = 1, we don't need the second value
```

---

## 7. `Option<T>` and `Result<T, E>`

### `Option<T>` — Value may or may not exist
```rust
pub fn find_pid(name: &str) -> Option<u32> {
    // Returns Some(pid) if found, None if not
}
```

### `Result<T, E>` — Operation may succeed or fail
```rust
pub fn read_i32(&self, addr: u32) -> Result<i32, MemoryError> {
    // Returns Ok(value) on success, Err(MemoryError) on failure
}
```

**Common patterns:**
- `.ok()?` — Convert Result to Option, then `?` returns None early
- `.unwrap_or(0)` — Use default value if None
- `.map_err(|e| ...)` — Transform the error type

---

## 8. `Arc` and `RwLock` — Thread Safety / ایمنی Thread

### `Arc<AtomicBool>` — Shared atomic flag
```rust
let running = Arc::new(AtomicBool::new(true));
// Multiple threads can safely read/write this flag
running.store(false, Ordering::SeqCst);  // atomic write
```

### `Arc<RwLock<GameState>>` — Shared mutable state
```rust
// Memory thread writes:
*state.write() = new_game_state;

// Overlay thread reads:
let game = state.read();
println!("{}", game.money);
```

**Why `RwLock`?** Multiple threads can read simultaneously, but only one can write at a time.
<br>**چرا `RwLock`؟** چندین thread می‌توانند همزمان بخوانند، اما فقط یکی در هر زمان می‌تواند بنویسد.

---

## 9. `unsafe` — Unsafe Code / کد ناامن

`unsafe` marks code that the compiler cannot fully verify. Common reasons in this project:

- **FFI calls** to Win32 APIs (ReadProcessMemory, WriteProcessMemory, etc.)
- **Raw pointer dereference** (`*const u8`, `*mut u8`)
- **Calling unsafe functions** from the `windows` crate

```rust
// SAFETY: ReadProcessMemory wrote exactly size_of::<T>() bytes successfully
Ok(unsafe { buf.assume_init() })
```

**Every `unsafe` block in this project is wrapped in a safe public function with proper error handling.**
<br>**هر بلوک `unsafe` در این پروژه در یک تابع عمومی امن با مدیریت خطای مناسب پیچیده شده.**

---

## 10. `move` — Closure Capture / گرفتن closure

`move` forces a closure to take ownership of captured variables (required for `thread::spawn`).
<br>`move` مجبور می‌کند closure مالکیت متغیرهای گرفته شده را بگیرد (برای `thread::spawn` لازم است).

```rust
let data = Arc::new(AtomicBool::new(true));
let handle = thread::spawn(move || {
    // `data` is moved into this closure — original can't use it anymore
    data.store(false, Ordering::SeqCst);
});
```

---

## 11. `impl` and `trait` / پیاده‌سازی و ویژگی

### `impl` — Add methods to a type
```rust
impl ProcessHandle {
    pub fn attach(name: &str) -> Result<Self, MemoryError> { ... }
    pub fn pid(&self) -> u32 { ... }
}
```

### `trait` — Define shared behavior
```rust
trait AsciiLower {
    fn to_ascii_lowercase(self) -> u16;
}
impl AsciiLower for u16 { ... }
```

---

## 12. `derive` — Auto Traits / ویژگی‌های خودکار

`derive` automatically implements common traits:

| Derive | Purpose |
|--------|---------|
| `Debug` | `println!("{:?}", value)` formatting |
| `Clone` | `value.clone()` deep copy |
| `Copy` | Implicit copy for small types (i32, f32, bool) |
| `Default` | `T::default()` creates default value |
| `Deserialize` | TOML/JSON parsing via serde |
| `Error` | Display + error chain via thiserror |

```rust
#[derive(Debug, Clone, Copy, Default)]
pub struct GameRect { pub x: i32, pub y: i32, ... }
```

---

## 13. `PhantomData` — Type-Level Marker / نشانگر سطح نوع

`PhantomData<T>` tells the compiler "this type logically owns a `T`" without actually storing one.
<br>`PhantomData<T>` به کامپایلر می‌گوید "این نوع منطقاً مالک یک `T` است" بدون ذخیره واقعی.

```rust
pub struct MemoryReader<'a> {
    handle: HANDLE,
    _marker: PhantomData<&'a ProcessHandle>,  // "this reader borrows from ProcessHandle"
}
```

**Purpose:** Enforces that `MemoryReader` cannot outlive the `ProcessHandle` it reads from.
<br>**هدف:** تضمین می‌کند که `MemoryReader` نمی‌تواند بیشتر از `ProcessHandle`‌ای که از آن می‌خواند عمر کند.

---

*This file is part of the CS 1.6 Tool v2 documentation.*
<br>*این فایل بخشی از مستندات CS 1.6 Tool v2 است.*
