//! PvP area identification
//!
//! Area IDs for PvP instances (warzones, arenas)

/// PvP instance kind: 8-player warzones vs 4-player arenas
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PvpAreaKind {
    Warzone,
    Arena,
}

/// 8-player warzones
pub const WARZONE_AREA_IDS: [i64; 9] = [
    137438956902,    // The Civil War (Alderaan)
    945872057669561, // Yavin Ruins
    137438992654,    // Novare Coast
    137438993104,    // Ancient Hypergate
    137438989517,    // The Voidstar
    833571547775744, // Odessen Proving Grounds
    137438988866,    // The Pit (Nar Shaddaa Huttball)
    137438953518,    // Quesh Huttball Pit
    833571547775746, // Vandin Huttball (The Sky Shredder)
];

/// 4-player arenas
pub const ARENA_AREA_IDS: [i64; 7] = [
    137438993370,    // Corellia Square
    137438993381,    // Makeb Mesa Arena
    945872057669563, // Mandalorian Battle Ring
    137438993376,    // Orbital Station
    833571547775745, // Rishi Cove
    137438993374,    // Tatooine Canyon
    945872057669629, // Onderon Palatial Ruins
];

/// Every PvP instance id (for indexes that otherwise only know defined areas)
pub fn pvp_area_ids() -> impl Iterator<Item = i64> {
    WARZONE_AREA_IDS.into_iter().chain(ARENA_AREA_IDS)
}

/// Classify a PvP area ID, or `None` if the area is not a PvP instance
pub fn pvp_area_kind(area_id: i64) -> Option<PvpAreaKind> {
    if WARZONE_AREA_IDS.contains(&area_id) {
        Some(PvpAreaKind::Warzone)
    } else if ARENA_AREA_IDS.contains(&area_id) {
        Some(PvpAreaKind::Arena)
    } else {
        None
    }
}

/// Check if an area ID corresponds to a PvP instance
pub fn is_pvp_area(area_id: i64) -> bool {
    pvp_area_kind(area_id).is_some()
}

/// "Deserter Detection" effects applied to every player the moment a PvP round
/// begins (the spawn barrier drops), before any EnterCombat. The first
/// application is the round-start anchor: the game clocks PvP DPS/HPS from
/// this moment. Warzones and arenas use distinct effect IDs. Also applied to
/// individual players on warzone respawns and when backfilling an in-progress
/// match, so only the local player's first application per round matters.
pub const DESERTER_DETECTION_EFFECT_IDS: [i64; 2] = [
    2730083076800512, // Warzones
    3297813328822272, // Arenas
];

/// System heal fired at every player when a PvP match ends: the final arena
/// round (a few ms before the Heal Target burst) and the end of every warzone.
/// Never occurs mid-match — not even on warzone spawn respawns.
pub const REBIRTH_ABILITY_ID: i64 = 773652459028480;

/// System abilities whose Heal events mark an arena round boundary: the game
/// heals every player to full the instant a round ends ("Heal Target"), and
/// additionally fires "Rebirth" when the final round ends the match (a few ms
/// before the Heal Target burst). Neither occurs mid-round.
pub const ARENA_ROUND_END_ABILITY_IDS: [i64; 2] = [
    REBIRTH_ABILITY_ID,  // Rebirth (match end only)
    813707324030976,     // Heal Target (every round end)
];

/// Display label for a PvP area's match type
pub fn pvp_match_label(area_id: i64) -> &'static str {
    match pvp_area_kind(area_id) {
        Some(PvpAreaKind::Warzone) => "Warzone Match",
        Some(PvpAreaKind::Arena) => "Arena Round",
        None => "PvP Match",
    }
}
