pub mod challenge;
pub mod combat;
pub mod effect_instance;
pub mod entity_info;
pub mod metrics;
pub mod pvp_faction;
pub mod shielding;
pub mod summary;

pub use challenge::{ChallengeTracker, ChallengeValue};
pub use combat::{ActiveBoss, CombatEncounter, ProcessingMode};
pub use effect_instance::EffectInstance;
pub use pvp_faction::{PvpFaction, PvpFactionMap, PvpFactionTracker, PvpOutcome};
pub use shielding::ShieldContext;

use chrono::NaiveDateTime;

use crate::dsl::HpMarker;

/// Active shield state for overlay display
#[derive(Debug, Clone)]
pub struct ActiveShield {
    pub label: String,
    pub remaining: i64,
    pub total: i64,
    /// Game icon to show beside the amount (see `ShieldDefinition::icon_id`)
    pub icon_ability_id: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub enum EncounterState {
    #[default]
    NotStarted,
    InCombat,
    PostCombat {
        exit_time: NaiveDateTime,
    },
}

/// Classification of the phase/content type where an encounter occurred
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum PhaseType {
    #[default]
    OpenWorld,
    Raid,
    Flashpoint,
    PvP,
    DummyParse,
}

/// Real-time boss health data for overlay display
#[derive(Debug, Clone, serde::Serialize)]
pub struct OverlayHealthEntry {
    /// Unique per-spawn entity id (NPC `log_id`). Disambiguates same-named entities.
    pub entity_id: i64,
    pub name: String,
    pub target_name: Option<String>,
    /// Role of the targeted player (from their discipline), when known
    #[serde(skip)]
    pub target_role: Option<crate::game_data::Role>,
    pub current: i32,
    pub max: i32,
    /// Used for sorting by encounter order (not serialized)
    #[serde(skip)]
    pub first_seen_at: Option<NaiveDateTime>,
    /// HP threshold markers from entity definition
    #[serde(skip)]
    pub hp_markers: Vec<HpMarker>,
    /// Active shields on this entity
    #[serde(skip)]
    pub active_shields: Vec<ActiveShield>,
    /// HP% threshold at which this entity is "pushed" out of combat (from entity definition)
    #[serde(skip)]
    pub pushes_at: Option<f32>,
}

impl OverlayHealthEntry {
    pub fn percent(&self) -> f32 {
        if self.max > 0 {
            (self.current as f32 / self.max as f32) * 100.0
        } else {
            0.0
        }
    }

    /// Whether this entity has been pushed out of combat (HP% at or below pushes_at threshold)
    pub fn is_pushed(&self) -> bool {
        self.pushes_at.is_some_and(|threshold| self.percent() <= threshold)
    }
}
