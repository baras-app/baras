//! The trigger type shared by timers, phases, counters and effect modifiers

use crate::serde_defaults::is_zero_f32_ref;
use crate::{
    AbilitySelector, ChargeDirection, EffectSelector, EntityFilter, EntitySelector, MitigationType, PositionConstraint,
};
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Trigger Types (shared across timers, phases, counters)
// ─────────────────────────────────────────────────────────────────────────────

/// Unified trigger type for timers, phases, and counters.
///
/// Different systems use different subsets:
/// - `[T]` = Timer only
/// - `[P]` = Phase only
/// - `[C]` = Counter only
/// - `[TPC]` = All systems
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Trigger {
    // ─── Combat State [TPC] ────────────────────────────────────────────────
    /// Combat starts. [TPC]
    #[default]
    CombatStart,

    /// Combat ends. [C only]
    CombatEnd,

    // ─── Abilities & Effects [TPC] ─────────────────────────────────────────
    /// Ability is cast. [TPC]
    AbilityCast {
        #[serde(default)]
        abilities: Vec<AbilitySelector>,
        #[serde(default = "EntityFilter::default_any")]
        source: EntityFilter,
        #[serde(default = "EntityFilter::default_any")]
        target: EntityFilter,
        /// Coordinate constraints on source/target (AND semantics, empty = any)
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        position: Vec<PositionConstraint>,
    },

    /// Effect/buff is applied. [TPC]
    EffectApplied {
        #[serde(default)]
        effects: Vec<EffectSelector>,
        #[serde(default)]
        source: EntityFilter,
        #[serde(default)]
        target: EntityFilter,
        /// Coordinate constraints on source/target (AND semantics, empty = any)
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        position: Vec<PositionConstraint>,
    },

    /// Effect/buff is removed. [TPC]
    EffectRemoved {
        #[serde(default)]
        effects: Vec<EffectSelector>,
        #[serde(default)]
        source: EntityFilter,
        #[serde(default)]
        target: EntityFilter,
        /// Coordinate constraints on source/target (AND semantics, empty = any)
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        position: Vec<PositionConstraint>,
    },

    /// Damage is taken from an ability. [TPC]
    DamageTaken {
        #[serde(default)]
        abilities: Vec<AbilitySelector>,
        #[serde(default)]
        source: EntityFilter,
        #[serde(default)]
        target: EntityFilter,
        /// Optional mitigation filter — if non-empty, only fires when the hit
        /// result matches one of the listed types (e.g. IMMUNE, RESIST).
        /// Empty (default) matches any hit result.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        mitigation: Vec<MitigationType>,
        /// Coordinate constraints on source/target (AND semantics, empty = any)
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        position: Vec<PositionConstraint>,
    },

    /// Damage is dealt to a target. [TPC]
    DamageDealt {
        #[serde(default)]
        abilities: Vec<AbilitySelector>,
        #[serde(default)]
        source: EntityFilter,
        #[serde(default)]
        target: EntityFilter,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        mitigation: Vec<MitigationType>,
        /// Coordinate constraints on source/target (AND semantics, empty = any)
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        position: Vec<PositionConstraint>,
    },

    /// Healing is received from an ability. [TPC]
    HealingTaken {
        #[serde(default)]
        abilities: Vec<AbilitySelector>,
        #[serde(default)]
        source: EntityFilter,
        #[serde(default)]
        target: EntityFilter,
        /// Coordinate constraints on source/target (AND semantics, empty = any)
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        position: Vec<PositionConstraint>,
    },

    /// Healing is dealt to a target. [M only]
    HealingDealt {
        #[serde(default)]
        abilities: Vec<AbilitySelector>,
        #[serde(default)]
        source: EntityFilter,
        #[serde(default)]
        target: EntityFilter,
        /// Coordinate constraints on source/target (AND semantics, empty = any)
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        position: Vec<PositionConstraint>,
    },

    /// Another effect's charges/stacks change. [M only]
    ChargesChanged {
        #[serde(default)]
        effects: Vec<EffectSelector>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        direction: Option<ChargeDirection>,
    },

    /// This effect's own charges/stacks change. [M only]
    SelfChargesChanged {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        direction: Option<ChargeDirection>,
    },

    /// The effect holder lands a killing blow (is the source of a Death event). [M only]
    KillingBlow,

    /// The effect holder spends a resource (energy/Force/heat/…). [M only]
    /// Resource type is ignored — only the magnitude matters.
    ResourceSpent {
        /// If > 0, the modifier's duration adjustment is scaled by
        /// `amount_spent / per_amount` (e.g. +1s per 10 energy). 0 = flat per event.
        #[serde(default, skip_serializing_if = "is_zero_f32_ref")]
        per_amount: f32,
    },

    /// Threat is modified by an ability (MODIFYTHREAT or TAUNT). [TPC]
    ThreatModified {
        /// Ability selectors. Empty matches any ability.
        #[serde(default)]
        abilities: Vec<AbilitySelector>,
        /// Who generated the threat (default: any)
        #[serde(default = "EntityFilter::default_any")]
        source: EntityFilter,
        /// Who received the threat change (default: any)
        #[serde(default = "EntityFilter::default_any")]
        target: EntityFilter,
        /// Exact threat value to match (None = any)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        threat_value: Option<f32>,
    },

    // ─── HP Thresholds [TPC] ───────────────────────────────────────────────
    /// Boss HP drops below threshold. [TPC]
    BossHpBelow {
        hp_percent: f32,
        #[serde(default)]
        selector: Vec<EntitySelector>,
    },

    /// Boss HP rises above threshold. [P only]
    BossHpAbove {
        hp_percent: f32,
        #[serde(default)]
        selector: Vec<EntitySelector>,
    },

    // ─── Entity Lifecycle [TPC] ────────────────────────────────────────────
    /// NPC appears (first seen in combat). [TPC]
    NpcAppears {
        #[serde(default)]
        selector: Vec<EntitySelector>,
        /// Coordinate constraints on the appearing NPC (AND semantics, empty = any).
        /// The NPC's position is checked regardless of the constraint's `entity` field.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        position: Vec<PositionConstraint>,
    },

    /// Entity dies. [TPC]
    EntityDeath {
        #[serde(default)]
        selector: Vec<EntitySelector>,
        /// Coordinate constraints on the dying entity (AND semantics, empty = any).
        /// The entity's position is checked regardless of the constraint's `entity` field.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        position: Vec<PositionConstraint>,
    },

    /// NPC sets its target. [T only]
    TargetSet {
        #[serde(default)]
        selector: Vec<EntitySelector>,
        #[serde(default)]
        target: EntityFilter,
    },

    // ─── Phase Events [TPC] ────────────────────────────────────────────────
    /// Phase is entered. [TC]
    PhaseEntered { phase_id: String },

    /// Phase ends. [TPC]
    PhaseEnded { phase_id: String },

    /// Any phase change occurs. [C only]
    AnyPhaseChange,

    // ─── Counter Events [TP] ───────────────────────────────────────────────
    /// Counter reaches a specific value. [TP]
    CounterReaches { counter_id: String, value: u32 },

    /// Counter value changes (any change, not just threshold crossing). [TPC]
    CounterChanges { counter_id: String },

    // ─── Timer Events [T only] ─────────────────────────────────────────────
    /// Another timer expires (chaining). [T only]
    TimerExpires { timer_id: String },

    /// Another timer starts (for cancellation). [T only]
    TimerStarted { timer_id: String },

    /// A timer has been canceled
    TimerCanceled { timer_id: String },

    // ─── Time-based [TP] ───────────────────────────────────────────────────
    /// Time elapsed since combat start. [TP]
    TimeElapsed { secs: f32 },

    // ─── System-specific ───────────────────────────────────────────────────
    /// Manual/debug trigger. [T only]
    Manual,

    /// Never triggers. [C only]
    Never,

    // ─── Composition [TPC] ─────────────────────────────────────────────────
    /// Any condition suffices (OR logic). [TPC]
    AnyOf { conditions: Vec<Trigger> },
}

