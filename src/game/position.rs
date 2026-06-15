//! Player position / coordinate reading and discovery module
//! ماژول خواندن و کشف مختصات بازیکن
//!
//! [EN] This module reads the player's world position (vec3 origin) from either
//! the game's entity structure or global RVAs in hw.dll / client.dll.
//! [FA] این ماژول موقعیت جهانی بازیکن (مبدأ vec3) را از ساختار entity بازی
//! یا آدرس‌های سراسری (RVA) در hw.dll / client.dll می‌خواند.
//!
//! [EN] Client entity typically contains only health (HP); the real world origin
//! is usually located via hw.dll offsets (entity pointer or global RVA).
//! [FA] entity معمولاً فقط HP دارد؛ مبدأ واقعی جهان معمولاً از طریق
//! آفست‌های hw.dll (اشاره‌گر entity یا آدرس سراسری) قابل دسترسی است.

use std::thread;
use std::time::Duration;

use crate::win::memory::MemoryReader;

/// [EN] Known offsets for m_vecOrigin / entvars.origin across various GoldSrc builds
/// [FA] آفست‌های شناخته‌شده m_vecOrigin / entvars.origin در buildهای مختلف GoldSrc
///
/// [EN] These offsets represent where the player position vector is stored relative
/// to the entity base address. Different game versions may use different offsets.
/// [FA] این آفست‌ها نشان‌دهنده محل ذخیره بردار موقعیت بازیکن نسبت به آدرس پایه entity هستند.
/// نسخه‌های مختلف بازی ممکن است از آفست‌های متفاوتی استفاده کنند.
pub const POS_OFFSET_CANDIDATES: &[u32] = &[
    0x8, 0x14, 0x20, // entvars: origin, oldorigin, velocity
    0x34, 0x38, 0x3C, 0x40, 0x44, 0x48, 0x128, 0x12C, 0x130, 0x134, 0x138, 0x13C, 0x140, 0x1A4,
    0x204, 0x334,
];

/// [EN] Known health (HP) offsets in hw.dll for different builds
/// [FA] آفست‌های شناخته‌شده سلامتی (HP) در hw.dll برای buildهای مختلف
const HP_OFFS_HW: &[u32] = &[0x59C, 0x334, 0x100, 0xB74, 0xFC, 0x14];

/// [EN] hw.dll RVAs — world entity pointer (priority given to 0x169438)
/// [FA] آدرس‌های سراسری hw.dll — اشاره‌گر entity جهان (اولویت با 0x169438)
///
/// [EN] These are Relative Virtual Addresses in hw.dll where the player entity
/// pointer can be found. The first entry (0x169438) is the most common across builds.
/// [FA] اینها آدرس‌های نسبی مجازی در hw.dll هستند که اشاره‌گر entity بازیکن
/// در آنها یافت می‌شود. مقدار اول (0x169438) رایج‌ترین مقدار در بین buildها است.
const LP_RVA_HW_FOR_POS: &[u32] = &[0x00169438, 0x0010FC80, 0x00176C68, 0x001694F0, 0x0013FDF4];

/// [EN] Direct vec3 positions in hw.dll — known builds (e.g., 8684: EntityOrigin)
/// [FA] بردار vec3 مستقیم در hw.dll — buildهای شناخته‌شده (مثلاً 8684: EntityOrigin)
///
/// [EN] These RVAs point directly to vec3 position data in hw.dll. The first entry
/// (0x7CD13C) works with non-Steam builds for dump movement functionality.
/// [FA] این آدرس‌های سراسری مستقیماً به داده بردار vec3 در hw.dll اشاره می‌کنند.
/// مقدار اول (0x7CD13C) با buildهای غیر-Steam برای عملکرد dump movement کار می‌کند.
const GLOBAL_ORIGIN_RVA_HW: &[u32] = &[
    0x0007CD13C, // non-Steam build — dump movement OK
    0x0012047A0,
    0x001230274,
    0x00122E324,
    0x00108AEC4,
];

/// [EN] Direct vec3 positions in client.dll — LocalOrigin and similar
/// [FA] بردار vec3 مستقیم در client.dll — LocalOrigin و مشابه
const GLOBAL_ORIGIN_RVA_CLIENT: &[u32] = &[0x0013E7F0, 0x0012D9F0];

/// [EN] Result of position discovery — contains the base address and offset
/// [FA] نتیجه کشف موقعیت — شامل آدرس پایه و آفست
///
/// [EN] When a valid player position is found, this struct holds:
///
/// - player: the base address (entity or global address)
/// - offset: the offset from base where the vec3 is located
///
/// [FA] هنگامی که موقعیت معتبر بازیکن یافت شود، این ساختار شامل:
///
/// - player: آدرس پایه (entity یا آدرس سراسری)
/// - offset: آفست از پایه که بردار vec3 در آن قرار دارد
pub struct PositionDiscovery {
    /// [EN] Base address — entity or global pointer
    /// [FA] آدرس پایه — entity یا اشاره‌گر سراسری
    pub player: u32,
    /// [EN] Offset from base address to the vec3 position
    /// [FA] آفست از آدرس پایه تا موقعیت vec3
    pub offset: u32,
}

/// [EN] Player candidate entity with health offset info
/// [FA] نامزد entity بازیکن با اطلاعات آفست سلامتی
#[derive(Clone, Copy)]
pub struct PlayerCandidate {
    /// [EN] Base address of the entity
    /// [FA] آدرس پایه entity
    pub player: u32,
    /// [EN] Offset where health (HP) is stored
    /// [FA] آفستی که سلامتی (HP) در آن ذخیره شده
    pub hp_offset: u32,
    /// [EN] Whether this candidate came from hw.dll (vs client.dll)
    /// [FA] آیا این نامزد از hw.dll آمده (برخلاف client.dll)
    pub from_hw: bool,
}

/// [EN] Validate if three floats look like plausible map coordinates
/// [FA] بررسی آیا سه عدد اعشاری مانند مختصات نقشه معقول به نظر می‌رسند
///
/// [EN] This function applies multiple heuristic filters to determine if a vec3
/// could be a valid world position in GoldSrc engine. The logic:
/// 1. All values must be finite (not NaN or infinity)
/// 2. Reject positions too close to origin (spawn/default values)
/// 3. Reject positions with extreme Z values (likely not world coords)
/// 4. Reject positions with extreme X or Y but near-zero other axis
/// 5. Final range check: X/Y within ±16384, Z within -1024 to 8192
///
/// [FA] این تابع فیلترهای شتابی متعددی اعمال می‌کند تا مشخص کند آیا یک بردار vec3
/// می‌تواند موقعیت معتبر جهانی در موتور GoldSrc باشد. منطق:
/// 1. همه مقادیر باید محدود باشند (NaN یا بی‌نهایت نباشند)
/// 2. رد موقعیت‌های خیلی نزدیک به مبدأ (مقدارهای spawn/پیش‌فرض)
/// 3. رد موقعیت‌های با مقدار Z شدید (احتمالاً مختصات جهانی نیستند)
/// 4. رد موقعیت‌های با X یا Y شدید اما محور دیگر نزدیک به صفر
/// 5. بررسی نهایی محدوده: X/Y در ±16384، Z در -1024 تا 8192
pub fn looks_like_coords(x: f32, y: f32, z: f32) -> bool {
    if !x.is_finite() || !y.is_finite() || !z.is_finite() {
        return false;
    }
    let horiz = x.abs().max(y.abs());
    if horiz < 8.0 && z.abs() < 8.0 {
        return false;
    }
    if x.abs() < 8.0 && y.abs() < 8.0 && z.abs() >= 8192.0 {
        return false;
    }
    if x.abs() >= 8192.0 && y.abs() < 8.0 {
        return false;
    }
    if y.abs() >= 8192.0 && x.abs() < 8.0 {
        return false;
    }
    x.abs() <= 16_384.0 && y.abs() <= 16_384.0 && (-1024.0..=8192.0).contains(&z)
}

/// [EN] Detect spawn/view stub coordinates — e.g., (0, 300, 0) or (300, 0, 0)
/// [FA] تشخیص مختصات stub spawn/view — مثلاً (0, 300, 0) یا (300, 0, 0)
///
/// [EN] Some memory locations contain fixed spawn or view positions that are
/// not the player's actual world position. This detects those patterns:
/// - One axis near zero, one axis > 40, third near zero
///
/// [FA] برخی مکان‌های حافظه حاوی موقعیت‌های spawn یا view ثابت هستند
/// که موقعیت واقعی جهانی بازیکن نیستند. این تابع آن الگوها را تشخیص می‌دهد:
/// - یک محور نزدیک صفر، یک محور بیش از 40، سومی نزدیک صفر
pub fn looks_like_spawn_stub(x: f32, y: f32, z: f32) -> bool {
    if x.abs() < 4.0 && z.abs() < 4.0 && y.abs() > 40.0 {
        return true;
    }
    y.abs() < 4.0 && z.abs() < 4.0 && x.abs() > 40.0
}

/// [EN] Check if vec3 values look like velocity data (not position)
/// [FA] بررسی آیا مقادیر vec3 مانند داده سرعت به نظر می‌رسند (نه موقعیت)
///
/// [EN] Velocity vectors have different characteristics than position vectors:
/// - Horizontal components typically between 1-500 units/sec
/// - Vertical component also bounded
/// - Used to identify velocity fields to avoid confusing with position
///
/// [FA] بردارهای سرعت ویژگی‌های متفاوتی نسبت به بردارهای موقعیت دارند:
/// - مؤلفه‌های افقی معمولاً بین 1-500 واحد بر ثانیه
/// - مؤلفه عمودی نیز محدود است
/// - برای شناسایی فیلدهای سرعت استفاده می‌شود تا با موقعیت اشتباه گرفته نشوند
fn looks_like_velocity(x: f32, y: f32, z: f32) -> bool {
    if !x.is_finite() || !y.is_finite() || !z.is_finite() {
        return false;
    }
    let h = x.abs().max(y.abs());
    (1.0..=500.0).contains(&h) && z.abs() <= 500.0
}

