//! پیدا کردن Local Player — buildهای مختلف CS 1.6 (Steam / non-Steam).

use crate::win::memory::MemoryReader;

const LP_RVA_HW: &[u32] = &[
    0x0010FC80, // non-Steam dump: HP=100 + world coords
    0x0012D6F4, // E:\games\gamecs\ non-Steam (dump verified)
    0x0032ABF4, // BLASTHACK / Steam 8684
    0x0013FDF4,
    0x00827484,
    0x0051FA44,
    0x007028FC,
];

const LP_RVA_CLIENT: &[u32] = &[
    0x0004B530, // CE player_health_from_base
    0x0011D438, // CE heal pointerscan base
];

const HP_OFFSET_CANDIDATES: &[u32] = &[0x14, 0xB74, 0xFC, 0x100];

pub struct LocalPlayerDiscovery {
    pub rva: u32,
    pub player: u32,
    pub health_offset: u32,
}

fn read_hp(reader: &MemoryReader, player: u32, hp_off: u32) -> Option<i32> {
    if player < 0x0100_0000 || player > 0x7FFF_0000 || player & 3 != 0 {
        return None;
    }
    reader
        .read_i32(player.wrapping_add(hp_off))
        .ok()
        .filter(|&hp| (0..=100).contains(&hp))
}

fn score_candidate(
    reader: &MemoryReader,
    player: u32,
    hp_off: u32,
    config_hp_off: u32,
    from_known_rva: bool,
) -> i32 {
    let Some(hp) = read_hp(reader, player, hp_off) else {
        return i32::MIN;
    };
    let mut score = 0;
    if from_known_rva {
        score += 1000;
    }
    if hp_off == config_hp_off {
        score += 500;
    } else if hp_off == 0xB74 {
        score += 300;
    } else if hp_off == 0xFC {
        score += 200;
    }
    if let Ok(armor) = reader.read_i32(player.wrapping_add(0x10C)) {
        if (0..=100).contains(&armor) {
            score += 100;
        }
    }
    // HP نزدیک 100 = بازیکن زنده محتمل‌تر
    score += hp;
    score
}

/// اول config RVA، بعد لیست شناخته‌شده، بعد scan محدود ماژول
pub fn discover(
    reader: &MemoryReader,
    module_base: u32,
    config_rva: u32,
    config_hp_off: u32,
    on_client: bool,
) -> Option<LocalPlayerDiscovery> {
    if module_base == 0 {
        return None;
    }

    let known = if on_client { LP_RVA_CLIENT } else { LP_RVA_HW };
    let (scan_lo, scan_hi) = if on_client {
        (0x100_000, 0x200_000)
    } else {
        (0x100_000, 0x600_000)
    };

    let mut try_rvas = vec![config_rva];
    for &rva in known {
        if rva != config_rva {
            try_rvas.push(rva);
        }
    }

    let mut hp_offs = vec![config_hp_off];
    for &off in HP_OFFSET_CANDIDATES {
        if off != config_hp_off {
            hp_offs.push(off);
        }
    }

    let mut best: Option<(i32, LocalPlayerDiscovery)> = None;

    let mut consider = |rva: u32, player: u32, hp_off: u32, known: bool| {
        let s = score_candidate(reader, player, hp_off, config_hp_off, known);
        if s == i32::MIN {
            return;
        }
        if best.as_ref().map(|(bs, _)| s > *bs).unwrap_or(true) {
            best = Some((
                s,
                LocalPlayerDiscovery {
                    rva,
                    player,
                    health_offset: hp_off,
                },
            ));
        }
    };

    for &rva in &try_rvas {
        let ptr_addr = module_base.wrapping_add(rva);
        let Ok(player) = reader.read_u32(ptr_addr) else {
            continue;
        };
        for &hp_off in &hp_offs {
            consider(rva, player, hp_off, true);
        }
    }

    for rva in (scan_lo..scan_hi).step_by(4) {
        if try_rvas.contains(&rva) {
            continue;
        }
        let ptr_addr = module_base.wrapping_add(rva);
        let Ok(player) = reader.read_u32(ptr_addr) else {
            continue;
        };
        for &hp_off in &hp_offs {
            consider(rva, player, hp_off, false);
        }
    }

    if let Some((_, found)) = best {
        let tag = if on_client { "client" } else { "hw" };
        tracing::info!(
            "local player: {tag}+{} → {:#x} HP+{:#x}",
            found.rva,
            found.player,
            found.health_offset
        );
        Some(found)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_non_empty() {
        assert!(!LP_RVA_HW.is_empty());
        assert!(!LP_RVA_CLIENT.is_empty());
    }
}