impl Trigger {
    /// Get the position constraints for event-driven triggers (empty otherwise).
    pub fn position_constraints(&self) -> &[PositionConstraint] {
        match self {
            Self::AbilityCast { position, .. }
            | Self::EffectApplied { position, .. }
            | Self::EffectRemoved { position, .. }
            | Self::DamageTaken { position, .. }
            | Self::DamageDealt { position, .. }
            | Self::HealingTaken { position, .. }
            | Self::HealingDealt { position, .. }
            | Self::NpcAppears { position, .. }
            | Self::EntityDeath { position, .. } => position,
            _ => &[],
        }
    }

    /// Whether this trigger variant supports position constraints.
    pub fn supports_position(&self) -> bool {
        matches!(
            self,
            Self::AbilityCast { .. }
                | Self::EffectApplied { .. }
                | Self::EffectRemoved { .. }
                | Self::DamageTaken { .. }
                | Self::DamageDealt { .. }
                | Self::HealingTaken { .. }
                | Self::HealingDealt { .. }
                | Self::NpcAppears { .. }
                | Self::EntityDeath { .. }
        )
    }

    /// Return this trigger with its position constraints replaced.
    /// No-op for variants without position support.
    pub fn with_position(mut self, constraints: Vec<PositionConstraint>) -> Self {
        match &mut self {
            Self::AbilityCast { position, .. }
            | Self::EffectApplied { position, .. }
            | Self::EffectRemoved { position, .. }
            | Self::DamageTaken { position, .. }
            | Self::DamageDealt { position, .. }
            | Self::HealingTaken { position, .. }
            | Self::HealingDealt { position, .. }
            | Self::NpcAppears { position, .. }
            | Self::EntityDeath { position, .. } => *position = constraints,
            _ => {}
        }
        self
    }

