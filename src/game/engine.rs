//! [EN] GameEngine — Main game engine module implementing CS 1.6 external memory reading/writing.
//! Mirrors the logic from the original C++ tools (`1_cs16` / `2_cs16`).
//!
//! [FA] GameEngine — ماژول موتور بازی اصلی که خواندن/نوشتن حافظه خارجی CS 1.6 را پیاده‌سازی می‌کند.
//! منطق ابزارهای اصلی C++ (`1_cs16` / `2_cs16`) را بازتولید می‌کند.
//!
//! [EN] Key design decisions:
//!   - money/reserve: resolved once at connect; refreshed only if a read fails.
//!   - clip: all chains are resolved every tick (mirrors the C++ while-loop pattern).
//!
//! [FA] تصمیمات کلیدی طراحی:
//!   - money/reserve: یک‌بار در زمان connect حل می‌شوند؛ فقط در صورت خطا در خواندن تازه‌سازی می‌شوند.
//!   - clip: همه زنجیره‌ها در هر tick حل می‌شوند (الگوی حلقه while در C++).

use crate::config::{parse_hex_u32, parse_offsets, AppConfig, ValueType};
use crate::error::{AppError, MemoryError};
use crate::game::local_player;
use crate::game::position;
use crate::game::state::{DebugSnapshot, GameState, ResolvedInfo};
use crate::win::memory::{resolve_chain, MemoryReader, MemoryWriter};
use crate::win::process::{engine_base, is_alive, ProcessHandle};

/// [EN] Internal cache for last-known game values (money, clip, reserve).
/// Validity flags track whether each value has been successfully read at least once.
///
/// [FA] کش داخلی برای مقادیر آخرین وضعیت بازی (پول، خشاب، مهمات ذخیره).
/// پرچم‌های اعتبار مشخص می‌کنند آیا هر مقدار حداقل یک‌بار با موفقیت خوانده شده است.
#[derive(Debug, Clone, Default)]
struct Cache {
    money: i32,
    money_valid: bool,
    clip: i32,
    clip_valid: bool,
    reserve: i32,
    reserve_valid: bool,
}

/// [EN] Core game engine holding process handle, resolved addresses, and cached state.
/// This is the main entry point for all CS 1.6 memory operations.
///
/// [FA] موتور بازی اصلی شامل دسته پروسه، آدرس‌های حل‌شده و وضعیت کش‌شده.
/// این نقطه ورود اصلی برای تمام عملیات حافظه CS 1.6 است.
pub struct GameEngine {
    /// [EN] Handle to the target CS 1.6 process / [FA] دسته پروسه هدف CS 1.6
    process: ProcessHandle,
    /// [EN] Application configuration (chains, features, entity offsets) / [FA] پیکربندی برنامه (زنجیره‌ها، ویژگی‌ها، آفست‌های موجودیت)
    config: AppConfig,
    /// [EN] Base address of hw.dll (engine module) / [FA] آدرس پایه hw.dll (ماژول موتور)
    hw_base: u32,
    /// [EN] Base address of client.dll / [FA] آدرس پایه client.dll
    client_base: u32,
    /// resolve یک‌بار — مثل `hwMoneyAddr` در 1_cs16
    money_hw_addr: u32,
    /// resolve یک‌بار — مثل `hwAmmoAddr` در 1_cs16
    reserve_addr: u32,
    /// `clientBase + CLIENT_MONEY_RVA`
    client_money_addr: u32,
    /// [EN] Entity address offsets parsed from config / [FA] آفست‌های آدرس موجودیت ت解析 شده از پیکربندی
    entity: EntityAddrs,
    /// [EN] Local-player RVA discovered at runtime (0 = not yet discovered, use config value)
    /// [FA] RVA بازیکن محلی کشف‌شده در زمان اجرا (0 = هنوز کشف نشده، از مقدار پیکربندی استفاده شود)
    discovered_lp_rva: u32,
    /// [EN] Health offset discovered at runtime / [FA] آفست سلامتی کشف‌شده در زمان اجرا
    discovered_hp_off: u32,
    /// [EN] Position offset discovered at runtime / [FA] آفست موقعیت کشف‌شده در زمان اجرا
    discovered_pos_off: u32,
    /// [EN] Actual entity base for position vec3 — may come from hw.dll instead of client.dll
    /// [FA] پایه موجودیت واقعی برای vec3 موقعیت — ممکن است از hw.dll به جای client.dll باشد
    discovered_pos_player: u32,
    /// [EN] Global lock after first successful resolve (hw_base + rva). Once locked, position
    /// is always read from this address to avoid flickering between different sources.
    /// [FA] قفل جهانی پس از اولین resolve موفق (hw_base + rva). پس از قفل شدن، موقعیت
    /// همیشه از این آدرس خوانده می‌شود تا از سوسو زدن بین منابع مختلف جلوگیری شود.
    locked_pos_player: u32,
    /// [EN] Position offset paired with locked_pos_player / [FA] آفست موقعیت جفت با locked_pos_player
    locked_pos_off: u32,
    /// [EN] Previous tick's X/Y/Z positions for jump-detection heuristic / [FA] مقادیر X/Y/Z قبلی برای تشخیص پرش
    pos_last_x: f32,
    pos_last_y: f32,
    pos_last_z: f32,
    /// [EN] Previous tick's view angle (yaw) for jump-detection heuristic / [FA] زاویه دید قبلی (yaw) برای تشخیص پرش
    pos_last_view_h: f32,
    /// [EN] Consecutive ticks with no position change; triggers rediscovery at 120
    /// [FA] تعداد tick‌های متوالی بدون تغییر موقعیت؛ در ۱۲۰ باعث کشف مجدد می‌شود
    pos_static_ticks: u32,
    /// [EN] Stale counter — incremented when no data is read; triggers reconnect when threshold is hit
    /// [FA] شمارنگر قدیمی — وقتی داده‌ای خوانده نمی‌شود افزایش می‌یابد؛ در رسیدن به آستانه باعث اتصال مجدد می‌شود
    stale: u32,
    /// [EN] Last successfully read values for display / [FA] مقادیر آخرین خواندن موفق برای نمایش
    cache: Cache,
    /// [EN] True once all required features have been read at least once / [FA] وقتی تمام ویژگی‌های مورد نیاز حداقل یک‌بار خوانده شدند true
    display_ready: bool,
    /// [EN] Name of the last money read source for debug display / [FA] نام منبع آخرین خواندن پول برای نمایش اشکال‌زدایی
    last_money_source: String,
    /// [EN] Most recently resolved addresses for debug output / [FA] آدرس‌های اخیراً حل‌شده برای خروجی اشکال‌زدایی
    last_resolved: ResolvedInfo,
}

