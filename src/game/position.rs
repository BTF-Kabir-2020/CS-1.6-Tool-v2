//! خواندن مختصات بازیکن (vec3 origin) از entity یا آدرس سراسری hw/client.
//!
//! client entity فقط HP دارد؛ world origin معمولاً روی hw.dll است (entity یا global RVA).

use std::thread;
use std::time::Duration;

use crate::win::memory::MemoryReader;

/// offsetهای شناخته‌شده m_vecOrigin / entvars.origin در buildهای مختلف GoldSrc
pub const POS_OFFSET_CANDIDATES: &[u32] = &[
    0x8, 0x14, 0x20, // entvars: origin, oldorigin, velocity
    0x34, 0x38, 0x3C, 0x40, 0x44, 0x48,
    0x128, 0x12C, 0x130,
    0x134,
    0x138, 0x13C, 0x140,
    0x1A4, 0x204, 0x334,
];

const HP_OFFS_HW: &[u32] = &[0x59C, 0x334, 0x100, 0xB74, 0xFC, 0x14];

/// hw.dll RVAs — world entity (اولویت با 0x169438)
const LP_RVA_HW_FOR_POS: &[u32] = &[
    0x00169438,
    0x0010FC80,
    0x00176C68,
    0x001694F0,
    0x0013FDF4,
];

/// vec3 مستقیم در hw.dll — buildهای شناخته‌شده (مثلاً 8684: EntityOrigin)
const GLOBAL_ORIGIN_RVA_HW: &[u32] = &[
    0x0007CD13C, // non-Steam build — dump movement OK
    0x0012047A0,
    0x001230274,
    0x00122E324,
    0x00108AEC4,
];

/// vec3 مستقیم در client.dll — LocalOrigin و مشابه
const GLOBAL_ORIGIN_RVA_CLIENT: &[u32] = &[
    0x0013E7F0,
    0x0012D9F0,
];

pub struct PositionDiscovery {
    /// پایه vec3 — entity یا آدرس سراسری
    pub player: u32,
    pub offset: u32,
}

#[derive(Clone, Copy)]
pub struct PlayerCandidate {
    pub player: u32,
    pub hp_offset: u32,
    pub from_hw: bool,
}

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

/// مختصات شبیه spawn/view ثابت — مثل (0, 300, 0) یا (300, 0, 0)
pub fn looks_like_spawn_stub(x: f32, y: f32, z: f32) -> bool {
    if x.abs() < 4.0 && z.abs() < 4.0 && y.abs() > 40.0 {
        return true;
    }
    y.abs() < 4.0 && z.abs() < 4.0 && x.abs() > 40.0
}

fn looks_like_velocity(x: f32, y: f32, z: f32) -> bool {
    if !x.is_finite() || !y.is_finite() || !z.is_finite() {
        return false;
    }
    let h = x.abs().max(y.abs());
    (1.0..=500.0).contains(&h) && z.abs() <= 500.0
}

/// edict→entvars (pev) در buildهای مختلف
const PEV_PTR_OFFS: &[u32] = &[0x0, 0x4, 0x8, 0xC, 0x10, 0x14, 0x18, 0x1C, 0x20, 0x24, 0x28, 0x2C];

/// شمارش معکوس آماده‌سازی — بعدش بلافاصله نمونه‌گیری شروع می‌شود
pub fn prepare_walk_test(prep_secs: u32) {
    for s in (1..=prep_secs).rev() {
        println!("  ⏳ {s} — Alt+Tab به بازی...");
        thread::sleep(Duration::from_secs(1));
    }
    println!("  ▶ الان W / strafe را نگه دار!");
}

fn valid_ptr(addr: u32) -> bool {
    (0x0100_0000..=0x7FFF_0000).contains(&addr) && addr & 3 == 0
}

fn snap_filter_vec3(x: f32, y: f32, z: f32) -> bool {
    x.is_finite() && y.is_finite() && z.is_finite() && (x != 0.0 || y != 0.0 || z != 0.0)
}

