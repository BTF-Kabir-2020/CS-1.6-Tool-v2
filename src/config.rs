//! [EN] Configuration module — TOML-based config parsing, validation, and hex/color helpers.
//! [FA] ماژول پیکربندی — پارس پیکربندی مبتنی بر TOML، اعتبارسنجی، و کمک‌کننده‌های hex/رنگ.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::error::AppError;

/// [EN] Data type for memory values — determines read size and interpretation.
/// [FA] نوع داده برای مقادیر حافظه — اندازه خواندن و تفسیر را تعیین می‌کند.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValueType {
    /// [EN] 32-bit signed integer.
    /// [FA] عدد صحیح ۳۲ بیتی با علامت.
    Int,
    /// [EN] 32-bit IEEE 754 floating point (default).
    /// [FA] عدد اعشاری ۳۲ بیتی IEEE 754 (پیش‌فرض).
    #[default]
    Float,
    /// [EN] Unsigned 8-bit byte.
    /// [FA] بایت ۸ بیتی بدون علامت.
    Byte,
}

impl ValueType {
    /// [EN] Parses a string into a `ValueType`. Accepts aliases like "i32", "4bytes", "f32", "u8".
    /// [FA] یک رشته را به `ValueType` پارس می‌کند. نام‌های مستعار مانند "i32"، "4bytes"، "f32"، "u8" را قبول می‌کند.
    pub fn parse(s: &str) -> Result<Self, AppError> {
        match s.to_lowercase().as_str() {
            "int" | "i32" | "4bytes" => Ok(Self::Int),
            "float" | "f32" => Ok(Self::Float),
            "byte" | "u8" => Ok(Self::Byte),
            other => Err(AppError::Config(format!("نوع نامعتبر: {other}"))),
        }
    }
}

/// [EN] Screen corner position for the overlay display.
/// [FA] موقعیت گوشه صفحه برای نمایش overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayPosition {
    /// [EN] Top-left corner of the screen.
    /// [FA] گوشه بالا-چپ صفحه.
    TopLeft,
    /// [EN] Top-right corner of the screen.
    /// [FA] گوشه بالا-راست صفحه.
    TopRight,
    /// [EN] Bottom-left corner of the screen.
    /// [FA] گوشه پایین-چپ صفحه.
    BottomLeft,
    /// [EN] Bottom-right corner of the screen.
    /// [FA] گوشه پایین-راست صفحه.
    BottomRight,
}

impl OverlayPosition {
    /// [EN] Parses a string into an `OverlayPosition`. Accepts hyphenated and underscored forms.
    /// [FA] یک رشته را به `OverlayPosition` پارس می‌کند. فرم‌های با خط تیره و خط زیر را قبول می‌کند.
    pub fn parse(s: &str) -> Result<Self, AppError> {
        match s.to_lowercase().replace('_', "-").as_str() {
            "top-left" => Ok(Self::TopLeft),
            "top-right" => Ok(Self::TopRight),
            "bottom-left" => Ok(Self::BottomLeft),
            "bottom-right" => Ok(Self::BottomRight),
            other => Err(AppError::Config(format!("position نامعتبر: {other}"))),
        }
    }
}

