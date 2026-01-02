//! Combat state machine for encounter lifecycle management.
//!
//! The combat state machine tracks the lifecycle of encounters:
//! - NotStarted: Waiting for combat to begin
//! - InCombat: Active combat, accumulating data
//! - PostCombat: Combat ended, grace period for trailing damage
//!
//! This module handles transitions between states and emits CombatStarted/CombatEnded signals.

use chrono::NaiveDateTime;

use crate::combat_log::CombatEvent;
use crate::encounter::EncounterState;
use crate::game_data::{effect_id, effect_type_id};
use crate::state::SessionCache;

use super::GameSignal;

/// Timeout in seconds before combat ends due to inactivity.
pub const COMBAT_TIMEOUT_SECONDS: i64 = 60;

/// Advance the combat state machine and emit CombatStarted/CombatEnded signals.
pub fn advance_combat_state(
    event: CombatEvent,
    cache: &mut SessionCache,
    post_combat_threshold_ms: i64,
) -> Vec<GameSignal> {
    // Track effect applications/removals for shield absorption
    track_encounter_effects(&event, cache);

    let effect_id = event.effect.effect_id;
    let effect_type_id = event.effect.type_id;
    let timestamp = event.timestamp;

    let current_state = cache
        .current_encounter()
        .map(|e| e.state.clone())
        .unwrap_or_default();

    match current_state {
        EncounterState::NotStarted => {
            handle_not_started(event, cache, effect_id, timestamp)
        }
        EncounterState::InCombat => {
            handle_in_combat(event, cache, effect_id, effect_type_id, timestamp)
        }
        EncounterState::PostCombat { exit_time } => {
            handle_post_combat(event, cache, effect_id, timestamp, exit_time, post_combat_threshold_ms)
        }
    }
}

/// Track effect applications/removals in the encounter for shield absorption calculation.
fn track_encounter_effects(event: &CombatEvent, cache: &mut SessionCache) {
    use crate::combat_log::EntityType;

    let Some(enc) = cache.current_encounter_mut() else { return };

    match event.effect.type_id {
        effect_type_id::APPLYEFFECT if event.target_entity.entity_type != EntityType::Empty => {
            enc.apply_effect(event);
        }
        effect_type_id::REMOVEEFFECT if event.source_entity.entity_type != EntityType::Empty => {
            enc.remove_effect(event);
        }
        _ => {}
    }
}

fn handle_not_started(
    event: CombatEvent,
    cache: &mut SessionCache,
    effect_id: i64,
    timestamp: NaiveDateTime,
) -> Vec<GameSignal> {
    let mut signals = Vec::new();

    if effect_id == effect_id::ENTERCOMBAT {
        let store = cache.store_events;
        if let Some(enc) = cache.current_encounter_mut() {
            enc.state = EncounterState::InCombat;
            enc.enter_combat_time = Some(timestamp);
            enc.track_event_entities(&event);
            enc.accumulate_data(&event);
            if store { enc.events.push(event); }

            signals.push(GameSignal::CombatStarted {
                timestamp,
                encounter_id: enc.id,
            });
        }
    } else {
        // Buffer non-damage events for the upcoming encounter
        let store = cache.store_events;
        if let Some(enc) = cache.current_encounter_mut() {
            enc.accumulate_data(&event);
            if store { enc.events.push(event); }
        }
    }

    signals
}

