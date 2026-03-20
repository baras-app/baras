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

/// Grace period for boss encounters before finalizing combat end (seconds).
/// Allows merging fake combat splits (e.g., loot chest "enemies", Kephess SM walker).
const BOSS_COMBAT_EXIT_GRACE_SECS: i64 = 3;

/// Grace period for non-boss encounters before finalizing combat end (seconds).
const TRASH_COMBAT_EXIT_GRACE_SECS: i64 = 1;

/// Soft-timeout for wipe detection after local player receives revive immunity (seconds).
/// If local player receives RECENTLY_REVIVED during a boss encounter and kill targets
/// aren't dead after this timeout, mark the encounter as a wipe and end it.
const REVIVE_IMMUNITY_WIPE_TIMEOUT_SECS: i64 = 5;

/// Check if we're within the grace window after a combat exit.
/// Returns the grace duration if within window, None otherwise.
fn within_grace_window(cache: &SessionCache, timestamp: NaiveDateTime) -> bool {
    let Some(exit_time) = cache.last_combat_exit_time else {
        return false;
    };

    let grace_secs = if cache
        .current_encounter()
        .map_or(false, |e| e.active_boss_idx().is_some())
    {
        BOSS_COMBAT_EXIT_GRACE_SECS
    } else {
        TRASH_COMBAT_EXIT_GRACE_SECS
    };

    timestamp.signed_duration_since(exit_time).num_seconds() <= grace_secs
}

/// Advance the combat state machine and emit CombatStarted/CombatEnded signals.
/// Returns (signals, was_accumulated) where was_accumulated indicates whether
/// the event was added to accumulated_data (for parquet write filtering).
pub fn advance_combat_state(
    event: &CombatEvent,
    cache: &mut SessionCache,
) -> (Vec<GameSignal>, bool) {
    // Track effect applications/removals for shield absorption
    track_encounter_effects(event, cache);

    let effect_id = event.effect.effect_id;
    let effect_type_id = event.effect.type_id;
    let timestamp = event.timestamp;

    let current_state = cache
        .current_encounter()
        .map(|e| e.state.clone())
        .unwrap_or_default();

    match current_state {
        EncounterState::NotStarted => handle_not_started(event, cache, effect_id, timestamp),
        EncounterState::InCombat => {
            handle_in_combat(event, cache, effect_id, effect_type_id, timestamp)
        }
        EncounterState::PostCombat { .. } => handle_post_combat(event, cache, effect_id, timestamp),
    }
}

/// Track effect applications/removals in the encounter for shield absorption calculation.
fn track_encounter_effects(event: &CombatEvent, cache: &mut SessionCache) {
    use crate::combat_log::EntityType;

    let Some(enc) = cache.current_encounter_mut() else {
        return;
    };

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
    event: &CombatEvent,
    cache: &mut SessionCache,
    effect_id: i64,
    timestamp: NaiveDateTime,
) -> (Vec<GameSignal>, bool) {
    let mut signals = Vec::new();
    let mut was_accumulated = false;

    if effect_id == effect_id::ENTERCOMBAT {
        if let Some(enc) = cache.current_encounter_mut() {
            enc.state = EncounterState::InCombat;
            enc.enter_combat_time = Some(timestamp);
            enc.track_event_entities(event);
            enc.accumulate_data(event);
            enc.track_event_line(event.line_number);
            was_accumulated = true;

            signals.push(GameSignal::CombatStarted {
                timestamp,
                encounter_id: enc.id,
            });
        }
    } else if effect_id != effect_id::DAMAGE {
        // Buffer non-damage events for the upcoming encounter (skip pre-combat damage)
        if let Some(enc) = cache.current_encounter_mut() {
            enc.accumulate_data(event);
            enc.track_event_line(event.line_number);
            was_accumulated = true;
        }
    }

    (signals, was_accumulated)
}