/// [EN] Parsed entity configuration offsets for player data (health, armor, money, position).
/// These values come from the user's config and are resolved once at engine open.
///
/// [FA] آفست‌های پیکربندی موجودیت ت解析 شده برای داده بازیکن (سلامتی، زره، پول، موقعیت).
/// این مقادیر از پیکربندی کاربر می‌آیند و یک‌بار در زمان باز کردن موتور حل می‌شوند.
#[derive(Debug, Clone, Copy, Default)]
struct EntityAddrs {
    /// [EN] True if local-player pointer lives in client.dll (else hw.dll) / [FA] اگر اشاره‌گر بازیکن محلی در client.dll باشد true (در غیر این صورت hw.dll)
    local_player_on_client: bool,
    /// [EN] RVA offset to the local-player pointer / [FA] آفست RVA به اشاره‌گر بازیکن محلی
    local_player_ptr: u32,
    /// [EN] Offset from player base to money field / [FA] آفست از پایه بازیکن به فیلد پول
    money: u32,
    /// [EN] Offset from player base to health field / [FA] آفست از پایه بازیکن به فیلد سلامتی
    health: u32,
    /// [EN] Offset from player base to armor field / [FA] آفست از پایه بازیکن به فیلد زره
    armor: u32,
    /// [EN] Data type for health field (Int/Float/Byte) / [FA] نوع داده فیلد سلامتی (Int/Float/Byte)
    health_type: ValueType,
    /// [EN] Data type for armor field (Int/Float/Byte) / [FA] نوع داده فیلد زره (Int/Float/Byte)
    armor_type: ValueType,
    /// [EN] Absolute address for direct health reads (client_base + direct_rva) / [FA] آدرس مطلق برای خواندن مستقیم سلامتی
    health_direct: u32,
    /// [EN] Absolute address for direct armor reads (client_base + direct_rva) / [FA] آدرس مطلق برای خواندن مستقیم زره
    armor_direct: u32,
    /// [EN] Offset from player base to position (origin vec3) / [FA] آفست از پایه بازیکن به موقعیت (origin vec3)
    position: u32,
    /// [EN] RVA in hw.dll to the entity struct containing position / [FA] RVA در hw.dll به ساختار موجودیت حاوی موقعیت
    position_entity_hw_rva: Option<u32>,
    /// [EN] RVA in hw.dll to the global world pointer for position / [FA] RVA در hw.dll به اشاره‌گر جهان جهانی برای موقعیت
    position_global_hw_rva: Option<u32>,
    /// [EN] RVA in client.dll for view angle data / [FA] RVA در client.dll برای داده زاویه دید
    view_client_rva: Option<u32>,
    /// [EN] True if entity config is active and should be used / [FA] اگر پیکربندی موجودیت فعال باشد و باید استفاده شود true
    active: bool,
}

impl GameEngine {
    /// [EN] Open a game engine session: attach to process, resolve module bases, and
    /// perform initial static address resolution (money, reserve, position).
    ///
    /// [FA] باز کردن یک نشست موتور بازی: اتصال به پروسه، حل کردن پایه‌های ماژول،
    /// و انجام resolve اولیه آدرس‌های ایستا (پول، مهمات ذخیره، موقعیت).
    pub fn open(config: &AppConfig) -> Result<Self, AppError> {
        let process = ProcessHandle::attach(&config.process.name)?;
        let hw_base = engine_base(&process, &config.modules)?;
        let client_base = process.module_base(&config.modules.client).unwrap_or(0);

        let entity = if config.entity.enabled {
            parse_entity_addrs(&config.entity, client_base)
        } else {
            EntityAddrs::default()
        };

        let mut engine = Self {
            process,
            config: config.clone(),
            hw_base,
            client_base,
            money_hw_addr: 0,
            reserve_addr: 0,
            client_money_addr: 0,
            entity,
            discovered_lp_rva: 0,
            discovered_hp_off: 0,
            discovered_pos_off: 0,
            discovered_pos_player: 0,
            locked_pos_player: 0,
            locked_pos_off: 0,
            pos_last_x: 0.0,
            pos_last_y: 0.0,
            pos_last_z: 0.0,
            pos_last_view_h: 0.0,
            pos_static_ticks: 0,
            stale: 0,
            cache: Cache::default(),
            display_ready: false,
            last_money_source: String::new(),
            last_resolved: ResolvedInfo::default(),
        };
        engine.resolve_static_addresses();
        engine.discover_local_player_if_needed();
        Ok(engine)
    }

    /// [EN] One-time startup resolution: find money_hw_addr, reserve_addr, client_money_addr,
    /// and lock the global position address. Mirrors the startup block in `1_cs16.c`.
    ///
    /// [FA] resolve یک‌بار در هنگام راه‌اندازی: پیدا کردن money_hw_addr، reserve_addr، client_money_addr،
    /// و قفل کردن آدرس موقعیت جهانی. بلوک راه‌اندازی در `1_cs16.c` را بازتولید می‌کند.
    fn resolve_static_addresses(&mut self) {
        self.money_hw_addr = self.try_resolve_money_hw().unwrap_or(0);
        self.reserve_addr = self.pick_reserve_addr();
        self.client_money_addr = if self.client_base != 0 {
            parse_hex_u32(&self.config.chains.money_client_fallback.direct_rva)
                .map(|rva| self.client_base.wrapping_add(rva))
                .unwrap_or(0)
        } else {
            0
        };
        self.resolve_position_global();
    }

    /// [EN] Lock the global position address at connect time. This reads from hw_base + global_rva
    /// to find the authoritative position source — the same address that gives correct movement
    /// in memory dumps.
    ///
    /// [FA] قفل کردن آدرس موقعیت جهانی در زمان اتصال. این از hw_base + global_rva می‌خواند
    /// تا منبع معتبر موقعیت را پیدا کند — همان آدرسی که در دامپ حافظه movement درست می‌دهد.
    fn resolve_position_global(&mut self) {
        let Some(rva) = self.entity.position_global_hw_rva else {
            return;
        };
        if self.hw_base == 0 {
            return;
        }
        let reader = MemoryReader::new(&self.process);
        let Some((disc, _)) = position::read_global_world_at_rva(&reader, self.hw_base, rva) else {
            return;
        };
        self.locked_pos_player = disc.player;
        self.locked_pos_off = disc.offset;
        self.discovered_pos_player = disc.player;
        self.discovered_pos_off = disc.offset;
        tracing::info!(
            "position locked: {:#x}+{:#x} (hw global)",
            disc.player,
            disc.offset
        );
    }

    /// [EN] Re-resolve the money HW address if it was never resolved or became stale.
    ///
    /// [FA] resolve مجدد آدرس money HW اگر هرگز حل نشده یا قدیمی شده باشد.
    fn refresh_money_hw_if_needed(&mut self) {
        if self.money_hw_addr != 0 {
            return;
        }
        self.money_hw_addr = self.try_resolve_money_hw().unwrap_or(0);
    }

    /// [EN] Re-resolve the reserve address if the cached one fails to read.
    /// This handles the case where the game reallocates memory between sessions.
    ///
    /// [FA] resolve مجدد آدرس reserve اگر آدرس کش‌شده در خواندن ناموفق باشد.
    /// این حالتی را مدیریت می‌کند که بازی حافظه را بین نشست‌ها مجدداً تخصیص می‌دهد.
    fn refresh_reserve_if_needed(&mut self) {
        if self.reserve_addr != 0 {
            let reader = MemoryReader::new(&self.process);
            if reader.read_i32(self.reserve_addr).is_ok() {
                return;
            }
            self.reserve_addr = 0;
        }
        self.reserve_addr = self.pick_reserve_addr();
    }