/// [EN] Root application configuration — loaded from `config.toml` or embedded defaults.
/// [FA] پیکربندی اصلی برنامه — از `config.toml` یا پیش‌فرض‌های تعبیه‌شده بارگذاری می‌شود.
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    /// [EN] Target process configuration (name to find).
    /// [FA] پیکربندی پروسس هدف (نام برای یافتن).
    pub process: ProcessConfig,
    /// [EN] DLL module names to resolve base addresses.
    /// [FA] نام‌های ماژول DLL برای resolve کردن آدرس‌های پایه.
    pub modules: ModulesConfig,
    /// [EN] Target values for memory writes (money, clip, reserve).
    /// [FA] مقادیر هدف برای نوشتن حافظه (پول، خشاب، ذخیره).
    pub targets: TargetsConfig,
    /// [EN] Feature toggles (write, overlay, debug, etc.).
    /// [FA] کلیدهای فعال/غیرفعال بودن قابلیت‌ها (نوشتن، overlay، دیباگ و غیره).
    pub features: FeaturesConfig,
    /// [EN] Timing configuration for loops and retries.
    /// [FA] پیکربندی زمان‌بندی برای حلقه‌ها و تلاش‌های مجدد.
    pub timing: TimingConfig,
    /// [EN] Entity-based memory reading configuration.
    /// [FA] پیکربندی خواندن حافظه مبتنی بر entity.
    #[serde(default)]
    pub entity: EntityConfig,
    /// [EN] Overlay display configuration.
    /// [FA] پیکربندی نمایش overlay.
    pub overlay: OverlayConfig,
    /// [EN] Pointer chain configurations for memory traversal.
    /// [FA] پیکربندی‌های زنجیره اشاره‌گر برای پیمایش حافظه.
    pub chains: ChainsConfig,
    /// [EN] Clip detection range configuration.
    /// [FA] پیکربندی محدوده تشخیص خشاب.
    pub clip_detection: ClipDetectionConfig,
    /// [EN] HUD synchronization configuration.
    /// [FA] پیکربندی همگام‌سازی HUD.
    #[serde(default)]
    pub hud_sync: HudSyncConfig,
    /// [EN] Debug console output configuration.
    /// [FA] پیکربندی خروجی کنسول دیباگ.
    #[serde(default)]
    pub debug_console: DebugConsoleConfig,
}

/// [EN] Process targeting configuration.
/// [FA] پیکربندی هدف‌گیری پروسس.
#[derive(Debug, Clone, Deserialize)]
pub struct ProcessConfig {
    /// [EN] Name of the target process to find (e.g., "hl.exe").
    /// [FA] نام پروسس هدف برای یافتن (مثلاً "hl.exe").
    pub name: String,
}

/// [EN] DLL module names used for address resolution.
/// [FA] نام‌های ماژول DLL استفاده شده برای resolve کردن آدرس‌ها.
#[derive(Debug, Clone, Deserialize)]
pub struct ModulesConfig {
    /// [EN] Main hardware module (e.g., "hw.dll").
    /// [FA] ماژول سخت‌افزاری اصلی (مثلاً "hw.dll").
    pub hw: String,
    /// [EN] Software module (e.g., "sw.dll").
    /// [FA] ماژول نرم‌افزاری (مثلاً "sw.dll").
    #[serde(default = "default_sw")]
    pub sw: String,
    /// [EN] Client module (e.g., "client.dll").
    /// [FA] ماژول کلاینت (مثلاً "client.dll").
    pub client: String,
}

/// [EN] Default software module name.
/// [FA] نام پیش‌فرض ماژول نرم‌افزاری.
fn default_sw() -> String {
    "sw.dll".into()
}

/// [EN] Target values to write into game memory.
/// [FA] مقادیر هدف برای نوشتن در حافظه بازی.
#[derive(Debug, Clone, Deserialize)]
pub struct TargetsConfig {
    /// [EN] Target money amount.
    /// [FA] مبلغ پول هدف.
    pub money: i32,
    /// [EN] Target clip (current magazine) ammo count.
    /// [FA] تعداد خشاب هدف (مجله فعلی).
    pub clip: i32,
    /// [EN] Target reserve ammo count.
    /// [FA] تعداد خشاب ذخیره هدف.
    pub reserve: i32,
}

