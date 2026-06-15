//! GameEngine — منطق مطابق `1_cs16` / `2_cs16`.
//!
//! - money/reserve: resolve یک‌بار در connect، refresh اگر read شکست بخورد
//! - clip: هر tick همه chainها resolve (مثل while در C++)

use crate::config::{parse_hex_u32, parse_offsets, AppConfig, ValueType};
use crate::error::{AppError, MemoryError};
use crate::game::local_player;
use crate::game::position;
use crate::game::state::{DebugSnapshot, GameState, ResolvedInfo};
use crate::win::memory::{resolve_chain, MemoryReader, MemoryWriter};
use crate::win::process::{engine_base, is_alive, ProcessHandle};

#[derive(Debug, Clone, Default)]
struct Cache {
    money: i32,
    money_valid: bool,
    clip: i32,
    clip_valid: bool,
    reserve: i32,
    reserve_valid: bool,
}

pub struct GameEngine {
    process: ProcessHandle,
    config: AppConfig,
    hw_base: u32,
    client_base: u32,
    /// resolve یک‌بار — مثل `hwMoneyAddr` در 1_cs16
    money_hw_addr: u32,
    /// resolve یک‌بار — مثل `hwAmmoAddr` در 1_cs16
    reserve_addr: u32,
    /// `clientBase + CLIENT_MONEY_RVA`
    client_money_addr: u32,
    entity: EntityAddrs,
    /// RVA کشف‌شده runtime (0 = هنوز discover نشده / از config)
    discovered_lp_rva: u32,
    discovered_hp_off: u32,
    discovered_pos_off: u32,
    /// entity واقعی برای vec3 — گاهی hw.dll نه client.dll
    discovered_pos_player: u32,
    /// قفل global بعد از اولین resolve (hw+rva)
    locked_pos_player: u32,
    locked_pos_off: u32,
    pos_last_x: f32,
    pos_last_y: f32,
    pos_last_z: f32,
    pos_last_view_h: f32,
    pos_static_ticks: u32,
    stale: u32,
    cache: Cache,
    display_ready: bool,
    last_money_source: String,
    last_resolved: ResolvedInfo,
}

#[derive(Debug, Clone, Copy, Default)]
struct EntityAddrs {
    local_player_on_client: bool,
    local_player_ptr: u32,
    money: u32,
    health: u32,
    armor: u32,
    health_type: ValueType,
    armor_type: ValueType,
    health_direct: u32,
    armor_direct: u32,
    position: u32,
    position_entity_hw_rva: Option<u32>,
    position_global_hw_rva: Option<u32>,
    view_client_rva: Option<u32>,
    active: bool,
}

impl GameEngine {
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