fn handle_in_combat(
    event: &CombatEvent,
    cache: &mut SessionCache,
    effect_id: i64,
    effect_type_id: i64,
    timestamp: NaiveDateTime,
) -> (Vec<GameSignal>, bool) {
    let mut signals = Vec::new();
    let mut was_accumulated = false;

    // Check for combat timeout
    // Skip timeout for victory-trigger encounters (e.g., Coratanni has long phases with no activity)
    // Uses difficulty-aware check: Trandos on Story/Veteran has no victory trigger, so normal timeout applies
    let has_victory_trigger = cache
        .current_encounter()
        .map_or(false, |enc| enc.has_active_victory_trigger());

    if !has_victory_trigger
        && let Some(enc) = cache.current_encounter()
        && let Some(last_activity) = enc.last_damage_time
    {
        let elapsed = timestamp.signed_duration_since(last_activity).num_seconds();
        if elapsed >= COMBAT_TIMEOUT_SECONDS {
            let encounter_id = enc.id;
            // End combat at last_activity_time
            if let Some(enc) = cache.current_encounter_mut() {
                enc.exit_combat_time = Some(last_activity);
                enc.state = EncounterState::PostCombat {
                    exit_time: last_activity,
                };
                let duration = enc.duration_seconds(None).unwrap_or(0) as f32;
                enc.challenge_tracker.finalize(last_activity, duration);
            }

            tracing::info!(
                "[ENCOUNTER] Combat timeout at {}, ending encounter {}",
                last_activity,
                encounter_id
            );

            signals.push(GameSignal::CombatEnded {
                timestamp: last_activity,
                encounter_id,
            });

            cache.push_new_encounter();
            // Re-process this event in the new encounter's state machine
            let (new_signals, new_accumulated) = advance_combat_state(event, cache);
            signals.extend(new_signals);
            return (signals, new_accumulated);
        }
    }

    // Check for revive immunity timeout (event-driven, works in Historical mode)
    // If local player received RECENTLY_REVIVED and 5+ seconds have passed (by event time),
    // end the encounter. This handles speedrun scenarios where players die to trash,
    // revive at medcenter, and quick-travel to the boss room.
    if let Some(enc) = cache.current_encounter() {
        if let Some(revive_time) = enc.local_player_revive_immunity_time {
            let elapsed = timestamp.signed_duration_since(revive_time).num_seconds();
            if elapsed >= REVIVE_IMMUNITY_WIPE_TIMEOUT_SECS {
                let is_boss_encounter = enc.active_boss_idx().is_some();
                // For boss encounters, check if kill targets are still alive (wipe)
                // For trash encounters, always end (no kill targets to check)
                // For victory-trigger bosses where victory hasn't fired: medcenter revive
                // is definitionally a wipe — the only success path is the victory trigger,
                // so if the player is reviving at medcenter it can only mean a wipe.
                let should_end = if is_boss_encounter {
                    enc.is_likely_wipe()
                        || (enc.has_active_victory_trigger() && !enc.victory_triggered)
                } else {
                    true // Always end trash encounters after revive timeout
                };

                if should_end {
                    let encounter_id = enc.id;

                    tracing::info!(
                        "[ENCOUNTER] Revive immunity timeout at {} (revive was at {}), ending encounter {} (is_boss: {})",
                        timestamp,
                        revive_time,
                        encounter_id,
                        is_boss_encounter
                    );

                    // End encounter and mark as wipe
                    if let Some(enc) = cache.current_encounter_mut() {
                        enc.all_players_dead = true; // Force wipe flag
                        enc.exit_combat_time = Some(revive_time);
                        enc.state = EncounterState::PostCombat {
                            exit_time: revive_time,
                        };
                        let duration = enc.duration_seconds(None).unwrap_or(0) as f32;
                        enc.challenge_tracker.finalize(revive_time, duration);
                    }

                    signals.push(GameSignal::CombatEnded {
                        timestamp: revive_time,
                        encounter_id,
                    });

                    cache.push_new_encounter();
                    // Re-process this event in the new encounter's state machine
                    let (new_signals, new_accumulated) = advance_combat_state(event, cache);
                    signals.extend(new_signals);
                    return (signals, new_accumulated);
                }
            }
        }
    }

    // OOC revive detected (local player revived without a battle rez)
    let local_player_ooc_revived = cache
        .current_encounter()
        .and_then(|enc| enc.local_player_ooc_revive_time)
        .is_some();

    let all_players_dead = cache
        .current_encounter()
        .map(|e| e.all_players_dead)
        .unwrap_or(false);

    // Check if local player received the post-death revive immortality buff
    // This means they clicked revive and are now out of combat with a grace period
    let local_player_revived = effect_type_id == effect_type_id::APPLYEFFECT
        && effect_id == effect_id::RECENTLY_REVIVED
        && cache.player_initialized
        && event.source_entity.log_id == cache.player.id;

    // Check if all kill targets are dead (boss encounter victory condition)
    // We check all NPC INSTANCES that match kill target class_ids
    let all_kill_targets_dead = cache.current_encounter().map_or(false, |enc| {
        let Some(def_idx) = enc.active_boss_idx() else {
            return false;
        };

        // Collect all kill target class IDs from the boss definition
        let kill_target_class_ids: std::collections::HashSet<i64> = enc.boss_definitions()[def_idx]
            .kill_targets()
            .flat_map(|e| e.ids.iter().copied())
            .collect();

        if kill_target_class_ids.is_empty() {
            return false;
        }

        // Find all NPC instances that are kill targets (by class_id)
        let kill_target_instances: Vec<_> = enc
            .npcs
            .values()
            .filter(|npc| kill_target_class_ids.contains(&npc.class_id))
            .collect();

        // Must have seen at least one kill target instance
        if kill_target_instances.is_empty() {
            return false;
        }

        // All seen kill target instances must be dead
        // Also consider dead if HP <= 0 (handles game race condition where death event is never logged)
        kill_target_instances
            .iter()
            .all(|npc| npc.is_dead || npc.current_hp <= 0)
    });

    // Check if this is a boss encounter (has boss definitions loaded OR boss NPCs detected)
    // For boss encounters, we don't want to end on local_player_revived because SWTOR
    // log buffering can cause RECENTLY_REVIVED to arrive before other players' DEATH events
    let is_boss_encounter = cache.current_encounter().map_or(false, |enc| {
        // Has boss definitions loaded for this area
        !enc.boss_definitions().is_empty()
        // OR has detected any boss NPCs in the encounter
        || enc.npcs.values().any(|npc| npc.is_boss)
    });

    // Only end non-boss encounters on local_player_revived
    // For boss fights, rely on all_players_dead or all_kill_targets_dead
    let should_end_on_local_revive = local_player_revived && !is_boss_encounter;

    // Check if this is a victory-trigger encounter that hasn't triggered yet
    // If so, ignore ExitCombat events until the victory trigger fires
    // Difficulty-aware: Trandos on Story/Veteran uses normal combat flow
    let should_ignore_exit_combat = cache.current_encounter().map_or(false, |enc| {
        enc.has_active_victory_trigger() && !enc.victory_triggered
    });

    if effect_id == effect_id::ENTERCOMBAT {
        // Check if this is a victory-trigger encounter
        // These are special long encounters (e.g., Coratanni) where players can legitimately
        // die, medcenter, and run back while the raid continues fighting
        // Difficulty-aware: Trandos on Story/Veteran uses normal combat flow
        let has_victory_trigger = cache
            .current_encounter()
            .map_or(false, |enc| enc.has_active_victory_trigger());

        if has_victory_trigger {
            // Victory-trigger encounters: always ignore EnterCombat (treated as rejoin).
            // Wipes on these encounters are split earlier via the revive immunity timeout
            // path, so by the time a new EnterCombat fires the encounter is already split.
            tracing::info!(
                "[ENCOUNTER] EnterCombat during InCombat at {} - victory-trigger encounter, ignoring (rejoin)",
                timestamp
            );
            return (signals, was_accumulated);
        }

        // Normal encounters: check if local player has received revive immunity (medcenter/probe revive)
        // If so, this is a new pull after a wipe, not a battle-rez rejoin
        let player_info = cache
            .current_encounter()
            .and_then(|enc| enc.players.get(&cache.player.id));

        let local_player_has_revive_immunity = player_info
            .map(|p| p.received_revive_immunity)
            .unwrap_or(false);

        tracing::info!(
            "[ENCOUNTER] EnterCombat during InCombat at {} - player_id: {}, player_found: {}, revive_immunity: {}",
            timestamp,
            cache.player.id,
            player_info.is_some(),
            local_player_has_revive_immunity
        );

        if local_player_has_revive_immunity {
            // Local player died, used medcenter, and is now re-entering combat
            // End the current encounter (mark as wipe) and start a new one
            let encounter_id = cache.current_encounter().map(|e| e.id).unwrap_or(0);

            tracing::info!(
                "[ENCOUNTER] Local player re-entering combat after medcenter revive at {}, ending encounter {} (wipe)",
                timestamp,
                encounter_id
            );

            if let Some(enc) = cache.current_encounter_mut() {
                enc.exit_combat_time = Some(timestamp);
                enc.state = EncounterState::PostCombat {
                    exit_time: timestamp,
                };
                let duration = enc.duration_seconds(None).unwrap_or(0) as f32;
                enc.challenge_tracker.finalize(timestamp, duration);
            }

            signals.push(GameSignal::CombatEnded {
                timestamp,
                encounter_id,
            });

            // Create new encounter and immediately start it with this EnterCombat event
            let new_encounter_id = cache.push_new_encounter();
            if let Some(enc) = cache.current_encounter_mut() {
                enc.state = EncounterState::InCombat;
                enc.enter_combat_time = Some(timestamp);
                enc.track_event_entities(event);
                enc.accumulate_data(event);
                enc.track_event_line(event.line_number);
                was_accumulated = true;
            }

            signals.push(GameSignal::CombatStarted {
                timestamp,
                encounter_id: new_encounter_id,
            });

            return (signals, was_accumulated);
        }

        // Normal case: battle rez rejoin - ignore this EnterCombat
        // ENTERCOMBAT only fires for local player, so this is always a rejoin scenario
        return (signals, was_accumulated);
    } else if effect_type_id == effect_type_id::AREAENTERED {
        // AreaEntered ALWAYS ends the encounter - player left the combat area
        // This takes priority over victory-trigger logic (can't continue a fight you left)
        let encounter_id = cache.current_encounter().map(|e| e.id).unwrap_or(0);
        tracing::info!(
            "[ENCOUNTER] AREAENTERED at {}, ending encounter {}",
            timestamp,
            encounter_id
        );
        if let Some(enc) = cache.current_encounter_mut() {
            // Area exit during combat = wipe unless victory trigger already fired
            // (medcentered, left instance, disconnected, etc.)
            if !enc.victory_triggered {
                enc.all_players_dead = true;
            }

            enc.exit_combat_time = Some(timestamp);
            enc.state = EncounterState::PostCombat {
                exit_time: timestamp,
            };
            let duration = enc.duration_seconds(None).unwrap_or(0) as f32;
            enc.challenge_tracker.finalize(timestamp, duration);
        }

        signals.push(GameSignal::CombatEnded {
            timestamp,
            encounter_id,
        });

        cache.push_new_encounter();
        return (signals, was_accumulated);
    } else if should_ignore_exit_combat {
        // For victory-trigger encounters, ignore all exit conditions except all_players_dead (wipe)
        if !all_players_dead {
            // Ignore all other exit conditions (ExitCombat, kill targets, local revive, etc.)
            if let Some(enc) = cache.current_encounter_mut() {
                enc.track_event_entities(event);
                enc.accumulate_data(event);
                enc.track_event_line(event.line_number);
                was_accumulated = true;
            }
            return (signals, was_accumulated); // Don't process further
        }
        // If all_players_dead, fall through to normal exit handling (wipe)
    }

    if all_players_dead
        || effect_id == effect_id::EXITCOMBAT
        || all_kill_targets_dead
        || should_end_on_local_revive
        || local_player_ooc_revived
    {
        // Check if we're within a grace window from a previous exit
        // If so, this is the "real" exit after a fake enter (holocron case)
        if within_grace_window(cache, timestamp) {
            let exit_time = cache.last_combat_exit_time.unwrap();
            let encounter_id = cache.current_encounter().map(|e| e.id).unwrap_or(0);

            if let Some(enc) = cache.current_encounter_mut() {
                enc.exit_combat_time = Some(exit_time);
                enc.state = EncounterState::PostCombat { exit_time };
                let duration = enc.duration_seconds(None).unwrap_or(0) as f32;
                enc.challenge_tracker.finalize(exit_time, duration);
            }

            tracing::info!(
                "[COMBAT-STATE] Ending encounter {} at {} (within grace window)",
                encounter_id,
                exit_time
            );

            signals.push(GameSignal::CombatEnded {
                timestamp: exit_time,
                encounter_id,
            });

            cache.last_combat_exit_time = None;
            cache.push_new_encounter();
        } else {
            // Start grace window - don't emit CombatEnded yet
            tracing::info!(
                "[ENCOUNTER] Starting grace window at {}, encounter {}",
                timestamp,
                cache.current_encounter().map(|e| e.id).unwrap_or(0)
            );
            cache.last_combat_exit_time = Some(timestamp);

            if let Some(enc) = cache.current_encounter_mut() {
                enc.exit_combat_time = Some(timestamp);
                enc.state = EncounterState::PostCombat {
                    exit_time: timestamp,
                };
                let duration = enc.duration_seconds(None).unwrap_or(0) as f32;
                enc.challenge_tracker.finalize(timestamp, duration);
            }
            // Note: Don't emit CombatEnded or push_new_encounter yet
        }
    } else {
        // Normal combat event
        if let Some(enc) = cache.current_encounter_mut() {
            enc.track_event_entities(event);
            enc.accumulate_data(event);
            enc.track_event_line(event.line_number);
            was_accumulated = true;
            if effect_id == effect_id::DAMAGE {
                enc.last_damage_time = Some(timestamp);
            }
        }
    }

    (signals, was_accumulated)
}