fn handle_in_combat(
    event: CombatEvent,
    cache: &mut SessionCache,
    effect_id: i64,
    effect_type_id: i64,
    timestamp: NaiveDateTime,
) -> Vec<GameSignal> {
    let mut signals = Vec::new();

    // Check for combat timeout
    if let Some(enc) = cache.current_encounter() && let Some(last_activity) = enc.last_combat_activity_time {
            let elapsed = timestamp.signed_duration_since(last_activity).num_seconds();
            if elapsed >= COMBAT_TIMEOUT_SECONDS {
                let encounter_id = enc.id;
                // End combat at last_activity_time
                if let Some(enc) = cache.current_encounter_mut() {
                    enc.flush_pending_absorptions();
                    enc.exit_combat_time = Some(last_activity);
                    enc.state = EncounterState::PostCombat {
                        exit_time: last_activity,
                    };
                    let duration = enc.duration_seconds().unwrap_or(0) as f32;
                    enc.challenge_tracker.finalize(last_activity, duration);
                }

                signals.push(GameSignal::CombatEnded {
                    timestamp: last_activity,
                    encounter_id,
                });

                cache.push_new_encounter();
                // Re-process this event in the new encounter's state machine
                signals.extend(advance_combat_state(event, cache, 0));
                return signals;
            }
    }

    let all_players_dead = cache
        .current_encounter()
        .map(|e| e.all_players_dead)
        .unwrap_or(false);

    // Check if all kill targets are dead (boss encounter victory condition)
    let all_kill_targets_dead = cache.active_boss_idx.map_or(false, |idx| {
        let kill_target_ids: Vec<i64> = cache.boss_definitions[idx]
            .kill_targets()
            .flat_map(|e| e.ids.iter().copied())
            .collect();
        cache.boss_state.all_kill_targets_dead(&kill_target_ids)
    });

    if effect_id == effect_id::ENTERCOMBAT {
        // Unexpected EnterCombat while in combat - terminate and restart
        let encounter_id = cache.current_encounter().map(|e| e.id).unwrap_or(0);
        if let Some(enc) = cache.current_encounter_mut() {
            enc.flush_pending_absorptions();
            enc.exit_combat_time = Some(timestamp);
            enc.state = EncounterState::PostCombat {
                exit_time: timestamp,
            };
            let duration = enc.duration_seconds().unwrap_or(0) as f32;
            enc.challenge_tracker.finalize(timestamp, duration);
        }

        signals.push(GameSignal::CombatEnded {
            timestamp,
            encounter_id,
        });

        cache.push_new_encounter();
        signals.extend(advance_combat_state(event, cache, 0));
    } else if effect_id == effect_id::EXITCOMBAT || all_players_dead || all_kill_targets_dead {
        let encounter_id = cache.current_encounter().map(|e| e.id).unwrap_or(0);
        let store = cache.store_events;
        if let Some(enc) = cache.current_encounter_mut() {
            enc.flush_pending_absorptions();
            enc.exit_combat_time = Some(timestamp);
            enc.state = EncounterState::PostCombat {
                exit_time: timestamp,
            };
            if store { enc.events.push(event); }
            let duration = enc.duration_seconds().unwrap_or(0) as f32;
            enc.challenge_tracker.finalize(timestamp, duration);
        }

        signals.push(GameSignal::CombatEnded {
            timestamp,
            encounter_id,
        });
    } else if effect_type_id == effect_type_id::AREAENTERED {
        let encounter_id = cache.current_encounter().map(|e| e.id).unwrap_or(0);
        if let Some(enc) = cache.current_encounter_mut() {
            enc.flush_pending_absorptions();
            enc.exit_combat_time = Some(timestamp);
            enc.state = EncounterState::PostCombat {
                exit_time: timestamp,
            };
            let duration = enc.duration_seconds().unwrap_or(0) as f32;
            enc.challenge_tracker.finalize(timestamp, duration);
        }

        signals.push(GameSignal::CombatEnded {
            timestamp,
            encounter_id,
        });

        cache.push_new_encounter();
    } else {
        // Normal combat event
        let store = cache.store_events;
        if let Some(enc) = cache.current_encounter_mut() {
            enc.track_event_entities(&event);
            enc.accumulate_data(&event);
            if store { enc.events.push(event); }
            if effect_id == effect_id::DAMAGE || effect_id == effect_id::HEAL {
                enc.last_combat_activity_time = Some(timestamp);
            }
        }
    }

    signals
}

fn handle_post_combat(
    event: CombatEvent,
    cache: &mut SessionCache,
    effect_id: i64,
    timestamp: NaiveDateTime,
    exit_time: NaiveDateTime,
    post_combat_threshold_ms: i64,
) -> Vec<GameSignal> {
    let mut signals = Vec::new();

    if effect_id == effect_id::ENTERCOMBAT {
        // New combat starting
        let new_encounter_id = cache.push_new_encounter();
        let store = cache.store_events;
        if let Some(enc) = cache.current_encounter_mut() {
            enc.state = EncounterState::InCombat;
            enc.enter_combat_time = Some(timestamp);
            enc.accumulate_data(&event);
            if store { enc.events.push(event); }
        }

        signals.push(GameSignal::CombatStarted {
            timestamp,
            encounter_id: new_encounter_id,
        });
    } else if effect_id == effect_id::DAMAGE {
        let elapsed = timestamp
            .signed_duration_since(exit_time)
            .num_milliseconds();
        if elapsed <= post_combat_threshold_ms {
            // Trailing damage - assign to ending encounter
            let store = cache.store_events;
            if let Some(enc) = cache.current_encounter_mut() {
                enc.track_event_entities(&event);
                enc.accumulate_data(&event);
                if store { enc.events.push(event); }
            }
        } else {
            // Beyond grace period - discard and start fresh
            cache.push_new_encounter();
        }
    } else {
        // Non-damage event - goes to next encounter
        cache.push_new_encounter();
        let store = cache.store_events;
        if let Some(enc) = cache.current_encounter_mut() {
            enc.accumulate_data(&event);
            if store { enc.events.push(event); }
        }
    }

    signals
}