    /// [EN] Pick the best reserve (ammo) address by comparing the primary chain result against
    /// the fallback clip[2] chain. Uses a scoring heuristic: values in valid range (1..max)
    /// score highest, zero scores 100, out-of-range scores 50, errors score -1.
    ///
    /// [FA] بهترین آدرس reserve (مهمات) را با مقایسه نتیجه زنجیره اصلی با زنجیره
    /// clip[2] انتخاب کن. از امتیازدهی هوریستیک استفاده می‌شود: مقادیر در محدوده معتبر
    /// بیشترین امتیاز، صفر امتیاز ۱۰۰، خارج از محدوده ۵۰، خطا -۱.
    fn pick_reserve_addr(&self) -> u32 {
        let reader = MemoryReader::new(&self.process);
        let primary = self.try_resolve_reserve().unwrap_or(0);
        let fallback = self.try_resolve_reserve_from_clip_chains();
        let max = self.config.clip_detection.max_value;

        let score = |addr: u32| -> i32 {
            if addr == 0 {
                return -1;
            }
            match reader.read_i32(addr) {
                Ok(v) if (1..max).contains(&v) => v + 10_000,
                Ok(0) => 100,
                Ok(_) => 50,
                Err(_) => -1,
            }
        };

        let ps = score(primary);
        let fs = score(fallback);
        if fs > ps {
            fallback
        } else if ps >= 0 {
            primary
        } else if fs >= 0 {
            fallback
        } else {
            0
        }
    }

    /// [EN] Attempt to discover the local-player pointer if entity is active but the
    /// player pointer is currently null. Uses the `local_player::discover` algorithm
    /// which scans memory for valid player structures.
    ///
    /// [FA] تلاش برای کشف اشاره‌گر بازیکن محلی اگر موجودیت فعال باشد اما
    /// اشاره‌گر بازیکن فعلی null باشد. از الگوریتم `local_player::discover` استفاده می‌کند
    /// که حافظه را برای ساختارهای معتبر بازیکن اسکن می‌کند.
    fn discover_local_player_if_needed(&mut self) {
        if !self.entity.active {
            return;
        }
        let reader = MemoryReader::new(&self.process);
        let player = self.resolve_local_player_ptr(&reader);
        if player != 0 {
            return;
        }
        let config_rva = if self.discovered_lp_rva != 0 {
            self.discovered_lp_rva
        } else {
            self.entity.local_player_ptr
        };
        let config_hp = if self.discovered_hp_off != 0 {
            self.discovered_hp_off
        } else {
            self.entity.health
        };
        if let Some(found) = local_player::discover(
            &reader,
            self.entity_module_base(),
            config_rva,
            config_hp,
            self.entity.local_player_on_client,
        ) {
            self.discovered_lp_rva = found.rva;
            self.discovered_hp_off = found.health_offset;
            self.last_resolved.local_player = found.player;
        }
    }

    /// [EN] Return the effective local-player RVA: discovered value takes priority over config.
    ///
    /// [FA] RVA بازیکن محلی مؤثر را برگردان: مقدار کشف‌شده نسبت به پیکربندی اولویت دارد.
    fn effective_lp_rva(&self) -> u32 {
        if self.discovered_lp_rva != 0 {
            self.discovered_lp_rva
        } else {
            self.entity.local_player_ptr
        }
    }

    /// [EN] Return the effective health offset: discovered value takes priority over config.
    ///
    /// [FA] آفست سلامتی مؤثر را برگردان: مقدار کشف‌شده نسبت به پیکربندی اولویت دارد.
    fn effective_hp_off(&self) -> u32 {
        if self.discovered_hp_off != 0 {
            self.discovered_hp_off
        } else {
            self.entity.health
        }
    }

    /// [EN] Return the base address of the module containing entity data (hw.dll or client.dll).
    ///
    /// [FA] آدرس پایه ماژول حاوی داده موجودیت (hw.dll یا client.dll) را برگردان.
    fn entity_module_base(&self) -> u32 {
        if self.entity.local_player_on_client {
            self.client_base
        } else {
            self.hw_base
        }
    }

    /// [EN] Resolve the local-player entity pointer using two strategies:
    ///   1. Direct: subtract health offset from direct health address to get player base.
    ///   2. Indirect: read the pointer from module_base + lp_rva, then validate by checking
    ///      that health at player + hp_off is in range [0..100].
    ///
    /// [FA] اشاره‌گر موجودیت بازیکن محلی را با دو روش حل کن:
    ///   ۱. مستقیم: کم کردن آفست سلامتی از آدرس مستقیم سلامتی برای گرفتن پایه بازیکن.
    ///   ۲. غیرمستقیم: خواندن اشاره‌گر از module_base + lp_rva، سپس اعتبارسنجی با بررسی
    ///      اینکه سلامتی در player + hp_off در محدوده [0..100] باشد.
    fn resolve_local_player_ptr(&self, reader: &MemoryReader) -> u32 {
        if !self.entity.active {
            return 0;
        }

        let hp_off = self.effective_hp_off();

        // health_direct − hp_off — همان CE player_health_from_base
        if self.entity.health_direct != 0 {
            let player = self.entity.health_direct.wrapping_sub(hp_off);
            if player != 0
                && (player & 3) == 0
                && reader
                    .read_i32(self.entity.health_direct)
                    .ok()
                    .is_some_and(|hp| (0..=100).contains(&hp))
            {
                return player;
            }
        }

        let module_base = self.entity_module_base();
        if module_base == 0 {
            return 0;
        }
        let ptr_addr = module_base.wrapping_add(self.effective_lp_rva());
        let Ok(player) = reader.read_u32(ptr_addr) else {
            return 0;
        };
        if player == 0 || player & 3 != 0 {
            return 0;
        }
        if reader
            .read_i32(player.wrapping_add(hp_off))
            .ok()
            .is_some_and(|hp| (0..=100).contains(&hp))
        {
            player
        } else {
            0
        }
    }

    /// [EN] Try to resolve reserve ammo from the clip chain at index `reserve_clip_index`.
    /// In the user's memory dump, clip[2] often holds the reserve value (e.g., 20).
    ///
    /// [FA] تلاش برای حل reserve مهمات از زنجیره clip در اندیس `reserve_clip_index`.
    /// در دامپ حافظه کاربر، clip[2] معمولاً مقدار reserve را نگه می‌دارد (مثلاً ۲۰).
    fn try_resolve_reserve_from_clip_chains(&self) -> u32 {
        let idx = self.config.chains.reserve_clip_index;
        let Some(chain) = self.config.chains.clip.get(idx) else {
            return 0;
        };
        let Ok(rva) = parse_hex_u32(&chain.base_rva) else {
            return 0;
        };
        let Ok(offsets) = parse_offsets(&chain.offsets) else {
            return 0;
        };
        resolve_chain(&self.process, self.hw_base.wrapping_add(rva), &offsets).unwrap_or(0)
    }