/// [EN] Feature toggles for controlling tool behavior.
/// [FA] کلیدهای فعال/غیرفعال برای کنترل رفتار ابزار.
#[derive(Debug, Clone, Deserialize)]
pub struct FeaturesConfig {
    /// [EN] Enable writing to game memory.
    /// [FA] فعال کردن نوشتن در حافظه بازی.
    pub write_enabled: bool,
    /// [EN] Enable the transparent overlay window.
    /// [FA] فعال کردن پنجره شفاف overlay.
    pub overlay_enabled: bool,
    /// [EN] Enable printing resolved addresses for debugging.
    /// [FA] فعال کردن چاپ آدرس‌های resolve شده برای دیباگ.
    #[serde(default)]
    pub debug_addresses: bool,
    /// [EN] Enable money reading/writing.
    /// [FA] فعال کردن خواندن/نوشتن پول.
    #[serde(default = "default_true")]
    pub money_enabled: bool,
    /// [EN] Enable clip ammo reading.
    /// [FA] فعال کردن خواندن خشاب مجله.
    #[serde(default = "default_true")]
    pub clip_enabled: bool,
    /// [EN] Enable clip ammo writing.
    /// [FA] فعال کردن نوشتن خشاب مجله.
    #[serde(default = "default_true")]
    pub clip_write_enabled: bool,
    /// [EN] Enable reserve ammo reading.
    /// [FA] فعال کردن خواندن خشاب ذخیره.
    #[serde(default = "default_true")]
    pub reserve_enabled: bool,
    /// [EN] Enable reserve ammo writing.
    /// [FA] فعال کردن نوشتن خشاب ذخیره.
    #[serde(default = "default_true")]
    pub reserve_write_enabled: bool,
    /// [EN] Enable health reading from entity.
    /// [FA] فعال کردن خواندن سلامتی از entity.
    #[serde(default)]
    pub hp_enabled: bool,
    /// [EN] Enable armor reading from entity.
    /// [FA] فعال کردن خواندن زره از entity.
    #[serde(default)]
    pub armor_enabled: bool,
    /// [EN] Enable position reading from entity.
    /// [FA] فعال کردن خواندن موقعیت از entity.
    #[serde(default)]
    pub position_enabled: bool,
}

/// [EN] Default boolean value: true.
/// [FA] مقدار پیش‌فرض بولی: درست.
fn default_true() -> bool {
    true
}

/// [EN] Timing configuration for loops and reconnection.
/// [FA] پیکربندی زمان‌بندی برای حلقه‌ها و اتصال مجدد.
#[derive(Debug, Clone, Deserialize)]
pub struct TimingConfig {
    /// [EN] Delay between memory loop iterations (milliseconds).
    /// [FA] تأخیر بین تکرارهای حلقه حافظه (میلی‌ثانیه).
    pub memory_loop_ms: u64,
    /// [EN] Delay between overlay render updates (milliseconds).
    /// [FA] تأخیر بین به‌روزرسانی‌های رندر overlay (میلی‌ثانیه).
    pub overlay_loop_ms: u64,
    /// [EN] Delay between connection retry attempts (milliseconds).
    /// [FA] تأخیر بین تلاش‌های مجدد اتصال (میلی‌ثانیه).
    #[serde(default = "default_retry")]
    pub connect_retry_ms: u64,
    /// [EN] Number of stale ticks before triggering a reconnection.
    /// [FA] تعداد tick‌های منقضی قبل از ایجاد اتصال مجدد.
    #[serde(default = "default_stale")]
    pub stale_reconnect_ticks: u32,
}

/// [EN] Default connection retry delay: 250ms.
/// [FA] تأخیر پیش‌فرض تلاش مجدد اتصال: ۲۵۰ میلی‌ثانیه.
fn default_retry() -> u64 {
    250
}
/// [EN] Default stale reconnect threshold: 40 ticks.
/// [FA] آستانه پیش‌فرض اتصال مجدد منقضی: ۴۰ tick.
fn default_stale() -> u32 {
    40
}