/// [EN] edict → entvars (pev) pointer offsets for various builds
/// [FA] آفست‌های اشاره‌گر edict → entvars (pev) برای buildهای مختلف
///
/// [EN] In GoldSrc, entities use an edict structure that contains a pointer (pev)
/// to the entity variables (entvars). This array contains possible offsets where
/// this pointer might be found in the edict structure.
///
/// [FA] در GoldSrc، entityها از ساختار edict استفاده می‌کنند که شامل اشاره‌گر (pev)
/// به متغیرهای entity (entvars) است. این آرایه شامل آفست‌های ممکنی است که
/// این اشاره‌گر ممکن است در ساختار edict یافت شود.
const PEV_PTR_OFFS: &[u32] = &[
    0x0, 0x4, 0x8, 0xC, 0x10, 0x14, 0x18, 0x1C, 0x20, 0x24, 0x28, 0x2C,
];

/// [EN] Countdown preparation — immediately followed by sampling
/// [FA] شمارش معکوس آماده‌سازی — بعدش بلافاصله نمونه‌گیری شروع می‌شود
///
/// [EN] Displays a countdown timer before starting position sampling.
/// The user should Alt+Tab to the game and prepare to move.
///
/// [FA] تایمر شمارش معکوس را قبل از شروع نمونه‌گیری موقعیت نمایش می‌دهد.
/// کاربر باید Alt+Tab به بازی کند و آماده حرکت شود.
pub fn prepare_walk_test(prep_secs: u32) {
    for s in (1..=prep_secs).rev() {
        println!("  ⏳ {s} — Alt+Tab به بازی...");
        thread::sleep(Duration::from_secs(1));
    }
    println!("  ▶ الان W / strafe را نگه دار!");
}

/// [EN] Validate a memory address is within reasonable user-space range
/// [FA] بررسی اعتبار آدرس حافظه در محدوده فضای کاربر منطقی
///
/// [EN] Windows user-space addresses are typically 0x01000000 to 0x7FFF0000.
/// Also checks 4-byte alignment (addr & 3 == 0) which is required for
/// reading 32-bit values.
///
/// [FA] آدرس‌های فضای کاربر ویندوز معمولاً بین 0x01000000 تا 0x7FFF0000 هستند.
/// همچنین تراز 4 بایتی (addr & 3 == 0) را بررسی می‌کند که برای خواندن
/// مقادیر 32 بیتی ضروری است.
fn valid_ptr(addr: u32) -> bool {
    (0x0100_0000..=0x7FFF_0000).contains(&addr) && addr & 3 == 0
}

/// [EN] Basic filter for vec3 values — must be finite and non-zero
/// [FA] فیلتر پایه برای مقادیر vec3 — باید محدود و غیرصفر باشند
fn snap_filter_vec3(x: f32, y: f32, z: f32) -> bool {
    x.is_finite() && y.is_finite() && z.is_finite() && (x != 0.0 || y != 0.0 || z != 0.0)
}

/// [EN] Check if vec3 values are plausible for player movement
/// [FA] بررسی آیا مقادیر vec3 برای حرکت بازیکن معقول هستند
///
/// [EN] Similar to looks_like_coords but less strict. Used during movement-based
/// discovery where the player is actively moving. Allows larger range values.
///
/// [FA] مشابه looks_like_coords اما سخت‌گیرانه‌تر نیست. در کشف مبتنی بر حرکت
/// استفاده می‌شود که بازیکن فعالانه در حال حرکت است. محدوده مقادیر بزرگ‌تری مجاز است.
fn plausible_for_movement(x: f32, y: f32, z: f32) -> bool {
    if !x.is_finite() || !y.is_finite() || !z.is_finite() {
        return false;
    }
    if x == 0.0 && y == 0.0 && z == 0.0 {
        return false;
    }
    x.abs() <= 32_768.0 && y.abs() <= 32_768.0 && z.abs() <= 16_384.0
}

/// [EN] Peek at a vec3 value at a specific memory location
/// [FA] نگاه به مقدار vec3 در یک مکان حافظه خاص
///
/// [EN] Reads three consecutive floats (12 bytes) from memory. Returns None if:
/// - Base address is outside valid user-space range
/// - Any float is not finite (NaN/infinity)
///
/// [FA] سه عدد اعشاری متوالی (12 بایت) از حافظه می‌خواند. None برمی‌گرداند اگر:
/// - آدرس پایه خارج از محدوده معتبر فضای کاربر باشد
/// - هر عدد اعشاری محدود نباشد (NaN/infinity)
pub fn peek_vec3(reader: &MemoryReader, base: u32, offset: u32) -> Option<(f32, f32, f32)> {
    if !(0x0100_0000..=0x7FFF_0000).contains(&base) {
        return None;
    }
    let x = reader.read_f32(base.wrapping_add(offset)).ok()?;
    let y = reader.read_f32(base.wrapping_add(offset + 4)).ok()?;
    let z = reader.read_f32(base.wrapping_add(offset + 8)).ok()?;
    if x.is_finite() && y.is_finite() && z.is_finite() {
        Some((x, y, z))
    } else {
        None
    }
}

/// [EN] Check if position is usable — rejects zero, stub, and out-of-bounds
/// [FA] بررسی آیا موقعیت قابل استفاده است — صفر، stub و خارج از محدوده رد می‌شوند
///
/// [EN] This is the primary position validation function. It combines:
/// 1. Finite value check
/// 2. Non-zero check
/// 3. Spawn stub detection
/// 4. Valid coordinate range check
///
/// [FA] این تابع اصلی اعتبارسنجی موقعیت است. ترکیب می‌کند:
/// 1. بررسی مقادیر محدود
/// 2. بررسی غیرصفر بودن
/// 3. تشخیص spawn stub
/// 4. بررسی محدوده مختصات معتبر
pub fn is_usable_position(x: f32, y: f32, z: f32) -> bool {
    if !x.is_finite() || !y.is_finite() || !z.is_finite() {
        return false;
    }
    if x == 0.0 && y == 0.0 && z == 0.0 {
        return false;
    }
    !looks_like_spawn_stub(x, y, z) && looks_like_coords(x, y, z)
}

/// [EN] Detect pitch byte misread as float Y — not world origin
/// [FA] تشخیص خواندن اشتباه بایت pitch به عنوان float Y — نه مبدأ جهانی
///
/// [EN] Some memory locations store pitch as a signed byte (-128 to 127).
/// If misread as a float, it appears as approximately -128.0. This detects
/// that pattern to avoid false positives.
///
/// [FA] برخی مکان‌های حافظه pitch را به عنوان بایت با علامت (-128 تا 127) ذخیره می‌کنند.
/// اگر به عنوان عدد اعشاری خوانده شود، تقریباً -128.0 به نظر می‌رسد.
/// این الگو را تشخیص می‌دهد تا از مثبت‌های کاذب جلوگیری شود.
fn looks_like_pitch_misread(y: f32, z: f32) -> bool {
    (y + 128.0).abs() < 1.0 && z.abs() <= 2.0
}

/// [EN] Detect view auxiliary vector — e.g., client+0x11D478
/// [FA] تشخیص بردار view auxiliary — مثلاً client+0x11D478
///
/// [EN] client.dll contains auxiliary view vectors that are NOT world positions.
/// These typically have one large horizontal component and one near-zero.
/// This function detects that pattern to exclude from world position candidates.
///
/// [FA] client.dll حاوی بردارهای view auxiliary است که موقعیت جهانی نیستند.
/// اینها معمولاً یک مؤلفه افقی بزرگ و یکی نزدیک صفر دارند.
/// این تابع آن الگو را تشخیص می‌دهد تا از نامزدهای موقعیت جهانی حذف شوند.
pub fn looks_like_view_aux(x: f32, y: f32, z: f32) -> bool {
    if !x.is_finite() || !y.is_finite() || !z.is_finite() {
        return false;
    }
    if x == 0.0 && y == 0.0 && z == 0.0 {
        return false;
    }
    if looks_like_pitch_misread(y, z) {
        return true;
    }
    let h = [x.abs(), y.abs()];
    let max_h = h[0].max(h[1]);
    let min_h = h[0].min(h[1]);
    // Pattern: (164, 1, 140) — one large horizontal axis, other near zero
    // الگوی (164, 1, 140) — یک محور افقی بزرگ، دیگری ~صفر
    if max_h >= 24.0 && min_h < 12.0 {
        return true;
    }
    // View angle: all three components small
    // زاویه دید: هر سه مؤلفه کوچک
    x.abs() <= 90.0 && y.abs() <= 360.0 && z.abs() <= 180.0 && max_h < 24.0
}

/// [EN] Validate real map XYZ — both horizontal axes must be meaningful
/// [FA] اعتبارسنجی XYZ واقعی نقشه — هر دو محور افقی باید معنی‌دار باشند
///
/// [EN] This is stricter than is_usable_position. It requires:
/// 1. Valid position (usable, not stub)
/// 2. Not a view auxiliary vector
/// 3. Both X and Y must be significant (not just one axis)
///
/// [FA] این سخت‌گیرانه‌تر از is_usable_position است. نیاز دارد:
/// 1. موقعیت معتبر (قابل استفاده، نه stub)
/// 2. بردار view auxiliary نباشد
/// 3. هر دو X و Y باید قابل توجه باشند (نه فقط یک محور)
pub fn looks_like_world_origin(x: f32, y: f32, z: f32) -> bool {
    if !is_usable_position(x, y, z) {
        return false;
    }
    if looks_like_view_aux(x, y, z) {
        return false;
    }
    let max_h = x.abs().max(y.abs());
    let min_h = x.abs().min(y.abs());
    max_h >= 32.0 && (min_h >= 8.0 || max_h >= 120.0)
}