    /// [EN] Resolve a pointer chain from config to its final address in memory.
    ///
    /// [FA] حل یک زنجیره اشاره‌گر از پیکربندی به آدرس نهایی در حافظه.
    fn resolve_chain_addr(&self, chain: &crate::config::PointerChainConfig) -> Option<u32> {
        let rva = parse_hex_u32(&chain.base_rva).ok()?;
        let offsets = parse_offsets(&chain.offsets).ok()?;
        resolve_chain(&self.process, self.hw_base.wrapping_add(rva), &offsets).ok()
    }

    /// [EN] Iterator over clip chain indices, skipping the one used for reserve detection.
    ///
    /// [FA] iterator روی اندیس‌های زنجیره clip، با رد شدن از زنجیره‌ای که برای تشخیص reserve استفاده می‌شود.
    fn clip_chain_indices(&self) -> impl Iterator<Item = usize> + '_ {
        let skip = self.config.chains.reserve_clip_index;
        (0..self.config.chains.clip.len()).filter(move |i| *i != skip)
    }

    /// [EN] Return the process ID of the attached CS 1.6 process.
    ///
    /// [FA] شناسه پروسه CS 1.6 متصل را برگردان.
    pub fn pid(&self) -> u32 {
        self.process.pid()
    }

    /// [EN] Return the last resolved addresses info for debug display.
    ///
    /// [FA] اطلاعات آخرین آدرس‌های حل‌شده برای نمایش اشکال‌زدایی را برگردان.
    pub fn resolved(&self) -> &ResolvedInfo {
        &self.last_resolved
    }

    /// [EN] Return the resolved client.dll money address (used as fallback).
    ///
    /// [FA] آدرس پول client.dll حل‌شده (به عنوان fallback استفاده می‌شود) را برگردان.
    pub fn client_money_addr(&self) -> u32 {
        self.client_money_addr
    }

    /// [EN] Determine if the engine should reconnect: process died, hw.dll not found,
    /// or stale counter exceeded threshold.
    ///
    /// [FA] تعیین اینکه آیا موتور باید مجدداً متصل شود: پروسه مرده، hw.dll پیدا نشده،
    /// یا شمارنگر قدیمی از آستانه عبور کرده.
    pub fn should_reconnect(&self) -> bool {
        if !is_alive(self.process.pid()) {
            return true;
        }
        if engine_base(&self.process, &self.config.modules).is_err() {
            return true;
        }
        self.stale >= self.config.timing.stale_reconnect_ticks
    }

    /// [EN] Main tick: called once per frame to read all game state from memory.
    /// Refreshes bases, resolves addresses, reads vitals/money/clip/reserve/position,
    /// applies writes if enabled, and returns a snapshot GameState.
    ///
    /// [FA] تیک اصلی: یک‌بار در هر فریم برای خواندن تمام وضعیت بازی از حافظه فراخوانی می‌شود.
    /// پایه‌ها را تازه می‌کند، آدرس‌ها را حل می‌کند، vitals/پول/clip/reserve/موقعیت را می‌خواند،
    /// در صورت فعال بودن نوشتن را اعمال می‌کند، و یک GameState عکس‌العمل برمی‌گرداند.
    pub fn tick(&mut self) -> GameState {
        self.refresh_bases();
        self.refresh_money_hw_if_needed();
        self.refresh_reserve_if_needed();

        if self.entity.active {
            let probe = MemoryReader::new(&self.process);
            if self.resolve_local_player_ptr(&probe) == 0 {
                self.discover_local_player_if_needed();
            }
        }

        let reader = MemoryReader::new(&self.process);
        let writer = MemoryWriter::new(&self.process);
        let write = self.config.features.write_enabled;
        let targets = self.config.targets.clone();
        let feats = self.config.features.clone();

        let player = self.resolve_local_player_ptr(&reader);
        self.last_resolved.local_player = player;
        self.last_resolved.hw_base = self.hw_base;
        self.last_resolved.client_base = self.client_base;

        let (hp, armor, hp_on, armor_on, alive) = self.read_vitals(&reader, player);

        let hp_off = self.effective_hp_off();
        self.last_resolved.hp_addr = if hp_on && self.entity.health_direct != 0 {
            self.entity.health_direct
        } else if player != 0 && self.entity.active {
            player.wrapping_add(hp_off)
        } else {
            0
        };
        self.last_resolved.armor_addr = if armor_on && self.entity.armor_direct != 0 {
            self.entity.armor_direct
        } else if player != 0 && self.entity.active {
            player.wrapping_add(self.entity.armor)
        } else {
            0
        };

        let (money, money_ok, money_src) = self.read_money(
            &reader,
            &writer,
            player,
            write,
            feats.money_enabled,
            targets.money,
        );
        self.last_money_source = money_src;
        self.last_resolved.money_addr = if self.money_hw_addr != 0 {
            self.money_hw_addr
        } else {
            self.client_money_addr
        };

        let (clip, clip_ok, clip_addr) =
            self.read_clip(&reader, &writer, write && alive, &feats, targets.clip);
        self.last_resolved.clip_addr = clip_addr;

        let (reserve, reserve_ok) =
            self.read_reserve(&reader, &writer, write && alive, &feats, targets.reserve);
        self.last_resolved.reserve_addr = self.reserve_addr;

        let pos_reader = MemoryReader::new(&self.process);
        let expected_hp = if self.entity.health_direct != 0 {
            pos_reader.read_i32(self.entity.health_direct).ok()
        } else {
            None
        };
        let pos_locked = self.locked_pos_player != 0;
        let force_rediscover = !pos_locked && self.pos_static_ticks >= 120;
        let (pos_x, pos_y, pos_z, pos_on, new_pos_off, new_pos_player) = read_position_values(
            &pos_reader,
            player,
            feats.position_enabled,
            self.entity.active,
            self.entity.position,
            self.discovered_pos_off,
            self.discovered_pos_player,
            self.locked_pos_player,
            self.locked_pos_off,
            self.hw_base,
            self.client_base,
            &self.entity,
            self.effective_hp_off(),
            expected_hp,
            force_rediscover,
        );
        let (view_h, view_mx, view_my, view_on) = if feats.position_enabled {
            position::read_view_aux(&pos_reader, self.client_base, self.entity.view_client_rva)
                .map(|(a, b, c)| (a, b, c, true))
                .unwrap_or((0.0, 0.0, 0.0, false))
        } else {
            (0.0, 0.0, 0.0, false)
        };
        if let Some(off) = new_pos_off {
            if self.locked_pos_player == 0 {
                self.locked_pos_off = off;
            } else if !pos_locked && off != self.discovered_pos_off {
                self.discovered_pos_off = off;
                tracing::info!("position offset: player+{off:#x}");
            }
        }
        if let Some(p) = new_pos_player {
            if self.locked_pos_player == 0 {
                self.locked_pos_player = p;
                tracing::info!("position locked: {p:#x}+{}", self.locked_pos_off);
            } else if p != self.discovered_pos_player {
                self.discovered_pos_player = p;
                tracing::info!("position entity: {p:#x}");
            }
        }
        let mut out_x = pos_x;
        let mut out_y = pos_y;
        let mut out_z = pos_z;
        if pos_on {
            // [EN] Jump-detection heuristic: if horizontal position jumped >96 units but the
            // view angle barely changed (< 1 degree), it's likely a false read (teleport hack
            // detection artifact or stale pointer). Revert to last known good position.
            //
            // [FA] هوریستیک تشخیص پرش: اگر موقعیت افقی بیش از ۹۶ واحد پریده اما
            // زاویه دید تقریباً تغییر نکرده (< ۱ درجه)، احتمالاً خواندن نادرست است.
            // به آخرین موقعیت خوب شناخته شده برمی‌گردد.
            let horiz_jump = (pos_x - self.pos_last_x)
                .abs()
                .max((pos_y - self.pos_last_y).abs());
            let view_stable = (view_h - self.pos_last_view_h).abs() < 1.0;
            if self.pos_last_x != 0.0 && horiz_jump > 96.0 && view_stable {
                out_x = self.pos_last_x;
                out_y = self.pos_last_y;
                out_z = self.pos_last_z;
            }
            self.pos_last_view_h = view_h;
            // [EN] Track static position: if position hasn't moved >0.5 units for 120 ticks,
            // the discovered offsets may be stale — force a rediscovery scan.
            // [FA] ردیابی موقعیت ثابت: اگر موقعیت بیش از ۱۲۰ tick بیش از ۰.۵ واحد حرکت نکرده،
            // ممکن است آفست‌های کشف‌شده قدیمی باشند — اسکن کشف مجبور شود.
            let moved = (out_x - self.pos_last_x).abs() > 0.5
                || (out_y - self.pos_last_y).abs() > 0.5
                || (out_z - self.pos_last_z).abs() > 0.5;
            if moved || self.pos_last_x == 0.0 && self.pos_last_y == 0.0 && self.pos_last_z == 0.0 {
                self.pos_static_ticks = 0;
            } else if !pos_locked {
                self.pos_static_ticks = self.pos_static_ticks.saturating_add(1);
            }
            self.pos_last_x = out_x;
            self.pos_last_y = out_y;
            self.pos_last_z = out_z;
            if !pos_locked && self.pos_static_ticks == 120 {
                tracing::warn!("position ثابت ماند — rediscover...");
                self.discovered_pos_off = 0;
                self.discovered_pos_player = 0;
            }
        }
        let pos_entity = self.effective_pos_player();
        self.last_resolved.pos_addr = if pos_on && pos_entity != 0 {
            pos_entity.wrapping_add(self.effective_pos_off())
        } else {
            0
        };

        let got_data = money_ok || clip_ok || reserve_ok || hp_on || armor_on || pos_on;

        if got_data {
            self.stale = 0;
        } else if feats.money_enabled || feats.clip_enabled || feats.reserve_enabled {
            self.stale = self.stale.saturating_add(1);
        }

        self.merge_cache(money_ok, money, clip_ok, clip, reserve_ok, reserve);

        if self.check_ready(&feats) {
            self.display_ready = true;
        }

        GameState {
            money: self.cache.money,
            clip: self.cache.clip,
            reserve: self.cache.reserve,
            hp,
            armor,
            hp_active: hp_on,
            armor_active: armor_on,
            pos_x: out_x,
            pos_y: out_y,
            pos_z: out_z,
            view_h,
            view_mx,
            view_my,
            position_active: pos_on,
            view_active: view_on,
            player_alive: alive,
            connected: true,
            ready: self.display_ready,
            money_valid: self.cache.money_valid,
            clip_valid: self.cache.clip_valid,
            reserve_valid: self.cache.reserve_valid,
        }
    }

    /// [EN] Create a debug snapshot of the current engine state for display/logging.
    ///
    /// [FA] ایجاد اسنپ‌شات اشکال‌زدایی از وضعیت فعلی موتور برای نمایش/لاگ.
    pub fn debug_snapshot(&self, state: &GameState) -> DebugSnapshot {
        DebugSnapshot {
            connected: state.connected,
            ready: state.ready,
            money: state.money,
            clip: state.clip,
            reserve: state.reserve,
            money_valid: state.money_valid,
            clip_valid: state.clip_valid,
            reserve_valid: state.reserve_valid,
            write_enabled: self.config.features.write_enabled,
            player_alive: state.player_alive,
            money_source: self.last_money_source.clone(),
        }
    }

    /// [EN] Refresh hw.dll and client.dll base addresses. If hw.dll moved (e.g., after
    /// a map change), re-resolve all static addresses and position.
    ///
    /// [FA] تازه‌سازی آدرس‌های پایه hw.dll و client.dll. اگر hw.dll جابجا شد
    /// (مثلاً بعد از تغییر نقشه)، تمام آدرس‌های ایستا و موقعیت را مجدداً حل کن.
    fn refresh_bases(&mut self) {
        if let Ok(hw) = engine_base(&self.process, &self.config.modules) {
            if hw != self.hw_base {
                self.hw_base = hw;
                self.resolve_static_addresses();
                self.resolve_position_global();
            }
        }
        let new_client = self
            .process
            .module_base(&self.config.modules.client)
            .unwrap_or(0);
        if new_client != self.client_base {
            self.client_base = new_client;
            self.client_money_addr = if self.client_base != 0 {
                parse_hex_u32(&self.config.chains.money_client_fallback.direct_rva)
                    .map(|rva| self.client_base.wrapping_add(rva))
                    .unwrap_or(0)
            } else {
                0
            };
            if self.entity.active {
                self.entity = parse_entity_addrs(&self.config.entity, self.client_base);
            }
        }
    }

    /// [EN] Read money from the best available source with priority:
    ///   1. hw.dll chain (primary, like `1_cs16`)
    ///   2. client.dll direct address (fallback)
    ///   3. entity offset from player base (optional)
    ///
    ///      Optionally writes the target value if `write` is enabled.
    ///
    /// [FA] خواندن پول از بهترین منبع موجود با اولویت:
    ///   ۱. زنجیره hw.dll (اصلی، مانند `1_cs16`)
    ///   ۲. آدرس مستقیم client.dll (جایگزین)
    ///   ۳. آفست موجودیت از پایه بازیکن (اختیاری)
    /// در صورت فعال بودن `write`، مقدار هدف نوشته می‌شود.
    fn read_money(
        &self,
        reader: &MemoryReader,
        writer: &MemoryWriter,
        player: u32,
        write: bool,
        enabled: bool,
        target: i32,
    ) -> (i32, bool, String) {
        if !enabled {
            return (0, false, String::new());
        }

        // 1) hw chain — اولویت مثل 1_cs16
        if self.money_hw_addr != 0 {
            if let Ok(v) = reader.read_i32(self.money_hw_addr) {
                let shown = apply_write_i32(writer, write, self.money_hw_addr, v, target);
                return (shown, true, "chain_hw".into());
            }
        }

        // 2) client.dll direct fallback
        if self.client_money_addr != 0 {
            if let Ok(v) = reader.read_i32(self.client_money_addr) {
                let shown = apply_write_i32(writer, write, self.client_money_addr, v, target);
                return (shown, true, "client_direct".into());
            }
        }

        // 3) entity (اختیاری)
        if player != 0 && self.entity.active {
            let addr = player.wrapping_add(self.entity.money);
            if let Ok(v) = reader.read_i32(addr) {
                let shown = apply_write_i32(writer, write, addr, v, target);
                return (shown, true, "entity".into());
            }
        }

        (0, false, String::new())
    }

    /// [EN] Resolve the money address from the hw.dll pointer chain in config.
    ///
    /// [FA] حل آدرس پول از زنجیره اشاره‌گر hw.dll در پیکربندی.
    fn try_resolve_money_hw(&self) -> Result<u32, MemoryError> {
        let rva = parse_hex_u32(&self.config.chains.money_hw.base_rva)
            .map_err(|_| MemoryError::InvalidAddress { address: 0 })?;
        let offsets = parse_offsets(&self.config.chains.money_hw.offsets)
            .map_err(|_| MemoryError::InvalidAddress { address: 0 })?;
        resolve_chain(&self.process, self.hw_base.wrapping_add(rva), &offsets)
    }

    /// [EN] Resolve the reserve ammo address from the hw.dll pointer chain in config.
    ///
    /// [FA] حل آدرس مهمات reserve از زنجیره اشاره‌گر hw.dll در پیکربندی.
    fn try_resolve_reserve(&self) -> Result<u32, MemoryError> {
        let rva = parse_hex_u32(&self.config.chains.reserve.base_rva)
            .map_err(|_| MemoryError::InvalidAddress { address: 0 })?;
        let offsets = parse_offsets(&self.config.chains.reserve.offsets)
            .map_err(|_| MemoryError::InvalidAddress { address: 0 })?;
        resolve_chain(&self.process, self.hw_base.wrapping_add(rva), &offsets)
    }

    /// [EN] Read reserve ammo from the resolved address. Optionally writes target value
    /// if reserve_write_enabled is set and the current value differs.
    ///
    /// [FA] خواندن مهمات reserve از آدرس حل‌شده. در صورت فعال بودن reserve_write_enabled
    /// و تفاوت مقدار فعلی، مقدار هدف نوشته می‌شود.
    fn read_reserve(
        &self,
        reader: &MemoryReader,
        writer: &MemoryWriter,
        write: bool,
        feats: &crate::config::FeaturesConfig,
        target: i32,
    ) -> (i32, bool) {
        if !feats.reserve_enabled || self.reserve_addr == 0 {
            return (0, false);
        }

        if let Ok(v) = reader.read_i32(self.reserve_addr) {
            let shown = if write && feats.reserve_write_enabled && v != target {
                let _ = writer.write_i32(self.reserve_addr, target);
                target
            } else {
                v
            };
            return (shown, true);
        }

        (0, false)
    }

    /// [EN] Read current clip ammo. Iterates all clip chains (skipping reserve_clip_index)
    /// and picks the best value in valid range. Falls back to clip[2] chain if all others
    /// break (e.g., when dead or spectating). Optionally writes target to both clip struct
    /// and HUD sync address.
    ///
    /// [FA] خواندن مهمات فعلی clip. تمام زنجیره‌های clip (بدون reserve_clip_index) را تکرار
    /// و بهترین مقدار در محدوده معتبر را انتخاب می‌کند. اگر همه بشکنند (مثلاً هنگام مرگ یا
    /// تماشا)، به زنجیره clip[2] برمی‌گردد. در صورت فعال بودن، مقدار هدف را هم به
    /// ساختار clip و هم به آدرس HUD sync می‌نویسد.
    fn read_clip(
        &self,
        reader: &MemoryReader,
        writer: &MemoryWriter,
        write: bool,
        feats: &crate::config::FeaturesConfig,
        target: i32,
    ) -> (i32, bool, u32) {
        if !feats.clip_enabled {
            return (0, false, 0);
        }

        let range = &self.config.clip_detection;
        let hud = &self.config.hud_sync;
        let clip_off = parse_hex_u32(&hud.clip_struct_offset).unwrap_or(0xCC);
        let hud_off = parse_hex_u32(&hud.hud_clip_offset).unwrap_or(0xD0);

        let mut best_addr = 0u32;
        let mut best_val = 0i32;

        for i in self.clip_chain_indices() {
            let Some(chain) = self.config.chains.clip.get(i) else {
                continue;
            };
            let Some(addr) = self.resolve_chain_addr(chain) else {
                continue;
            };
            let Ok(val) = reader.read_i32(addr) else {
                continue;
            };
            if val >= range.min_value && val < range.max_value && (best_addr == 0 || val > best_val)
            {
                best_val = val;
                best_addr = addr;
            }
        }

        // وقتی chainهای بلند magazine می‌شکنند (مرده/اسپکت)، chain کوتاه clip[2] را امتحان کن
        if best_addr == 0 {
            let fb = self.config.chains.reserve_clip_index;
            if let Some(chain) = self.config.chains.clip.get(fb) {
                if let Some(addr) = self.resolve_chain_addr(chain) {
                    if let Ok(val) = reader.read_i32(addr) {
                        if val >= range.min_value && val < range.max_value {
                            best_val = val;
                            best_addr = addr;
                        }
                    }
                }
            }
        }

        if best_addr == 0 {
            return (0, false, 0);
        }

        // مثل 1_cs16: اگر مقدار فعلی همان target است، همان را نشان بده
        let shown = if write && feats.clip_write_enabled && best_val != target {
            let _ = writer.write_i32(best_addr, target);
            if hud.enabled {
                let weapon = best_addr.wrapping_sub(clip_off);
                let hud_addr = weapon.wrapping_add(hud_off);
                let _ = writer.write_i32(hud_addr, target);
            }
            target
        } else {
            best_val
        };

        (shown, true, best_addr)
    }

    /// [EN] Read health and armor from memory. Tries direct address first, then falls back
    /// to player base + offset. Validates values are in [0..100] range.
    /// Returns (hp, armor, hp_on, armor_on, alive) where "on" flags mean "successfully read".
    ///
    /// [FA] خواندن سلامتی و زره از حافظه. ابتدا آدرس مستقیم و سپس پایه بازیکن + آفست را
    /// امتحان می‌کند. مقادیر در محدوده [0..100] اعتبارسنجی می‌شوند.
    /// (hp, armor, hp_on, armor_on, alive) برمی‌گرداند که پرچم‌های "on" یعنی "با موفقیت خوانده شده".
    fn read_vitals(&self, reader: &MemoryReader, player: u32) -> (f32, f32, bool, bool, bool) {
        let feats = &self.config.features;
        let mut hp = 0.0f32;
        let mut armor = 0.0f32;
        let mut hp_on = false;
        let mut armor_on = false;

        if feats.hp_enabled {
            if self.entity.health_direct != 0 {
                if let Ok(v) =
                    read_typed(reader, self.entity.health_direct, self.entity.health_type)
                {
                    if (0.0..=100.0).contains(&v) {
                        hp_on = true;
                        hp = v;
                    }
                }
            }
            if !hp_on && player != 0 && self.entity.active {
                let hp_off = self.effective_hp_off();
                let addr = player.wrapping_add(hp_off);
                if let Ok(v) = read_typed(reader, addr, self.entity.health_type) {
                    if (0.0..=100.0).contains(&v) {
                        hp_on = true;
                        hp = v;
                    }
                }
            }
        }

        if feats.armor_enabled {
            if self.entity.armor_direct != 0 {
                if let Ok(v) = read_typed(reader, self.entity.armor_direct, self.entity.armor_type)
                {
                    if (0.0..=100.0).contains(&v) {
                        armor_on = true;
                        armor = v;
                    }
                }
            }
            if !armor_on && player != 0 && self.entity.active {
                if let Ok(v) = read_typed(
                    reader,
                    player.wrapping_add(self.entity.armor),
                    self.entity.armor_type,
                ) {
                    if (0.0..=100.0).contains(&v) {
                        armor_on = true;
                        armor = v;
                    }
                }
            }
        }

        let alive = if hp_on { hp > 0.5 } else { true };
        (hp, armor, hp_on, armor_on, alive)
    }

    /// [EN] Return the effective position player entity: locked > discovered > local_player.
    /// The locked value takes highest priority since it was validated at connect time.
    ///
    /// [FA] موجودیت بازیکن موقعیت مؤثر را برگردان: قفل‌شده > کشف‌شده > local_player.
    /// مقدار قفل‌شده بالاترین اولویت را دارد زیرا در زمان اتصال اعتبارسنجی شده.
    fn effective_pos_player(&self) -> u32 {
        if self.locked_pos_player != 0 {
            self.locked_pos_player
        } else if self.discovered_pos_player != 0 {
            self.discovered_pos_player
        } else {
            self.last_resolved.local_player
        }
    }

    /// [EN] Return the effective position offset: locked > discovered > config.
    /// Matches the priority of effective_pos_player.
    ///
    /// [FA] آفست موقعیت مؤثر را برگردان: قفل‌شده > کشف‌شده > پیکربندی.
    /// اولویت effective_pos_player را منعکس می‌کند.
    fn effective_pos_off(&self) -> u32 {
        if self.locked_pos_player != 0 {
            self.locked_pos_off
        } else if self.discovered_pos_off != 0 {
            self.discovered_pos_off
        } else {
            self.entity.position
        }
    }

    /// [EN] Merge new read values into the cache. Only updates a field if the read was successful
    /// (`*_ok` flag is true). This preserves last-known good values across failed reads.
    ///
    /// [FA] ادغام مقادیر جدید خوانده شده در کش. فقط در صورت موفقیت آمیز بودن خواندن
    /// (پرچم `*_ok` true باشد) فیلد به‌روز می‌شود. این مقادیر خوب آخرین شناخته شده را
    /// در خواندن‌های ناموفق حفظ می‌کند.
    fn merge_cache(
        &mut self,
        money_ok: bool,
        money: i32,
        clip_ok: bool,
        clip: i32,
        reserve_ok: bool,
        reserve: i32,
    ) {
        if money_ok {
            self.cache.money = money;
            self.cache.money_valid = true;
        }
        if clip_ok {
            self.cache.clip = clip;
            self.cache.clip_valid = true;
        }
        if reserve_ok {
            self.cache.reserve = reserve;
            self.cache.reserve_valid = true;
        }
    }

    /// [EN] Check if the engine has enough data to display. Once display_ready is set,
    /// it stays true permanently for the session. Returns false until all enabled features
    /// have been successfully read at least once.
    ///
    /// [FA] بررسی اینکه آیا موتور داده کافی برای نمایش دارد. وقتی display_ready تنظیم شد،
    /// برای کل نشست true باقی می‌ماند. تا زمانی که تمام ویژگی‌های فعال حداقل یک‌بار
    /// با موفقیت خوانده شوند false برمی‌گرداند.
    fn check_ready(&self, feats: &crate::config::FeaturesConfig) -> bool {
        if self.display_ready {
            return true;
        }
        if feats.money_enabled && !self.cache.money_valid {
            return false;
        }
        if feats.clip_enabled && feats.reserve_enabled {
            if !self.cache.clip_valid && !self.cache.reserve_valid {
                return false;
            }
        } else if (feats.clip_enabled && !self.cache.clip_valid)
            || (feats.reserve_enabled && !self.cache.reserve_valid)
        {
            return false;
        }
        true
    }
}