/// [EN] Entity-based memory reading configuration — uses known CS 1.6 offsets.
/// [FA] پیکربندی خواندن حافظه مبتنی بر entity — از offsetهای شناخته‌شده CS 1.6 استفاده می‌کند.
/// [FA] خواندن از Local Player entity — offsetهای شناخته‌شده CS 1.6 (BLASTHACK dumper).
#[derive(Debug, Clone, Deserialize)]
pub struct EntityConfig {
    /// [EN] Enable entity-based reading.
    /// [FA] فعال کردن خواندن مبتنی بر entity.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// [EN] Module to use for local player base: `hw` = hw.dll + local_player_rva, `client` = client.dll + local_player_rva.
    /// [FA] ماژول برای استفاده از پایه بازیکن محلی: `hw` = hw.dll + local_player_rva ، `client` = client.dll + local_player_rva
    #[serde(default = "default_local_player_module")]
    pub local_player_module: String,
    /// [EN] RVA of the local player pointer within the selected module.
    /// [FA] RVA اشاره‌گر بازیکن محلی در ماژول انتخابی.
    #[serde(default = "default_local_player_rva")]
    pub local_player_rva: String,
    /// [EN] Offset from entity base to money field.
    /// [FA] آفست از پایه entity تا فیلد پول.
    #[serde(default = "default_money_offset")]
    pub money_offset: String,
    /// [EN] Offset from entity base to health field.
    /// [FA] آفست از پایه entity تا فیلد سلامتی.
    #[serde(default = "default_health_offset")]
    pub health_offset: String,
    /// [EN] Offset from entity base to armor field.
    /// [FA] آفست از پایه entity تا فیلد زره.
    #[serde(default = "default_armor_offset")]
    pub armor_offset: String,
    /// [EN] Data type for health field ("float" or "int").
    /// [FA] نوع داده فیلد سلامتی ("float" یا "int").
    #[serde(default = "default_float_type")]
    pub health_type: String,
    /// [EN] Data type for armor field ("float" or "int").
    /// [FA] نوع داده فیلد زره ("float" یا "int").
    #[serde(default = "default_float_type")]
    pub armor_type: String,
    /// [EN] client.dll direct RVA for health — fallback if entity fails.
    /// [FA] RVA مستقیم client.dll برای سلامتی — جایگزین اگر entity کار نکرد.
    #[serde(default)]
    pub health_direct_rva: Option<String>,
    /// [EN] client.dll direct RVA for armor.
    /// [FA] RVA مستقیم client.dll برای زره.
    #[serde(default)]
    pub armor_direct_rva: Option<String>,
    /// [EN] Offset from entity to position vec3 (X,Y,Z) — typically entvars.origin = 0x8.
    /// [FA] آفست از entity تا موقعیت vec3 (X,Y,Z) — معمولاً entvars.origin = 0x8.
    #[serde(default = "default_position_offset")]
    pub position_offset: String,
    /// [EN] hw.dll RVA for entity engine (e.g., 0x169438).
    /// [FA] RVA hw.dll برای entity موتور (مثلاً 0x169438).
    #[serde(default)]
    pub position_entity_hw_rva: Option<String>,
    /// [EN] hw.dll RVA — direct vec3 (e.g., EntityOrigin).
    /// [FA] RVA hw.dll — vec3 مستقیم (مثلاً EntityOrigin).
    #[serde(default)]
    pub position_global_hw_rva: Option<String>,
    /// [EN] client.dll RVA — direct vec3 (e.g., LocalOrigin).
    /// [FA] RVA client.dll — vec3 مستقیم (مثلاً LocalOrigin).
    #[serde(default)]
    pub position_global_client_rva: Option<String>,
    /// [EN] client.dll RVA — camera/view angles (NOT map XYZ), e.g., 0x11D478.
    /// [FA] RVA client.dll — زوایای دوربین/نمایش (نه XYZ نقشه)، مثلاً 0x11D478.
    #[serde(default)]
    pub view_client_rva: Option<String>,
}

/// [EN] Default local player module: "hw".
/// [FA] ماژول پیش‌فرض بازیکن محلی: "hw".
fn default_local_player_module() -> String {
    "hw".into()
}

/// [EN] Default local player RVA: "0x32ABF4".
/// [FA] RVA پیش‌فرض بازیکن محلی: "0x32ABF4".
fn default_local_player_rva() -> String {
    "0x32ABF4".into()
}
/// [EN] Default money offset: "0xE4".
/// [FA] آفست پیش‌فرض پول: "0xE4".
fn default_money_offset() -> String {
    "0xE4".into()
}
/// [EN] Default health offset: "0xB74".
/// [FA] آفست پیش‌فرض سلامتی: "0xB74".
fn default_health_offset() -> String {
    "0xB74".into()
}
/// [EN] Default armor offset: "0x10C".
/// [FA] آفست پیش‌فرض زره: "0x10C".
fn default_armor_offset() -> String {
    "0x10C".into()
}
/// [EN] Default position offset: "0x8".
/// [FA] آفست پیش‌فرض موقعیت: "0x8".
fn default_position_offset() -> String {
    "0x8".into()
}
/// [EN] Default data type: "float".
/// [FA] نوع داده پیش‌فرض: "float".
fn default_float_type() -> String {
    "float".into()
}

