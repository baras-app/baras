//! Replay tab: per-entity movement tracks plus cast and effect spans for one
//! encounter. Times are combat seconds.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ReplayData {
    pub duration_secs: f32,
    pub entities: Vec<ReplayEntity>,
    pub casts: Vec<ReplayCast>,
    pub effects: Vec<ReplayEffect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReplayEntityKind {
    Player,
    Companion,
    Npc,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayEntity {
    /// Log instance id: unique per NPC instance, so same-named adds stay apart
    pub id: i64,
    pub name: String,
    pub kind: ReplayEntityKind,
    pub class_id: i64,
    pub is_boss: bool,
    pub discipline_id: Option<i64>,
    pub class_icon: Option<String>,
    pub role_icon: Option<String>,
    pub track: ReplayTrack,
    /// Target changes in time order; None = target cleared
    pub targets: Vec<(f32, Option<i64>)>,
    /// Intervals spent dead
    pub dead: Vec<(f32, f32)>,
    /// Damage dealt per whole second of combat (players only)
    pub damage: Vec<u32>,
    /// Effective healing done per whole second of combat (players only)
    pub healing: Vec<u32>,
}

/// Position, facing and health samples as parallel arrays, in time order
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ReplayTrack {
    pub t: Vec<f32>,
    pub x: Vec<f32>,
    pub y: Vec<f32>,
    pub z: Vec<f32>,
    pub facing: Vec<f32>,
    pub hp: Vec<i32>,
    pub max_hp: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayCast {
    pub entity_id: i64,
    pub ability_id: i64,
    pub name: String,
    pub start: f32,
    /// When the cast actually stopped, per the log
    pub end: f32,
    /// When it would finish at its base cast or channel time
    pub nominal_end: f32,
    pub is_channel: bool,
    pub interrupted: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplayEffect {
    pub target_id: i64,
    pub source_id: i64,
    pub effect_id: i64,
    /// Ability the effect comes from, for its icon
    pub ability_id: i64,
    pub name: String,
    pub start: f32,
    pub end: f32,
    pub is_debuff: bool,
}