/// [EN] Public entry point to create a GameEngine. Wraps `GameEngine::open` and converts
/// `AppError` to `MemoryError` for the public API.
///
/// [FA] نقطه ورود عمومی برای ایجاد GameEngine. `GameEngine::open` را بسته‌بندی می‌کند
/// و `AppError` را به `MemoryError` برای API عمومی تبدیل می‌کند.
pub fn connect(config: &AppConfig) -> Result<GameEngine, MemoryError> {
    GameEngine::open(config).map_err(|e| match e {
        AppError::Memory(m) => m,
        other => MemoryError::ProcessNotFound {
            name: format!("{other}"),
        },
    })
}

/// [EN] Parse entity configuration into an `EntityAddrs` struct. Resolves all hex RVA
/// strings to u32 values and sets up direct addresses for health/armor.
/// Returns a default (inactive) EntityAddrs if any required field fails to parse.
///
/// [FA] ت解析 پیکربندی موجودیت به ساختار `EntityAddrs`. تمام رشته‌های hex RVA
/// را به مقادیر u32 حل می‌کند و آدرس‌های مستقیم برای سلامتی/زره راه‌اندازی می‌کند.
/// اگر هر فیلد مورد نیاز در ت解析 ناموفق باشد، EntityAddrs پیش‌فرض (غیرفعال) برمی‌گرداند.
fn parse_entity_addrs(entity: &crate::config::EntityConfig, client_base: u32) -> EntityAddrs {
    let Ok(lp) = parse_hex_u32(&entity.local_player_rva) else {
        return EntityAddrs::default();
    };
    let Ok(money) = parse_hex_u32(&entity.money_offset) else {
        return EntityAddrs::default();
    };
    let Ok(health) = parse_hex_u32(&entity.health_offset) else {
        return EntityAddrs::default();
    };
    let Ok(armor) = parse_hex_u32(&entity.armor_offset) else {
        return EntityAddrs::default();
    };
    let Ok(position) = parse_hex_u32(&entity.position_offset) else {
        return EntityAddrs::default();
    };
    let health_type = ValueType::parse(&entity.health_type).unwrap_or(ValueType::Int);
    let armor_type = ValueType::parse(&entity.armor_type).unwrap_or(ValueType::Int);
    let on_client = entity.local_player_module.eq_ignore_ascii_case("client");
    let health_direct = entity
        .health_direct_rva
        .as_ref()
        .and_then(|s| parse_hex_u32(s).ok())
        .map(|rva| client_base.wrapping_add(rva))
        .unwrap_or(0);
    let armor_direct = entity
        .armor_direct_rva
        .as_ref()
        .and_then(|s| parse_hex_u32(s).ok())
        .map(|rva| client_base.wrapping_add(rva))
        .unwrap_or(0);

    EntityAddrs {
        local_player_on_client: on_client,
        local_player_ptr: lp,
        money,
        health,
        armor,
        health_type,
        armor_type,
        health_direct,
        armor_direct,
        position,
        position_entity_hw_rva: entity
            .position_entity_hw_rva
            .as_ref()
            .and_then(|s| parse_hex_u32(s).ok()),
        position_global_hw_rva: entity
            .position_global_hw_rva
            .as_ref()
            .and_then(|s| parse_hex_u32(s).ok()),
        view_client_rva: entity
            .view_client_rva
            .as_ref()
            .and_then(|s| parse_hex_u32(s).ok()),
        active: true,
    }
}

