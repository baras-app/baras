//! Base cast and channel times, generated from data/ability_data.csv
//! (`cast_time`, `channel_time`).

use super::base_ability_id;

include!(concat!(env!("OUT_DIR"), "/channel_abilities.rs"));

/// Seconds a channelled ability runs with no alacrity; None when it isn't a channel.
pub fn channel_secs(ability_id: i64) -> Option<f32> {
    CHANNEL_TIMES
        .get(&ability_id)
        .or_else(|| CHANNEL_TIMES.get(&base_ability_id(ability_id)))
        .copied()
}

/// Seconds an ability takes to cast with no alacrity; None when it is instant.
pub fn cast_secs(ability_id: i64) -> Option<f32> {
    CAST_TIMES
        .get(&ability_id)
        .or_else(|| CAST_TIMES.get(&base_ability_id(ability_id)))
        .copied()
}
