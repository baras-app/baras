//! Challenge/metrics tracking during boss encounters.
//!
//! Challenges track specific metrics during boss fights (e.g., "avoid all X attacks").
//! This module processes combat events through the challenge tracker to accumulate
//! metrics that are later evaluated to determine success/failure.

use crate::combat_log::{CombatEvent, Entity, EntityType};
use crate::context::resolve;
use crate::dsl::{ChallengeContext, EntityInfo};
use crate::game_data::{effect_id, effect_type_id};
use crate::state::SessionCache;

/// Process events through the challenge tracker to accumulate metrics.
/// Challenge data is polled with other combat metrics, not pushed via signals.
pub fn process_challenge_events(event: &CombatEvent, cache: &mut SessionCache) {
    // Get local player ID and current target for filter matching
    let local_player_id = cache.player.id;
    let Some(enc) = cache.current_encounter_mut() else {
        return;
    };
    if !enc.challenge_tracker.is_active() {
        return;
    }
    let current_target_id = enc.get_current_target(local_player_id);

    // Convert entities to EntityInfo
    let source = entity_to_info(&event.source_entity, local_player_id, current_target_id);
    let target = entity_to_info(&event.target_entity, local_player_id, current_target_id);

    // Borrow the context from encounter state (phase, counters, HP); the
    // tracker is a disjoint field, so it can be mutated alongside
    let boss_npc_ids = enc.challenge_tracker.boss_npc_ids();
    let ctx = ChallengeContext {
        current_phase: enc.current_phase.as_deref(),
        counters: &enc.counters,
        npcs: &enc.npcs,
        boss_npc_ids: &boss_npc_ids,
    };
    let tracker = &mut enc.challenge_tracker;

    // Process based on event type - just accumulate, no signals needed
    let timestamp = event.timestamp;
    match event.effect.effect_id {
        effect_id::DAMAGE => {
            let damage = event.details.dmg_amount as i64;
            let absorbed = event.details.dmg_absorbed as i64;
            tracker.process_damage(
                &ctx,
                &source,
                &target,
                event.action.action_id as u64,
                damage,
                absorbed,
                timestamp,
            );
        }
        effect_id::HEAL => {
            let healing = event.details.heal_amount as i64;
            let effective_healing = event.details.heal_effective as i64;
            tracker.process_healing(
                &ctx,
                &source,
                &target,
                event.action.action_id as u64,
                healing,
                effective_healing,
                timestamp,
            );
        }
        effect_id::ABILITYACTIVATE => {
            tracker.process_ability(
                &ctx,
                &source,
                &target,
                event.action.action_id as u64,
                timestamp,
            );
        }
        effect_id::ABILITYINTERRUPT => {
            tracker.process_interrupt(
                &ctx,
                &source,
                event.action.action_id as u64,
                timestamp,
            );
        }
        _ => {
            if event.effect.type_id == effect_type_id::APPLYEFFECT {
                tracker.process_effect_applied(
                    &ctx,
                    &source,
                    &target,
                    event.effect.effect_id as u64,
                    event.details.charges,
                    false,
                    timestamp,
                );
            } else if event.effect.type_id == effect_type_id::MODIFYCHARGES {
                tracker.process_effect_applied(
                    &ctx,
                    &source,
                    &target,
                    event.effect.effect_id as u64,
                    event.details.charges,
                    true,
                    timestamp,
                );
            } else if event.effect.type_id == effect_type_id::REMOVEEFFECT {
                tracker.process_effect_removed(
                    &ctx,
                    &source,
                    &target,
                    event.effect.effect_id as u64,
                    timestamp,
                );
            }
        }
    }

    // Update condition-active duration tracking (BossHpRange, Counter scoping)
    tracker.update_condition_tracking(&ctx, timestamp);
}

/// Convert a combat log Entity to EntityInfo for challenge matching.
pub fn entity_to_info(
    entity: &Entity,
    local_player_id: i64,
    current_target_id: Option<i64>,
) -> EntityInfo {
    let is_current_target = current_target_id == Some(entity.log_id);
    match entity.entity_type {
        EntityType::Player => EntityInfo {
            entity_id: entity.log_id,
            name: resolve(entity.name).to_string(),
            is_player: true,
            is_local_player: entity.log_id == local_player_id,
            is_current_target,
            npc_id: None,
        },
        EntityType::Npc | EntityType::Companion => EntityInfo {
            entity_id: entity.log_id,
            name: resolve(entity.name).to_string(),
            is_player: false,
            is_local_player: false,
            is_current_target,
            npc_id: Some(entity.class_id),
        },
        _ => EntityInfo::default(),
    }
}