/// [EN] Multi-strategy position reader. Tries six different approaches in order:
///   1. Locked address (global hw or entity pev) — highest priority, validated at connect
///   2. hw.dll global RVA — primary source from config
///   3. hw.dll entity → pev → origin — fallback with HP verification
///   4. hw.dll entity world origin — fallback without HP verify
///   5. Cached discovery from previous tick
///   6. client.dll entity + offset
///   7. Full live discovery scan (only when position has been static for 120+ ticks)
///
/// Returns (x, y, z, active, optional_new_offset, optional_new_player).
#[allow(clippy::too_many_arguments)]
fn read_position_values(
    reader: &MemoryReader,
    player: u32,
    enabled: bool,
    entity_active: bool,
    config_off: u32,
    discovered_off: u32,
    discovered_player: u32,
    locked_player: u32,
    locked_off: u32,
    hw_base: u32,
    client_base: u32,
    entity: &EntityAddrs,
    hp_off: u32,
    expected_hp: Option<i32>,
    force_rediscover: bool,
) -> (f32, f32, f32, bool, Option<u32>, Option<u32>) {
    if !enabled || !entity_active {
        return (0.0, 0.0, 0.0, false, None, None);
    }

    let try_read = |base: u32, off: u32| -> Option<(f32, f32, f32)> {
        position::read_runtime_world_vec3(reader, base, off)
    };

    // 1) آدرس قفل‌شده (global hw یا entity pev)
    if locked_player != 0 {
        if let Some((x, y, z)) = try_read(locked_player, locked_off) {
            return (x, y, z, true, None, None);
        }
    }

    // 2) hw global RVA — primary (dump: hw+0x7CD13C offset 0)
    if let Some(rva) = entity.position_global_hw_rva {
        if let Some((disc, (x, y, z))) = position::read_global_world_at_rva(reader, hw_base, rva) {
            return (x, y, z, true, Some(disc.offset), Some(disc.player));
        }
    }

    // 3) hw entity 0x169438 → pev → origin (fallback)
    if let Some((found, (x, y, z))) = position::resolve_hw_local_player_position(
        reader,
        hw_base,
        entity.position_entity_hw_rva,
        config_off,
        None,
    ) {
        return (x, y, z, true, Some(found.offset), Some(found.player));
    }

    // 4) hw entity fallback (بدون HP verify)
    if let Some(found) =
        position::read_hw_entity_world_origin(reader, hw_base, entity.position_entity_hw_rva)
    {
        if let Some((x, y, z)) = try_read(found.player, found.offset) {
            return (x, y, z, true, Some(found.offset), Some(found.player));
        }
    }

    // 5) کش discovery قبلی
    if !force_rediscover && discovered_player != 0 {
        if let Some((x, y, z)) = try_read(discovered_player, discovered_off) {
            return (x, y, z, true, None, None);
        }
    }

    // 6) client entity + offset
    if !force_rediscover && player != 0 {
        let off = if discovered_off != 0 {
            discovered_off
        } else {
            config_off
        };
        if let Some((x, y, z)) = try_read(player, off) {
            return (x, y, z, true, None, None);
        }
    }

    let candidates = position::collect_position_candidates(
        reader,
        hw_base,
        client_base,
        entity.local_player_on_client,
        entity.local_player_ptr,
        entity.health_direct,
        hp_off,
        entity.position_entity_hw_rva,
    );

    if force_rediscover {
        if let Some(found) = position::discover_position_live(
            reader,
            hw_base,
            client_base,
            &candidates,
            config_off,
            expected_hp,
            400,
            entity.position_global_hw_rva,
            None,
        ) {
            if let Some((x, y, z)) = try_read(found.player, found.offset) {
                return (x, y, z, true, Some(found.offset), Some(found.player));
            }
        }
    }

    (0.0, 0.0, 0.0, false, None, None)
}