    /// Returns a human-readable label for this trigger type.
    pub fn label(&self) -> &'static str {
        match self {
            Self::CombatStart => "Combat Start",
            Self::CombatEnd => "Combat End",
            Self::AbilityCast { .. } => "Ability Cast",
            Self::EffectApplied { .. } => "Effect Applied",
            Self::EffectRemoved { .. } => "Effect Removed",
            Self::DamageTaken { .. } => "Damage Taken",
            Self::DamageDealt { .. } => "Damage Dealt",
            Self::HealingTaken { .. } => "Healing Taken",
            Self::HealingDealt { .. } => "Healing Dealt",
            Self::ChargesChanged { .. } => "Charges Changed",
            Self::SelfChargesChanged { .. } => "Self Charges Changed",
            Self::ResourceSpent { .. } => "Resource Spent",
            Self::KillingBlow => "Killing Blow",
            Self::ThreatModified { .. } => "Threat Modified",
            Self::BossHpBelow { .. } => "Boss HP Below",
            Self::BossHpAbove { .. } => "Boss HP Above",
            Self::NpcAppears { .. } => "NPC Appears",
            Self::EntityDeath { .. } => "Entity Death",
            Self::TargetSet { .. } => "Target Set",
            Self::PhaseEntered { .. } => "Phase Entered",
            Self::PhaseEnded { .. } => "Phase Ended",
            Self::AnyPhaseChange => "Any Phase Change",
            Self::CounterReaches { .. } => "Counter Reaches",
            Self::CounterChanges { .. } => "Counter Changes",
            Self::TimerExpires { .. } => "Timer Expires",
            Self::TimerStarted { .. } => "Timer Started",
            Self::TimerCanceled { .. } => "Timer Canceled",
            Self::TimeElapsed { .. } => "Time Elapsed",
            Self::Manual => "Manual",
            Self::Never => "Never",
            Self::AnyOf { .. } => "Any Of (OR)",
        }
    }

    /// Returns the snake_case type name for this trigger.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::CombatStart => "combat_start",
            Self::CombatEnd => "combat_end",
            Self::AbilityCast { .. } => "ability_cast",
            Self::EffectApplied { .. } => "effect_applied",
            Self::EffectRemoved { .. } => "effect_removed",
            Self::DamageTaken { .. } => "damage_taken",
            Self::DamageDealt { .. } => "damage_dealt",
            Self::HealingTaken { .. } => "healing_taken",
            Self::HealingDealt { .. } => "healing_dealt",
            Self::ChargesChanged { .. } => "charges_changed",
            Self::SelfChargesChanged { .. } => "self_charges_changed",
            Self::ResourceSpent { .. } => "resource_spent",
            Self::KillingBlow => "killing_blow",
            Self::ThreatModified { .. } => "threat_modified",
            Self::BossHpBelow { .. } => "boss_hp_below",
            Self::BossHpAbove { .. } => "boss_hp_above",
            Self::NpcAppears { .. } => "npc_appears",
            Self::EntityDeath { .. } => "entity_death",
            Self::TargetSet { .. } => "target_set",
            Self::PhaseEntered { .. } => "phase_entered",
            Self::PhaseEnded { .. } => "phase_ended",
            Self::AnyPhaseChange => "any_phase_change",
            Self::CounterReaches { .. } => "counter_reaches",
            Self::CounterChanges { .. } => "counter_changes",
            Self::TimerExpires { .. } => "timer_expires",
            Self::TimerStarted { .. } => "timer_started",
            Self::TimerCanceled { .. } => "timer_canceled",
            Self::TimeElapsed { .. } => "time_elapsed",
            Self::Manual => "manual",
            Self::Never => "never",
            Self::AnyOf { .. } => "any_of",
        }
    }
}