/// [EN] Alias for looks_like_world_origin
/// [FA] نام مستعار برای looks_like_world_origin
pub fn is_usable_world_position(x: f32, y: f32, z: f32) -> bool {
    looks_like_world_origin(x, y, z)
}

/// [EN] Read a vec3 with usability validation
/// [FA] خواندن vec3 با اعتبارسنجی قابلیت استفاده
pub fn read_vec3(reader: &MemoryReader, base: u32, offset: u32) -> Option<(f32, f32, f32)> {
    peek_vec3(reader, base, offset).filter(|&(x, y, z)| is_usable_position(x, y, z))
}

/// [EN] Read a world-validated vec3
/// [FA] خواندن vec3 با اعتبارسنجی جهانی
pub fn read_world_vec3(reader: &MemoryReader, base: u32, offset: u32) -> Option<(f32, f32, f32)> {
    peek_vec3(reader, base, offset).filter(|&(x, y, z)| is_usable_world_position(x, y, z))
}

/// [EN] Read view auxiliary vector from client.dll
/// [FA] خواندن بردار view auxiliary از client.dll
///
/// [EN] This reads the view/angle data from client.dll at the configured RVA.
/// The view_rva parameter is the Relative Virtual Address of the view
/// structure within client.dll.
///
/// [FA] این داده view/angle را از client.dll در آدرس سراسری پیکربندی شده می‌خواند.
/// پارامتر view_rva آدرس نسبی مجازی ساختار view در client.dll است.
pub fn read_view_aux(
    reader: &MemoryReader,
    client_base: u32,
    view_rva: Option<u32>,
) -> Option<(f32, f32, f32)> {
    let rva = view_rva?;
    if client_base == 0 {
        return None;
    }
    peek_vec3(reader, client_base.wrapping_add(rva), 0)
}

/// [EN] Runtime world position check — less strict than looks_like_world_origin
/// [FA] بررسی موقعیت جهانی در زمان اجرا — کمتر سخت‌گیرانه از looks_like_world_origin
///
/// [EN] Used during active gameplay when the player is moving. Less strict
/// because movement can produce temporarily unusual coordinates.
///
/// [FA] در طول گیمپلی فعال هنگامی که بازیکن در حال حرکت است استفاده می‌شود.
/// کمتر سخت‌گیرانه است زیرا حرکت می‌تواند مختصات موقتاً غیرعادی تولید کند.
fn is_runtime_world_xyz(x: f32, y: f32, z: f32) -> bool {
    is_usable_position(x, y, z) && !looks_like_view_aux(x, y, z)
}

/// [EN] Peek at vec3 with runtime world validation
/// [FA] نگاه به vec3 با اعتبارسنجی جهانی در زمان اجرا
fn peek_runtime_world(reader: &MemoryReader, base: u32, offset: u32) -> Option<(f32, f32, f32)> {
    peek_vec3(reader, base, offset).filter(|&(x, y, z)| is_runtime_world_xyz(x, y, z))
}

/// [EN] Read XYZ for runtime — less strict filter than read_world_vec3
/// [FA] خواندن XYZ برای runtime — فیلتر کمتر سخت‌گیر از read_world_vec3
pub fn read_runtime_world_vec3(
    reader: &MemoryReader,
    base: u32,
    offset: u32,
) -> Option<(f32, f32, f32)> {
    peek_runtime_world(reader, base, offset)
}

/// [EN] Resolve local player position via hw+0x169438 → edict → pev → origin
/// [FA] حل موقعیت بازیکن محلی از طریق hw+0x169438 → edict → pev → origin
///
/// [EN] This is the primary position resolution function for hw.dll-based builds.
/// The algorithm:
/// 1. Read entity pointer from hw.dll at the configured RVA
/// 2. Validate the entity pointer is valid
/// 3. Try each HP offset to find health match
/// 4. For each valid HP offset, scan for position candidates
/// 5. Score each candidate based on position plausibility and offset likelihood
/// 6. Return the highest-scoring candidate
///
/// [FA] این تابع اصلی حل موقعیت برای buildهای مبتنی بر hw.dll است. الگوریتم:
/// 1. خواندن اشاره‌گر entity از hw.dll در آدرس سراسری پیکربندی شده
/// 2. اعتبارسنجی اشاره‌گر entity معتبر باشد
/// 3. امتحان هر آفست HP برای یافتن تطابق سلامتی
/// 4. برای هر آفست HP معتبر، اسکن نامزدهای موقعیت
/// 5. امتیازدهی هر نامزد بر اساس معقول بودن موقعیت و احتمال آفست
/// 6. برگرداندن نامزد با بالاترین امتیاز
pub fn resolve_hw_local_player_position(
    reader: &MemoryReader,
    hw_base: u32,
    entity_hw_rva: Option<u32>,
    config_off: u32,
    expected_hp: Option<i32>,
) -> Option<(PositionDiscovery, (f32, f32, f32))> {
    let rva = entity_hw_rva?;
    if hw_base == 0 {
        return None;
    }
    let entity = reader.read_u32(hw_base.wrapping_add(rva)).ok()?;
    if !valid_ptr(entity) {
        return None;
    }

    let mut best: Option<(i32, PositionDiscovery, (f32, f32, f32))> = None;
    let mut consider = |base: u32, off: u32, bonus: i32| {
        let Some(xyz) = peek_runtime_world(reader, base, off) else {
            return;
        };
        let mut score = bonus + score_offset(off, config_off);
        // Bonus for larger XY values (more likely to be real position)
        score += (xyz.0.abs() + xyz.1.abs()) as i32 / 40;
        if base != entity {
            score += 1200;
        }
        if off == 0x34 || off == 0x8 {
            score += 400;
        }
        if best.as_ref().map(|(bs, _, _)| score > *bs).unwrap_or(true) {
            best = Some((
                score,
                PositionDiscovery {
                    player: base,
                    offset: off,
                },
                xyz,
            ));
        }
    };

    // Try each HP offset to verify entity identity
    for &hoff in HP_OFFS_HW {
        if !hp_matches(reader, entity, hoff, expected_hp) {
            continue;
        }
        let cand = PlayerCandidate {
            player: entity,
            hp_offset: hoff,
            from_hw: true,
        };
        // Collect all position hits from entity and pev
        for (base, off) in collect_entity_hits(reader, cand, config_off, expected_hp, true, false) {
            let bonus = if base == entity { 200 } else { 800 };
            consider(base, off, bonus);
        }
    }

    best.map(|(_, d, xyz)| (d, xyz))
}

/// [EN] Read world origin from hw entity + pev structure
/// [FA] خواندن مبدأ جهانی از entity hw + ساختار pev
///
/// [EN] Similar to resolve_hw_local_player_position but without HP verification.
/// Used for diagnostic/dump purposes where we want to read all possible positions
/// regardless of health match.
///
/// [FA] مشابه resolve_hw_local_player_position اما بدون تأیید HP.
/// برای اهداف تشخیصی/dump استفاده می‌شود که می‌خواهیم همه موقعیت‌های ممکن
/// را بدون توجه به تطابق سلامتی بخوانیم.
pub fn read_hw_entity_world_origin(
    reader: &MemoryReader,
    hw_base: u32,
    entity_hw_rva: Option<u32>,
) -> Option<PositionDiscovery> {
    let rva = entity_hw_rva?;
    if hw_base == 0 {
        return None;
    }
    let entity = reader.read_u32(hw_base.wrapping_add(rva)).ok()?;
    if !valid_ptr(entity) {
        return None;
    }

    let mut best: Option<(i32, PositionDiscovery)> = None;
    let mut consider = |base: u32, off: u32, bonus: i32| {
        let Some((x, y, z)) = peek_runtime_world(reader, base, off) else {
            return;
        };
        let mut score = bonus + score_offset(off, 0x34);
        score += (x.abs() + y.abs()) as i32 / 40;
        if off == 0x34 || off == 0x8 {
            score += 400;
        }
        let _ = z;
        if best.as_ref().map(|(bs, _)| score > *bs).unwrap_or(true) {
            best = Some((
                score,
                PositionDiscovery {
                    player: base,
                    offset: off,
                },
            ));
        }
    };

    // Try direct entity offsets
    for &off in POS_OFFSET_CANDIDATES {
        consider(entity, off, 200);
    }
    // Try pev pointer offsets
    for &pev_off in PEV_PTR_OFFS {
        let Ok(pev) = reader.read_u32(entity.wrapping_add(pev_off)) else {
            continue;
        };
        if !valid_ptr(pev) {
            continue;
        }
        for &off in &[0x0u32, 0x8, 0x14, 0x20, 0x34, 0x38, 0x44] {
            consider(pev, off, 800);
        }
    }
    best.map(|(_, d)| d)
}

/// [EN] Score a movement delta between two position samples
/// [FA] امتیازدهی تغییرات حرکت بین دو نمونه موقعیت
///
/// [EN] This function evaluates whether a position change between two time
/// samples is consistent with player movement. Key logic:
/// 1. Reject if time delta too small (< 40ms)
/// 2. Reject if new position looks like view aux (not world)
/// 3. Large horizontal movement gets bonus
/// 4. New position looking like world origin gets bonus
/// 5. Add offset score for likely offsets
///
/// [FA] این تابع ارزیابی می‌کند آیا تغییر موقعیت بین دو نمونه زمانی
/// با حرکت بازیکن سازگار است. منطق کلیدی:
/// 1. رد اگر تفاضل زمانی خیلی کوچک (< 40ms) باشد
/// 2. رد اگر موقعیت جدید مانند view aux باشد (نه جهانی)
/// 3. حرکت افقی بزرگ پاداش می‌گیرد
/// 4. موقعیت جدید مانند مبدأ جهانی باشد پاداش می‌گیرد
/// 5. اضافه کردن امتیاز آفست برای آفست‌های محتمل
fn score_movement_delta(
    v0: (f32, f32, f32),
    v1: (f32, f32, f32),
    d: f32,
    off: u32,
    config_off: u32,
) -> Option<f32> {
    if d <= 0.04 || looks_like_view_aux(v1.0, v1.1, v1.2) {
        return None;
    }
    let horiz_d = (v0.0 - v1.0).abs().max((v0.1 - v1.1).abs());
    let mut score = d;
    if horiz_d > 0.5 {
        score += 2000.0;
    }
    if looks_like_world_origin(v1.0, v1.1, v1.2) {
        score += 1000.0;
    } else if looks_like_coords(v1.0, v1.1, v1.2) {
        score += 200.0;
    }
    score += score_offset(off, config_off) as f32;
    Some(score)
}

