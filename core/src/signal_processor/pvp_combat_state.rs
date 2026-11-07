//! PvP combat state handling.
//!
//! PvP instances ignore all PvE encounter boundary logic — inactivity timeouts,
//! revive-immunity splits, wipe detection, and ExitCombat ends don't apply.
//! `combat_state::advance_combat_state` dispatches here for InCombat encounters
//! whose area is a PvP instance.

use chrono::NaiveDateTime;

use crate::combat_log::CombatEvent;
use crate::encounter::EncounterState;
use crate::game_data::{ARENA_ROUND_END_ABILITY_IDS, REBIRTH_ABILITY_ID, effect_id, effect_type_id};
use crate::state::SessionCache;

use super::GameSignal;

/// Advance an InCombat warzone encounter.
///
/// A warzone match is one contiguous encounter: EnterCombat/ExitCombat from
/// respawns and rejoins never split it. The primary boundary is the "Rebirth"
/// full heal the game fires at every player the instant the match ends.
/// Zoning out (AreaEntered with a different area ID) remains as a fallback
/// for quits/disconnects; the encounter end is then backdated to the final
/// ExitCombat seen, so scoreboard idle time isn't counted.
pub fn handle_in_combat_warzone(
    event: &CombatEvent,
    cache: &mut SessionCache,
) -> (Vec<GameSignal>, bool) {
    let mut signals = Vec::new();
    let timestamp = event.timestamp;

    let match_ended = event.effect.effect_id == effect_id::HEAL
        && event.action.action_id == REBIRTH_ABILITY_ID;
    if match_ended {
        end_pvp_match(cache, &mut signals, timestamp, true, "Warzone match ended");
        return (signals, false);
    }

    if zoned_out(event, cache) {
        let exit_time = cache
            .current_encounter()
            .and_then(|e| e.last_exit_combat_time)
            .unwrap_or(timestamp);
        end_pvp_match(cache, &mut signals, exit_time, true, "Zoned out of warzone");
        return (signals, false);
    }

    let was_accumulated = accumulate_match_event(event, cache);
    (signals, was_accumulated)
}

/// Advance an InCombat arena encounter (best-of-3 rounds).
///
/// Each round is its own encounter. Stealth-outs and combat drops mid-round
/// never split it; the boundary is the round-end heal burst: the game heals
/// every player to full via "Heal Target" the instant a round ends (with
/// "Rebirth" firing first when the final round ends the match). The first
/// such heal ends the round; the rest of the burst lands in the next
/// encounter's pre-combat buffer like any between-round event. The next
/// round starts on its EnterCombat as usual.
pub fn handle_in_combat_arena(
    event: &CombatEvent,
    cache: &mut SessionCache,
) -> (Vec<GameSignal>, bool) {
    let mut signals = Vec::new();
    let timestamp = event.timestamp;

    // Round success from faction deaths: only a friendly-team wipe is a
    // failure; a win or an undetermined round both report success.
    let round_success = || {
        cache
            .current_encounter()
            .map_or(true, |e| e.arena_outcome != Some(crate::encounter::PvpOutcome::Loss))
    };

    // Require damage before honoring a round-end heal: arenas also fire a
    // "Heal Target" prep burst ~30s before the barrier drops, and a local
    // player backfilling between rounds starts the encounter (via Deserter
    // Detection) before that burst lands. A round that hasn't seen damage
    // can't have ended.
    let round_ended = event.effect.effect_id == effect_id::HEAL
        && ARENA_ROUND_END_ABILITY_IDS.contains(&event.action.action_id)
        && cache
            .current_encounter()
            .is_some_and(|e| e.last_damage_time.is_some());
    if round_ended {
        let success = round_success();
        if event.action.action_id == REBIRTH_ABILITY_ID
            && let Some(enc) = cache.current_encounter_mut()
        {
            enc.pvp_match_end = true;
        }
        end_pvp_match(cache, &mut signals, timestamp, success, "Arena round ended");
        return (signals, false);
    }

    if zoned_out(event, cache) {
        // Quit/disconnect mid-round: backdate to the last combat activity.
        let success = round_success();
        let exit_time = cache
            .current_encounter()
            .and_then(|e| e.last_exit_combat_time.max(e.last_damage_time))
            .unwrap_or(timestamp);
        end_pvp_match(cache, &mut signals, exit_time, success, "Zoned out of arena");
        return (signals, false);
    }

    let was_accumulated = accumulate_match_event(event, cache);
    (signals, was_accumulated)
}

/// AreaEntered with a different area ID. Re-entering the SAME area (e.g. a
/// Voidstar round swap) is not a zone-out — the match continues.
fn zoned_out(event: &CombatEvent, cache: &SessionCache) -> bool {
    event.effect.type_id == effect_type_id::AREAENTERED
        && cache
            .current_encounter()
            .and_then(|e| e.area_id)
            .is_some_and(|id| id != event.effect.effect_id)
}

/// End the current PvP encounter at `exit_time` and start a fresh one.
/// Warzones always report success (win/loss isn't derivable from the log);
/// arena rounds report failure when the friendly team was wiped.
fn end_pvp_match(
    cache: &mut SessionCache,
    signals: &mut Vec<GameSignal>,
    exit_time: NaiveDateTime,
    success: bool,
    reason: &str,
) {
    let encounter_id = cache.current_encounter().map(|e| e.id).unwrap_or(0);

    if let Some(enc) = cache.current_encounter_mut() {
        enc.exit_combat_time = Some(exit_time);
        enc.state = EncounterState::PostCombat { exit_time };
        let duration = enc.duration_seconds(None).unwrap_or(0) as f32;
        enc.challenge_tracker.finalize(exit_time, duration);
    }

    tracing::info!(
        "[ENCOUNTER] {reason}, ending encounter {encounter_id} at {exit_time}"
    );

    signals.push(GameSignal::CombatEnded {
        timestamp: exit_time,
        encounter_id,
        success,
    });

    cache.push_new_encounter();
}

/// Accumulate a mid-match event: entity tracking, data, faction inference,
/// and the activity timestamps used to backdate the encounter end.
fn accumulate_match_event(event: &CombatEvent, cache: &mut SessionCache) -> bool {
    let local_player_id = cache.player.id;
    let Some(enc) = cache.current_encounter_mut() else {
        return false;
    };

    enc.track_event_entities(event);
    enc.accumulate_data(event, local_player_id);
    enc.track_event_line(event.line_number);

    // Infer friend/enemy factions from player-to-player damage/healing
    enc.pvp_factions.observe(event, local_player_id);

    if event.effect.effect_id == effect_id::DAMAGE {
        enc.last_damage_time = Some(event.timestamp);
    } else if event.effect.effect_id == effect_id::EXITCOMBAT {
        // Doesn't end the match, but becomes the backdate anchor when the
        // player zones out (warzone end, arena early-quit).
        enc.last_exit_combat_time = Some(event.timestamp);
    }

    true
}