impl Default for EntityConfig {
    /// [EN] Creates an `EntityConfig` with all default CS 1.6 offsets.
    /// [FA] یک `EntityConfig` با تمام آفست‌های پیش‌فرض CS 1.6 ایجاد می‌کند.
    fn default() -> Self {
        Self {
            enabled: true,
            local_player_module: default_local_player_module(),
            local_player_rva: default_local_player_rva(),
            money_offset: default_money_offset(),
            health_offset: default_health_offset(),
            armor_offset: default_armor_offset(),
            health_type: default_float_type(),
            armor_type: default_float_type(),
            health_direct_rva: None,
            armor_direct_rva: None,
            position_offset: default_position_offset(),
            position_entity_hw_rva: None,
            position_global_hw_rva: None,
            position_global_client_rva: None,
            view_client_rva: None,
        }
    }
}

/// [EN] Pointer chain configuration — base RVA plus a list of dereference offsets.
/// [FA] پیکربندی زنجیره اشاره‌گر — RVA پایه به اضافه لیستی از آفست‌های dereference.
#[derive(Debug, Clone, Deserialize)]
pub struct PointerChainConfig {
    /// [EN] Base RVA of the pointer chain.
    /// [FA] RVA پایه زنجیره اشاره‌گر.
    pub base_rva: String,
    /// [EN] List of hex offsets to dereference along the chain.
    /// [FA] لیست آفست‌های hex برای dereference در طول زنجیره.
    pub offsets: Vec<String>,
}

/// [EN] Fallback configuration for direct money reading from client.dll.
/// [FA] پیکربندی جایگزین برای خواندن مستقیم پول از client.dll.
#[derive(Debug, Clone, Deserialize)]
pub struct MoneyClientFallback {
    /// [EN] Direct RVA to money value in client.dll.
    /// [FA] RVA مستقیم به مقدار پول در client.dll.
    pub direct_rva: String,
}

/// [EN] Pointer chain configurations for all memory targets.
/// [FA] پیکربندی‌های زنجیره اشاره‌گر برای تمام اهداف حافظه.
#[derive(Debug, Clone, Deserialize)]
pub struct ChainsConfig {
    /// [EN] Pointer chain for reserve ammo reading.
    /// [FA] زنجیره اشاره‌گر برای خواندن خشاب ذخیره.
    pub reserve: PointerChainConfig,
    /// [EN] Pointer chain for money reading via hw.dll.
    /// [FA] زنجیره اشاره‌گر برای خواندن پول از طریق hw.dll.
    pub money_hw: PointerChainConfig,
    /// [EN] Fallback for direct money reading from client.dll.
    /// [FA] جایگزین برای خواندن مستقیم پول از client.dll.
    pub money_client_fallback: MoneyClientFallback,
    /// [EN] Pointer chains for clip ammo (multiple weapons supported).
    /// [FA] زنجیره‌های اشاره‌گر برای خشاب مجله (پشتیبانی از چند سلاح).
    pub clip: Vec<PointerChainConfig>,
    /// [EN] Index into `chains.clip` used for reserve ammo — excluded from magazine selection.
    /// [FA] اندیس `chains.clip` برای reserve (ذخیره) — از انتخاب magazine حذف می‌شود.
    #[serde(default = "default_reserve_clip_index")]
    pub reserve_clip_index: usize,
}

/// [EN] Default reserve clip chain index: 2.
/// [FA] اندیس پیش‌فرض زنجیره خشاب ذخیره: ۲.
fn default_reserve_clip_index() -> usize {
    2
}

/// [EN] Clip detection configuration — valid range for magazine ammo values.
/// [FA] پیکربندی تشخیص خشاب — محدوده معتبر برای مقادیر خشاب مجله.
#[derive(Debug, Clone, Deserialize)]
pub struct ClipDetectionConfig {
    /// [EN] Minimum valid clip ammo value.
    /// [FA] حداقل مقدار معتبر خشاب مجله.
    pub min_value: i32,
    /// [EN] Maximum valid clip ammo value.
    /// [FA] حداکثر مقدار معتبر خشاب مجله.
    pub max_value: i32,
}

