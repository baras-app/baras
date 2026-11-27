//! Names for abilities and effects the game shows none for (boss mechanics,
//! hidden helpers), generated from the internal names in data/ability_data.csv
//! and data/effect_data.csv.

use super::base_ability_id;

include!(concat!(env!("OUT_DIR"), "/internal_names.rs"));

/// A readable name for an ability or effect the log writes without one
pub fn internal_name(id: i64) -> Option<&'static str> {
    INTERNAL_NAMES
        .get(&id)
        .or_else(|| INTERNAL_NAMES.get(&base_ability_id(id)))
        .copied()
}
