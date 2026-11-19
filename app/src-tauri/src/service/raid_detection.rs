//! Raid-frame OCR candidate lifecycle.

use std::sync::Arc;

use baras_core::raid_detect::{CandidateSet, PlayerCandidate};
use chrono::NaiveDateTime;

use crate::state::MutexExt;
use crate::state::SharedState;

const ROSTER_WINDOW_MINUTES: i64 = 30;

/// Current-area roster for OCR. Names identify and health only supports.
pub(super) async fn raid_detection_candidates(shared: &Arc<SharedState>) -> Vec<PlayerCandidate> {
    if shared.is_session_not_live().await {
        return Vec::new();
    }
    let session_guard = shared.session.read().await;
    let Some(session) = session_guard.as_ref() else {
        return Vec::new();
    };
    let session = session.read().await;
    let Some(cache) = session.session_cache.as_ref() else {
        return Vec::new();
    };
    let Some(last_event) = session.last_event_time else {
        return Vec::new();
    };

    // PvP rosters are match-specific. Players from before this area are not candidates.
    let pvp_roster_started_at = shared
        .pvp_ocr_roster_started_at
        .locked()
        .or(cache.current_area.entered_at);
    let cutoff = candidate_cutoff(last_event, cache.current_area.area_id, pvp_roster_started_at);
    let mut set = CandidateSet::new();
    for player in cache.player_disciplines.values() {
        if let Some(last_seen) = player.last_seen_at
            && last_seen >= cutoff
        {
            set.observe_raw(
                player.id,
                baras_core::context::resolve(player.name),
                (player.current_hp, player.max_hp),
                last_seen,
            );
        }
    }

    // Ability targets may not exist in the discipline roster yet.
    let mut roster = shared.ability_roster.locked();
    roster.expire_before(cutoff);
    for candidate in roster.candidates() {
        set.observe_raw(
            candidate.entity_id,
            &candidate.name,
            (candidate.current_hp, candidate.max_hp),
            candidate.last_seen,
        );
    }
    set.candidates()
}

fn candidate_cutoff(last_event: NaiveDateTime, area_id: i64, entered_at: Option<NaiveDateTime>) -> NaiveDateTime {
    let rolling_cutoff = last_event - chrono::Duration::minutes(ROSTER_WINDOW_MINUTES);

    if baras_core::game_data::is_pvp_area(area_id) {
        entered_at.map_or(rolling_cutoff, |entered_at| rolling_cutoff.max(entered_at))
    } else {
        rolling_cutoff
    }
}

pub(super) fn should_reset_on_area_entry(current_area_id: i64, entered_area_id: i64, is_live: bool) -> bool {
    is_live && current_area_id != entered_area_id && baras_core::game_data::is_pvp_area(entered_area_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PVP_AREA_ID: i64 = 137_438_956_902;

    fn ts(offset_secs: i64) -> NaiveDateTime {
        chrono::DateTime::from_timestamp(1_700_000_000 + offset_secs, 0)
            .expect("valid timestamp")
            .naive_utc()
    }

    #[test]
    fn pvp_candidates_must_have_been_seen_since_area_entry() {
        assert_eq!(candidate_cutoff(ts(1_800), PVP_AREA_ID, Some(ts(1_500))), ts(1_500),);
    }

    #[test]
    fn normal_rolling_window_still_applies() {
        assert_eq!(candidate_cutoff(ts(3_600), PVP_AREA_ID, Some(ts(0))), ts(1_800),);
        assert_eq!(candidate_cutoff(ts(3_600), 0, Some(ts(3_500))), ts(1_800));
    }

    #[test]
    fn only_a_live_transition_into_a_different_pvp_area_resets() {
        assert!(should_reset_on_area_entry(0, PVP_AREA_ID, true));
        assert!(!should_reset_on_area_entry(PVP_AREA_ID, PVP_AREA_ID, true));
        assert!(!should_reset_on_area_entry(0, PVP_AREA_ID, false));
        assert!(!should_reset_on_area_entry(PVP_AREA_ID, 0, true));
    }
}