/// [EN] HUD synchronization configuration — syncs clip value to game HUD.
/// [FA] پیکربندی همگام‌سازی HUD — مقدار خشاب را با HUD بازی همگام می‌کند.
#[derive(Debug, Clone, Deserialize)]
pub struct HudSyncConfig {
    /// [EN] Enable HUD synchronization.
    /// [FA] فعال کردن همگام‌سازی HUD.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// [EN] Offset to the clip struct within the HUD data.
    /// [FA] آفست به ساختار clip در داده‌های HUD.
    #[serde(default = "default_clip_struct")]
    pub clip_struct_offset: String,
    /// [EN] Offset to the HUD clip display value.
    /// [FA] آفست به مقدار نمایش خشاب در HUD.
    #[serde(default = "default_hud_clip")]
    pub hud_clip_offset: String,
}

/// [EN] Default clip struct offset: "0xCC".
/// [FA] آفست پیش‌فرض ساختار clip: "0xCC".
fn default_clip_struct() -> String {
    "0xCC".into()
}
/// [EN] Default HUD clip offset: "0xD0".
/// [FA] آفست پیش‌فرض خشاب HUD: "0xD0".
fn default_hud_clip() -> String {
    "0xD0".into()
}

impl Default for HudSyncConfig {
    /// [EN] Creates a default `HudSyncConfig` with HUD sync enabled.
    /// [FA] یک `HudSyncConfig` پیش‌فرض با همگام‌سازی HUD فعال ایجاد می‌کند.
    fn default() -> Self {
        Self {
            enabled: true,
            clip_struct_offset: default_clip_struct(),
            hud_clip_offset: default_hud_clip(),
        }
    }
}

/// [EN] Overlay window configuration — position, font, colors, and display options.
/// [FA] پیکربندی پنجره overlay — موقعیت، فونت، رنگ‌ها، و گزینه‌های نمایش.
#[derive(Debug, Clone, Deserialize)]
pub struct OverlayConfig {
    /// [EN] Horizontal pixel offset from the screen edge.
    /// [FA] آفست افقی پیکسل از لبه صفحه.
    pub offset_x: i32,
    /// [EN] Vertical pixel offset from the screen edge.
    /// [FA] آفست عمودی پیکسل از لبه صفحه.
    pub offset_y: i32,
    /// [EN] Font size in pixels.
    /// [FA] اندازه فونت به پیکسل.
    pub font_size: i32,
    /// [EN] Font family name (e.g., "Consolas").
    /// [FA] نام خانواده فونت (مثلاً "Consolas").
    pub font_name: String,
    /// [EN] Use bold font weight.
    /// [FA] استفاده از وزن فونت بولد.
    #[serde(default = "default_true")]
    pub font_bold: bool,
    /// [EN] Vertical spacing between text lines (pixels).
    /// [FA] فاصله عمودی بین خطوط متن (پیکسل).
    #[serde(default = "default_spacing")]
    pub line_spacing: i32,
    /// [EN] Corner position for the overlay (e.g., "top-left").
    /// [FA] موقعیت گوشه برای overlay (مثلاً "top-left").
    #[serde(default = "default_pos")]
    pub position: String,
    /// [EN] Margin from the screen edge (pixels).
    /// [FA] حاشیه از لبه صفحه (پیکسل).
    #[serde(default = "default_margin")]
    pub margin: i32,
    /// [EN] Color configuration for overlay text elements.
    /// [FA] پیکربندی رنگ برای عناصر متن overlay.
    pub colors: OverlayColors,
    /// [EN] Which elements to display in the overlay.
    /// [FA] کدام عناصر در overlay نمایش داده شوند.
    pub display: OverlayDisplay,
}

/// [EN] Default line spacing: 22 pixels.
/// [FA] فاصله پیش‌فرض خط: ۲۲ پیکسل.
fn default_spacing() -> i32 {
    22
}
/// [EN] Default overlay position: "top-left".
/// [FA] موقعیت پیش‌فرض overlay: "top-left".
fn default_pos() -> String {
    "top-left".into()
}
/// [EN] Default margin: 12 pixels.
/// [FA] حاشیه پیش‌فرض: ۱۲ پیکسل.
fn default_margin() -> i32 {
    12
}