/// [EN] Calculate squared distance between two positions
/// [FA] محاسبه فاصله مربعی بین دو موقعیت
///
/// [EN] Uses squared distance to avoid expensive sqrt() calls.
/// The squared value is sufficient for comparison purposes.
///
/// [FA] از فاصله مربعی استفاده می‌کند تا از فراخوانی‌های پرهزینه sqrt() جلوگیری شود.
/// مقدار مربعی برای مقاصد مقایسه کافی است.
fn pos_delta_sq(a: (f32, f32, f32), b: (f32, f32, f32)) -> f32 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    let dz = a.2 - b.2;
    dx * dx + dy * dy + dz * dz
}

/// [EN] Check if this is a client stub entity (not real player)
/// [FA] بررسی آیا این یک entity stub client است (بازیکن واقعی نیست)
///
/// [EN] Client stubs have health at a fixed offset from a base address.
/// This checks if the player address matches that pattern.
///
/// [FA] stubهای client سلامتی را در آفست ثابتی از آدرس پایه دارند.
/// این بررسی می‌کند آیا آدرس بازیکن با آن الگو مطابقت دارد.
fn is_client_stub(player: u32, health_direct: u32, client_hp_off: u32) -> bool {
    health_direct != 0 && player == health_direct.wrapping_sub(client_hp_off)
}

/// [EN] Score an offset based on likelihood and config match
/// [FA] امتیازدهی آفست بر اساس احتمال و تطابق پیکربندی
///
/// [EN] Common offsets (0x8, 0x34, 0x38, 0x44) get higher scores because
/// they are the most likely locations for player position data. Offsets
/// matching the config get a bonus. Very large offsets get penalized.
///
/// [FA] آفست‌های رایج (0x8، 0x34، 0x38، 0x44) امتیاز بیشتری می‌گیرند زیرا
/// محتمل‌ترین مکان‌ها برای داده موقعیت بازیکن هستند. آفست‌هایی که با
/// پیکربندی مطابقت دارند پاداش می‌گیرند. آفست‌های خیلی بزرگ جریمه می‌شوند.
fn score_offset(offset: u32, config_off: u32) -> i32 {
    let mut score = 0;
    if offset == config_off {
        score += 200;
    }
    match offset {
        0x8 | 0x34 | 0x38 | 0x44 => score += 500,
        0x134 => score += 350,
        0x138 => score += 200,
        0x124 | 0x128 => score -= 5000,
        _ => {}
    }
    if offset >= 0x1000 {
        score -= 800;
    }
    score
}

/// [EN] Verify HP at a specific offset matches expected value
/// [FA] تأیید HP در یک آفست خاص با مقدار مورد انتظار مطابقت دارد
///
/// [EN] Reads the 32-bit integer at player + offset and checks:
/// 1. Value is in valid HP range (0-100)
/// 2. If expected HP is provided, it must match exactly
///
/// [FA] مقدار 32 بیتی در بازیکن + آفست را می‌خواند و بررسی می‌کند:
/// 1. مقدار در محدوده معتبر HP (0-100) باشد
/// 2. اگر HP مورد انتظار ارائه شده باشد، باید دقیقاً مطابقت داشته باشد
fn hp_matches(reader: &MemoryReader, player: u32, hp_off: u32, expected: Option<i32>) -> bool {
    let Ok(hp) = reader.read_i32(player.wrapping_add(hp_off)) else {
        return false;
    };
    if !(0..=100).contains(&hp) {
        return false;
    }
    expected.is_none_or(|exp| hp == exp)
}

/// [EN] Build ordered list of offsets to try, starting with config offset
/// [FA] ساخت لیست مرتب آفست‌ها برای امتحان، با شروع از آفست پیکربندی
///
/// [EN] The config offset is tried first (highest priority), then all other
/// candidates in order. This ensures the most likely offset is tested first.
///
/// [FA] آفست پیکربندی اول امتحان می‌شود (بالاترین اولویت)، سپس همه نامزدهای
/// دیگر به ترتیب. این تضمین می‌کند محتمل‌ترین آفست اول آزمایش شود.
fn offsets_to_try(config_off: u32) -> Vec<u32> {
    let mut v = vec![config_off];
    for &off in POS_OFFSET_CANDIDATES {
        if off != config_off {
            v.push(off);
        }
    }
    v
}

/// [EN] Add a hit to the results list, avoiding duplicates
/// [FA] اضافه کردن hit به لیست نتایج، با جلوگیری از تکرار
fn push_hit(hits: &mut Vec<(u32, u32)>, player: u32, off: u32) {
    if !hits.iter().any(|&(p, o)| p == player && o == off) {
        hits.push((player, off));
    }
}

/// [EN] Collect all valid position hits from an entity candidate
/// [FA] جمع‌آوری همه hitهای موقعیت معتبر از یک نامزد entity
///
/// [EN] This is a core scanning function that checks multiple memory locations
/// for valid position data. The algorithm:
/// 1. Verify HP matches expected value
/// 2. Try all configured offsets on the player entity
/// 3. Follow edict → pev pointer chain and check entvars offsets
/// 4. Check for direct entvars in player entity
/// 5. If wide_scan enabled, scan entire entity memory region
///
/// [FA] این تابع اسکن اصلی است که مکان‌های حافظه متعددی را برای داده موقعیت
/// معتبر بررسی می‌کند. الگوریتم:
/// 1. تأیید تطابق HP با مقدار مورد انتظار
/// 2. امتحان همه آفست‌های پیکربندی شده روی entity بازیکن
/// 3. دنبال کردن زنجیره اشاره‌گر edict → pev و بررسی آفست‌های entvars
/// 4. بررسی entvars مستقیم در entity بازیکن
/// 5. اگر wide_scan فعال باشد، اسکن کل ناحیه حافظه entity
fn collect_entity_hits(
    reader: &MemoryReader,
    cand: PlayerCandidate,
    config_off: u32,
    expected_hp: Option<i32>,
    wide_scan: bool,
    for_movement: bool,
) -> Vec<(u32, u32)> {
    if !hp_matches(reader, cand.player, cand.hp_offset, expected_hp) {
        return Vec::new();
    }
    let mut hits = Vec::new();
    let check = |v: (f32, f32, f32)| {
        if for_movement {
            plausible_for_movement(v.0, v.1, v.2)
        } else {
            looks_like_coords(v.0, v.1, v.2)
        }
    };

    // Try direct entity offsets
    for off in offsets_to_try(config_off) {
        if let Some(v) = peek_vec3(reader, cand.player, off) {
            if check(v) {
                push_hit(&mut hits, cand.player, off);
            }
        }
    }

    // entvars via edict → pev pointer chain
    for &pev_off in PEV_PTR_OFFS {
        let Ok(pev) = reader.read_u32(cand.player.wrapping_add(pev_off)) else {
            continue;
        };
        if !(0x0100_0000..=0x7FFF_0000).contains(&pev) {
            continue;
        }
        for &off in &[0x8u32, 0x14, 0x20, 0x34, 0x38, 0x44] {
            if let Some(v) = peek_vec3(reader, pev, off) {
                if check(v) {
                    push_hit(&mut hits, pev, off);
                }
            }
        }
    }

    // Direct entvars in player entity (some builds)
    if let Ok(entvars) = reader.read_u32(cand.player) {
        if (0x0100_0000..=0x7FFF_0000).contains(&entvars) {
            for &off in &[0x8u32, 0x14, 0x20, 0x34] {
                if let Some(v) = peek_vec3(reader, entvars, off) {
                    if check(v) {
                        push_hit(&mut hits, entvars, off);
                    }
                }
            }
        }
    }

    // Wide scan: check entire entity memory region (0 to 0x2000)
    if wide_scan {
        for off in (0..0x2000).step_by(4) {
            if offsets_to_try(config_off).contains(&off) {
                continue;
            }
            if let Some(v) = peek_vec3(reader, cand.player, off) {
                if check(v) {
                    push_hit(&mut hits, cand.player, off);
                }
            }
        }
    }
    hits
}

