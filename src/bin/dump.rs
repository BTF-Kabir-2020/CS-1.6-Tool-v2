//! یک‌بار attach به hl.exe و validate کردن همه chain/offsetها.

use std::path::PathBuf;

use clap::Parser;
use cs16_tool_v2::config::{parse_hex_u32, parse_offsets, AppConfig};
use cs16_tool_v2::win::memory::{resolve_chain, MemoryReader};
use cs16_tool_v2::win::process::{engine_base, ProcessHandle};

#[derive(Parser)]
#[command(name = "cs16-dump", about = "Dump و validate offset/chain از hl.exe")]
struct Cli {
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,
}

/// RVAهای شناخته‌شده local player از BLASTHACK / UC / CE
const LOCAL_PLAYER_CANDIDATES: &[(&str, &str)] = &[
    ("hw.dll+0x32ABF4", "0x32ABF4"),
    ("hw.dll+0x13FDF4", "0x13FDF4"),
    ("client.dll+0x4B530", "0x4B530"),
    ("client.dll+0x17EF28", "0x17EF28"),
];

/// offsetهای health/armor/money شناخته‌شده
const HEALTH_CANDIDATES: &[(&str, u32)] = &[
    ("CE player+0x14", 0x14),
    ("m_dwHealth (BLASTHACK)", 0xB74),
    ("m_iHealth alt", 0xFC),
    ("m_iClientHealth", 0x59C),
];
const ARMOR_CANDIDATES: &[(&str, u32)] =
    &[("m_dwArmor (BLASTHACK)", 0x10C), ("m_iArmor alt", 0x100)];
const MONEY_CANDIDATES: &[(&str, u32)] =
    &[("m_dwMoney (BLASTHACK)", 0xE4), ("m_iAccount alt", 0x94)];

