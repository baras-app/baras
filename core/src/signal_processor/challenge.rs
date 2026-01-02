//! Challenge/metrics tracking during boss encounters.
//!
//! Challenges track specific metrics during boss fights (e.g., "avoid all X attacks").
//! This module processes combat events through the challenge tracker to accumulate
//! metrics that are later evaluated to determine success/failure.

use crate::boss::EntityInfo;
use crate::combat_log::{CombatEvent, Entity, EntityType};
use crate::context::resolve;
use crate::game_data::{effect_id, effect_type_id};
use crate::state::SessionCache;

/// Process events through the challenge tracker to accumulate metrics.
/// Challenge data is polled with other combat metrics, not pushed via signals.
pub fn process_challenge_events(event: &CombatEvent, cache: &mut SessionCache) {
    // Get boss_npc_ids from encounter's tracker (need to extract before mutable borrow)
    let boss_npc_ids = match cache.current_encounter() {
        Some(enc) if enc.challenge_tracker.is_active() => {
            enc.challenge_tracker.boss_npc_ids().to_vec()
        }
        _ => return, // No active challenge tracking
    };

    // Build context from current encounter state (phase, counters, HP)
    let ctx = cache.current_encounter().unwrap().challenge_context(&boss_npc_ids);

    // Get local player ID for local_player matching
    let local_player_id = cache.player.id;

    // Convert entities to EntityInfo
    let source = entity_to_info(&event.source_entity, local_player_id);
    let target = entity_to_info(&event.target_entity, local_player_id);

    // Get mutable access to the encounter's tracker
    let Some(enc) = cache.current_encounter_mut() else { return };
    let tracker = &mut enc.challenge_tracker;

    // Process based on event type - just accumulate, no signals needed
    let timestamp = event.timestamp;
    match event.effect.effect_id {
        effect_id::DAMAGE => {
            let damage = event.details.dmg_effective as i64;
            tracker.process_damage(
                &ctx,
                &source,
                &target,
                event.action.action_id as u64,
                damage,
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
        effect_id::DEATH => {
            tracker.process_death(&ctx, &target, timestamp);
        }
        _ => {
            if event.effect.type_id == effect_type_id::APPLYEFFECT {
                tracker.process_effect_applied(
                    &ctx,
                    &source,
                    &target,
                    event.effect.effect_id as u64,
                    timestamp,
                );
            }
        }
    }
}

/// Convert a combat log Entity to EntityInfo for challenge matching.
pub fn entity_to_info(entity: &Entity, local_player_id: i64) -> EntityInfo {
    match entity.entity_type {
        EntityType::Player => EntityInfo {
            entity_id: entity.log_id,
            name: resolve(entity.name).to_string(),
            is_player: true,
            is_local_player: entity.log_id == local_player_id,
            npc_id: None,
        },
        EntityType::Npc | EntityType::Companion => EntityInfo {
            entity_id: entity.log_id,
            name: resolve(entity.name).to_string(),
            is_player: false,
            is_local_player: false,
            npc_id: Some(entity.class_id),
        },
        _ => EntityInfo::default(),
    }
}