/// [EN] Collect global origin base addresses from hw.dll and client.dll
/// [FA] جمع‌آوری آدرس‌های پایه مبدأ سراسری از hw.dll و client.dll
///
/// [EN] This scans for player position data stored directly in game modules
/// (not via entity pointers). Checks both direct vec3 values and pointer
/// chains. Returns deduplicated list of valid base addresses.
///
/// [FA] این داده موقعیت بازیکن ذخیره شده مستقیماً در ماژول‌های بازی را اسکن می‌کند
/// (نه از طریق اشاره‌گرهای entity). هم مقادیر vec3 مستقیم و هم زنجیره‌های
/// اشاره‌گر را بررسی می‌کند. لیست بدون تکرار آدرس‌های پایه معتبر برمی‌گرداند.
pub fn collect_global_origin_bases(
    reader: &MemoryReader,
    hw_base: u32,
    client_base: u32,
    config_global_hw_rva: Option<u32>,
    config_global_client_rva: Option<u32>,
) -> Vec<u32> {
    let mut out = Vec::new();
    let mut add = |addr: u32| {
        if valid_ptr(addr) && !out.contains(&addr) {
            out.push(addr);
        }
    };
    // Helper: add both direct slot and dereferenced pointer
    let mut add_slot = |slot: u32| {
        add(slot);
        if let Ok(ptr) = reader.read_u32(slot) {
            if valid_ptr(ptr) {
                add(ptr);
                // Also check common pev offsets from pointer
                for pev_off in [0x8u32, 0x34] {
                    add(ptr.wrapping_add(pev_off));
                }
            }
        }
    };

    if let Some(rva) = config_global_hw_rva {
        if hw_base != 0 {
            add_slot(hw_base.wrapping_add(rva));
        }
    }
    if let Some(rva) = config_global_client_rva {
        if client_base != 0 {
            add_slot(client_base.wrapping_add(rva));
        }
    }
    if hw_base != 0 {
        for &rva in GLOBAL_ORIGIN_RVA_HW {
            add_slot(hw_base.wrapping_add(rva));
        }
    }
    if client_base != 0 {
        for &rva in GLOBAL_ORIGIN_RVA_CLIENT {
            add_slot(client_base.wrapping_add(rva));
        }
    }
    out
}

/// [EN] Discover position by global movement — scan module memory
/// [FA] کشف موقعیت از طریق حرکت سراسری — اسکن حافظه ماژول
#[allow(dead_code)]
fn discover_global_movement(
    reader: &MemoryReader,
    bases: &[u32],
    wait_ms: u64,
) -> Option<PositionDiscovery> {
    let mut snaps: Vec<(u32, (f32, f32, f32))> = Vec::new();
    // Take initial snapshot of all valid positions
    for &base in bases {
        if let Some(v) =
            peek_vec3(reader, base, 0).filter(|&(x, y, z)| plausible_for_movement(x, y, z))
        {
            snaps.push((base, v));
        }
    }
    if snaps.is_empty() {
        return None;
    }

    // Wait for player to move
    thread::sleep(Duration::from_millis(wait_ms));

    // Find the position that changed the most (likely the player)
    let mut best: Option<(f32, PositionDiscovery)> = None;
    for (base, v0) in snaps {
        let Some(v1) = peek_vec3(reader, base, 0) else {
            continue;
        };
        let d = pos_delta_sq(v0, v1);
        if let Some(score) = score_movement_delta(v0, v1, d, 0, 0) {
            if best.as_ref().map(|(bd, _)| score > *bd).unwrap_or(true) {
                best = Some((
                    score,
                    PositionDiscovery {
                        player: base,
                        offset: 0,
                    },
                ));
            }
        }
    }
    best.map(|(_, d)| d)
}

/// [EN] Collect all player candidates from hw.dll and client.dll
/// [FA] جمع‌آوری همه نامزدهای بازیکن از hw.dll و client.dll
///
/// [EN] This function identifies potential player entities by:
/// 1. Reading entity pointers from hw.dll RVAs
/// 2. Checking HP at various offsets to verify entity identity
/// 3. Also checking client.dll local player pointer
/// 4. Filtering out client stubs (non-real entities)
///
/// [FA] این تابع entityهای بالقوه بازیکن را با شناسایی می‌کند:
/// 1. خواندن اشاره‌گرهای entity از آدرس‌های سراسری hw.dll
/// 2. بررسی HP در آفست‌های مختلف برای تأیید هویت entity
/// 3. همچنین بررسی اشاره‌گر بازیکن محلی client.dll
/// 4. فیلتر کردن stubهای client (entityهای غیرواقعی)
///
/// [clippy::too_many_arguments] — Required due to multiple config parameters
/// [clippy::too_many_arguments] — به دلیل پارامترهای پیکربندی متعدد مورد نیاز است
#[allow(clippy::too_many_arguments)]
pub fn collect_position_candidates(
    reader: &MemoryReader,
    hw_base: u32,
    client_base: u32,
    on_client: bool,
    config_lp_rva: u32,
    health_direct: u32,
    client_hp_off: u32,
    config_entity_hw_rva: Option<u32>,
) -> Vec<PlayerCandidate> {
    let mut out = Vec::new();
    let mut add = |player: u32, hp_offset: u32, from_hw: bool| {
        // Validate address is in user-space and aligned
        if !(0x0100_0000..=0x7FFF_0000).contains(&player) || player & 3 != 0 {
            return;
        }
        // Avoid duplicates
        if out
            .iter()
            .any(|c: &PlayerCandidate| c.player == player && c.hp_offset == hp_offset)
        {
            return;
        }
        out.push(PlayerCandidate {
            player,
            hp_offset,
            from_hw,
        });
    };

    // Scan hw.dll entity pointers
    if hw_base != 0 {
        let mut rvas: Vec<u32> = LP_RVA_HW_FOR_POS.to_vec();
        // Add configured RVA at front (highest priority)
        if let Some(rva) = config_entity_hw_rva {
            if !rvas.contains(&rva) {
                rvas.insert(0, rva);
            }
        }
        for &rva in &rvas {
            let Ok(player) = reader.read_u32(hw_base.wrapping_add(rva)) else {
                continue;
            };
            for &hoff in HP_OFFS_HW {
                if hp_matches(reader, player, hoff, None) {
                    add(player, hoff, true);
                }
            }
        }
    }

    // Check client.dll local player
    let lp_base = if on_client { client_base } else { hw_base };
    if lp_base != 0 {
        if let Ok(player) = reader.read_u32(lp_base.wrapping_add(config_lp_rva)) {
            if !is_client_stub(player, health_direct, client_hp_off) {
                add(player, client_hp_off, false);
            }
        }
    }

    out
}

/// [EN] Backward-compatible player candidate collection
/// [FA] جمع‌آوری نامزد بازیکن سازگار با نسخه‌های قبلی
///
/// [EN] Legacy function that returns only player addresses (not full candidate info).
/// Kept for compatibility with older dump formats.
///
/// [FA] تابع قدیمی که فقط آدرس‌های بازیکن را برمی‌گرداند (نه اطلاعات کامل نامزد).
/// برای سازگاری با قالب‌های dump قدیمی نگه داشته شده.
pub fn collect_player_candidates(
    reader: &MemoryReader,
    hw_base: u32,
    client_base: u32,
    on_client: bool,
    config_lp_rva: u32,
    health_direct: u32,
    hp_off: u32,
) -> Vec<u32> {
    collect_position_candidates(
        reader,
        hw_base,
        client_base,
        on_client,
        config_lp_rva,
        health_direct,
        hp_off,
        None,
    )
    .into_iter()
    .map(|c| c.player)
    .collect()
}

/// [EN] Discover position by movement — compare two time samples
/// [FA] کشف موقعیت از طریق حرکت — مقایسه دو نمونه زمانی
///
/// [EN] This is the primary movement-based position discovery function.
/// Algorithm:
/// 1. Take initial snapshot of all valid positions from candidates
/// 2. Wait for player to move (user holds W key)
/// 3. Re-read all positions and calculate deltas
/// 4. Score each delta based on movement plausibility
/// 5. Return the position with highest movement score
///
/// [FA] این تابع اصلی کشف موقعیت مبتنی بر حرکت است. الگوریتم:
/// 1. گرفتن نمونه اولیه از همه موقعیت‌های معتبر از نامزدها
/// 2. صبر کردن برای حرکت بازیکن (کاربر کلید W را نگه می‌دارد)
/// 3. خواندن مجدد همه موقعیت‌ها و محاسبه تغییرات
/// 4. امتیازدهی هر تغییر بر اساس معقول بودن حرکت
/// 5. برگرداندن موقعیت با بالاترین امتیاز حرکت
pub fn discover_by_movement(
    reader: &MemoryReader,
    candidates: &[PlayerCandidate],
    config_off: u32,
    expected_hp: Option<i32>,
    wait_ms: u64,
    wide_scan: bool,
) -> Option<PositionDiscovery> {
    let mut snaps: Vec<(u32, u32, (f32, f32, f32))> = Vec::new();
    for &cand in candidates {
        for (player, off) in
            collect_entity_hits(reader, cand, config_off, expected_hp, wide_scan, true)
        {
            if let Some(v) = peek_vec3(reader, player, off) {
                snaps.push((player, off, v));
            }
        }
    }
    if snaps.is_empty() {
        return None;
    }

    // Wait for player movement
    thread::sleep(Duration::from_millis(wait_ms));

    // Find position with largest valid movement
    let mut best: Option<(f32, PositionDiscovery)> = None;
    for (player, off, v0) in snaps {
        let Some(v1) = peek_vec3(reader, player, off) else {
            continue;
        };
        let d = pos_delta_sq(v0, v1);
        if let Some(score) = score_movement_delta(v0, v1, d, off, config_off) {
            if best.as_ref().map(|(bd, _)| score > *bd).unwrap_or(true) {
                best = Some((
                    score,
                    PositionDiscovery {
                        player,
                        offset: off,
                    },
                ));
            }
        }
    }
    best.map(|(_, d)| d)
}

