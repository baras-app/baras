//! Unified trigger evaluation for all systems (timers, phases, counters, victory).
//!
//! This module provides the single source of truth for "does this trigger match?"
//! across all systems. Each system calls these functions instead of maintaining
//! its own matching logic, ensuring consistent behavior and no gaps.
//!
//! There are three evaluation contexts:
//! - **Signal-based**: Does a trigger match any signal in a batch? (`check_signal_trigger`)
//! - **Event-based**: Does a trigger match a raw combat event? (`check_event_trigger`)
//! - **Timer-based**: Does a trigger match expired/started timer IDs? (`check_timer_trigger`)

use std::collections::HashSet;

use crate::combat_log::CombatEvent;
use crate::dsl::EntityDefinition;
use crate::dsl::Trigger;
use crate::game_data::{effect_id, effect_type_id};
use crate::timers::matches_source_target_filters;

use super::GameSignal;

// ═══════════════════════════════════════════════════════════════════════════
// Filter Context (shared by all systems)
// ═══════════════════════════════════════════════════════════════════════════

/// Context needed for source/target filter evaluation.
/// Used by phases, counters, and victory triggers.
pub struct FilterContext<'a> {
    pub entities: &'a [EntityDefinition],
    pub local_player_id: Option<i64>,
    pub current_target_id: Option<i64>,
    pub boss_entity_ids: &'a HashSet<i64>,
}

// ═══════════════════════════════════════════════════════════════════════════
// Signal-based trigger evaluation
// ═══════════════════════════════════════════════════════════════════════════

