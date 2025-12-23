//! PvP area identification
//!
//! Area IDs for PvP instances (warzones, arenas)

use hashbrown::HashSet;
use std::sync::LazyLock;

/// Known PvP area IDs
static PVP_AREA_IDS: LazyLock<HashSet<i64>> = LazyLock::new(|| {
    [
        137438956902,
        945872057669561,
        137438992654,
        137438993104,
        137438989517,
        833571547775744,
        137438988866,
        137438953518,
        833571547775746,
        137438993370,
        137438993381,
        945872057669563,
        137438993376,
        833571547775745,
        137438993374,
        945872057669629,
    ]
    .into_iter()
    .collect()
});

/// Check if an area ID corresponds to a PvP instance
pub fn is_pvp_area(area_id: i64) -> bool {
    PVP_AREA_IDS.contains(&area_id)
}