/// [EN] Discover position by tracking individual float changes
/// [FA] کشف موقعیت از طریق ردیابی تغییرات float منفرد
///
/// [EN] Similar to Cheat Engine's "changed value" search. Scans memory for
/// float values that change between two time samples. Then checks if
/// consecutive floats (forming a vec3) all changed consistently.
///
/// [FA] مشابه جستجوی "مقدار تغییر یافته" در Cheat Engine. حافظه را برای
/// مقادیر اعشاری که بین دو نمونه زمانی تغییر می‌کنند اسکن می‌کند.
/// سپس بررسی می‌کند آیا اعداد اعشاری متوالی ( تشکیل دهنده vec3) همه
/// به طور سازگار تغییر کرده‌اند.
fn discover_by_changing_floats(
    reader: &MemoryReader,
    base: u32,
    scan_size: u32,
    wait_ms: u64,
) -> Option<PositionDiscovery> {
    if !(0x0100_0000..=0x7FFF_0000).contains(&base) {
        return None;
    }
    // Take initial snapshot of all float values in range
    let mut snap: Vec<(u32, f32)> = Vec::new();
    for off in (0..scan_size).step_by(4) {
        let Ok(v) = reader.read_f32(base.wrapping_add(off)) else {
            continue;
        };
        if v.is_finite() && v.abs() > 2.0 && v.abs() < 20_000.0 {
            snap.push((off, v));
        }
    }
    if snap.is_empty() {
        return None;
    }

    println!("  … {wait_ms}ms float-scan — **W را نگه دار**");
    thread::sleep(Duration::from_millis(wait_ms));

    // Find which floats changed
    let mut changed = Vec::new();
    for (off, v0) in &snap {
        let Ok(v1) = reader.read_f32(base.wrapping_add(*off)) else {
            continue;
        };
        if (v1 - v0).abs() > 0.3 {
            changed.push(*off);
        }
    }
    if changed.is_empty() {
        return None;
    }

    // Look for consecutive changed floats (likely vec3)
    let mut best: Option<(f32, u32)> = None;
    for &off in &changed {
        // Check if offset+4 and offset+8 also changed (forming vec3)
        if !changed.contains(&(off + 4)) || !changed.contains(&(off + 8)) {
            continue;
        }
        let Some(v1) = peek_vec3(reader, base, off) else {
            continue;
        };
        if looks_like_spawn_stub(v1.0, v1.1, v1.2) {
            continue;
        }
        // Get initial values from snapshot
        let v0x = snap
            .iter()
            .find(|(o, _)| *o == off)
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        let v0y = snap
            .iter()
            .find(|(o, _)| *o == off + 4)
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        let v0z = snap
            .iter()
            .find(|(o, _)| *o == off + 8)
            .map(|(_, v)| *v)
            .unwrap_or(0.0);
        let d = pos_delta_sq((v0x, v0y, v0z), v1);
        if d <= 0.04 {
            continue;
        }
        let mut score = d;
        if looks_like_coords(v1.0, v1.1, v1.2) {
            score += 2000.0;
        }
        if best.as_ref().map(|(bs, _)| score > *bs).unwrap_or(true) {
            best = Some((score, off));
        }
    }
    best.map(|(_, off)| PositionDiscovery {
        player: base,
        offset: off,
    })
}

/// [EN] Discover position via velocity change → origin at vel-0x18 (entvars)
/// [FA] کشف موقعیت از طریق تغییر سرعت → مبدأ در vel-0x18 (entvars)
///
/// [EN] In GoldSrc entvars, velocity is stored at offset 0x20 and origin at 0x8.
/// The difference is 0x18 bytes. So if we find velocity changing, the origin
/// should be at velocity_offset - 0x18.
///
/// [FA] در entvars GoldSrc، سرعت در آفست 0x20 و مبدأ در 0x8 ذخیره شده.
/// تفاضل 0x18 بایت است. بنابراین اگر تغییر سرعت را پیدا کنیم،
/// مبدأ باید در velocity_offset - 0x18 باشد.
#[allow(dead_code)]
fn discover_by_velocity(
    reader: &MemoryReader,
    candidates: &[PlayerCandidate],
    expected_hp: Option<i32>,
    wait_ms: u64,
) -> Option<PositionDiscovery> {
    let mut snaps: Vec<(u32, u32, (f32, f32, f32))> = Vec::new();
    for &cand in candidates {
        if !hp_matches(reader, cand.player, cand.hp_offset, expected_hp) {
            continue;
        }
        let bases = [cand.player];
        for &base in &bases {
            // Scan entity memory for velocity-like values
            for off in (0x8..0x800).step_by(4) {
                if let Some(v) =
                    peek_vec3(reader, base, off).filter(|&(x, y, z)| looks_like_velocity(x, y, z))
                {
                    snaps.push((base, off, v));
                }
            }
        }
        // Also scan pev memory
        for &pev_off in PEV_PTR_OFFS {
            let Ok(pev) = reader.read_u32(cand.player.wrapping_add(pev_off)) else {
                continue;
            };
            if !(0x0100_0000..=0x7FFF_0000).contains(&pev) {
                continue;
            }
            for off in (0x8..0x400).step_by(4) {
                if let Some(v) =
                    peek_vec3(reader, pev, off).filter(|&(x, y, z)| looks_like_velocity(x, y, z))
                {
                    snaps.push((pev, off, v));
                }
            }
        }
    }
    if snaps.is_empty() {
        return None;
    }

    // Wait for velocity to change
    thread::sleep(Duration::from_millis(wait_ms));

    // Find velocity that changed, then read origin at vel-0x18
    let mut best: Option<(f32, PositionDiscovery)> = None;
    for (base, vel_off, v0) in snaps {
        let Some(v1) = peek_vec3(reader, base, vel_off) else {
            continue;
        };
        if pos_delta_sq(v0, v1) <= 0.04 {
            continue;
        }
        // Origin is 0x18 bytes before velocity in entvars
        let origin_off = vel_off.saturating_sub(0x18);
        let Some(origin) = peek_vec3(reader, base, origin_off) else {
            continue;
        };
        if looks_like_spawn_stub(origin.0, origin.1, origin.2) {
            continue;
        }
        let score = pos_delta_sq(v0, v1)
            + if looks_like_coords(origin.0, origin.1, origin.2) {
                1500.0
            } else {
                0.0
            };
        if best.as_ref().map(|(bs, _)| score > *bs).unwrap_or(true) {
            best = Some((
                score,
                PositionDiscovery {
                    player: base,
                    offset: origin_off,
                },
            ));
        }
    }
    best.map(|(_, d)| d)
}

/// [EN] Scan module memory for movement — direct + pointer dereference
/// [FA] اسکن حافظه ماژول برای حرکت — مستقیم + ارجاع اشاره‌گر
///
/// [EN] This performs a comprehensive scan of a memory module range.
/// For each address, it checks both the direct value and the dereferenced
/// pointer value. Then compares two time samples to find movement.
///
/// [FA] این اسکن جامع محدوده حافظه ماژول را انجام می‌دهد.
/// برای هر آدرس، هم مقدار مستقیم و هم مقدار ارجاع شده اشاره‌گر را بررسی می‌کند.
/// سپس دو نمونه زمانی را مقایسه می‌کند تا حرکت را پیدا کند.
pub fn scan_module_globals_for_movement(
    reader: &MemoryReader,
    module_base: u32,
    start_rva: u32,
    end_rva: u32,
    step: u32,
    wait_ms: u64,
    max_snaps: usize,
) -> Option<PositionDiscovery> {
    if module_base == 0 || step == 0 || start_rva >= end_rva {
        return None;
    }
    let mut snaps: Vec<(u32, (f32, f32, f32))> = Vec::new();
    let mut rva = start_rva;
    // Scan memory region for valid vec3 values
    while rva < end_rva && snaps.len() < max_snaps {
        let slot = module_base.wrapping_add(rva);
        // Check both direct slot and pointer dereference
        for &base in &[slot, reader.read_u32(slot).unwrap_or(0)] {
            if !valid_ptr(base) {
                continue;
            }
            if let Some(v) =
                peek_vec3(reader, base, 0).filter(|&(x, y, z)| snap_filter_vec3(x, y, z))
            {
                if !snaps.iter().any(|(b, _)| *b == base) {
                    snaps.push((base, v));
                }
            }
        }
        rva = rva.saturating_add(step);
    }
    if snaps.is_empty() {
        return None;
    }

    println!("  … {wait_ms}ms module-scan — **W را نگه دار**");
    thread::sleep(Duration::from_millis(wait_ms));

    // Find position with largest valid movement
    let mut best: Option<(f32, PositionDiscovery)> = None;
    for (base, v0) in snaps {
        let Some(v1) = peek_vec3(reader, base, 0) else {
            continue;
        };
        let d = pos_delta_sq(v0, v1);
        if let Some(score) = score_movement_delta(v0, v1, d, 0, 0) {
            if best.as_ref().map(|(bd, _)| score > *bd).unwrap_or(true) {
                best = Some((
                    score,
                    PositionDiscovery {
                        player: base,
                        offset: 0,
                    },
                ));
            }
        }
    }
    best.map(|(_, d)| d)
}

/// [EN] Select the best mover from a list of snapshots
/// [FA] انتخاب بهترین حرکت‌کننده از لیست نمونه‌ها
///
/// [EN] Takes a list of position snapshots, waits for movement, then returns
/// the position that moved the most. This is a helper function used by
/// multiple discovery strategies.
///
/// [FA] لیستی از نمونه‌های موقعیت می‌گیرد، برای حرکت صبر می‌کند، سپس
/// موقعیتی را برمی‌گرداند که بیشتر حرکت کرده. این تابع کمکی توسط
/// چندین استراتژی کشف استفاده می‌شود.
fn pick_best_mover(
    snaps: Vec<(u32, u32, (f32, f32, f32))>,
    reader: &MemoryReader,
    wait_ms: u64,
) -> Option<PositionDiscovery> {
    if snaps.is_empty() {
        return None;
    }
    println!("  … {wait_ms}ms — **W را نگه دار**");
    thread::sleep(Duration::from_millis(wait_ms));

    let mut best: Option<(f32, PositionDiscovery)> = None;
    for (player, off, v0) in snaps {
        let Some(v1) = peek_vec3(reader, player, off) else {
            continue;
        };
        if looks_like_spawn_stub(v1.0, v1.1, v1.2) {
            continue;
        }
        let d = pos_delta_sq(v0, v1);
        if d <= 0.04 {
            continue;
        }
        let Some(score) = score_movement_delta(v0, v1, d, off, 0) else {
            continue;
        };
        if best.as_ref().map(|(bs, _)| score > *bs).unwrap_or(true) {
            best = Some((
                score,
                PositionDiscovery {
                    player,
                    offset: off,
                },
            ));
        }
    }
    best.map(|(_, d)| d)
}