fn handle_post_combat(
    event: &CombatEvent,
    cache: &mut SessionCache,
    effect_id: i64,
    timestamp: NaiveDateTime,
) -> (Vec<GameSignal>, bool) {
    let mut signals = Vec::new();
    let mut was_accumulated = false;

    // During grace window, only respond to ENTERCOMBAT (to restore combat)
    // All other events are buffered/ignored until grace expires
    let in_grace_window = within_grace_window(cache, timestamp);

    if effect_id == effect_id::ENTERCOMBAT {
        if in_grace_window {
            // Restore encounter to InCombat - this "corrects" the fake exit
            if let Some(enc) = cache.current_encounter_mut() {
                enc.state = EncounterState::InCombat;
                enc.exit_combat_time = None;
                // Track line number - grace period events are part of this encounter
                enc.track_event_line(event.line_number);
            }
            // Keep last_combat_exit_time set - we'll use it if another exit comes quickly
            // Don't emit any signals - combat "continues"
            // Note: was_accumulated remains false - grace window events not accumulated
        } else {
            // Outside grace window - finalize previous encounter and start new
            finalize_pending_combat_exit(cache, &mut signals);

            let new_encounter_id = cache.push_new_encounter();
            if let Some(enc) = cache.current_encounter_mut() {
                enc.state = EncounterState::InCombat;
                enc.enter_combat_time = Some(timestamp);
                enc.accumulate_data(event);
                enc.track_event_line(event.line_number);
                was_accumulated = true;
            }

            signals.push(GameSignal::CombatStarted {
                timestamp,
                encounter_id: new_encounter_id,
            });
        }
    } else if in_grace_window {
        // Grace window events belong to this encounter — accumulate them so trailing
        // death/damage events are written to parquet (e.g. NPC deaths after ExitCombat)
        if let Some(enc) = cache.current_encounter_mut() {
            enc.accumulate_data(event);
            enc.track_event_line(event.line_number);
            was_accumulated = true;
        }
    } else if effect_id == effect_id::DAMAGE {
        // Discard post-combat damage - start fresh encounter
        finalize_pending_combat_exit(cache, &mut signals);
        cache.push_new_encounter();
        // was_accumulated remains false - damage discarded
    } else {
        // Non-damage event - goes to next encounter
        finalize_pending_combat_exit(cache, &mut signals);
        cache.push_new_encounter();
        if let Some(enc) = cache.current_encounter_mut() {
            enc.accumulate_data(event);
            enc.track_event_line(event.line_number);
            was_accumulated = true;
        }
    }

    (signals, was_accumulated)
}

