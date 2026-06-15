use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValueType {
    Int,
    #[default]
    Float,
    Byte,
}

impl ValueType {
    pub fn parse(s: &str) -> Result<Self, AppError> {
        match s.to_lowercase().as_str() {
            "int" | "i32" | "4bytes" => Ok(Self::Int),
            "float" | "f32" => Ok(Self::Float),
            "byte" | "u8" => Ok(Self::Byte),
            other => Err(AppError::Config(format!("نوع نامعتبر: {other}"))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayPosition {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl OverlayPosition {
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

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub process: ProcessConfig,
    pub modules: ModulesConfig,
    pub targets: TargetsConfig,
    pub features: FeaturesConfig,
    pub timing: TimingConfig,
    #[serde(default)]
    pub entity: EntityConfig,
    pub overlay: OverlayConfig,
    pub chains: ChainsConfig,
    pub clip_detection: ClipDetectionConfig,
    #[serde(default)]
    pub hud_sync: HudSyncConfig,
    #[serde(default)]
    pub debug_console: DebugConsoleConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProcessConfig {
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModulesConfig {
    pub hw: String,
    #[serde(default = "default_sw")]
    pub sw: String,
    pub client: String,
}

fn default_sw() -> String {
    "sw.dll".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct TargetsConfig {
    pub money: i32,
    pub clip: i32,
    pub reserve: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FeaturesConfig {
    pub write_enabled: bool,
    pub overlay_enabled: bool,
    #[serde(default)]
    pub debug_addresses: bool,
    #[serde(default = "default_true")]
    pub money_enabled: bool,
    #[serde(default = "default_true")]
    pub clip_enabled: bool,
    #[serde(default = "default_true")]
    pub clip_write_enabled: bool,
    #[serde(default = "default_true")]
    pub reserve_enabled: bool,
    #[serde(default = "default_true")]
    pub reserve_write_enabled: bool,
    #[serde(default)]
    pub hp_enabled: bool,
    #[serde(default)]
    pub armor_enabled: bool,
    #[serde(default)]
    pub position_enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct TimingConfig {
    pub memory_loop_ms: u64,
    pub overlay_loop_ms: u64,
    #[serde(default = "default_retry")]
    pub connect_retry_ms: u64,
    #[serde(default = "default_stale")]
    pub stale_reconnect_ticks: u32,
}

fn default_retry() -> u64 {
    250
}
fn default_stale() -> u32 {
    40
}

/// خواندن از Local Player entity — offsetهای شناخته‌شده CS 1.6 (BLASTHACK dumper).
#[derive(Debug, Clone, Deserialize)]
pub struct EntityConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// `hw` = hw.dll + local_player_rva ، `client` = client.dll + local_player_rva
    #[serde(default = "default_local_player_module")]
    pub local_player_module: String,
    #[serde(default = "default_local_player_rva")]
    pub local_player_rva: String,
    #[serde(default = "default_money_offset")]
    pub money_offset: String,
    #[serde(default = "default_health_offset")]
    pub health_offset: String,
    #[serde(default = "default_armor_offset")]
    pub armor_offset: String,
    #[serde(default = "default_float_type")]
    pub health_type: String,
    #[serde(default = "default_float_type")]
    pub armor_type: String,
    /// client.dll direct RVA — CE «heal» (اگر entity کار نکرد)
    #[serde(default)]
    pub health_direct_rva: Option<String>,
    /// client.dll direct RVA — CE «armor»
    #[serde(default)]
    pub armor_direct_rva: Option<String>,
    /// player + offset → vec3 (X,Y,Z) — معمولاً entvars.origin = 0x8
    #[serde(default = "default_position_offset")]
    pub position_offset: String,
    /// hw.dll RVA برای entity موتور (مثلاً 0x169438)
    #[serde(default)]
    pub position_entity_hw_rva: Option<String>,
    /// hw.dll RVA — vec3 مستقیم (مثلاً EntityOrigin)
    #[serde(default)]
    pub position_global_hw_rva: Option<String>,
    /// client.dll RVA — vec3 مستقیم (مثلاً LocalOrigin)
    #[serde(default)]
    pub position_global_client_rva: Option<String>,
    /// client.dll RVA — دوربین/ماوس/ارتفاع (NOT map XYZ) — مثلاً 0x11D478
    #[serde(default)]
    pub view_client_rva: Option<String>,
}

fn default_local_player_module() -> String {
    "hw".into()
}

fn default_local_player_rva() -> String {
    "0x32ABF4".into()
}
fn default_money_offset() -> String {
    "0xE4".into()
}
fn default_health_offset() -> String {
    "0xB74".into()
}
fn default_armor_offset() -> String {
    "0x10C".into()
}
fn default_position_offset() -> String {
    "0x8".into()
}
fn default_float_type() -> String {
    "float".into()
}

impl Default for EntityConfig {
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

#[derive(Debug, Clone, Deserialize)]
pub struct PointerChainConfig {
    pub base_rva: String,
    pub offsets: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MoneyClientFallback {
    pub direct_rva: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChainsConfig {
    pub reserve: PointerChainConfig,
    pub money_hw: PointerChainConfig,
    pub money_client_fallback: MoneyClientFallback,
    pub clip: Vec<PointerChainConfig>,
    /// اندیس `chains.clip` برای reserve (ذخیره) — از انتخاب magazine حذف می‌شود
    #[serde(default = "default_reserve_clip_index")]
    pub reserve_clip_index: usize,
}

fn default_reserve_clip_index() -> usize {
    2
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClipDetectionConfig {
    pub min_value: i32,
    pub max_value: i32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HudSyncConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_clip_struct")]
    pub clip_struct_offset: String,
    #[serde(default = "default_hud_clip")]
    pub hud_clip_offset: String,
}

fn default_clip_struct() -> String {
    "0xCC".into()
}
fn default_hud_clip() -> String {
    "0xD0".into()
}

impl Default for HudSyncConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            clip_struct_offset: default_clip_struct(),
            hud_clip_offset: default_hud_clip(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OverlayConfig {
    pub offset_x: i32,
    pub offset_y: i32,
    pub font_size: i32,
    pub font_name: String,
    #[serde(default = "default_true")]
    pub font_bold: bool,
    #[serde(default = "default_spacing")]
    pub line_spacing: i32,
    #[serde(default = "default_pos")]
    pub position: String,
    #[serde(default = "default_margin")]
    pub margin: i32,
    pub colors: OverlayColors,
    pub display: OverlayDisplay,
}

fn default_spacing() -> i32 {
    22
}
fn default_pos() -> String {
    "top-left".into()
}
fn default_margin() -> i32 {
    12
}

#[derive(Debug, Clone, Deserialize)]
pub struct OverlayColors {
    #[serde(default = "default_gold")]
    pub money: String,
    #[serde(default = "default_green")]
    pub ammo: String,
    #[serde(default = "default_red")]
    pub hp: String,
    #[serde(default = "default_blue")]
    pub armor: String,
    #[serde(default = "default_cyan")]
    pub position: String,
    #[serde(default = "default_green")]
    pub default: String,
}

fn default_gold() -> String {
    "0x00D7FF".into()
}
fn default_green() -> String {
    "0x00FF00".into()
}
fn default_red() -> String {
    "0x4444FF".into()
}
fn default_blue() -> String {
    "0xFFAA00".into()
}
fn default_cyan() -> String {
    "0xFFFF88".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct OverlayDisplay {
    pub show_money: bool,
    pub show_ammo: bool,
    pub show_hp: bool,
    pub show_armor: bool,
    #[serde(default)]
    pub show_position: bool,
    #[serde(default = "default_true")]
    pub show_view_aux: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DebugConsoleConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_interval")]
    pub interval_ms: u64,
    #[serde(default = "default_true")]
    pub clear_screen: bool,
}

fn default_interval() -> u64 {
    1000
}

impl Default for DebugConsoleConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_ms: default_interval(),
            clear_screen: true,
        }
    }
}

impl AppConfig {
    const EMBEDDED: &'static str = include_str!("../config.toml");

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

pub fn parse_hex_u32(value: &str) -> Result<u32, AppError> {
    let trimmed = value.trim();
    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
        .unwrap_or(trimmed);
    u32::from_str_radix(hex, 16).map_err(|e| AppError::Config(format!("hex '{value}': {e}")))
}

pub fn parse_offsets(values: &[String]) -> Result<Vec<u32>, AppError> {
    values.iter().map(|v| parse_hex_u32(v)).collect()
}

pub fn parse_color(value: &str) -> Result<u32, AppError> {
    parse_hex_u32(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex() {
        assert_eq!(parse_hex_u32("0x32ABF4").unwrap(), 0x32ABF4);
        assert_eq!(parse_hex_u32("E4").unwrap(), 0xE4);
    }
}