/// [EN] Collect all movement snapshots from all discovery sources
/// [FA] جمع‌آوری همه نمونه‌های حرکت از همه منابع کشف
///
/// [EN] This is a comprehensive snapshot collector that checks:
/// 1. Global origin addresses from hw.dll/client.dll
/// 2. Entity-based candidates via collect_entity_hits
/// 3. Velocity-based origin detection (vel-0x18)
/// 4. pev pointer chain scanning
///
/// [FA] این یک جمع‌آوری کننده نمونه جامع است که بررسی می‌کند:
/// 1. آدرس‌های مبدأ سراسری از hw.dll/client.dll
/// 2. نامزدهای مبتنی بر entity از طریق collect_entity_hits
/// 3. تشخیص مبدأ مبتنی بر سرعت (vel-0x18)
/// 4. اسکن زنجیره اشاره‌گر pev
#[allow(clippy::too_many_arguments)]
fn collect_all_movement_snaps(
    reader: &MemoryReader,
    hw_base: u32,
    client_base: u32,
    candidates: &[PlayerCandidate],
    config_off: u32,
    expected_hp: Option<i32>,
    config_global_hw_rva: Option<u32>,
    config_global_client_rva: Option<u32>,
) -> Vec<(u32, u32, (f32, f32, f32))> {
    let mut snaps = Vec::new();
    let mut push = |player: u32, off: u32| {
        if let Some(v) = peek_vec3(reader, player, off)
            .filter(|&(x, y, z)| snap_filter_vec3(x, y, z) && !looks_like_spawn_stub(x, y, z))
        {
            if !snaps.iter().any(|&(p, o, _)| p == player && o == off) {
                snaps.push((player, off, v));
            }
        }
    };

    // Global origin bases
    for &base in &collect_global_origin_bases(
        reader,
        hw_base,
        client_base,
        config_global_hw_rva,
        config_global_client_rva,
    ) {
        push(base, 0);
    }

    // Entity-based candidates
    for &cand in candidates {
        for (player, off) in collect_entity_hits(reader, cand, config_off, expected_hp, true, true)
        {
            push(player, off);
        }
        if !hp_matches(reader, cand.player, cand.hp_offset, expected_hp) {
            continue;
        }
        // Velocity-based origin detection
        for off in (0x8..0x800).step_by(4) {
            if let Some(v) = peek_vec3(reader, cand.player, off)
                .filter(|&(x, y, z)| looks_like_velocity(x, y, z))
            {
                // Origin is 0x18 bytes before velocity in entvars
                let origin_off = off.saturating_sub(0x18);
                push(cand.player, origin_off);
                let _ = v;
            }
        }
        // pev chain scanning
        for &pev_off in PEV_PTR_OFFS {
            let Ok(pev) = reader.read_u32(cand.player.wrapping_add(pev_off)) else {
                continue;
            };
            if !(0x0100_0000..=0x7FFF_0000).contains(&pev) {
                continue;
            }
            for off in (0x8..0x400).step_by(4) {
                if peek_vec3(reader, pev, off).is_some_and(|(x, y, z)| looks_like_velocity(x, y, z))
                {
                    push(pev, off.saturating_sub(0x18));
                }
            }
        }
    }

    snaps
}

/// [EN] Read global world position at RVA — direct or via pointer
/// [FA] خواندن موقعیت جهانی سراسری در آدرس سراسری — مستقیم یا از طریق اشاره‌گر
///
/// [EN] This function first tries to read the vec3 directly at the RVA.
/// If that fails (values look like view aux), it dereferences the pointer
/// and checks common offsets (0x0, 0x34, 0x8).
///
/// [FA] این تابع ابتدا سعی می‌کند vec3 را مستقیماً در آدرس سراسری بخواند.
/// اگر ناموفق باشد (مقادیر مانند view aux به نظر برسند)، اشاره‌گر را
/// ارجاع می‌دهد و آفست‌های رایج (0x0, 0x34, 0x8) را بررسی می‌کند.
pub fn read_global_world_at_rva(
    reader: &MemoryReader,
    hw_base: u32,
    rva: u32,
) -> Option<(PositionDiscovery, (f32, f32, f32))> {
    if hw_base == 0 {
        return None;
    }
    let slot = hw_base.wrapping_add(rva);
    // Try direct read first
    if let Some(xyz) = read_runtime_world_vec3(reader, slot, 0) {
        return Some((
            PositionDiscovery {
                player: slot,
                offset: 0,
            },
            xyz,
        ));
    }
    // Try pointer dereference
    if let Ok(ptr) = reader.read_u32(slot) {
        if valid_ptr(ptr) {
            for &off in &[0u32, 0x34, 0x8] {
                if let Some(xyz) = read_runtime_world_vec3(reader, ptr, off) {
                    return Some((
                        PositionDiscovery {
                            player: ptr,
                            offset: off,
                        },
                        xyz,
                    ));
                }
            }
        }
    }
    None
}

/// [EN] Read position from configured global RVAs — for dump/verdict
/// [FA] خواندن موقعیت از آدرس‌های سراسری پیکربندی شده — برای dump/verdict
///
/// [EN] This reads position data from the user-configured RVAs in hw.dll.
/// It checks both direct values and pointer chains, scoring each candidate
/// based on position plausibility and offset likelihood.
///
/// [FA] این داده موقعیت را از آدرس‌های سراسری پیکربندی شده کاربر در hw.dll می‌خواند.
/// هم مقادیر مستقیم و هم زنجیره‌های اشاره‌گر را بررسی می‌کند و هر نامزد را
/// بر اساس معقول بودن موقعیت و احتمال آفست امتیازدهی می‌کند.
pub fn read_configured_global_position(
    reader: &MemoryReader,
    hw_base: u32,
    _client_base: u32,
    config_global_hw_rva: Option<u32>,
    config_global_client_rva: Option<u32>,
) -> Option<PositionDiscovery> {
    let try_rva = |module_base: u32, rva: u32, bonus: i32| -> Option<(i32, PositionDiscovery)> {
        if module_base == 0 {
            return None;
        }
        let slot = module_base.wrapping_add(rva);
        let mut best: Option<(i32, PositionDiscovery)> = None;
        let mut consider = |base: u32, off: u32, extra: i32| {
            let Some((x, y, z)) = read_world_vec3(reader, base, off) else {
                return;
            };
            if looks_like_spawn_stub(x, y, z) {
                return;
            }
            let mut score = bonus + extra + (x.abs() + y.abs()) as i32 / 50;
            if looks_like_world_origin(x, y, z) {
                score += 500;
            }
            // Bonus for non-trivial Y and Z (real 3D position)
            if y.abs() > 8.0 && z.abs() > 8.0 {
                score += 300;
            }
            if best.as_ref().map(|(bs, _)| score > *bs).unwrap_or(true) {
                best = Some((
                    score,
                    PositionDiscovery {
                        player: base,
                        offset: off,
                    },
                ));
            }
        };
        // Try direct slot first
        consider(slot, 0, 0);
        // If direct read failed, try pointer dereference
        if read_world_vec3(reader, slot, 0).is_none() {
            if let Ok(ptr) = reader.read_u32(slot) {
                if valid_ptr(ptr) {
                    consider(ptr, 0, 50);
                    for &off in &[0x8u32, 0x34] {
                        consider(ptr, off, 25);
                    }
                }
            }
        }
        best
    };

    let mut best: Option<(i32, PositionDiscovery)> = None;
    // Client global is NOT used for world — it's the same as view_client_rva
    // client global برای world استفاده نمی‌شود — همان view_client_rva است
    if let Some(rva) = config_global_hw_rva {
        if let Some(hit) = try_rva(hw_base, rva, 2000) {
            best = Some(hit);
        }
    }
    let _ = config_global_client_rva;
    best.map(|(_, d)| d)
}

/// [EN] Live position discovery — combines multiple strategies with single sleep
/// [FA] کشف موقعیت زنده — ترکیب چندین استراتژی با یک sleep مشترک
///
/// [EN] This is the main live discovery function that tries multiple strategies
/// in order of efficiency:
/// 1. Movement-based discovery (fastest)
/// 2. Float change tracking (slower but thorough)
/// 3. Velocity-based discovery
///
/// [FA] این تابع اصلی کشف زنده است که چندین استراتژی را به ترتیب کارایی امتحان می‌کند:
/// 1. کشف مبتنی بر حرکت (سریع‌ترین)
/// 2. ردیابی تغییر float (کندتر اما کامل‌تر)
/// 3. کشف مبتنی بر سرعت
#[allow(clippy::too_many_arguments)]
pub fn discover_position_live(
    reader: &MemoryReader,
    hw_base: u32,
    client_base: u32,
    candidates: &[PlayerCandidate],
    config_off: u32,
    expected_hp: Option<i32>,
    wait_ms: u64,
    config_global_hw_rva: Option<u32>,
    config_global_client_rva: Option<u32>,
) -> Option<PositionDiscovery> {
    // Strategy 1: Movement-based discovery
    let snaps = collect_all_movement_snaps(
        reader,
        hw_base,
        client_base,
        candidates,
        config_off,
        expected_hp,
        config_global_hw_rva,
        config_global_client_rva,
    );
    if let Some(d) = pick_best_mover(snaps, reader, wait_ms) {
        return Some(d);
    }

    // Strategy 2: Float change tracking (slower but thorough)
    // Priority: hw entity with HP at 0x59C, then other hw entities
    let priority = candidates
        .iter()
        .find(|c| c.from_hw && c.hp_offset == 0x59C)
        .or_else(|| candidates.iter().find(|c| c.from_hw));

    let mut tried = std::collections::HashSet::new();
    if let Some(c) = priority {
        tried.insert(c.player);
        if let Some(d) = discover_by_changing_floats(reader, c.player, 0x8000, wait_ms) {
            return Some(d);
        }
        // Also try pev memory
        for &pev_off in PEV_PTR_OFFS {
            let Ok(pev) = reader.read_u32(c.player.wrapping_add(pev_off)) else {
                continue;
            };
            if !valid_ptr(pev) || !tried.insert(pev) {
                continue;
            }
            if let Some(d) = discover_by_changing_floats(reader, pev, 0x4000, wait_ms) {
                return Some(d);
            }
        }
    }

    // Try remaining candidates
    for c in candidates {
        if !c.from_hw || !tried.insert(c.player) {
            continue;
        }
        if let Some(d) = discover_by_changing_floats(reader, c.player, 0x4000, wait_ms) {
            return Some(d);
        }
    }

    None
}