/// [EN] Overlay text color configuration — hex color strings (e.g., "0x00D7FF").
/// [FA] پیکربندی رنگ متن overlay — رشته‌های رنگ hex (مثلاً "0x00D7FF").
#[derive(Debug, Clone, Deserialize)]
pub struct OverlayColors {
    /// [EN] Money display color.
    /// [FA] رنگ نمایش پول.
    #[serde(default = "default_gold")]
    pub money: String,
    /// [EN] Ammo display color.
    /// [FA] رنگ نمایش خشاب.
    #[serde(default = "default_green")]
    pub ammo: String,
    /// [EN] Health display color.
    /// [FA] رنگ نمایش سلامتی.
    #[serde(default = "default_red")]
    pub hp: String,
    /// [EN] Armor display color.
    /// [FA] رنگ نمایش زره.
    #[serde(default = "default_blue")]
    pub armor: String,
    /// [EN] Position display color.
    /// [FA] رنگ نمایش موقعیت.
    #[serde(default = "default_cyan")]
    pub position: String,
    /// [EN] Default/fallback color.
    /// [FA] رنگ پیش‌فرض/جایگزین.
    #[serde(default = "default_green")]
    pub default: String,
}

/// [EN] Default money color: gold (0x00D7FF).
/// [FA] رنگ پیش‌فرض پول: طلایی (0x00D7FF).
fn default_gold() -> String {
    "0x00D7FF".into()
}
/// [EN] Default ammo/default color: green (0x00FF00).
/// [FA] رنگ پیش‌فرض خشاب/پیش‌فرض: سبز (0x00FF00).
fn default_green() -> String {
    "0x00FF00".into()
}
/// [EN] Default health color: red (0x4444FF — BGR format).
/// [FA] رنگ پیش‌فرض سلامتی: قرمز (0x4444FF — فرمت BGR).
fn default_red() -> String {
    "0x4444FF".into()
}
/// [EN] Default armor color: blue (0xFFAA00 — BGR format).
/// [FA] رنگ پیش‌فرض زره: آبی (0xFFAA00 — فرمت BGR).
fn default_blue() -> String {
    "0xFFAA00".into()
}
/// [EN] Default position color: cyan (0xFFFF88 — BGR format).
/// [FA] رنگ پیش‌فرض موقعیت: فیروزه‌ای (0xFFFF88 — فرمت BGR).
fn default_cyan() -> String {
    "0xFFFF88".into()
}

/// [EN] Overlay element visibility configuration.
/// [FA] پیکربندی نمایان بودن عناصر overlay.
#[derive(Debug, Clone, Deserialize)]
pub struct OverlayDisplay {
    /// [EN] Show money in overlay.
    /// [FA] نمایش پول در overlay.
    pub show_money: bool,
    /// [EN] Show ammo counts in overlay.
    /// [FA] نمایش تعداد خشاب در overlay.
    pub show_ammo: bool,
    /// [EN] Show health in overlay.
    /// [FA] نمایش سلامتی در overlay.
    pub show_hp: bool,
    /// [EN] Show armor in overlay.
    /// [FA] نمایش زره در overlay.
    pub show_armor: bool,
    /// [EN] Show player position coordinates.
    /// [FA] نمایش مختصات موقعیت بازیکن.
    #[serde(default)]
    pub show_position: bool,
    /// [EN] Show auxiliary view information.
    /// [FA] نمایش اطلاعات نمای کمکی.
    #[serde(default = "default_true")]
    pub show_view_aux: bool,
}

/// [EN] Debug console configuration — controls periodic debug output.
/// [FA] پیکربندی کنسول دیباگ — کنترل خروجی دوره‌ای دیباگ.
#[derive(Debug, Clone, Deserialize)]
pub struct DebugConsoleConfig {
    /// [EN] Enable debug console output.
    /// [FA] فعال کردن خروجی کنسول دیباگ.
    #[serde(default)]
    pub enabled: bool,
    /// [EN] Interval between debug output updates (milliseconds).
    /// [FA] فاصله بین به‌روزرسانی‌های خروجی دیباگ (میلی‌ثانیه).
    #[serde(default = "default_interval")]
    pub interval_ms: u64,
    /// [EN] Clear screen before each debug output update.
    /// [FA] پاک کردن صفحه قبل از هر به‌روزرسانی خروجی دیباگ.
    #[serde(default = "default_true")]
    pub clear_screen: bool,
}

