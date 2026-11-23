//! Defensive cooldowns, generated from data/defensive_cooldowns.txt plus each
//! ID's Republic mirror: the abilities pressed and the effects they give.

use super::base_ability_id;

include!(concat!(env!("OUT_DIR"), "/defensive_cooldowns.rs"));

/// Pressing this ability is using a defensive cooldown.
pub fn is_defensive_ability(ability_id: i64) -> bool {
    DEFENSIVE_ABILITIES.contains(&ability_id) || DEFENSIVE_ABILITIES.contains(&base_ability_id(ability_id))
}

/// A defensive cooldown's effect: damage reduction, absorb, reflect, defence
/// or an accuracy debuff on attackers.
pub fn is_defensive_effect(effect_id: i64) -> bool {
    DEFENSIVE_EFFECTS.contains(&effect_id)
}
