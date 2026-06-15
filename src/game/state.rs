//! [EN] Game state structures — shared between memory, overlay, and main threads.
//! [FA] ساختارهای وضعیت بازی — بین threadهای حافظه، overlay و اصلی به اشتراک گذاشته می‌شوند.

use std::fmt;

/// [EN] Snapshot of all game data — read by overlay thread, written by memory thread.
/// Protected by `Arc<RwLock<GameState>>` for thread-safe access.
/// [FA] اسنپ‌شات تمام داده‌های بازی — توسط thread overlay خوانده و توسط thread حافظه نوشته می‌شود.
/// توسط `Arc<RwLock<GameState>>` برای دسترسی ایمن thread محافظت می‌شود.
#[derive(Debug, Clone, Default)]
pub struct GameState {
    /// [EN] Current money amount in the game.
    /// [FA] مبلغ پول فعلی در بازی.
    pub money: i32,
    /// [EN] Current magazine ammo count (clip).
    /// [FA] تعداد خشاب مجله فعلی.
    pub clip: i32,
    /// [EN] Reserve (total) ammo count.
    /// [FA] تعداد خشاب ذخیره (کل).
    pub reserve: i32,
    /// [EN] Player health points (0.0 – 100.0).
    /// [FA] امتیاز سلامتی بازیکن (۰.۰ تا ۱۰۰.۰).
    pub hp: f32,
    /// [EN] Player armor points (0.0 – 100.0).
    /// [FA] امتیاز زره بازیکن (۰.۰ تا ۱۰۰.۰).
    pub armor: f32,
    /// [EN] Player X position (world coordinates).
    /// [FA] موقعیت X بازیکن (مختصات جهانی).
    pub pos_x: f32,
    /// [EN] Player Y position (world coordinates).
    /// [FA] موقعیت Y بازیکن (مختصات جهانی).
    pub pos_y: f32,
    /// [EN] Player Z position (world coordinates).
    /// [FA] موقعیت Z بازیکن (مختصات جهانی).
    pub pos_z: f32,
    /// [EN] Camera horizontal angle (pitch) — NOT map coordinates.
    /// [FA] زاویه افقی دوربین (pitch) — نه مختصات نقشه.
    pub view_h: f32,
    /// [EN] Mouse X movement value.
    /// [FA] مقدار حرکت X ماوس.
    pub view_mx: f32,
    /// [EN] Mouse Y movement value.
    /// [FA] مقدار حرکت Y ماوس.
    pub view_my: f32,
    /// [EN] Whether HP reading is active and producing valid data.
    /// [FA] آیا خواندن HP فعال است و داده معتبر تولید می‌کند.
    pub hp_active: bool,
    /// [EN] Whether armor reading is active and producing valid data.
    /// [FA] آیا خواندن زره فعال است و داده معتبر تولید می‌کند.
    pub armor_active: bool,
    /// [EN] Whether position reading is active and producing valid data.
    /// [FA] آیا خواندن موقعیت فعال است و داده معتبر تولید می‌کند.
    pub position_active: bool,
    /// [EN] Whether view/camera reading is active.
    /// [FA] آیا خواندن نمای/دوربین فعال است.
    pub view_active: bool,
    /// [EN] Whether the player is currently alive (HP > 0).
    /// [FA] آیا بازیکن در حال حاضر زنده است (HP > ۰).
    pub player_alive: bool,
    /// [EN] Whether the tool is connected to the game process.
    /// [FA] آیا ابزار به پروسس بازی متصل است.
    pub connected: bool,
    /// [EN] Whether at least one data field has been successfully read.
    /// [FA] آیا حداقل یک فیلد داده با موفقیت خوانده شده است.
    pub ready: bool,
    /// [EN] Whether the money value was read successfully in the last tick.
    /// [FA] آیا مقدار پول در آخرین tick با موفقیت خوانده شده.
    pub money_valid: bool,
    /// [EN] Whether the clip ammo value was read successfully in the last tick.
    /// [FA] آیا مقدار خشاب مجله در آخرین tick با موفقیت خوانده شده.
    pub clip_valid: bool,
    /// [EN] Whether the reserve ammo value was read successfully in the last tick.
    /// [FA] آیا مقدار خشاب ذخیره در آخرین tick با موفقیت خوانده شده.
    pub reserve_valid: bool,
}

/// [EN] Controls which fields are displayed in the status line.
/// [FA] کنترل می‌کند کدام فیلدها در خط وضعیت نمایش داده شوند.
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
    /// [EN] Creates a `StatusDisplay` that shows all available fields.
    /// [FA] یک `StatusDisplay` ایجاد می‌کند که تمام فیلدهای موجود را نشان می‌دهد.
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

