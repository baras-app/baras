mod alacrity_buffs;
mod boss_registry;
mod bosses;
mod discipline;
mod discipline_abilities;
mod effects;
mod flashpoint_bosses;
mod flashpoints;
mod interrupt_abilities;
mod lair_bosses;
mod patches;
mod mirror_abilities;
mod pvp_instance;
mod raid_bosses;
mod raids;
mod shield_absorbs;
mod attack_types;
mod off_gcd_abilities;
mod shield_effects;
mod world_bosses;

pub use alacrity_buffs::{ALACRITY_BUFFS, AlacrityBuff};
pub use boss_registry::{
    clear_boss_registry, is_registered_boss, lookup_registered_name, register_hp_overlay_entity,
};
pub use bosses::{
    BossInfo, ContentType, Difficulty, difficulty_id, get_boss_ids, is_boss,
    lookup_area_content_type, lookup_boss,
};
pub use discipline::{Class, Discipline, DisciplineFilter, Role};
pub use discipline_abilities::DISCIPLINE_ABILITIES;
pub use effects::*;
pub use flashpoints::{FLASHPOINT_AREAS, get_flashpoint_name, is_flashpoint};
pub use interrupt_abilities::INTERRUPT_ABILITIES;
pub use mirror_abilities::{MIRROR_ABILITIES, MirrorAbility};
pub use pvp_instance::{
    ARENA_ROUND_END_ABILITY_IDS, DESERTER_DETECTION_EFFECT_IDS, PvpAreaKind, REBIRTH_ABILITY_ID,
    is_pvp_area, pvp_area_ids, pvp_area_kind, pvp_match_label,
};
pub use raids::{OPERATION_AREAS, get_operation_name, is_operation, is_world_boss};
pub use shield_absorbs::{SHIELD_INFO, ShieldInfo, get_shield_info, is_known_shield};
pub use attack_types::ATTACK_TYPES;
pub use off_gcd_abilities::OFF_GCD_ABILITIES;
pub use shield_effects::SHIELD_EFFECT_IDS;

pub use patches::{game_version_at, server_name};
