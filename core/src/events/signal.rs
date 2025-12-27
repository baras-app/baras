use crate::context::IStr;
use crate::combat_log::EntityType;
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
        /// NPC class/template ID (0 for players)
        npc_id: i64,
        entity_name: String,
        timestamp: NaiveDateTime,
    },
    EntityRevived {
        entity_id: i64,
        entity_type: EntityType,
        /// NPC class/template ID (0 for players)
        npc_id: i64,
        timestamp: NaiveDateTime,
    },

    /// NPC is first seen in the current encounter (for add spawn detection)
    NpcFirstSeen {
        entity_id: i64,
        /// NPC class/template ID
        npc_id: i64,
        entity_name: String,
        timestamp: NaiveDateTime,
    },

    // Effect tracking
    EffectApplied {
        effect_id: i64,
        /// The ability/action that caused this effect
        action_id: i64,
        source_id: i64,
        source_name: IStr,
        source_entity_type: EntityType,
        /// NPC class/template ID of source (0 for players/companions)
        source_npc_id: i64,
        target_id: i64,
        target_name: IStr,
        target_entity_type: EntityType,
        /// NPC class/template ID of target (0 for players/companions)
        target_npc_id: i64,
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
        /// NPC class/template ID of source (0 for players/companions)
        source_npc_id: i64,
        target_id: i64,
        target_name: IStr,
        target_entity_type: EntityType,
        /// NPC class/template ID of target (0 for players/companions)
        target_npc_id: i64,
        timestamp: NaiveDateTime,
    },

    /// Entity changed their target (TARGETSET effect)
    TargetChanged {
        source_id: i64,
        target_id: i64,
        target_name: IStr,
        /// NPC class/template ID (for boss detection)
        target_npc_id: i64,
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

    // ─── Boss Encounter Signals ────────────────────────────────────────────────
    /// A boss encounter was detected (first boss NPC seen in combat).
    /// Emitted once per combat when a known boss NPC is first encountered.
    BossEncounterDetected {
        /// The definition ID (e.g., "apex_vanguard")
        definition_id: String,
        /// Display name of the boss encounter
        boss_name: String,
        /// Index into SessionCache.boss_definitions
        definition_idx: usize,
        /// Entity instance ID of the boss NPC that triggered detection
        entity_id: i64,
        /// NPC class/template ID
        npc_id: i64,
        timestamp: NaiveDateTime,
    },

    /// Boss HP has changed (for phase transition detection)
    BossHpChanged {
        entity_id: i64,
        /// NPC class/template ID (for matching against boss definitions)
        npc_id: i64,
        entity_name: String,
        current_hp: i64,
        max_hp: i64,
        timestamp: NaiveDateTime,
    },

    /// Boss phase has changed
    PhaseChanged {
        boss_id: String,
        old_phase: Option<String>,
        new_phase: String,
        timestamp: NaiveDateTime,
    },

    /// Counter value has changed
    CounterChanged {
        counter_id: String,
        old_value: u32,
        new_value: u32,
        timestamp: NaiveDateTime,
    },
}

impl GameSignal {
    /// Get the timestamp from any signal variant
    pub fn timestamp(&self) -> NaiveDateTime {
        match self {
            Self::CombatStarted { timestamp, .. }
            | Self::CombatEnded { timestamp, .. }
            | Self::EntityDeath { timestamp, .. }
            | Self::EntityRevived { timestamp, .. }
            | Self::NpcFirstSeen { timestamp, .. }
            | Self::EffectApplied { timestamp, .. }
            | Self::EffectRemoved { timestamp, .. }
            | Self::EffectChargesChanged { timestamp, .. }
            | Self::AbilityActivated { timestamp, .. }
            | Self::TargetChanged { timestamp, .. }
            | Self::TargetCleared { timestamp, .. }
            | Self::AreaEntered { timestamp, .. }
            | Self::PlayerInitialized { timestamp, .. }
            | Self::DisciplineChanged { timestamp, .. }
            | Self::BossEncounterDetected { timestamp, .. }
            | Self::BossHpChanged { timestamp, .. }
            | Self::PhaseChanged { timestamp, .. }
            | Self::CounterChanged { timestamp, .. } => *timestamp,
        }
    }
}