/// Check if a trigger is satisfied by any signal in the batch.
///
/// This is the unified signal-based trigger checker used by phases, counters,
/// and victory triggers. It handles every trigger variant that can be matched
/// against GameSignals, including AnyOf composition.
///
/// Trigger types that require different evaluation contexts (CombatEvent or
/// timer IDs) return false here and must be checked via `check_event_trigger()`
/// or `check_timer_trigger()` respectively.
pub fn check_signal_trigger(
    trigger: &Trigger,
    signals: &[GameSignal],
    filter_ctx: &FilterContext,
) -> bool {
    match trigger {
        // ─── Combat State ──────────────────────────────────────────────────
        Trigger::CombatStart => signals
            .iter()
            .any(|s| matches!(s, GameSignal::CombatStarted { .. })),

        Trigger::CombatEnd => signals
            .iter()
            .any(|s| matches!(s, GameSignal::CombatEnded { .. })),

        // ─── HP Thresholds ─────────────────────────────────────────────────
        Trigger::BossHpBelow { .. } => signals.iter().any(|s| {
            if let GameSignal::BossHpChanged {
                npc_id,
                entity_name,
                old_hp_percent,
                new_hp_percent,
                ..
            } = s
            {
                trigger.matches_boss_hp_below(
                    filter_ctx.entities,
                    *npc_id,
                    entity_name,
                    *old_hp_percent,
                    *new_hp_percent,
                )
            } else {
                false
            }
        }),

        Trigger::BossHpAbove { .. } => signals.iter().any(|s| {
            if let GameSignal::BossHpChanged {
                npc_id,
                entity_name,
                old_hp_percent,
                new_hp_percent,
                ..
            } = s
            {
                trigger.matches_boss_hp_above(
                    filter_ctx.entities,
                    *npc_id,
                    entity_name,
                    *old_hp_percent,
                    *new_hp_percent,
                )
            } else {
                false
            }
        }),

        // ─── Entity Lifecycle ──────────────────────────────────────────────
        Trigger::NpcAppears { .. } => signals.iter().any(|s| {
            if let GameSignal::NpcFirstSeen {
                npc_id,
                entity_name,
                ..
            } = s
            {
                trigger.matches_npc_appears(filter_ctx.entities, *npc_id, entity_name)
            } else {
                false
            }
        }),

        Trigger::EntityDeath { .. } => signals.iter().any(|s| {
            if let GameSignal::EntityDeath {
                npc_id,
                entity_name,
                ..
            } = s
            {
                trigger.matches_entity_death(filter_ctx.entities, *npc_id, entity_name)
            } else {
                false
            }
        }),

        // ─── Phase Events ──────────────────────────────────────────────────
        Trigger::PhaseEntered { .. } => signals.iter().any(|s| {
            if let GameSignal::PhaseChanged { new_phase, .. } = s {
                trigger.matches_phase_entered(new_phase)
            } else {
                false
            }
        }),

        Trigger::PhaseEnded { .. } => signals.iter().any(|s| match s {
            GameSignal::PhaseChanged {
                old_phase: Some(old),
                ..
            } => trigger.matches_phase_ended(old),
            GameSignal::PhaseEndTriggered { phase_id, .. } => trigger.matches_phase_ended(phase_id),
            _ => false,
        }),

        Trigger::AnyPhaseChange => signals
            .iter()
            .any(|s| matches!(s, GameSignal::PhaseChanged { .. })),

        // ─── Counter Events ────────────────────────────────────────────────
        Trigger::CounterReaches { .. } => signals.iter().any(|s| {
            if let GameSignal::CounterChanged {
                counter_id,
                old_value,
                new_value,
                ..
            } = s
            {
                trigger.matches_counter_reaches(counter_id, *old_value, *new_value)
            } else {
                false
            }
        }),

        // ─── Damage/Healing (signal-based with source/target filters) ──────
        Trigger::DamageTaken { .. } => signals.iter().any(|s| {
            if let GameSignal::DamageTaken {
                ability_id,
                ability_name,
                source_id,
                source_entity_type,
                source_name,
                source_npc_id,
                target_id,
                target_entity_type,
                target_name,
                target_npc_id,
                ..
            } = s
            {
                let ability_name_str = crate::context::resolve(*ability_name);

                if !trigger.matches_damage_taken(*ability_id as u64, Some(ability_name_str)) {
                    return false;
                }

                matches_source_target_filters(
                    trigger,
                    filter_ctx.entities,
                    *source_id,
                    *source_entity_type,
                    *source_name,
                    *source_npc_id,
                    *target_id,
                    *target_entity_type,
                    *target_name,
                    *target_npc_id,
                    filter_ctx.local_player_id,
                    filter_ctx.current_target_id,
                    filter_ctx.boss_entity_ids,
                )
            } else {
                false
            }
        }),

        Trigger::HealingTaken { .. } => signals.iter().any(|s| {
            if let GameSignal::HealingDone {
                ability_id,
                ability_name,
                source_id,
                source_entity_type,
                source_name,
                source_npc_id,
                target_id,
                target_entity_type,
                target_name,
                target_npc_id,
                ..
            } = s
            {
                let ability_name_str = crate::context::resolve(*ability_name);

                if !trigger.matches_healing_taken(*ability_id as u64, Some(ability_name_str)) {
                    return false;
                }

                matches_source_target_filters(
                    trigger,
                    filter_ctx.entities,
                    *source_id,
                    *source_entity_type,
                    *source_name,
                    *source_npc_id,
                    *target_id,
                    *target_entity_type,
                    *target_name,
                    *target_npc_id,
                    filter_ctx.local_player_id,
                    filter_ctx.current_target_id,
                    filter_ctx.boss_entity_ids,
                )
            } else {
                false
            }
        }),

        // ─── Timer triggers (handled by check_timer_trigger, not signals) ──
        Trigger::TimerExpires { .. } | Trigger::TimerStarted { .. } => false,

        // ─── Event-based triggers (handled by check_event_trigger, not signals)
        Trigger::AbilityCast { .. }
        | Trigger::EffectApplied { .. }
        | Trigger::EffectRemoved { .. } => false,

        // ─── Not signal-based ──────────────────────────────────────────────
        Trigger::TimeElapsed { .. }
        | Trigger::TargetSet { .. }
        | Trigger::Manual
        | Trigger::Never => false,

        // ─── Composition ───────────────────────────────────────────────────
        Trigger::AnyOf { conditions } => conditions
            .iter()
            .any(|c| check_signal_trigger(c, signals, filter_ctx)),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Event-based trigger evaluation
// ═══════════════════════════════════════════════════════════════════════════

/// Check if a trigger is satisfied by a raw combat event.
///
/// This handles AbilityCast, EffectApplied, and EffectRemoved triggers by
/// inspecting the CombatEvent fields directly. Source/target filters are
/// checked when a FilterContext is provided.
///
/// For EffectRemoved, this also checks the action that caused the removal
/// (e.g., "Dying Light" removing "Surging Flame"), not just the effect itself.
pub fn check_event_trigger(
    trigger: &Trigger,
    event: &CombatEvent,
    filter_ctx: Option<&FilterContext>,
) -> bool {
    // Check AbilityCast triggers
    if event.effect.effect_id == effect_id::ABILITYACTIVATE {
        let ability_id = event.action.action_id as u64;
        let ability_name = crate::context::resolve(event.action.name);
        if trigger.matches_ability(ability_id, Some(ability_name))
            && check_event_filters(trigger, event, filter_ctx)
        {
            return true;
        }
    }

    // Check EffectApplied triggers
    if event.effect.type_id == effect_type_id::APPLYEFFECT {
        let eff_id = event.effect.effect_id as u64;
        let eff_name = crate::context::resolve(event.effect.effect_name);
        if trigger.matches_effect_applied(eff_id, Some(eff_name))
            && check_event_filters(trigger, event, filter_ctx)
        {
            return true;
        }
    }

    // Check EffectRemoved triggers
    // Matches either:
    // 1. The effect being removed (event.effect.effect_id)
    // 2. The ability doing the removing (event.action.action_id)
    if event.effect.type_id == effect_type_id::REMOVEEFFECT {
        let eff_id = event.effect.effect_id as u64;
        let eff_name = crate::context::resolve(event.effect.effect_name);
        if trigger.matches_effect_removed(eff_id, Some(eff_name))
            && check_event_filters(trigger, event, filter_ctx)
        {
            return true;
        }
        // Also try matching the ability doing the removing
        let action_id = event.action.action_id as u64;
        let action_name = crate::context::resolve(event.action.name);
        if trigger.matches_effect_removed(action_id, Some(action_name))
            && check_event_filters(trigger, event, filter_ctx)
        {
            return true;
        }
    }

    false
}

/// Check source/target filters for an event-based trigger.
/// Returns true (passes) if no filter context is available (backward compat).
fn check_event_filters(
    trigger: &Trigger,
    event: &CombatEvent,
    filter_ctx: Option<&FilterContext>,
) -> bool {
    let Some(ctx) = filter_ctx else {
        return true; // No context = no filtering
    };

    matches_source_target_filters(
        trigger,
        ctx.entities,
        event.source_entity.log_id,
        event.source_entity.entity_type,
        event.source_entity.name,
        event.source_entity.class_id,
        event.target_entity.log_id,
        event.target_entity.entity_type,
        event.target_entity.name,
        event.target_entity.class_id,
        ctx.local_player_id,
        ctx.current_target_id,
        ctx.boss_entity_ids,
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// Timer-ID-based trigger evaluation
// ═══════════════════════════════════════════════════════════════════════════

/// Check if a trigger matches any expired or started timer IDs.
/// Handles TimerExpires, TimerStarted, and AnyOf composition.
pub fn check_timer_trigger(
    trigger: &Trigger,
    expired_timer_ids: &[String],
    started_timer_ids: &[String],
) -> bool {
    match trigger {
        Trigger::TimerExpires { timer_id } => expired_timer_ids.contains(timer_id),
        Trigger::TimerStarted { timer_id } => started_timer_ids.contains(timer_id),
        Trigger::AnyOf { conditions } => conditions
            .iter()
            .any(|c| check_timer_trigger(c, expired_timer_ids, started_timer_ids)),
        _ => false,
    }
}