/// [EN] Simplified snapshot for debug console rendering.
/// [FA] اسنپ‌شات ساده‌شده برای رندر کنسول دیباگ.
#[derive(Debug, Clone, Default)]
pub struct DebugSnapshot {
    /// [EN] Connection status to the game process.
    /// [FA] وضعیت اتصال به پروسس بازی.
    pub connected: bool,
    /// [EN] Whether at least one field was read successfully.
    /// [FA] آیا حداقل یک فیلد با موفقیت خوانده شده.
    pub ready: bool,
    /// [EN] Current money value.
    /// [FA] مقدار پول فعلی.
    pub money: i32,
    /// [EN] Current clip ammo value.
    /// [FA] مقدار خشاب مجله فعلی.
    pub clip: i32,
    /// [EN] Current reserve ammo value.
    /// [FA] مقدار خشاب ذخیره فعلی.
    pub reserve: i32,
    /// [EN] Whether money was read successfully.
    /// [FA] آیا پول با موفقیت خوانده شده.
    pub money_valid: bool,
    /// [EN] Whether clip was read successfully.
    /// [FA] آیا خشاب مجله با موفقیت خوانده شده.
    pub clip_valid: bool,
    /// [EN] Whether reserve was read successfully.
    /// [FA] آیا خشاب ذخیره با موفقیت خوانده شده.
    pub reserve_valid: bool,
    /// [EN] Whether memory writes are enabled.
    /// [FA] آیا نوشتن در حافظه فعال است.
    pub write_enabled: bool,
    /// [EN] Whether the player is currently alive.
    /// [FA] آیا بازیکن در حال حاضر زنده است.
    pub player_alive: bool,
    /// [EN] Description of where the money value was read from.
    /// [FA] توضیح اینکه مقدار پول از کجا خوانده شده.
    pub money_source: String,
}

/// [EN] Resolved memory addresses — used for debug logging.
/// [FA] آدرس‌های حافظه resolve شده — برای لاگ دیباگ استفاده می‌شود.
#[derive(Debug, Clone, Default)]
pub struct ResolvedInfo {
    /// [EN] Base address of hw.dll / sw.dll module.
    /// [FA] آدرس پایه ماژول hw.dll / sw.dll.
    pub hw_base: u32,
    /// [EN] Base address of client.dll module.
    /// [FA] آدرس پایه ماژول client.dll.
    pub client_base: u32,
    /// [EN] Address of the local player entity.
    /// [FA] آدرس entity بازیکن محلی.
    pub local_player: u32,
    /// [EN] Resolved money memory address.
    /// [FA] آدرس حافظه resolve شده پول.
    pub money_addr: u32,
    /// [EN] Resolved reserve ammo memory address.
    /// [FA] آدرس حافظه resolve شده خشاب ذخیره.
    pub reserve_addr: u32,
    /// [EN] Resolved clip ammo memory address.
    /// [FA] آدرس حافظه resolve شده خشاب مجله.
    pub clip_addr: u32,
    /// [EN] Resolved health memory address.
    /// [FA] آدرس حافظه resolve شده سلامتی.
    pub hp_addr: u32,
    /// [EN] Resolved armor memory address.
    /// [FA] آدرس حافظه resolve شده زره.
    pub armor_addr: u32,
    /// [EN] Resolved position memory address.
    /// [FA] آدرس حافظه resolve شده موقعیت.
    pub pos_addr: u32,
}

/// [EN] Formats a field value with right-alignment, or shows "--" if invalid.
/// [FA] مقدار فیلد را با تراز راست فرمت می‌دهد، یا اگر نامعتبر باشد "--" نشان می‌دهد.
fn field(valid: bool, v: i32, w: usize) -> String {
    if valid {
        format!("{v:>w$}")
    } else {
        "--".into()
    }
}

impl GameState {
    /// [EN] Formats the game state into a single-line status string for console/overlay display.
    /// Returns localized messages for waiting/reading states.
    /// [FA] وضعیت بازی را در یک رشته وضعیت یکخطی برای نمایش کنسول/overlay فرمت می‌دهد.
    /// پیام‌های بومی برای حالت‌های انتظار/خواندن برمی‌گرداند.
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

    /// [EN] Returns a waiting message based on the current connection state.
    /// [FA] یک پیام انتظار بر اساس وضعیت اتصال فعلی برمی‌گرداند.
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

/// [EN] Implement Display trait so GameState can be printed directly with `println!("{}", state)`.
/// [FA] پیاده‌سازی Display trait تا GameState بتواند مستقیماً با `println!("{}", state)` چاپ شود.
impl fmt::Display for GameState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_status(&StatusDisplay::all()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [EN] Ensures invalid reserve values show "--" instead of fake zeros.
    /// [FA] اطمینان از اینکه مقادیر نامعتبر ذخیره به جای صفر جعلی "--" نشان داده شوند.
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