    /// همان بلوک startup در 1_cs16: resolve money + reserve یک‌بار
    fn resolve_static_addresses(&mut self) {
        self.money_hw_addr = self
            .try_resolve_money_hw()
            .unwrap_or(0);
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

    /// قفل hw+position_global_hw_rva در connect — همان آدرسی که dump movement OK می‌دهد
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

    fn refresh_money_hw_if_needed(&mut self) {
        if self.money_hw_addr != 0 {
            return;
        }
        self.money_hw_addr = self.try_resolve_money_hw().unwrap_or(0);
    }

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

    /// primary reserve گاهی resolve می‌شود ولی مقدار 0/اشتباه دارد — fallback clip[2] را مقایسه کن
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

    fn effective_lp_rva(&self) -> u32 {
        if self.discovered_lp_rva != 0 {
            self.discovered_lp_rva
        } else {
            self.entity.local_player_ptr
        }
    }

    fn effective_hp_off(&self) -> u32 {
        if self.discovered_hp_off != 0 {
            self.discovered_hp_off
        } else {
            self.entity.health
        }
    }

    fn entity_module_base(&self) -> u32 {
        if self.entity.local_player_on_client {
            self.client_base
        } else {
            self.hw_base
        }
    }

    fn resolve_local_player_ptr(&self, reader: &MemoryReader) -> u32 {
        if !self.entity.active {
            return 0;
        }

        let hp_off = self.effective_hp_off();

        // health_direct − hp_off — همان CE player_health_from_base
        if self.entity.health_direct != 0 {
            let player = self.entity.health_direct.wrapping_sub(hp_off);
            if player != 0 && (player & 3) == 0 {
                if reader
                    .read_i32(self.entity.health_direct)
                    .ok()
                    .is_some_and(|hp| (0..=100).contains(&hp))
                {
                    return player;
                }
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

    /// clip[2] در dump کاربر = reserve (20) — از `reserve_clip_index` در config
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
        resolve_chain(
            &self.process,
            self.hw_base.wrapping_add(rva),
            &offsets,
        )
        .unwrap_or(0)
    }

    fn resolve_chain_addr(&self, chain: &crate::config::PointerChainConfig) -> Option<u32> {
        let rva = parse_hex_u32(&chain.base_rva).ok()?;
        let offsets = parse_offsets(&chain.offsets).ok()?;
        resolve_chain(
            &self.process,
            self.hw_base.wrapping_add(rva),
            &offsets,
        )
        .ok()
    }

    fn clip_chain_indices(&self) -> impl Iterator<Item = usize> + '_ {
        let skip = self.config.chains.reserve_clip_index;
        (0..self.config.chains.clip.len()).filter(move |i| *i != skip)
    }

    pub fn pid(&self) -> u32 {
        self.process.pid()
    }

    pub fn resolved(&self) -> &ResolvedInfo {
        &self.last_resolved
    }

    pub fn client_money_addr(&self) -> u32 {
        self.client_money_addr
    }

    pub fn should_reconnect(&self) -> bool {
        if !is_alive(self.process.pid()) {
            return true;
        }
        if engine_base(&self.process, &self.config.modules).is_err() {
            return true;
        }
        self.stale >= self.config.timing.stale_reconnect_ticks
    }

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

        let (clip, clip_ok, clip_addr) = self.read_clip(
            &reader,
            &writer,
            write && alive,
            &feats,
            targets.clip,
        );
        self.last_resolved.clip_addr = clip_addr;

        let (reserve, reserve_ok) = self.read_reserve(
            &reader,
            &writer,
            write && alive,
            &feats,
            targets.reserve,
        );
        self.last_resolved.reserve_addr = self.reserve_addr;

        drop(reader);
        drop(writer);

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
                tracing::info!(
                    "position locked: {p:#x}+{}",
                    self.locked_pos_off
                );
            } else if p != self.discovered_pos_player {
                self.discovered_pos_player = p;
                tracing::info!("position entity: {p:#x}");
            }
        }
        let mut out_x = pos_x;
        let mut out_y = pos_y;
        let mut out_z = pos_z;
        if pos_on {
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

        let got_data = money_ok
            || clip_ok
            || reserve_ok
            || hp_on
            || armor_on
            || pos_on;

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

    fn try_resolve_money_hw(&self) -> Result<u32, MemoryError> {
        let rva = parse_hex_u32(&self.config.chains.money_hw.base_rva)
            .map_err(|_| MemoryError::InvalidAddress { address: 0 })?;
        let offsets = parse_offsets(&self.config.chains.money_hw.offsets)
            .map_err(|_| MemoryError::InvalidAddress { address: 0 })?;
        resolve_chain(
            &self.process,
            self.hw_base.wrapping_add(rva),
            &offsets,
        )
    }

    fn try_resolve_reserve(&self) -> Result<u32, MemoryError> {
        let rva = parse_hex_u32(&self.config.chains.reserve.base_rva)
            .map_err(|_| MemoryError::InvalidAddress { address: 0 })?;
        let offsets = parse_offsets(&self.config.chains.reserve.offsets)
            .map_err(|_| MemoryError::InvalidAddress { address: 0 })?;
        resolve_chain(
            &self.process,
            self.hw_base.wrapping_add(rva),
            &offsets,
        )
    }

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
            if val >= range.min_value && val < range.max_value && (best_addr == 0 || val > best_val) {
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

    fn read_vitals(
        &self,
        reader: &MemoryReader,
        player: u32,
    ) -> (f32, f32, bool, bool, bool) {
        let feats = &self.config.features;
        let mut hp = 0.0f32;
        let mut armor = 0.0f32;
        let mut hp_on = false;
        let mut armor_on = false;

        if feats.hp_enabled {
            if self.entity.health_direct != 0 {
                if let Ok(v) = read_typed(reader, self.entity.health_direct, self.entity.health_type)
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

    fn effective_pos_player(&self) -> u32 {
        if self.locked_pos_player != 0 {
            self.locked_pos_player
        } else if self.discovered_pos_player != 0 {
            self.discovered_pos_player
        } else {
            self.last_resolved.local_player
        }
    }

    fn effective_pos_off(&self) -> u32 {
        if self.locked_pos_player != 0 {
            self.locked_pos_off
        } else if self.discovered_pos_off != 0 {
            self.discovered_pos_off
        } else {
            self.entity.position
        }
    }

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
        } else if feats.clip_enabled && !self.cache.clip_valid {
            return false;
        } else if feats.reserve_enabled && !self.cache.reserve_valid {
            return false;
        }
        true
    }
}

pub fn connect(config: &AppConfig) -> Result<GameEngine, MemoryError> {
    GameEngine::open(config).map_err(|e| match e {
        AppError::Memory(m) => m,
        other => MemoryError::ProcessNotFound {
            name: format!("{other}"),
        },
    })
}

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
        if let Some((disc, (x, y, z))) =
            position::read_global_world_at_rva(reader, hw_base, rva)
        {
            return (
                x,
                y,
                z,
                true,
                Some(disc.offset),
                Some(disc.player),
            );
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
        return (
            x,
            y,
            z,
            true,
            Some(found.offset),
            Some(found.player),
        );
    }

    // 4) hw entity fallback (بدون HP verify)
    if let Some(found) =
        position::read_hw_entity_world_origin(reader, hw_base, entity.position_entity_hw_rva)
    {
        if let Some((x, y, z)) = try_read(found.player, found.offset) {
            return (
                x,
                y,
                z,
                true,
                Some(found.offset),
                Some(found.player),
            );
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
                return (
                    x,
                    y,
                    z,
                    true,
                    Some(found.offset),
                    Some(found.player),
                );
            }
        }
    }

    (0.0, 0.0, 0.0, false, None, None)
}

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

fn read_typed(reader: &MemoryReader, addr: u32, ty: ValueType) -> Result<f32, MemoryError> {
    match ty {
        ValueType::Int => reader.read_i32(addr).map(|v| v as f32),
        ValueType::Float => reader.read_f32(addr),
        ValueType::Byte => reader.read::<u8>(addr).map(|v| v as f32),
    }
}

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
