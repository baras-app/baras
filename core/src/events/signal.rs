use crate::context::IStr;
use crate::log::EntityType;
use chrono::NaiveDateTime;

/// Signals emitted by the EventProcessor for cross-cutting concerns.
/// These represent "interesting things that happened" at a higher level
/// than raw log events.
#[derive(Debug, Clone)]
pub enum GameSignal {
    // Combat lifecycle
    CombatStarted {
        timestamp: NaiveDateTime,
        encounter_id: u64,
    },
    CombatEnded {
        timestamp: NaiveDateTime,
        encounter_id: u64,
    },

    // Entity state changes
    EntityDeath {
        entity_id: i64,
        entity_type: EntityType,
        timestamp: NaiveDateTime,
    },
    EntityRevived {
        entity_id: i64,
        entity_type: EntityType,
        timestamp: NaiveDateTime,
    },

    // Effect tracking
    EffectApplied {
        effect_id: i64,
        /// The ability/action that caused this effect
        action_id: i64,
        source_id: i64,
        target_id: i64,
        target_name: IStr,
        target_entity_type: EntityType,
        timestamp: NaiveDateTime,
        /// Initial charges (if applicable, from log)
        charges: Option<u8>,
    },
    EffectRemoved {
        effect_id: i64,
        source_id: i64,
        target_id: i64,
        timestamp: NaiveDateTime,
    },
    /// Effect charges/stacks changed (ModifyCharges event)
    EffectChargesChanged {
        effect_id: i64,
        /// The ability/action that caused this charge change
        action_id: i64,
        target_id: i64,
        timestamp: NaiveDateTime,
        /// New charge count
        charges: u8,
    },

    // Ability activation (for timer triggers and raid frame registration)
    AbilityActivated {
        ability_id: i64,
        source_id: i64,
        target_id: i64,
        target_name: IStr,
        target_entity_type: EntityType,
        timestamp: NaiveDateTime,
    },

    /// Entity changed their target (TARGETSET effect)
    TargetChanged {
        source_id: i64,
        target_id: i64,
        target_name: IStr,
        target_entity_type: EntityType,
        timestamp: NaiveDateTime,
    },

    /// Entity cleared their target (TARGETCLEARED effect)
    TargetCleared {
        source_id: i64,
        timestamp: NaiveDateTime,
    },

    // Area transitions
    AreaEntered {
        area_id: i64,
        area_name: String,
        difficulty_id: i64,
        difficulty_name: String,
        timestamp: NaiveDateTime,
    },

    // Player initialization
    PlayerInitialized {
        entity_id: i64,
        timestamp: NaiveDateTime,
    },

    /// Player discipline detected (fires for ALL players, not just local)
    DisciplineChanged {
        entity_id: i64,
        discipline_id: i64,
        timestamp: NaiveDateTime,
    },
}
