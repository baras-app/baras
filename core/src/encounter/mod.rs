pub mod challenge;
pub mod combat;
pub mod effect_instance;
pub mod entity_info;
pub mod metrics;
pub mod shielding;
pub mod summary;

pub use challenge::{ChallengeTracker, ChallengeValue};
pub use combat::{ActiveBoss, CombatEncounter, ProcessingMode};
pub use effect_instance::EffectInstance;
pub use shielding::PendingAbsorption;

use chrono::NaiveDateTime;

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
pub struct BossHealthEntry {
    pub name: String,
    pub current: i32,
    pub max: i32,
    /// Used for sorting by encounter order (not serialized)
    #[serde(skip)]
    pub first_seen_at: Option<NaiveDateTime>,
}

impl BossHealthEntry {
    pub fn percent(&self) -> f32 {
        if self.max > 0 {
            (self.current as f32 / self.max as f32) * 100.0
        } else {
            0.0
        }
    }
}