/// [EN] Print diagnostic vec3 values at important addresses (no movement needed)
/// [FA] چاپ مقادیر vec3 تشخیصی در آدرس‌های مهم (بدون نیاز به حرکت)
///
/// [EN] This is a diagnostic function that dumps current vec3 values at
/// known entity offsets and global RVAs. Useful for debugging and
/// identifying correct offsets for a specific game build.
///
/// [FA] این تابع تشخیصی مقادیر vec3 فعلی را در آفست‌های entity شناخته شده
/// و آدرس‌های سراسری چاپ می‌کند. برای عیب‌یابی و شناسایی آفست‌های صحیح
/// برای یک build خاص بازی مفید است.
pub fn print_position_diagnostics(
    reader: &MemoryReader,
    hw_base: u32,
    client_base: u32,
    entity_hw: u32,
    config_global_hw_rva: Option<u32>,
) {
    println!("  diag (مقادیر فعلی — بدون حرکت):");
    // Check common entity offsets
    for &off in &[0x8u32, 0x34, 0x128, 0x134, 0x334] {
        if let Some((x, y, z)) = peek_vec3(reader, entity_hw, off) {
            println!("    entity+{off:#x}  X={x:.1} Y={y:.1} Z={z:.1}");
        }
    }
    // Check global RVAs
    for &rva in GLOBAL_ORIGIN_RVA_HW {
        if hw_base == 0 {
            break;
        }
        let slot = hw_base.wrapping_add(rva);
        if let Some((x, y, z)) = peek_vec3(reader, slot, 0) {
            println!("    hw+{rva:#x} direct  X={x:.1} Y={y:.1} Z={z:.1}");
        }
        if let Ok(ptr) = reader.read_u32(slot) {
            if valid_ptr(ptr) {
                if let Some((x, y, z)) = peek_vec3(reader, ptr, 0) {
                    println!("    hw+{rva:#x} → {ptr:#x}+0  X={x:.1} Y={y:.1} Z={z:.1}");
                }
                if let Some((x, y, z)) = peek_vec3(reader, ptr, 0x8) {
                    println!("    hw+{rva:#x} → {ptr:#x}+8  X={x:.1} Y={y:.1} Z={z:.1}");
                }
            }
        }
    }
    // Check configured RVA
    if let Some(rva) = config_global_hw_rva {
        if hw_base != 0 {
            let slot = hw_base.wrapping_add(rva);
            if let Some((x, y, z)) = peek_vec3(reader, slot, 0) {
                println!("    config hw+{rva:#x}  X={x:.1} Y={y:.1} Z={z:.1}");
            }
        }
    }
    let _ = client_base;
}

/// [EN] Static fallback position discovery (no movement required)
/// [FA] کشف موقعیت جایگزین ایستا (بدون نیاز به حرکت)
///
/// [EN] When movement-based discovery fails, this function tries to find
/// position by scanning all candidates and their offsets. It scores
/// each valid position based on offset likelihood and HW origin.
///
/// [FA] هنگامی که کشف مبتنی بر حرکت ناموفق است، این تابع سعی می‌کند
/// با اسکن همه نامزدها و آفست‌هایشان موقعیت را پیدا کند.
/// هر موقعیت معتبر را بر اساس احتمال آفست و مبدأ HW امتیازدهی می‌کند.
fn fallback_static(
    reader: &MemoryReader,
    candidates: &[PlayerCandidate],
    config_off: u32,
    expected_hp: Option<i32>,
    health_direct: u32,
    client_hp_off: u32,
) -> Option<PositionDiscovery> {
    let mut best: Option<(i32, PositionDiscovery)> = None;

    for &cand in candidates {
        // Skip client stubs
        if is_client_stub(cand.player, health_direct, client_hp_off) {
            continue;
        }
        if !hp_matches(reader, cand.player, cand.hp_offset, expected_hp) {
            continue;
        }
        for off in offsets_to_try(config_off) {
            let Some((x, y, z)) = read_vec3(reader, cand.player, off) else {
                continue;
            };
            if looks_like_spawn_stub(x, y, z) {
                continue;
            }
            let mut s = score_offset(off, config_off);
            // HW entities get bonus (more reliable source)
            if cand.from_hw {
                s += 2000;
            }
            // HP at 0x59C is most common — give bonus
            if cand.hp_offset == 0x59C {
                s += 500;
            }
            s += (x.abs() + y.abs()) as i32 / 100;
            if best.as_ref().map(|(bs, _)| s > *bs).unwrap_or(true) {
                best = Some((
                    s,
                    PositionDiscovery {
                        player: cand.player,
                        offset: off,
                    },
                ));
            }
        }
    }
    best.map(|(_, d)| d)
}

/// [EN] Discover player position and offset — combined movement + static
/// [FA] کشف موقعیت و آفست بازیکن — ترکیب حرکت + ایستا
///
/// [EN] This is the top-level discovery function that first tries movement-based
/// discovery (fastest and most reliable), then falls back to static analysis.
/// Used by the dump/verdict system to suggest the correct player offset.
///
/// [FA] این تابع کشف سطح بالا است که ابتدا کشف مبتنی بر حرکت (سریع‌ترین
/// و قابل اعتمادترین) را امتحان می‌کند، سپس به تحلیل ایستا بازمی‌گردد.
/// توسط سیستم dump/verdict برای پیشنهاد آفست صحیح بازیکن استفاده می‌شود.
#[allow(clippy::too_many_arguments)]
pub fn discover_player_and_offset(
    reader: &MemoryReader,
    hw_base: u32,
    client_base: u32,
    candidates: &[PlayerCandidate],
    config_off: u32,
    expected_hp: Option<i32>,
    health_direct: u32,
    client_hp_off: u32,
    config_global_hw_rva: Option<u32>,
    config_global_client_rva: Option<u32>,
) -> Option<PositionDiscovery> {
    // Try movement-based discovery first (250ms wait)
    if let Some(d) = discover_position_live(
        reader,
        hw_base,
        client_base,
        candidates,
        config_off,
        expected_hp,
        250,
        config_global_hw_rva,
        config_global_client_rva,
    ) {
        return Some(d);
    }
    // Fall back to static analysis
    fallback_static(
        reader,
        candidates,
        config_off,
        expected_hp,
        health_direct,
        client_hp_off,
    )
}

/// [EN] Legacy offset discovery alias
/// [FA] نام مستعار کشف آفست قدیمی
///
/// [EN] Simplified function that finds the best offset for a given player
/// entity without HP verification. Used by older code paths.
///
/// [FA] تابع ساده شده که بهترین آفست را برای یک entity بازیکن خاص
/// بدون تأیید HP پیدا می‌کند. توسط مسیرهای کد قدیمی‌تر استفاده می‌شود.
pub fn discover_offset(reader: &MemoryReader, player: u32, config_off: u32) -> Option<u32> {
    let cand = PlayerCandidate {
        player,
        hp_offset: 0x14,
        from_hw: false,
    };
    let hits = collect_entity_hits(reader, cand, config_off, None, true, false);
    hits.into_iter()
        .map(|(_, off)| off)
        .max_by_key(|&off| score_offset(off, config_off))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_map_coords() {
        assert!(looks_like_coords(1024.0, -512.0, 64.0));
        assert!(!looks_like_coords(0.0, 0.0, 0.0));
        assert!(!looks_like_coords(0.0, 0.0, 8192.0));
        assert!(!looks_like_coords(0.2, -0.1, 0.3));
    }

    #[test]
    fn usable_position_rejects_zero() {
        assert!(!is_usable_position(0.0, 0.0, 0.0));
        assert!(is_usable_position(148.0, 15.4, 7.9));
        assert!(!is_usable_position(300.0, 0.0, 0.0));
    }

    #[test]
    fn view_aux_vs_world_origin() {
        assert!(looks_like_view_aux(164.0, 1.0, 140.0));
        assert!(!looks_like_world_origin(164.0, 1.0, 140.0));
        assert!(looks_like_world_origin(512.0, -300.0, 64.0));
        assert!(looks_like_world_origin(113.8, 21.8, 19.9));
        // entity misread — pitch byte as Y
        assert!(looks_like_view_aux(2190.0, -128.0, 0.0));
        assert!(!looks_like_world_origin(2190.0, -128.0, 0.0));
    }

    #[test]
    fn spawn_stub_rejected() {
        assert!(looks_like_spawn_stub(0.0, 300.0, 0.0));
        assert!(looks_like_spawn_stub(300.0, 0.0, 0.0));
        assert!(!looks_like_spawn_stub(512.0, -300.0, 64.0));
    }

    #[test]
    fn movement_delta() {
        let a = (100.0, 200.0, 64.0);
        let b = (110.0, 200.0, 64.0);
        assert!(pos_delta_sq(a, b) > 0.04);
        assert!(pos_delta_sq(a, a) < 0.04);
    }
}
