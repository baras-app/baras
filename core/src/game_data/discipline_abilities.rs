//! Which disciplines have each ability, for ability-based discipline detection.
//! Used to infer disciplines for players that never emit DisciplineChanged
//! events (e.g., enemy players in PvP).
//!
//! Generated at build time from data/ability_data.csv (`disciplines` column)
//! and data/discipline_ids.csv.

use super::base_ability_id;

include!(concat!(env!("OUT_DIR"), "/discipline_abilities.rs"));

/// Narrow a player's discipline candidates (0 = nothing seen yet) by an ability
/// they used. Shared-tree abilities (Furious Strike: Rage or Fury) and
/// advanced-class abilities (Enrage: any Juggernaut) each leave several
/// candidates; together they leave one.
///
/// Evidence that contradicts the candidates replaces them only when it is an
/// activation: damage can carry someone else's ability (a reflect buff's hits
/// are logged with the protected player as source).
pub fn narrow_disciplines(candidates: u64, ability_id: i64, is_activation: bool) -> u64 {
    let Some(&mask) = ABILITY_DISCIPLINES
        .get(&ability_id)
        .or_else(|| ABILITY_DISCIPLINES.get(&base_ability_id(ability_id)))
    else {
        return candidates;
    };
    match candidates & mask {
        0 if candidates == 0 || is_activation => mask,
        0 => candidates,
        narrowed => narrowed,
    }
}

/// The discipline GUID once the candidates are down to one.
pub fn resolved_discipline(candidates: u64) -> Option<i64> {
    (candidates.count_ones() == 1).then(|| DISCIPLINE_GUIDS[candidates.trailing_zeros() as usize])
}
