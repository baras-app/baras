mod boss_registry;
mod bosses;
mod discipline;
mod effects;
mod flashpoint_bosses;
mod flashpoints;
mod lair_bosses;
mod pvp_instance;
mod raid_bosses;
mod raids;
mod shield_effects;

pub use boss_registry::{
    clear_boss_registry, is_registered_boss, lookup_registered_name, register_hp_overlay_entity,
};
pub use bosses::{
    BossInfo, ContentType, Difficulty, get_boss_ids, is_boss, lookup_area_content_type, lookup_boss,
};
pub use discipline::{Class, Discipline, Role};
pub use effects::*;
pub use flashpoints::{FLASHPOINT_AREAS, get_flashpoint_name, is_flashpoint};
pub use pvp_instance::is_pvp_area;
pub use raids::{OPERATION_AREAS, get_operation_name, is_operation, is_world_boss};
pub use shield_effects::SHIELD_EFFECT_IDS;