fn plausible_for_movement(x: f32, y: f32, z: f32) -> bool {
    if !x.is_finite() || !y.is_finite() || !z.is_finite() {
        return false;
    }
    if x == 0.0 && y == 0.0 && z == 0.0 {
        return false;
    }
    x.abs() <= 32_768.0 && y.abs() <= 32_768.0 && z.abs() <= 16_384.0
}

pub fn peek_vec3(reader: &MemoryReader, base: u32, offset: u32) -> Option<(f32, f32, f32)> {
    if base < 0x0100_0000 || base > 0x7FFF_0000 {
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

/// مختصات قابل نمایش — صفر، stub و خارج از محدوده نقشه رد می‌شوند
pub fn is_usable_position(x: f32, y: f32, z: f32) -> bool {
    if !x.is_finite() || !y.is_finite() || !z.is_finite() {
        return false;
    }
    if x == 0.0 && y == 0.0 && z == 0.0 {
        return false;
    }
    !looks_like_spawn_stub(x, y, z) && looks_like_coords(x, y, z)
}

/// pitch byte (-128) اشتباه به‌عنوان float Y خوانده شده — نه world origin
fn looks_like_pitch_misread(y: f32, z: f32) -> bool {
    (y + 128.0).abs() < 1.0 && z.abs() <= 2.0
}

/// client+0x11D478 و مشابه — یک محور بزرگ + یکی ~صفر (NOT world XY)
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
    // الگوی (164, 1, 140) — یک محور افقی بزرگ، دیگری ~صفر
    if max_h >= 24.0 && min_h < 12.0 {
        return true;
    }
    // زاویه دید: هر سه مؤلفه کوچک
    x.abs() <= 90.0 && y.abs() <= 360.0 && z.abs() <= 180.0 && max_h < 24.0
}

/// XYZ واقعی نقشه — هر دو محور افقی معنی‌دار
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

pub fn is_usable_world_position(x: f32, y: f32, z: f32) -> bool {
    looks_like_world_origin(x, y, z)
}

pub fn read_vec3(reader: &MemoryReader, base: u32, offset: u32) -> Option<(f32, f32, f32)> {
    peek_vec3(reader, base, offset).filter(|&(x, y, z)| is_usable_position(x, y, z))
}

pub fn read_world_vec3(reader: &MemoryReader, base: u32, offset: u32) -> Option<(f32, f32, f32)> {
    peek_vec3(reader, base, offset).filter(|&(x, y, z)| is_usable_world_position(x, y, z))
}

/// H / mouse-ish vec3 از client.dll (view_client_rva)
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

/// runtime — کمتر سخت‌گیر از looks_like_world_origin
fn is_runtime_world_xyz(x: f32, y: f32, z: f32) -> bool {
    is_usable_position(x, y, z) && !looks_like_view_aux(x, y, z)
}

fn peek_runtime_world(
    reader: &MemoryReader,
    base: u32,
    offset: u32,
) -> Option<(f32, f32, f32)> {
    peek_vec3(reader, base, offset).filter(|&(x, y, z)| is_runtime_world_xyz(x, y, z))
}

/// خواندن XYZ برای runtime — فیلتر کمتر سخت‌گیر از read_world_vec3
pub fn read_runtime_world_vec3(
    reader: &MemoryReader,
    base: u32,
    offset: u32,
) -> Option<(f32, f32, f32)> {
    peek_runtime_world(reader, base, offset)
}

/// hw+0x169438 → edict → pev → origin (با HP verify)
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

    for &hoff in HP_OFFS_HW {
        if !hp_matches(reader, entity, hoff, expected_hp) {
            continue;
        }
        let cand = PlayerCandidate {
            player: entity,
            hp_offset: hoff,
            from_hw: true,
        };
        for (base, off) in collect_entity_hits(
            reader,
            cand,
            config_off,
            expected_hp,
            true,
            false,
        ) {
            let bonus = if base == entity { 200 } else { 800 };
            consider(base, off, bonus);
        }
    }

    best.map(|(_, d, xyz)| (d, xyz))
}

