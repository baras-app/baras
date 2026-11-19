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
use crate::game_data::effect_type_id;
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
    /// Last-known entity positions from the encounter (for position constraints)
    pub entity_positions: std::sync::Arc<hashbrown::HashMap<i64, crate::combat_log::Position>>,
    /// Faction map for PvP encounters (None in PvE); Arc-shared like positions
    pub pvp_factions: Option<std::sync::Arc<crate::encounter::PvpFactionMap>>,
}

impl FilterContext<'_> {
    fn position_of(&self, entity_id: i64) -> Option<crate::combat_log::Position> {
        self.entity_positions.get(&entity_id).copied()
    }
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
pub fn check_signal_trigger(trigger: &Trigger, signals: &[GameSignal], filter_ctx: &FilterContext) -> bool {
    match trigger {
        // ─── Combat State ──────────────────────────────────────────────────
        Trigger::CombatStart => signals.iter().any(|s| matches!(s, GameSignal::CombatStarted { .. })),

        Trigger::CombatEnd => signals.iter().any(|s| matches!(s, GameSignal::CombatEnded { .. })),

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
                entity_id,
                npc_id,
                entity_name,
                ..
            } = s
            {
                // Position constraints apply to the appearing NPC itself:
                // its position satisfies both source and target constraints.
                let npc_pos = filter_ctx.position_of(*entity_id);
                trigger.matches_npc_appears(filter_ctx.entities, *npc_id, entity_name)
                    && crate::dsl::triggers::matches_position_constraints(
                        trigger.position_constraints(),
                        npc_pos,
                        npc_pos,
                    )
            } else {
                false
            }
        }),

        Trigger::EntityDeath { .. } => signals.iter().any(|s| {
            if let GameSignal::EntityDeath {
                entity_id,
                npc_id,
                entity_name,
                ..
            } = s
            {
                // Position constraints apply to the dying entity itself:
                // its position satisfies both source and target constraints.
                let death_pos = filter_ctx.position_of(*entity_id);
                trigger.matches_entity_death(filter_ctx.entities, *npc_id, entity_name)
                    && crate::dsl::triggers::matches_position_constraints(
                        trigger.position_constraints(),
                        death_pos,
                        death_pos,
                    )
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

        Trigger::PhaseEnded { .. } => signals.iter().any(|s| {
            matches!(s, GameSignal::PhaseEndTriggered { phase_id, .. }
                if trigger.matches_phase_ended(phase_id))
        }),

        Trigger::AnyPhaseChange => signals.iter().any(|s| matches!(s, GameSignal::PhaseChanged { .. })),

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

        Trigger::CounterChanges { .. } => signals.iter().any(|s| {
            if let GameSignal::CounterChanged { counter_id, .. } = s {
                trigger.matches_counter_changes(counter_id)
            } else {
                false
            }
        }),

        // ─── Damage/Healing (signal-based with source/target filters) ──────
        Trigger::DamageTaken { .. } | Trigger::DamageDealt { .. } => signals.iter().any(|s| {
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
                defense_type_id,
                ..
            } = s
            {
                let ability_name_str = crate::context::resolve(*ability_name);

                if !trigger.matches_damage_taken(*ability_id as u64, Some(ability_name_str), *defense_type_id) {
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
                    filter_ctx.pvp_factions.as_deref(),
                    filter_ctx.position_of(*source_id),
                    filter_ctx.position_of(*target_id),
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
                    filter_ctx.pvp_factions.as_deref(),
                    filter_ctx.position_of(*source_id),
                    filter_ctx.position_of(*target_id),
                )
            } else {
                false
            }
        }),

        // ─── Ability Cast (signal-based with resolved target) ──────────────
        Trigger::AbilityCast { .. } => signals.iter().any(|s| {
            if let GameSignal::AbilityActivated {
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

                if !trigger.matches_ability(*ability_id as u64, Some(ability_name_str)) {
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
                    filter_ctx.pvp_factions.as_deref(),
                    filter_ctx.position_of(*source_id),
                    filter_ctx.position_of(*target_id),
                )
            } else {
                false
            }
        }),

        // ─── Threat Modified ───────────────────────────────────────────────
        Trigger::ThreatModified { .. } => signals.iter().any(|s| {
            if let GameSignal::ThreatModified {
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
                threat,
                ..
            } = s
            {
                let ability_name_str = crate::context::resolve(*ability_name);
                if !trigger.matches_threat_modified(*ability_id as u64, Some(ability_name_str), *threat) {
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
                    filter_ctx.pvp_factions.as_deref(),
                    filter_ctx.position_of(*source_id),
                    filter_ctx.position_of(*target_id),
                )
            } else {
                false
            }
        }),

        // ─── Timer triggers (handled by check_timer_trigger, not signals) ──
        Trigger::TimerExpires { .. } | Trigger::TimerStarted { .. } | Trigger::TimerCanceled { .. } => false,

        // ─── Event-based triggers (handled by check_event_trigger, not signals)
        Trigger::EffectApplied { .. } | Trigger::EffectRemoved { .. } | Trigger::TargetSet { .. } => false,

        // ─── Modifier-only triggers (handled by EffectTracker) ────────────
        Trigger::ChargesChanged { .. }
        | Trigger::SelfChargesChanged { .. }
        | Trigger::ResourceSpent { .. }
        | Trigger::KillingBlow => false,

        // ─── Not signal-based ──────────────────────────────────────────────
        Trigger::TimeElapsed { .. } | Trigger::Manual | Trigger::Never => false,

        // ─── Composition ───────────────────────────────────────────────────
        Trigger::AnyOf { conditions } => conditions.iter().any(|c| check_signal_trigger(c, signals, filter_ctx)),
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
/// EffectRemoved matches only the effect being removed, never the action that
/// removed it, so phases/counters/victory agree with the signal-based timer path.
pub fn check_event_trigger(trigger: &Trigger, event: &CombatEvent, filter_ctx: Option<&FilterContext>) -> bool {
    // Handle AnyOf composition first — recurse into each sub-condition so that
    // source/target filters on inner triggers are checked correctly.
    // This must be checked before the type-specific branches below, because
    // the matches_* methods recurse into AnyOf for ID matching but the
    // subsequent check_event_filters would use AnyOf's own filters (None),
    // silently bypassing source/target constraints on inner conditions.
    if let Trigger::AnyOf { conditions } = trigger {
        return conditions.iter().any(|c| check_event_trigger(c, event, filter_ctx));
    }

    // Check AbilityCast triggers from raw event.
    // Note: SWTOR logs self as target for most ability activations, so target
    // filtering is unreliable here. Source matching is always correct.
    if event.effect.effect_id == crate::game_data::effect_id::ABILITYACTIVATE {
        let action_id = event.action.action_id as u64;
        let action_name = crate::context::resolve(event.action.name);
        if trigger.matches_ability(action_id, Some(action_name)) && check_event_filters(trigger, event, filter_ctx) {
            return true;
        }
    }

    // Check DamageTaken triggers from raw event.
    if event.effect.type_id == crate::game_data::effect_type_id::APPLYEFFECT
        && event.effect.effect_id == crate::game_data::effect_id::DAMAGE
    {
        let action_id = event.action.action_id as u64;
        let action_name = crate::context::resolve(event.action.name);
        if trigger.matches_damage_taken(action_id, Some(action_name), event.details.defense_type_id)
            && check_event_filters(trigger, event, filter_ctx)
        {
            return true;
        }
    }

    // Check EffectApplied triggers
    if event.effect.type_id == effect_type_id::APPLYEFFECT {
        let eff_id = event.effect.effect_id as u64;
        let eff_name = crate::context::resolve(event.effect.effect_name);
        if trigger.matches_effect_applied(eff_id, Some(eff_name)) && check_event_filters(trigger, event, filter_ctx) {
            return true;
        }
    }

    // Check ThreatModified triggers from raw event (EVENT type with MODIFYTHREAT/TAUNT effect).
    if event.effect.type_id == effect_type_id::EVENT
        && (event.effect.effect_id == crate::game_data::effect_id::MODIFYTHREAT
            || event.effect.effect_id == crate::game_data::effect_id::TAUNT)
    {
        let action_id = event.action.action_id as u64;
        let action_name = crate::context::resolve(event.action.name);
        if trigger.matches_threat_modified(action_id, Some(action_name), event.details.threat)
            && check_event_filters(trigger, event, filter_ctx)
        {
            return true;
        }
    }

    // Check TargetSet triggers (player/NPC targeting an entity)
    if event.effect.effect_id == crate::game_data::effect_id::TARGETSET
        && matches!(trigger, Trigger::TargetSet { .. })
        && check_event_filters(trigger, event, filter_ctx)
    {
        return true;
    }

    // Check EffectRemoved triggers (the effect being removed only)
    if event.effect.type_id == effect_type_id::REMOVEEFFECT {
        let eff_id = event.effect.effect_id as u64;
        let eff_name = crate::context::resolve(event.effect.effect_name);
        if trigger.matches_effect_removed(eff_id, Some(eff_name)) && check_event_filters(trigger, event, filter_ctx) {
            return true;
        }
    }

    false
}

/// Check source/target filters for an event-based trigger.
/// Returns true (passes) if no filter context is available (backward compat).
fn check_event_filters(trigger: &Trigger, event: &CombatEvent, filter_ctx: Option<&FilterContext>) -> bool {
    // Position constraints read straight off the event; they apply even
    // without a filter context.
    if !crate::dsl::triggers::matches_position_constraints(
        trigger.position_constraints(),
        event.source_entity.position,
        event.target_entity.position,
    ) {
        return false;
    }

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
        ctx.pvp_factions.as_deref(),
        event.source_entity.position,
        event.target_entity.position,
    )
}

// ═══════════════════════════════════════════════════════════════════════════
// Timer-ID-based trigger evaluation
// ═══════════════════════════════════════════════════════════════════════════

/// Check if a trigger matches any expired, started, or canceled timer IDs.
/// Handles TimerExpires, TimerStarted, TimerCanceled, and AnyOf composition.
pub fn check_timer_trigger(
    trigger: &Trigger,
    expired_timer_ids: &[String],
    started_timer_ids: &[String],
    canceled_timer_ids: &[String],
) -> bool {
    match trigger {
        Trigger::TimerExpires { timer_id } => expired_timer_ids.contains(timer_id),
        Trigger::TimerStarted { timer_id } => started_timer_ids.contains(timer_id),
        Trigger::TimerCanceled { timer_id } => canceled_timer_ids.contains(timer_id),
        Trigger::AnyOf { conditions } => conditions
            .iter()
            .any(|c| check_timer_trigger(c, expired_timer_ids, started_timer_ids, canceled_timer_ids)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat_log::LogParser;

    // Real Dxun line: the "Dying Light" action removes the "Surging Flame" effect.
    const REMOVE_LINE: &str = "[00:00:20.292] [The Ultimate Hunter {4381150109761536}:25340001190499|(56.40,227.46,35.07,-185.82)|(105540/10553973)] [=] [Dying Light {4381085685252096}] [RemoveEffect {836045448945478}: Surging Flame {4381085685252434}]";
    const DYING_LIGHT_ACTION: u64 = 4381085685252096;
    const SURGING_FLAME_EFFECT: u64 = 4381085685252434;

    fn parse(line: &str) -> CombatEvent {
        LogParser::new(chrono::Local::now().naive_local())
            .parse_line(1, line)
            .expect("line should parse")
    }

    fn effect_removed(id: u64) -> Trigger {
        toml::from_str(&format!("type = \"effect_removed\"\neffects = [{id}]")).unwrap()
    }

    #[test]
    fn effect_removed_matches_removed_effect_only() {
        let event = parse(REMOVE_LINE);

        assert!(
            check_event_trigger(&effect_removed(SURGING_FLAME_EFFECT), &event, None),
            "should match the effect being removed"
        );
        // Regression: the event path used to also match the action that caused
        // the removal, making phase/victory triggers fire on "ability X removes
        // ANY effect" while the signal-based timer path did not.
        assert!(
            !check_event_trigger(&effect_removed(DYING_LIGHT_ACTION), &event, None),
            "must not match the action doing the removing"
        );
    }
}