fn main() {
    if let Err(e) = run() {
        eprintln!("\nخطا: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let config = AppConfig::load(&cli.config)?;

    println!("═══════════════════════════════════════════════════");
    println!(" CS 1.6 Memory Dump — PID attach + chain validate");
    println!("═══════════════════════════════════════════════════\n");

    let process = ProcessHandle::attach(&config.process.name)?;
    let pid = process.pid();
    println!("✓ متصل به {} (PID={pid})\n", config.process.name);

    // ── ماژول‌ها ──
    let hw_base = engine_base(&process, &config.modules).unwrap_or(0);
    let client_base = process.module_base(&config.modules.client).unwrap_or(0);
    let sw_base = if !config.modules.sw.is_empty() {
        process.module_base(&config.modules.sw).unwrap_or(0)
    } else {
        0
    };

    println!("── Module Bases ──");
    print_mod("hw.dll", hw_base);
    print_mod("client.dll", client_base);
    print_mod("sw.dll", sw_base);
    println!();

    let reader = MemoryReader::new(&process);

    // ── Chains از config ──
    println!("── Pointer Chains (config.toml) ──");
    test_chain(
        &process,
        hw_base,
        "money_hw",
        &config.chains.money_hw.base_rva,
        &config.chains.money_hw.offsets,
        &reader,
        "i32",
    );
    test_chain(
        &process,
        hw_base,
        "reserve",
        &config.chains.reserve.base_rva,
        &config.chains.reserve.offsets,
        &reader,
        "i32",
    );
    for (i, chain) in config.chains.clip.iter().enumerate() {
        test_chain(
            &process,
            hw_base,
            &format!("clip[{i}]"),
            &chain.base_rva,
            &chain.offsets,
            &reader,
            "i32",
        );
    }

    let client_money_rva = &config.chains.money_client_fallback.direct_rva;
    if let Ok(rva) = parse_hex_u32(client_money_rva) {
        if client_base != 0 {
            let addr = client_base.wrapping_add(rva);
            match reader.read_i32(addr) {
                Ok(v) => {
                    println!("  ✓ money_client_fallback @ {addr:#010x} (client+{rva:#x}) = {v}")
                }
                Err(_) => println!("  ✗ money_client_fallback @ {addr:#010x} — read failed"),
            }
        }
    }
    println!();

    // ── Local Player candidates ──
    println!("── Local Player Pointer ──");
    let mut best_player = 0u32;
    let mut best_label = String::new();

    for (label, rva_str) in LOCAL_PLAYER_CANDIDATES {
        let Ok(rva) = parse_hex_u32(rva_str) else {
            continue;
        };
        let (base, mod_name) = if label.starts_with("client") {
            (client_base, "client")
        } else {
            (hw_base, "hw")
        };
        if base == 0 {
            println!("  ✗ {label} — {mod_name}.dll not loaded");
            continue;
        }
        let ptr_addr = base.wrapping_add(rva);
        match reader.read_u32(ptr_addr) {
            Ok(player) if player != 0 && player > 0x10000 => {
                println!("  ✓ {label} → ptr@{ptr_addr:#010x} → player={player:#010x}");
                if best_player == 0 {
                    best_player = player;
                    best_label = label.to_string();
                }
            }
            Ok(v) => println!("  ✗ {label} → ptr@{ptr_addr:#010x} = {v:#x} (null/invalid)"),
            Err(_) => println!("  ✗ {label} → ptr@{ptr_addr:#010x} — read failed"),
        }
    }

    // config entity RVA (ماژول از config.toml)
    let on_client_cfg = config
        .entity
        .local_player_module
        .eq_ignore_ascii_case("client");
    let cfg_lp_module_base = if on_client_cfg { client_base } else { hw_base };
    let cfg_lp_tag = if on_client_cfg { "client" } else { "hw" };
    let mut cfg_entity_player = 0u32;

    if let Ok(rva) = parse_hex_u32(&config.entity.local_player_rva) {
        if cfg_lp_module_base != 0 {
            let ptr_addr = cfg_lp_module_base.wrapping_add(rva);
            match reader.read_u32(ptr_addr) {
                Ok(player) if player != 0 && player > 0x10000 => {
                    cfg_entity_player = player;
                    println!(
                        "  ✓ config entity local_player @ {cfg_lp_tag}+{rva:#x} → player={player:#010x}"
                    );
                    if best_player == 0 {
                        best_player = player;
                        best_label = format!("config ({cfg_lp_tag})");
                    }
                }
                Ok(v) => println!(
                    "  ✗ config entity local_player @ {cfg_lp_tag}+{rva:#x} = {v:#x} (null/invalid)"
                ),
                Err(_) => println!("  ✗ config entity local_player — read failed"),
            }
        }
    }

    // health_direct − hp_off (CE player_health_from_base)
    let cfg_hp_off = parse_hex_u32(&config.entity.health_offset).unwrap_or(0x14);
    if let Some(rva) = config.entity.health_direct_rva.as_deref() {
        if let Ok(off) = parse_hex_u32(rva) {
            if client_base != 0 {
                let hd = client_base.wrapping_add(off);
                let derived = hd.wrapping_sub(cfg_hp_off);
                if derived != 0 && derived > 0x10000 {
                    println!(
                        "  ✓ health_direct − {cfg_hp_off:#x} → player={derived:#010x} (CE entity)"
                    );
                    if cfg_entity_player == 0 {
                        cfg_entity_player = derived;
                    }
                    if best_player == 0 {
                        best_player = derived;
                        best_label = "health_direct".into();
                    }
                }
            }
        }
    }
    println!();

    if best_player == 0 {
        println!("⚠ Local Player پیدا نشد — وارد match شو و دوباره dump بزن.\n");
        return Ok(());
    }

    // entity اصلی: config/health_direct اولویت دارد نه اولین hw candidate
    let entity_player = if cfg_entity_player != 0 {
        cfg_entity_player
    } else {
        best_player
    };
    let entity_label = if cfg_entity_player != 0 {
        if cfg_entity_player == best_player {
            best_label.clone()
        } else {
            format!("config ({cfg_lp_tag})")
        }
    } else {
        best_label.clone()
    };

    println!("── Entity Fields (player={entity_player:#010x} via {entity_label}) ──");

    for (name, off) in HEALTH_CANDIDATES {
        let addr = entity_player.wrapping_add(*off);
        let i = reader.read_i32(addr).unwrap_or(-999);
        let f = reader.read_f32(addr).unwrap_or(f32::NAN);
        let mark = if (1..=100).contains(&i) { "★" } else { " " };
        println!("  {mark} {name} +{off:#x} @ {addr:#010x}  int={i}  float={f:.2}");
    }
    for (name, off) in ARMOR_CANDIDATES {
        let addr = entity_player.wrapping_add(*off);
        let i = reader.read_i32(addr).unwrap_or(-999);
        let mark = if (0..=100).contains(&i) { "★" } else { " " };
        println!("  {mark} {name} +{off:#x} @ {addr:#010x}  int={i}");
    }
    for (name, off) in MONEY_CANDIDATES {
        let addr = entity_player.wrapping_add(*off);
        let i = reader.read_i32(addr).unwrap_or(-999);
        let mark = if i >= 0 && i <= 16000 { "★" } else { " " };
        println!("  {mark} {name} +{off:#x} @ {addr:#010x}  int={i}");
    }
    println!();

    // ── Position (vec3 origin) ──
    println!("── Position (player vec3 X/Y/Z) ──");
    let wait_ms = 800u64;
    let cfg_pos_off = parse_hex_u32(&config.entity.position_offset).unwrap_or(0x8);
    let pos_entity_hw_rva = config
        .entity
        .position_entity_hw_rva
        .as_deref()
        .and_then(|s| parse_hex_u32(s).ok());
    let pos_global_hw_rva = config
        .entity
        .position_global_hw_rva
        .as_deref()
        .and_then(|s| parse_hex_u32(s).ok());
    let pos_global_client_rva = config
        .entity
        .position_global_client_rva
        .as_deref()
        .and_then(|s| parse_hex_u32(s).ok());
    let view_client_rva = config
        .entity
        .view_client_rva
        .as_deref()
        .and_then(|s| parse_hex_u32(s).ok());
    let health_direct_addr = config
        .entity
        .health_direct_rva
        .as_deref()
        .and_then(|s| parse_hex_u32(s).ok())
        .map(|rva| client_base.wrapping_add(rva))
        .unwrap_or(0);
    let expected_hp = if health_direct_addr != 0 {
        reader.read_i32(health_direct_addr).ok()
    } else {
        None
    };
    let pos_candidates = cs16_tool_v2::game::collect_position_candidates(
        &reader,
        hw_base,
        client_base,
        on_client_cfg,
        parse_hex_u32(&config.entity.local_player_rva).unwrap_or(0x4B530),
        health_direct_addr,
        cfg_hp_off,
        pos_entity_hw_rva,
    );
    let global_bases = cs16_tool_v2::game::collect_global_origin_bases(
        &reader,
        hw_base,
        client_base,
        pos_global_hw_rva,
        pos_global_client_rva,
    );
    println!(
        "  candidates: {} entity + {} global RVA (hw اول، client stub حذف)",
        pos_candidates.len(),
        global_bases.len()
    );
    for c in &pos_candidates {
        let tag = if c.from_hw { "hw" } else { "client" };
        println!("    {tag} player={:#010x} HP+{:#x}", c.player, c.hp_offset);
    }
    if let Some(rva) = view_client_rva {
        if let Some((h, mx, my)) =
            cs16_tool_v2::game::read_view_aux(&reader, client_base, Some(rva))
        {
            println!("  view client+{rva:#x} → H={h:.1} Mx={mx:.1} My={my:.1}  (NOT map XYZ)");
        }
    }

    cs16_tool_v2::game::prepare_walk_test(2);
    if let Some(found) = cs16_tool_v2::game::discover_position_live(
        &reader,
        hw_base,
        client_base,
        &pos_candidates,
        cfg_pos_off,
        expected_hp,
        wait_ms,
        pos_global_hw_rva,
        pos_global_client_rva,
    ) {
        if let Some((x, y, z)) = cs16_tool_v2::game::peek_vec3(&reader, found.player, found.offset)
        {
            let kind = if cs16_tool_v2::game::looks_like_view_aux(x, y, z) {
                "view/camera (NOT map)"
            } else if found.offset == 0 {
                "global world"
            } else {
                "entity world"
            };
            println!(
                "  → MOVEMENT OK ({kind}) base={:#x} offset=\"{:#x}\"  X={x:.1} Y={y:.1} Z={z:.1}",
                found.player, found.offset
            );
        }
    } else {
        println!("  ✗ movement test: هیچ offset با حرکت عوض نشد");
        let hw_entity = pos_candidates
            .iter()
            .find(|c| c.from_hw && c.hp_offset == 0x59C)
            .map(|c| c.player)
            .unwrap_or(0);
        if hw_entity != 0 {
            cs16_tool_v2::game::print_position_diagnostics(
                &reader,
                hw_base,
                client_base,
                hw_entity,
                pos_entity_hw_rva,
            );
        }
        let hw_scan_end = process
            .module_size(&config.modules.hw)
            .unwrap_or(0x0020_0000)
            .min(0x0030_0000);
        if hw_base != 0 {
            println!("  scanning hw.dll 0x80000..{hw_scan_end:#x} (step 4, ptr+direct)...");
            if let Some(found) = cs16_tool_v2::game::scan_module_globals_for_movement(
                &reader,
                hw_base,
                0x0008_0000,
                hw_scan_end,
                4,
                wait_ms,
                2048,
            ) {
                let rva = found.player.wrapping_sub(hw_base);
                if let Some((x, y, z)) = cs16_tool_v2::game::peek_vec3(&reader, found.player, 0) {
                    println!(
                        "  → SCAN OK hw+{rva:#x}  X={x:.1} Y={y:.1} Z={z:.1}  → position_global_hw_rva = \"{rva:#x}\""
                    );
                }
            }
        }
        let client_scan_end = process
            .module_size(&config.modules.client)
            .unwrap_or(0x0020_0000)
            .min(0x0030_0000);
        if client_base != 0 {
            println!("  scanning client.dll 0x100000..{client_scan_end:#x} (step 4)...");
            if let Some(found) = cs16_tool_v2::game::scan_module_globals_for_movement(
                &reader,
                client_base,
                0x0010_0000,
                client_scan_end,
                4,
                wait_ms,
                2048,
            ) {
                let rva = found.player.wrapping_sub(client_base);
                if let Some((x, y, z)) = cs16_tool_v2::game::peek_vec3(&reader, found.player, 0) {
                    println!(
                        "  → SCAN OK client+{rva:#x}  X={x:.1} Y={y:.1} Z={z:.1}  → position_global_client_rva = \"{rva:#x}\""
                    );
                }
            }
        }
        println!("  (fallback ثابت غیرفعال — فقط offsetهای متحرک معتبرند)");
    }
    println!();

    // ── Scan: int 1..100 در محدوده entity ──
    println!("── Health Scan (int 1..100, player+0..0x2000) ──");
    let mut hits = Vec::new();
    for off in (0..0x2000).step_by(4) {
        let addr = entity_player.wrapping_add(off);
        if let Ok(v) = reader.read_i32(addr) {
            if (1..=100).contains(&v) {
                hits.push((off, v));
            }
        }
    }
    if hits.is_empty() {
        println!("  (هیچ int 1..100 پیدا نشد — شاید مرده‌ای یا HP=0)");
    } else {
        for (off, v) in hits.iter().take(20) {
            let known = known_offset_label(*off);
            println!("  +{off:#06x} = {v}{known}");
        }
        if hits.len() > 20 {
            println!("  ... و {} مورد دیگر", hits.len() - 20);
        }
    }
    println!();

    // ── Smart scan: entity با HP + Money + Armor معتبر ──
    if hw_base != 0 {
        println!("── Smart Scan (HP 1..100 + money 0..16000 + armor 0..100) ──");
        let mut smart = Vec::new();
        for rva in (0x100000..0xF00000).step_by(4) {
            let ptr_addr = hw_base.wrapping_add(rva);
            let Ok(player) = reader.read_u32(ptr_addr) else {
                continue;
            };
            if player < 0x01000000 || player > 0x7FFF0000 || player & 3 != 0 {
                continue;
            }
            let Ok(hp) = reader.read_i32(player.wrapping_add(0xB74)) else {
                continue;
            };
            let Ok(money) = reader.read_i32(player.wrapping_add(0xE4)) else {
                continue;
            };
            let Ok(armor) = reader.read_i32(player.wrapping_add(0x10C)) else {
                continue;
            };
            if (1..=100).contains(&hp) && (0..=16000).contains(&money) && (0..=100).contains(&armor)
            {
                smart.push((rva, player, hp, armor, money));
            }
        }
        if smart.is_empty() {
            println!("  (local player با offsetهای BLASTHACK پیدا نشد)");
            println!("  → احتمالاً در منو/مرده‌ای یا build متفاوت است");
        } else {
            for (rva, player, hp, armor, money) in smart.iter().take(5) {
                println!(
                    "  ★★★ LOCAL PLAYER? hw+{rva:#07x} → {player:#010x}  HP={hp} armor={armor} money={money}"
                );
            }
        }
        println!();
    }

    // ── Brute scan: hw.dll static RVAs → pointer → HP ──
    if hw_base != 0 {
        println!("── Auto-Scan local_player (hw.dll RVAs → entity → HP 1..100) ──");
        let mut found = Vec::new();
        // محدوده .data معمول hw.dll
        for rva in (0x100000..0xF00000).step_by(4) {
            let ptr_addr = hw_base.wrapping_add(rva);
            let Ok(player) = reader.read_u32(ptr_addr) else {
                continue;
            };
            if player < 0x01000000 || player > 0x7FFF0000 || player & 3 != 0 {
                continue;
            }
            for &hp_off in &[0xB74u32, 0xFC, 0x100, 0x334, 0x59C] {
                let Ok(hp) = reader.read_i32(player.wrapping_add(hp_off)) else {
                    continue;
                };
                if (1..=100).contains(&hp) {
                    found.push((rva, player, hp_off, hp));
                }
            }
        }
        found.sort_by_key(|x| x.0);
        found.dedup_by_key(|x| (x.1, x.2));
        if found.is_empty() {
            println!("  (هیچ RVA با HP معتبر پیدا نشد — در match زنده باش)");
        } else {
            // اول HP=100 (بازیکن زنده) را نشان بده
            let full: Vec<_> = found.iter().filter(|(_, _, _, hp)| *hp == 100).collect();
            if !full.is_empty() {
                println!("  ── HP=100 (local player محتمل):");
                for (rva, player, hp_off, hp) in full.iter().take(10) {
                    let armor = reader.read_i32(player.wrapping_add(0x10C)).unwrap_or(-1);
                    let money = reader.read_i32(player.wrapping_add(0xE4)).unwrap_or(-1);
                    println!(
                        "  ★★ hw+{rva:#07x} → {player:#010x}  HP+{hp_off:#x}={hp}  armor={armor}  money={money}"
                    );
                }
            }
            println!("  ── همه candidateها:");
            for (rva, player, hp_off, hp) in found.iter().take(15) {
                println!("  ★ hw+{rva:#07x} → player={player:#010x}  HP+{hp_off:#x}={hp}");
            }
        }
        println!();
    }

    // ── Brute scan: client.dll ──
    if client_base != 0 {
        println!("── Auto-Scan local_player (client.dll) ──");
        let mut found = Vec::new();
        for rva in (0x100000..0x200000).step_by(4) {
            let ptr_addr = client_base.wrapping_add(rva);
            let Ok(player) = reader.read_u32(ptr_addr) else {
                continue;
            };
            if player < 0x01000000 || player > 0x7FFF0000 || player & 3 != 0 {
                continue;
            }
            for &hp_off in &[0x14u32, 0xB74, 0xFC, 0x100] {
                let Ok(hp) = reader.read_i32(player.wrapping_add(hp_off)) else {
                    continue;
                };
                if (1..=100).contains(&hp) {
                    found.push((rva, player, hp_off, hp));
                }
            }
        }
        if found.is_empty() {
            println!("  (nothing)");
        } else {
            for (rva, player, hp_off, hp) in found.iter().take(10) {
                println!("  ★ client+{rva:#07x} → player={player:#010x}  HP+{hp_off:#x}={hp}");
            }
        }
        println!();
    }

    // ── Scan clip/reserve chain bases: آیا offsetهای ammo درست‌اند؟ ──
    println!("── Ammo Chains (magazine vs reserve) ──");
    let reserve_idx = config.chains.reserve_clip_index;
    let mut clip_vals: Vec<(usize, u32, i32, bool)> = Vec::new();

    for (i, chain) in config.chains.clip.iter().enumerate() {
        let role = if i == reserve_idx {
            "reserve"
        } else {
            "magazine"
        };
        if let (Ok(rva), Ok(offs)) = (
            parse_hex_u32(&chain.base_rva),
            parse_offsets(&chain.offsets),
        ) {
            match resolve_chain(&process, hw_base.wrapping_add(rva), &offs) {
                Ok(addr) => {
                    let v = reader.read_i32(addr).unwrap_or(-1);
                    let ok = v > 0 && v < config.clip_detection.max_value;
                    let mark = if ok { "✓" } else { "✗" };
                    println!("  {mark} clip[{i}] ({role}) @ {addr:#010x} (hw+{rva:#x}) = {v}");
                    clip_vals.push((i, addr, v, ok));
                }
                Err(e) => println!("  ✗ clip[{i}] ({role}) — chain broken: {e}"),
            }
        }
    }

    let (reserve_rva, reserve_offs) = (
        parse_hex_u32(&config.chains.reserve.base_rva),
        parse_offsets(&config.chains.reserve.offsets),
    );
    let mut reserve_primary_ok = false;
    if let (Ok(rva), Ok(offs)) = (reserve_rva, reserve_offs) {
        match resolve_chain(&process, hw_base.wrapping_add(rva), &offs) {
            Ok(addr) => {
                let v = reader.read_i32(addr).unwrap_or(-1);
                reserve_primary_ok = v > 0 && v < config.clip_detection.max_value;
                let mark = if reserve_primary_ok { "✓" } else { "✗" };
                println!("  {mark} reserve (primary) @ {addr:#010x} (hw+{rva:#x}) = {v}");
            }
            Err(e) => {
                print!("  ✗ reserve (primary) — chain broken: {e} — steps:");
                let mut addr = hw_base.wrapping_add(rva);
                for (step, &off) in offs.iter().enumerate() {
                    match reader.read_u32(addr) {
                        Ok(n) => {
                            print!(" [{step}] {addr:#x}→{n:#x}+{off:x}");
                            addr = n.wrapping_add(off);
                        }
                        Err(_) => {
                            print!(" [{step}] {addr:#x} FAIL");
                            break;
                        }
                    }
                }
                println!();
            }
        }
    }

    let reserve_fb = clip_vals
        .iter()
        .find(|(i, _, v, ok)| *i == reserve_idx && *ok && *v > 0);
    if !reserve_primary_ok {
        if let Some((i, addr, v, _)) = reserve_fb {
            println!(
                "  → reserve fallback: clip[{i}] @ {addr:#x} = {v} (reserve_clip_index={reserve_idx})"
            );
        } else {
            println!("  ✗ reserve fallback clip[{reserve_idx}] هم کار نکرد");
        }
    }

    let magazine: Vec<_> = clip_vals
        .iter()
        .filter(|(i, _, _, ok)| *i != reserve_idx && *ok)
        .collect();
    if magazine.is_empty() {
        println!("  ✗ هیچ chain magazine معتبر نیست — در match با اسلحه تست کن");
    } else {
        let best = magazine.iter().max_by_key(|(_, _, v, _)| *v).unwrap();
        println!(
            "  → magazine pick: clip[{}] @ {:#x} = {} (بیشترین مقدار بین magazine chains)",
            best.0, best.1, best.2
        );
    }
    println!();

    // ── جمع‌بندی ──
    println!("── Verdict ──");
    let on_client = config
        .entity
        .local_player_module
        .eq_ignore_ascii_case("client");
    let lp_module_base = if on_client { client_base } else { hw_base };
    let lp_tag = if on_client { "client" } else { "hw" };
    if lp_module_base != 0 {
        if let Some(found) = cs16_tool_v2::game::discover_local_player(
            &reader,
            lp_module_base,
            parse_hex_u32(&config.entity.local_player_rva).unwrap_or(0x32ABF4),
            parse_hex_u32(&config.entity.health_offset).unwrap_or(0xB74),
            on_client,
        ) {
            let hp = reader
                .read_i32(found.player.wrapping_add(found.health_offset))
                .unwrap_or(-1);
            println!("  AUTO-DISCOVER OK ({lp_tag}):");
            println!("    local_player_rva = \"{:#x}\"", found.rva);
            println!("    health_offset    = \"{:#x}\"", found.health_offset);
            println!("    player           = {:#x}", found.player);
            println!("    HP now           = {hp}");
        } else {
            println!("  AUTO-DISCOVER: failed (enter match alive)");
        }
    }
    if let Some(rva) = config.entity.health_direct_rva.as_deref() {
        if let Ok(off) = parse_hex_u32(rva) {
            if client_base != 0 {
                let addr = client_base.wrapping_add(off);
                let hp = reader.read_i32(addr).unwrap_or(-1);
                let mark = if (1..=100).contains(&hp) {
                    "✓"
                } else {
                    "✗"
                };
                println!("  {mark} health_direct client+{off:#x} @ {addr:#010x} = {hp}");
            }
        }
    }
    if let Some(rva) = config.entity.armor_direct_rva.as_deref() {
        if let Ok(off) = parse_hex_u32(rva) {
            if client_base != 0 {
                let addr = client_base.wrapping_add(off);
                let armor = reader.read_i32(addr).unwrap_or(-1);
                let mark = if (0..=100).contains(&armor) {
                    "✓"
                } else {
                    "✗"
                };
                println!("  {mark} armor_direct client+{off:#x} @ {addr:#010x} = {armor}");
            }
        }
    }
    let cfg_hp = parse_hex_u32(&config.entity.health_offset).unwrap_or(0);
    let cfg_hp_val = reader
        .read_i32(entity_player.wrapping_add(cfg_hp))
        .unwrap_or(-1);
    let cfg_lp_rva = parse_hex_u32(&config.entity.local_player_rva).unwrap_or(0);
    let cfg_lp = if cfg_lp_module_base != 0 {
        reader
            .read_u32(cfg_lp_module_base.wrapping_add(cfg_lp_rva))
            .unwrap_or(0)
    } else {
        0
    };

    if cfg_lp == 0 {
        println!("  ✗ local_player_rva={cfg_lp_rva:#x} ({cfg_lp_tag}) → NULL (offset اشتباه یا در منو هستی)");
    } else if cfg_lp != entity_player {
        println!(
            "  ⚠ local_player_rva={cfg_lp_rva:#x} ({cfg_lp_tag}) → {cfg_lp:#x} (entity={entity_player:#x})"
        );
    } else {
        println!("  ✓ local_player_rva={cfg_lp_rva:#x} ({cfg_lp_tag}) → OK");
    }

    let cfg_pos_off = parse_hex_u32(&config.entity.position_offset).unwrap_or(0x8);
    let pos_entity_hw_rva = config
        .entity
        .position_entity_hw_rva
        .as_deref()
        .and_then(|s| parse_hex_u32(s).ok());
    let pos_global_hw_rva = config
        .entity
        .position_global_hw_rva
        .as_deref()
        .and_then(|s| parse_hex_u32(s).ok());
    let view_client_rva = config
        .entity
        .view_client_rva
        .as_deref()
        .and_then(|s| parse_hex_u32(s).ok());
    if config.features.position_enabled {
        let pos_cands = cs16_tool_v2::game::collect_position_candidates(
            &reader,
            hw_base,
            client_base,
            on_client_cfg,
            cfg_lp_rva,
            health_direct_addr,
            cfg_hp,
            pos_entity_hw_rva,
        );
        let exp_hp = if health_direct_addr != 0 {
            reader.read_i32(health_direct_addr).ok()
        } else {
            None
        };
        let pos_found =
            cs16_tool_v2::game::read_hw_entity_world_origin(&reader, hw_base, pos_entity_hw_rva)
                .or_else(|| {
                    cs16_tool_v2::game::read_configured_global_position(
                        &reader,
                        hw_base,
                        client_base,
                        pos_global_hw_rva,
                        None,
                    )
                })
                .or_else(|| {
                    cs16_tool_v2::game::discover_position_live(
                        &reader,
                        hw_base,
                        client_base,
                        &pos_cands,
                        cfg_pos_off,
                        exp_hp,
                        400,
                        pos_global_hw_rva,
                        None,
                    )
                });
        if let Some(rva) = view_client_rva {
            if let Some((h, mx, my)) =
                cs16_tool_v2::game::read_view_aux(&reader, client_base, Some(rva))
            {
                println!(
                    "  ✓ view_aux client+{rva:#x} → H={h:.0} Mx={mx:.0} My={my:.0}  (دوربین/ماوس)"
                );
            }
        }
        if let Some(found) = pos_found {
            if let Some((x, y, z)) =
                cs16_tool_v2::game::peek_vec3(&reader, found.player, found.offset)
            {
                if cs16_tool_v2::game::looks_like_world_origin(x, y, z) {
                    println!(
                        "  ✓ world XYZ offset=\"{:#x}\" @ {:#x} → X={x:.0} Y={y:.0} Z={z:.0}",
                        found.offset, found.player
                    );
                } else {
                    println!(
                        "  ⚠ movement hit @ {:#x} ولی world نیست — X={x:.0} Y={y:.0} Z={z:.0}",
                        found.player
                    );
                }
            }
        } else {
            println!(
                "  ✗ world XYZ: hw entity + movement test — در match با W/strafe دوباره dump بزن"
            );
        }
    }

    if (1..=100).contains(&cfg_hp_val) {
        println!("  ✓ health_offset={cfg_hp:#x} → HP={cfg_hp_val} (int) — type=int بگذار");
    } else {
        println!("  ✗ health_offset={cfg_hp:#x} → int={cfg_hp_val} — offset/type اشتباه");
        if let Some((off, v)) = hits.first() {
            println!("  → پیشنهاد: health_offset = \"{off:#x}\" (الان HP={v})");
        }
    }

    if !config.entity.enabled {
        println!("  ⚠ entity.enabled = false در config — HP write غیرفعال است");
    }
    if !config.features.hp_enabled {
        println!("  ⚠ hp_enabled = false — HP در overlay نمایش داده نمی‌شود");
    }
    if config.entity.health_type.to_lowercase() == "float" {
        println!("  ⚠ health_type = float — m_dwHealth باید int باشد");
    }

    if reserve_idx >= config.chains.clip.len() {
        println!(
            "  ✗ reserve_clip_index={reserve_idx} خارج از محدوده clip (len={})",
            config.chains.clip.len()
        );
    } else if let Some((_, addr, v, ok)) = clip_vals.iter().find(|(i, _, _, _)| *i == reserve_idx) {
        if *ok {
            println!(
                "  ✓ reserve_clip_index={reserve_idx} → clip[{reserve_idx}] @ {addr:#x} = {v}"
            );
        } else {
            println!(
                "  ✗ reserve_clip_index={reserve_idx} → clip[{reserve_idx}] مقدار نامعتبر ({v})"
            );
        }
    }

    if magazine.is_empty() {
        println!(
            "  ✗ ammo magazine: هیچ chain clip[0..{}] کار نکرد",
            reserve_idx
        );
    } else {
        let best = magazine.iter().max_by_key(|(_, _, v, _)| *v).unwrap();
        println!(
            "  ✓ ammo magazine: clip[{}] = {} (engine همین را می‌نویسد)",
            best.0, best.2
        );
    }
    if reserve_primary_ok || reserve_fb.is_some() {
        println!("  ✓ ammo reserve: OK (primary یا clip[{reserve_idx}] fallback)");
    } else {
        println!("  ✗ ammo reserve: شکسته — infinite reserve کار نمی‌کند");
    }

    println!();
    Ok(())
}

fn print_mod(name: &str, base: u32) {
    if base != 0 {
        println!("  ✓ {name} = {base:#010x}");
    } else {
        println!("  ✗ {name} — not loaded");
    }
}

fn test_chain(
    process: &ProcessHandle,
    hw_base: u32,
    name: &str,
    base_rva: &str,
    offsets: &[String],
    reader: &MemoryReader,
    ty: &str,
) {
    if hw_base == 0 {
        println!("  ✗ {name} — hw.dll not loaded");
        return;
    }
    let Ok(rva) = parse_hex_u32(base_rva) else {
        println!("  ✗ {name} — bad base_rva");
        return;
    };
    let Ok(offs) = parse_offsets(offsets) else {
        println!("  ✗ {name} — bad offsets");
        return;
    };
    let base = hw_base.wrapping_add(rva);
    match resolve_chain(process, base, &offs) {
        Ok(addr) => {
            let val = if ty == "i32" {
                reader
                    .read_i32(addr)
                    .map(|v| format!("{v}"))
                    .unwrap_or_else(|_| "?".into())
            } else {
                "?".into()
            };
            println!("  ✓ {name} @ {addr:#010x} (hw+{rva:#x}) = {val}");
        }
        Err(e) => println!("  ✗ {name} — chain broken: {e}"),
    }
}

fn known_offset_label(off: u32) -> String {
    match off {
        0xB74 => "  ← m_dwHealth (BLASTHACK)".into(),
        0xFC => "  ← m_iHealth alt".into(),
        0x10C => "  ← m_dwArmor".into(),
        0xE4 => "  ← m_dwMoney".into(),
        0x15C => "  ← m_dwLifeState".into(),
        _ => String::new(),
    }
}