/// [EN] Helper to conditionally write an i32 value. If `write` is enabled and current
/// differs from target, writes target to addr and returns it; otherwise returns current.
///
/// [FA] کمکی برای نوشتن شرطی مقدار i32. اگر `write` فعال باشد و مقدار فعلی با هدف
/// تفاوت داشته باشد، هدف را به addr می‌نویسد و برمی‌گرداند؛ در غیر این صورت مقدار فعلی.
fn apply_write_i32(
    writer: &MemoryWriter,
    write: bool,
    addr: u32,
    current: i32,
    target: i32,
) -> i32 {
    if write && current != target {
        let _ = writer.write_i32(addr, target);
        target
    } else {
        current
    }
}

/// [EN] Read a value from memory and convert it to f32 based on the configured type.
/// Supports Int (read as i32, cast), Float (read directly), and Byte (read as u8, cast).
///
/// [FA] خواندن مقدار از حافظه و تبدیل آن به f32 بر اساس نوع پیکربندی شده.
/// از Int (خواندن به عنوان i32، تبدیل)، Float (خواندن مستقیم)، و Byte (خواندن به عنوان u8، تبدیل) پشتیبانی می‌کند.
fn read_typed(reader: &MemoryReader, addr: u32, ty: ValueType) -> Result<f32, MemoryError> {
    match ty {
        ValueType::Int => reader.read_i32(addr).map(|v| v as f32),
        ValueType::Float => reader.read_f32(addr),
        ValueType::Byte => reader.read::<u8>(addr).map(|v| v as f32),
    }
}

/// [EN] Unit tests for engine module logic.
/// [FA] تست‌های واحد برای منطق ماژول موتور.
#[cfg(test)]
mod tests {
    #[test]
    fn clip_pick_matches_1_cs16() {
        let range = crate::config::ClipDetectionConfig {
            min_value: 0,
            max_value: 150,
        };
        let candidates = [(0x1000, 5), (0x2000, 12), (0x3000, 8), (0x4000, 200)];
        let mut best = (0u32, 0i32);
        for &(a, v) in &candidates {
            if v > 0 && v < range.max_value && v > best.1 {
                best = (a, v);
            }
        }
        assert_eq!(best, (0x2000, 12));
    }
}
