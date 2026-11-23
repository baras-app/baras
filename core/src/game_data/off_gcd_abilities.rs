//! Off-GCD ability IDs, generated from data/ability_data.csv (no GCD in the game data)
//! plus data/off_gcd_extra.txt.

use super::base_ability_id;

include!(concat!(env!("OUT_DIR"), "/off_gcd_abilities.rs"));

pub fn is_off_gcd(ability_id: i64) -> bool {
    OFF_GCD_ABILITIES.contains(&ability_id) || OFF_GCD_ABILITIES.contains(&base_ability_id(ability_id))
}
