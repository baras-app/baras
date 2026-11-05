//! Per-pull progress facts captured at summary time (see
//! `docs/history-dashboard-plan.md` §3.1). Shared leaf types: the backend
//! stores them in the history record and the frontend renders them.

use serde::{Deserialize, Serialize};

/// Why a pull ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullEnd {
    Kill,
    /// Whole active roster dead
    Wipe,
    /// Boss reset / re-pulled without a wipe (new single-instance boss seen)
    Reset,
    /// Log ended (file truncated or still being written) before combat exit
    LogEnd,
    #[default]
    Unknown,
}

/// Lowest HP seen on one boss-definition NPC during the pull.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BossHpRecord {
    pub name: String,
    pub npc_id: i64,
    /// Minimum HP % observed (the real "how far")
    pub lowest_pct: f32,
    /// Combat seconds when `lowest_pct` was reached
    pub lowest_at_secs: f32,
    /// HP % at the last sample (100 after a reset)
    pub last_pct: f32,
    pub dead: bool,
}

/// Furthest phase reached, resolved against the definition's phase order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhaseRef {
    pub id: String,
    pub name: String,
    /// Index in the definition's phase list
    pub index: u16,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PullProgress {
    /// One per boss-definition NPC engaged
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bosses: Vec<BossHpRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_reached: Option<PhaseRef>,
    /// `(phase_id, combat_secs)` in the order phases were entered
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phases_seen: Vec<(String, f32)>,
}

/// One player death, in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PullDeath {
    pub entity_id: i64,
    pub name: String,
    /// Combat seconds since the pull started
    pub secs: f32,
}
