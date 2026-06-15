//! وضعیت بازی — بین threadها به اشتراک گذاشته می‌شود.

use std::fmt;

#[derive(Debug, Clone, Default)]
pub struct GameState {
    pub money: i32,
    pub clip: i32,
    pub reserve: i32,
    pub hp: f32,
    pub armor: f32,
    pub pos_x: f32,
    pub pos_y: f32,
    pub pos_z: f32,
    /// view_client_rva — معمولاً H / pitch / yaw (NOT map)
    pub view_h: f32,
    pub view_mx: f32,
    pub view_my: f32,
    pub hp_active: bool,
    pub armor_active: bool,
    pub position_active: bool,
    pub view_active: bool,
    pub player_alive: bool,
    pub connected: bool,
    pub ready: bool,
    pub money_valid: bool,
    pub clip_valid: bool,
    pub reserve_valid: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StatusDisplay {
    pub show_money: bool,
    pub show_ammo: bool,
    pub show_hp: bool,
    pub show_armor: bool,
    pub show_position: bool,
    pub show_view_aux: bool,
}

impl StatusDisplay {
    pub fn all() -> Self {
        Self {
            show_money: true,
            show_ammo: true,
            show_hp: true,
            show_armor: true,
            show_position: true,
            show_view_aux: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DebugSnapshot {
    pub connected: bool,
    pub ready: bool,
    pub money: i32,
    pub clip: i32,
    pub reserve: i32,
    pub money_valid: bool,
    pub clip_valid: bool,
    pub reserve_valid: bool,
    pub write_enabled: bool,
    pub player_alive: bool,
    pub money_source: String,
}

#[derive(Debug, Clone, Default)]
pub struct ResolvedInfo {
    pub hw_base: u32,
    pub client_base: u32,
    pub local_player: u32,
    pub money_addr: u32,
    pub reserve_addr: u32,
    pub clip_addr: u32,
    pub hp_addr: u32,
    pub armor_addr: u32,
    pub pos_addr: u32,
}

fn field(valid: bool, v: i32, w: usize) -> String {
    if valid {
        format!("{v:>w$}")
    } else {
        "--".into()
    }
}

impl GameState {
    pub fn format_status(&self, d: &StatusDisplay) -> String {
        if !self.connected {
            return "[منتظر بازی...]".into();
        }
        let has_any = self.money_valid || self.clip_valid || self.reserve_valid;
        if !self.ready && !has_any {
            return "[در حال خواندن...]".into();
        }

        let mut parts = Vec::new();
        if d.show_money {
            parts.push(format!("💰 {}", field(self.money_valid, self.money, 6)));
        }
        if d.show_ammo {
            parts.push(format!(
                "🔫 {}/{}",
                field(self.clip_valid, self.clip, 2),
                field(self.reserve_valid, self.reserve, 2)
            ));
        }
        if d.show_hp && self.hp_active {
            parts.push(format!("❤ {:>3.0}", self.hp));
        }
        if d.show_armor && self.armor_active {
            parts.push(format!("🛡 {:>3.0}", self.armor));
        }
        if d.show_position && self.position_active {
            parts.push(format!(
                "📍 {:>7.0} {:>7.0} {:>5.0}",
                self.pos_x, self.pos_y, self.pos_z
            ));
        }
        if d.show_view_aux && self.view_active {
            parts.push(format!(
                "📐 H:{:>5.0} M:{:>5.0}/{:<5.0}",
                self.view_h, self.view_mx, self.view_my
            ));
        }

        if parts.is_empty() {
            "CS16 v2 — Connected".into()
        } else {
            parts.join("  ")
        }
    }

    pub fn waiting_message(&self, game_found: bool) -> String {
        if !game_found {
            "[CS16 v2] Waiting for hl.exe...".into()
        } else if !self.connected {
            "[CS16 v2] Connecting... (enter a match)".into()
        } else {
            "[CS16 v2] Reading memory...".into()
        }
    }
}

impl fmt::Display for GameState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_status(&StatusDisplay::all()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_fake_zero_reserve() {
        let s = GameState {
            connected: true,
            ready: true,
            clip: 30,
            clip_valid: true,
            reserve_valid: false,
            ..Default::default()
        };
        assert!(s.format_status(&StatusDisplay::all()).contains("30/--"));
    }
}