/// [EN] Default debug interval: 1000ms.
/// [FA] فاصله پیش‌فرض دیباگ: ۱۰۰۰ میلی‌ثانیه.
fn default_interval() -> u64 {
    1000
}

impl Default for DebugConsoleConfig {
    /// [EN] Creates a default `DebugConsoleConfig` with debug disabled.
    /// [FA] یک `DebugConsoleConfig` پیش‌فرض با دیباگ غیرفعال ایجاد می‌کند.
    fn default() -> Self {
        Self {
            enabled: false,
            interval_ms: default_interval(),
            clear_screen: true,
        }
    }
}

impl AppConfig {
    /// [EN] Embedded default config.toml — used when no external config file is found.
    /// [FA] پیکربندی پیش‌فرض تعبیه‌شده config.toml — وقتی فایل پیکربندی خارجی یافت نشود استفاده می‌شود.
    const EMBEDDED: &'static str = include_str!("../config.toml");

    /// [EN] Loads configuration from the given path, falling back to embedded defaults.
    /// [FA] پیکربندی را از مسیر داده شده بارگذاری می‌کند و در صورت عدم وجود از پیش‌فرض‌های تعبیه‌شده استفاده می‌کند.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, AppError> {
        let text = if path.as_ref().exists() {
            fs::read_to_string(path.as_ref())
                .map_err(|e| AppError::Config(format!("خواندن config: {e}")))?
        } else {
            eprintln!("config.toml پیدا نشد — از نسخه پیش‌فرض استفاده می‌شود");
            Self::EMBEDDED.to_string()
        };
        let cfg: Self =
            toml::from_str(&text).map_err(|e| AppError::Config(format!("parse: {e}")))?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// [EN] Validates the loaded configuration for required fields and valid values.
    /// [FA] پیکربندی بارگذاری شده را برای فیلدهای ضروری و مقادیر معتبر اعتبارسنجی می‌کند.
    fn validate(&self) -> Result<(), AppError> {
        if self.process.name.is_empty() {
            return Err(AppError::Config("process.name خالی".into()));
        }
        if self.chains.clip.is_empty() {
            return Err(AppError::Config("حداقل یک chains.clip لازم است".into()));
        }
        OverlayPosition::parse(&self.overlay.position)?;
        Ok(())
    }
}

/// [EN] Parses a hex string (with or without "0x" prefix) into a `u32`.
/// [FA] یک رشته hex (با یا بدون پیشوند "0x") را به `u32` پارس می‌کند.
pub fn parse_hex_u32(value: &str) -> Result<u32, AppError> {
    let trimmed = value.trim();
    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    u32::from_str_radix(hex, 16).map_err(|e| AppError::Config(format!("hex '{value}': {e}")))
}

/// [EN] Parses a vector of hex strings into a vector of `u32` values.
/// [FA] یک بردار از رشته‌های hex را به بردار مقادیر `u32` پارس می‌کند.
pub fn parse_offsets(values: &[String]) -> Result<Vec<u32>, AppError> {
    values.iter().map(|v| parse_hex_u32(v)).collect()
}

/// [EN] Parses a hex color string into a `u32` value (delegates to `parse_hex_u32`).
/// [FA] یک رشته رنگ hex را به مقدار `u32` پارس می‌کند (به `parse_hex_u32` واگذار می‌کند).
pub fn parse_color(value: &str) -> Result<u32, AppError> {
    parse_hex_u32(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [EN] Tests hex string parsing with and without "0x" prefix.
    /// [FA] تست پارس رشته hex با و بدون پیشوند "0x".
    #[test]
    fn parse_hex() {
        assert_eq!(parse_hex_u32("0x32ABF4").unwrap(), 0x32ABF4);
        assert_eq!(parse_hex_u32("E4").unwrap(), 0xE4);
    }
}
