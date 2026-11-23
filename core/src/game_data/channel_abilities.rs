//! Base channel times, generated from data/ability_data.csv (`channel_time`).

use super::base_ability_id;

include!(concat!(env!("OUT_DIR"), "/channel_abilities.rs"));

/// Seconds a channelled ability runs with no alacrity; None when it isn't a channel.
pub fn channel_secs(ability_id: i64) -> Option<f32> {
    CHANNEL_TIMES
        .get(&ability_id)
        .or_else(|| CHANNEL_TIMES.get(&base_ability_id(ability_id)))
        .copied()
}
