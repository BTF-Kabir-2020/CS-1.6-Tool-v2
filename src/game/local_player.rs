/// [EN] Find the Local Player entity — handles different CS 1.6 builds (Steam / non-Steam).
/// [FA] پیدا کردن موجودیت Local Player — پشتیبانی از نسخه‌های مختلف CS 1.6 (Steam / non-Steam).
use crate::win::memory::MemoryReader;

/// [EN] Known RVAs for the hw.dll local player pointer (non-Steam & Steam 8684 builds).
/// [FA] آدرس‌های RVA شناخته‌شده اشاره‌گر local player در hw.dll (نسخه‌های non-Steam و Steam 8684).
const LP_RVA_HW: &[u32] = &[
    0x0010FC80, // non-Steam dump: HP=100 + world coords
    0x0012D6F4, // E:\games\gamecs\ non-Steam (dump verified)
    0x0032ABF4, // BLASTHACK / Steam 8684
    0x0013FDF4, 0x00827484, 0x0051FA44, 0x007028FC,
];

/// [EN] Known RVAs for client.dll local player pointer (Cheat Engine references).
/// [FA] آدرس‌های RVA شناخته‌شده اشاره‌گر local player در client.dll (مرجع Cheat Engine).
const LP_RVA_CLIENT: &[u32] = &[
    0x0004B530, // CE player_health_from_base
    0x0011D438, // CE heal pointerscan base
];

/// [EN] Candidate offsets from the player entity base address to the health field.
/// [FA] آفست‌های نامزد از آدرس پایه موجودیت بازیکن تا فیلد health.
const HP_OFFSET_CANDIDATES: &[u32] = &[0x14, 0xB74, 0xFC, 0x100];

/// [EN] Result of a successful local player discovery.
/// [FA] نتیجه پیدا کردن موفقیت‌آمیز local player.
pub struct LocalPlayerDiscovery {
    /// [EN] Relative Virtual Address of the pointer to the player entity.
    /// [FA] آدرس نسبی مجازی (RVA) اشاره‌گر به موجودیت بازیکن.
    pub rva: u32,
    /// [EN] Absolute address of the local player entity in game memory.
    /// [FA] آدرس مطلق موجودیت local player در حافظه بازی.
    pub player: u32,
    /// [EN] Offset from player base to the health (HP) field.
    /// [FA] آفست از پایه بازیکن تا فیلد سلامتی (HP).
    pub health_offset: u32,
}

/// [EN] Try to read the player's health at a given offset; returns None if invalid.
/// [FA] تلاش برای خواندن سلامتی بازیکن در آفست مشخص؛ None در صورت نامعتبر بودن.
fn read_hp(reader: &MemoryReader, player: u32, hp_off: u32) -> Option<i32> {
    // Validate the player pointer: must be in usable memory range and DWORD-aligned
    if !(0x0100_0000..=0x7FFF_0000).contains(&player) || player & 3 != 0 {
        return None;
    }
    reader
        .read_i32(player.wrapping_add(hp_off))
        .ok()
        .filter(|&hp| (0..=100).contains(&hp))
}

/// [EN] Score a candidate (rva, player ptr, hp_off) to pick the best local player match.
/// [FA] امتیازدهی به یک نامزد (rva, اشاره‌گر بازیکن, hp_off) برای انتخاب بهترین تطابق local player.
fn score_candidate(
    reader: &MemoryReader,
    player: u32,
    hp_off: u32,
    config_hp_off: u32,
    from_known_rva: bool,
) -> i32 {
    // If health is unreadable, candidate is invalid
    let Some(hp) = read_hp(reader, player, hp_off) else {
        return i32::MIN;
    };
    let mut score = 0;
    // Known RVA gets highest priority
    if from_known_rva {
        score += 1000;
    }
    // Matching the configured HP offset is preferred
    if hp_off == config_hp_off {
        score += 500;
    } else if hp_off == 0xB74 {
        score += 300;
    } else if hp_off == 0xFC {
        score += 200;
    }
    // Bonus if armor is also valid (0–100)
    if let Ok(armor) = reader.read_i32(player.wrapping_add(0x10C)) {
        if (0..=100).contains(&armor) {
            score += 100;
        }
    }
    // HP نزدیک 100 = بازیکن زنده محتمل‌تر
    score += hp;
    score
}

/// [EN] Discover the local player entity: try config RVA first, then known list, then scan.
/// [FA] پیدا کردن موجودیت local player: ابتدا RVA پیکربندی، سپس لیست شناخته‌شده، سپس اسکن ماژول.
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

    // Select the appropriate known RVA list and scan range based on the target module
    let known = if on_client { LP_RVA_CLIENT } else { LP_RVA_HW };
    let (scan_lo, scan_hi) = if on_client {
        (0x100_000, 0x200_000)
    } else {
        (0x100_000, 0x600_000)
    };

    // Build prioritized RVA list: config first, then known addresses
    let mut try_rvas = vec![config_rva];
    for &rva in known {
        if rva != config_rva {
            try_rvas.push(rva);
        }
    }

    // Build prioritized HP offset list: config first, then candidates
    let mut hp_offs = vec![config_hp_off];
    for &off in HP_OFFSET_CANDIDATES {
        if off != config_hp_off {
            hp_offs.push(off);
        }
    }

    let mut best: Option<(i32, LocalPlayerDiscovery)> = None;

    // Closure to evaluate and potentially update the best candidate
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

    // Phase 1: try all known/config RVA addresses
    for &rva in &try_rvas {
        let ptr_addr = module_base.wrapping_add(rva);
        let Ok(player) = reader.read_u32(ptr_addr) else {
            continue;
        };
        for &hp_off in &hp_offs {
            consider(rva, player, hp_off, true);
        }
    }

    // Phase 2: linear scan over the module's address range
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