/// hw entity + pev → origin نقشه
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

    for &off in POS_OFFSET_CANDIDATES {
        consider(entity, off, 200);
    }
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

fn pos_delta_sq(a: (f32, f32, f32), b: (f32, f32, f32)) -> f32 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    let dz = a.2 - b.2;
    dx * dx + dy * dy + dz * dz
}

fn is_client_stub(player: u32, health_direct: u32, client_hp_off: u32) -> bool {
    health_direct != 0 && player == health_direct.wrapping_sub(client_hp_off)
}

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

fn hp_matches(
    reader: &MemoryReader,
    player: u32,
    hp_off: u32,
    expected: Option<i32>,
) -> bool {
    let Ok(hp) = reader.read_i32(player.wrapping_add(hp_off)) else {
        return false;
    };
    if !(0..=100).contains(&hp) {
        return false;
    }
    expected.is_none_or(|exp| hp == exp)
}

fn offsets_to_try(config_off: u32) -> Vec<u32> {
    let mut v = vec![config_off];
    for &off in POS_OFFSET_CANDIDATES {
        if off != config_off {
            v.push(off);
        }
    }
    v
}

fn push_hit(hits: &mut Vec<(u32, u32)>, player: u32, off: u32) {
    if !hits.iter().any(|&(p, o)| p == player && o == off) {
        hits.push((player, off));
    }
}

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

    for off in offsets_to_try(config_off) {
        if let Some(v) = peek_vec3(reader, cand.player, off) {
            if check(v) {
                push_hit(&mut hits, cand.player, off);
            }
        }
    }

    // entvars از edict→pev
    for &pev_off in PEV_PTR_OFFS {
        let Ok(pev) = reader.read_u32(cand.player.wrapping_add(pev_off)) else {
            continue;
        };
        if pev < 0x0100_0000 || pev > 0x7FFF_0000 {
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

    // entvars ممکن است مستقیم در player باشد
    if let Ok(entvars) = reader.read_u32(cand.player) {
        if entvars >= 0x0100_0000 && entvars <= 0x7FFF_0000 {
            for &off in &[0x8u32, 0x14, 0x20, 0x34] {
                if let Some(v) = peek_vec3(reader, entvars, off) {
                    if check(v) {
                        push_hit(&mut hits, entvars, off);
                    }
                }
            }
        }
    }

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

/// vec3 مستقیم یا pointer در hw/client
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
    let mut add_slot = |slot: u32| {
        add(slot);
        if let Ok(ptr) = reader.read_u32(slot) {
            if valid_ptr(ptr) {
                add(ptr);
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

#[allow(dead_code)]
fn discover_global_movement(
    reader: &MemoryReader,
    bases: &[u32],
    wait_ms: u64,
) -> Option<PositionDiscovery> {
    let mut snaps: Vec<(u32, (f32, f32, f32))> = Vec::new();
    for &base in bases {
        if let Some(v) = peek_vec3(reader, base, 0).filter(|&(x, y, z)| plausible_for_movement(x, y, z)) {
            snaps.push((base, v));
        }
    }
    if snaps.is_empty() {
        return None;
    }

    thread::sleep(Duration::from_millis(wait_ms));

    let mut best: Option<(f32, PositionDiscovery)> = None;
    for (base, v0) in snaps {
        let Some(v1) = peek_vec3(reader, base, 0) else {
            continue;
        };
        let d = pos_delta_sq(v0, v1);
        if let Some(score) = score_movement_delta(v0, v1, d, 0, 0) {
            if best.as_ref().map(|(bd, _)| score > *bd).unwrap_or(true) {
                best = Some((score, PositionDiscovery { player: base, offset: 0 }));
            }
        }
    }
    best.map(|(_, d)| d)
}

/// entityهای محتمل — hw اول، client stub حذف
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
        if player < 0x0100_0000 || player > 0x7FFF_0000 || player & 3 != 0 {
            return;
        }
        if out.iter().any(|c: &PlayerCandidate| c.player == player && c.hp_offset == hp_offset) {
            return;
        }
        out.push(PlayerCandidate {
            player,
            hp_offset,
            from_hw,
        });
    };

    if hw_base != 0 {
        let mut rvas: Vec<u32> = LP_RVA_HW_FOR_POS.to_vec();
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

/// backward compat برای dump قدیمی
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

/// دو نمونه با فاصله زمانی — فقط جفت‌هایی که با حرکت عوض شده‌اند
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

    thread::sleep(Duration::from_millis(wait_ms));

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

/// هر float جداگانه — مثل CE «changed value» هنگام راه رفتن
fn discover_by_changing_floats(
    reader: &MemoryReader,
    base: u32,
    scan_size: u32,
    wait_ms: u64,
) -> Option<PositionDiscovery> {
    if base < 0x0100_0000 || base > 0x7FFF_0000 {
        return None;
    }
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

    let mut best: Option<(f32, u32)> = None;
    for &off in &changed {
        if !changed.contains(&(off + 4)) || !changed.contains(&(off + 8)) {
            continue;
        }
        let Some(v1) = peek_vec3(reader, base, off) else {
            continue;
        };
        if looks_like_spawn_stub(v1.0, v1.1, v1.2) {
            continue;
        }
        let v0x = snap.iter().find(|(o, _)| *o == off).map(|(_, v)| *v).unwrap_or(0.0);
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

/// velocity عوض شد → origin در 0x18 قبل از آن (entvars)
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
            for off in (0x8..0x800).step_by(4) {
                if let Some(v) = peek_vec3(reader, base, off).filter(|&(x, y, z)| {
                    looks_like_velocity(x, y, z)
                }) {
                    snaps.push((base, off, v));
                }
            }
        }
        for &pev_off in PEV_PTR_OFFS {
            let Ok(pev) = reader.read_u32(cand.player.wrapping_add(pev_off)) else {
                continue;
            };
            if pev < 0x0100_0000 || pev > 0x7FFF_0000 {
                continue;
            }
            for off in (0x8..0x400).step_by(4) {
                if let Some(v) = peek_vec3(reader, pev, off).filter(|&(x, y, z)| {
                    looks_like_velocity(x, y, z)
                }) {
                    snaps.push((pev, off, v));
                }
            }
        }
    }
    if snaps.is_empty() {
        return None;
    }

    thread::sleep(Duration::from_millis(wait_ms));

    let mut best: Option<(f32, PositionDiscovery)> = None;
    for (base, vel_off, v0) in snaps {
        let Some(v1) = peek_vec3(reader, base, vel_off) else {
            continue;
        };
        if pos_delta_sq(v0, v1) <= 0.04 {
            continue;
        }
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

/// dump: اسکن محدوده ماژول — مستقیم + pointer
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
    while rva < end_rva && snaps.len() < max_snaps {
        let slot = module_base.wrapping_add(rva);
        for &base in &[slot, reader.read_u32(slot).unwrap_or(0)] {
            if !valid_ptr(base) {
                continue;
            }
            if let Some(v) = peek_vec3(reader, base, 0).filter(|&(x, y, z)| snap_filter_vec3(x, y, z))
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

    let mut best: Option<(f32, PositionDiscovery)> = None;
    for (base, v0) in snaps {
        let Some(v1) = peek_vec3(reader, base, 0) else {
            continue;
        };
        let d = pos_delta_sq(v0, v1);
        if let Some(score) = score_movement_delta(v0, v1, d, 0, 0) {
            if best.as_ref().map(|(bd, _)| score > *bd).unwrap_or(true) {
                best = Some((score, PositionDiscovery { player: base, offset: 0 }));
            }
        }
    }
    best.map(|(_, d)| d)
}

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
        if let Some(v) = peek_vec3(reader, player, off).filter(|&(x, y, z)| {
            snap_filter_vec3(x, y, z) && !looks_like_spawn_stub(x, y, z)
        }) {
            if !snaps.iter().any(|&(p, o, _)| p == player && o == off) {
                snaps.push((player, off, v));
            }
        }
    };

    for &base in &collect_global_origin_bases(
        reader,
        hw_base,
        client_base,
        config_global_hw_rva,
        config_global_client_rva,
    ) {
        push(base, 0);
    }

    for &cand in candidates {
        for (player, off) in
            collect_entity_hits(reader, cand, config_off, expected_hp, true, true)
        {
            push(player, off);
        }
        if !hp_matches(reader, cand.player, cand.hp_offset, expected_hp) {
            continue;
        }
        for off in (0x8..0x800).step_by(4) {
            if let Some(v) = peek_vec3(reader, cand.player, off).filter(|&(x, y, z)| {
                looks_like_velocity(x, y, z)
            }) {
                let origin_off = off.saturating_sub(0x18);
                push(cand.player, origin_off);
                let _ = v;
            }
        }
        for &pev_off in PEV_PTR_OFFS {
            let Ok(pev) = reader.read_u32(cand.player.wrapping_add(pev_off)) else {
                continue;
            };
            if pev < 0x0100_0000 || pev > 0x7FFF_0000 {
                continue;
            }
            for off in (0x8..0x400).step_by(4) {
                if peek_vec3(reader, pev, off)
                    .is_some_and(|(x, y, z)| looks_like_velocity(x, y, z))
                {
                    push(pev, off.saturating_sub(0x18));
                }
            }
        }
    }

    snaps
}

/// Runtime: hw+rva — اگر slot خودش vec3 world است، pointer chase نمی‌کنیم
/// (floatهای X/Y به‌اشتباه valid_ptr می‌شوند و مختصات پرش می‌زند).
pub fn read_global_world_at_rva(
    reader: &MemoryReader,
    hw_base: u32,
    rva: u32,
) -> Option<(PositionDiscovery, (f32, f32, f32))> {
    if hw_base == 0 {
        return None;
    }
    let slot = hw_base.wrapping_add(rva);
    if let Some(xyz) = read_runtime_world_vec3(reader, slot, 0) {
        return Some((
            PositionDiscovery {
                player: slot,
                offset: 0,
            },
            xyz,
        ));
    }
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

/// خواندن مستقیم از RVAهای config — برای dump/verdict (همه candidateها)
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
        consider(slot, 0, 0);
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
    // client global برای world استفاده نمی‌شود — همان view_client_rva است
    if let Some(rva) = config_global_hw_rva {
        if let Some(hit) = try_rva(hw_base, rva, 2000) {
            best = Some(hit);
        }
    }
    let _ = config_global_client_rva;
    best.map(|(_, d)| d)
}

/// global → pev → entity → velocity — یک sleep مشترک
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

/// dump: نمایش vec3 در آدرس‌های مهم (بدون نیاز به حرکت)
pub fn print_position_diagnostics(
    reader: &MemoryReader,
    hw_base: u32,
    client_base: u32,
    entity_hw: u32,
    config_global_hw_rva: Option<u32>,
) {
    println!("  diag (مقادیر فعلی — بدون حرکت):");
    for &off in &[0x8u32, 0x34, 0x128, 0x134, 0x334] {
        if let Some((x, y, z)) = peek_vec3(reader, entity_hw, off) {
            println!("    entity+{off:#x}  X={x:.1} Y={y:.1} Z={z:.1}");
        }
    }
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
            if cand.from_hw {
                s += 2000;
            }
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

/// dump: movement + fallback static (فقط برای پیشنهاد)
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
    fallback_static(
        reader,
        candidates,
        config_off,
        expected_hp,
        health_direct,
        client_hp_off,
    )
}

/// alias قدیمی
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