/// Finalize any pending combat exit (emit CombatEnded if grace window was active).
fn finalize_pending_combat_exit(cache: &mut SessionCache, signals: &mut Vec<GameSignal>) {
    if let Some(exit_time) = cache.last_combat_exit_time.take() {
        let encounter_id = cache.current_encounter().map(|e| e.id).unwrap_or(0);
        signals.push(GameSignal::CombatEnded {
            timestamp: exit_time,
            encounter_id,
        });
    }
}

/// Tick the combat state machine using wall-clock time.
///
/// This provides a fallback timeout when the event stream stops (e.g., player dies
/// and revives but no new combat events arrive). Called periodically from the tail loop.
///
/// Returns CombatEnded signal if combat times out due to inactivity.
/// Also handles grace window expiration for combat exit.
///
/// `now` should be the interpolated game time (game-clock anchored to
/// monotonic `Instant`). This avoids comparing the system clock against
/// SWTOR's game timestamps, which can differ by tens of seconds on
/// machines with clock skew.
pub fn tick_combat_state(cache: &mut SessionCache, now: NaiveDateTime) -> Vec<GameSignal> {
    let mut signals = Vec::new();

    let current_state = cache
        .current_encounter()
        .map(|e| e.state.clone())
        .unwrap_or_default();

    // Check for OOC revive (wall-clock fallback for when no new events arrive after the revive)
    // Start a grace window so trailing death events can still arrive and inform wipe classification
    if let Some(enc) = cache.current_encounter()
        && enc.state == EncounterState::InCombat
        && let Some(revive_time) = enc.local_player_ooc_revive_time
        && cache.last_combat_exit_time.is_none()
    {
        tracing::info!(
            "[ENCOUNTER] OOC revive in tick at {}, starting grace window for encounter {}",
            revive_time,
            enc.id
        );

        if let Some(enc) = cache.current_encounter_mut() {
            enc.exit_combat_time = Some(revive_time);
            enc.state = EncounterState::PostCombat {
                exit_time: revive_time,
            };
            let duration = enc.duration_seconds(None).unwrap_or(0) as f32;
            enc.challenge_tracker.finalize(revive_time, duration);
        }

        cache.last_combat_exit_time = Some(revive_time);
        // Don't emit CombatEnded yet — grace window will finalize
        // (handled by the existing grace window expiration logic below)
    }

    // Check for revive immunity soft-timeout
    // If local player received RECENTLY_REVIVED and timeout has elapsed, end the encounter.
    // For boss encounters: only end if kill targets aren't dead (is_likely_wipe)
    // For trash encounters: always end after timeout (player died and revived = fight over for them)
    if let Some(enc) = cache.current_encounter()
        && enc.state == EncounterState::InCombat
        && let Some(revive_time) = enc.local_player_revive_immunity_time
    {
        let elapsed = now.signed_duration_since(revive_time).num_seconds();
        if elapsed >= REVIVE_IMMUNITY_WIPE_TIMEOUT_SECS {
            let is_boss_encounter = enc.active_boss_idx().is_some();
            // For boss encounters, check if kill targets are still alive (wipe)
            // For trash encounters, always end (no kill targets to check)
            let should_end = if is_boss_encounter {
                enc.is_likely_wipe()
            } else {
                true // Always end trash encounters after revive timeout
            };

            if should_end {
                let encounter_id = enc.id;

                tracing::info!(
                    "[ENCOUNTER] Revive immunity soft-timeout at {}, ending encounter {} (wipe, is_boss: {})",
                    now,
                    encounter_id,
                    is_boss_encounter
                );

                // End encounter and mark as wipe
                if let Some(enc) = cache.current_encounter_mut() {
                    enc.all_players_dead = true; // Force wipe flag
                    enc.exit_combat_time = Some(revive_time);
                    enc.state = EncounterState::PostCombat {
                        exit_time: revive_time,
                    };
                    let duration = enc.duration_seconds(None).unwrap_or(0) as f32;
                    enc.challenge_tracker.finalize(revive_time, duration);
                }

                cache.last_combat_exit_time = None;
                signals.push(GameSignal::CombatEnded {
                    timestamp: revive_time,
                    encounter_id,
                });
                cache.push_new_encounter();

                return signals;
            }
        }
    }

    // Check for grace window expiration
    if let Some(exit_time) = cache.last_combat_exit_time {
        let grace_secs = if cache
            .current_encounter()
            .map_or(false, |e| e.active_boss_idx().is_some())
        {
            BOSS_COMBAT_EXIT_GRACE_SECS
        } else {
            TRASH_COMBAT_EXIT_GRACE_SECS
        };

        let elapsed = now.signed_duration_since(exit_time).num_seconds();
        if elapsed > grace_secs {
            match current_state {
                EncounterState::PostCombat { .. } => {
                    // Grace expired while in PostCombat - finalize the encounter
                    let encounter_id = cache.current_encounter().map(|e| e.id).unwrap_or(0);
                    signals.push(GameSignal::CombatEnded {
                        timestamp: exit_time,
                        encounter_id,
                    });

                    cache.last_combat_exit_time = None;
                    cache.push_new_encounter();
                }
                EncounterState::InCombat => {
                    // Grace expired while back in InCombat - Kephess case
                    // The fake exit was corrected, just clear the grace window
                    cache.last_combat_exit_time = None;
                }
                _ => {
                    cache.last_combat_exit_time = None;
                }
            }
            return signals;
        }
    }

    // Only check combat timeout during active combat
    if !matches!(current_state, EncounterState::InCombat) {
        return signals;
    }

    // Check wall-clock timeout
    // Skip timeout for victory-trigger encounters (e.g., Coratanni has long phases with no activity)
    let has_victory_trigger = cache
        .current_encounter()
        .map_or(false, |enc| enc.has_active_victory_trigger());

    if !has_victory_trigger
        && let Some(enc) = cache.current_encounter()
        && let Some(last_activity) = enc.last_damage_time
    {
        let elapsed = now.signed_duration_since(last_activity).num_seconds();
        if elapsed >= COMBAT_TIMEOUT_SECONDS {
            let encounter_id = enc.id;

            // End combat at last_activity_time (same as event-driven timeout)
            if let Some(enc) = cache.current_encounter_mut() {
                enc.exit_combat_time = Some(last_activity);
                enc.state = EncounterState::PostCombat {
                    exit_time: last_activity,
                };
                let duration = enc.duration_seconds(None).unwrap_or(0) as f32;
                enc.challenge_tracker.finalize(last_activity, duration);
            }

            cache.last_combat_exit_time = None;
            cache.push_new_encounter();

            return vec![GameSignal::CombatEnded {
                timestamp: last_activity,
                encounter_id,
            }];
        }
    }

    signals
}
