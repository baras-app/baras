//! Matching tests using synthetic names and representative health values.

use chrono::NaiveDateTime;

use super::candidates::{CandidateSet, PlayerCandidate};
use super::matching::{MatchConfig, RowObservation, assign_rows};

fn ts(offset_secs: i64) -> NaiveDateTime {
    chrono::DateTime::from_timestamp(1_700_000_000 + offset_secs, 0)
        .expect("valid timestamp")
        .naive_utc()
}

/// Builds a candidate set from `(name, current_hp, max_hp)` triples.
fn build(players: &[(&str, i32, i32)]) -> Vec<PlayerCandidate> {
    let mut set = CandidateSet::new();
    for (i, (name, current, max)) in players.iter().enumerate() {
        set.observe_raw(1000 + i as i64, name, (*current, *max), ts(i as i64));
    }
    set.candidates()
}

/// Eight synthetic players, mid-fight.
fn combat_group() -> Vec<PlayerCandidate> {
    build(&[
        ("@Test Alpha#1", 271_245, 493_172),
        ("@Tést Bravo#2", 147_569, 368_922),
        ("@Test Charlie-Long#3", 115_319, 443_534),
        ("@Test Delta#4", 322_802, 448_336),
        ("@Test Echo Player#5", 114_221, 439_311),
        ("@Test Foxtrot#6", 95_575, 434_431),
        ("@T'est Golf#7", 248_762, 436_425),
        ("@Test Hotel Player#8", 246_734, 440_596),
    ])
}

/// Eight-man group from a healer POV, out of combat — every player at full
/// health, so current and max coincide and four values share a prefix.
fn healer_pov_group() -> Vec<PlayerCandidate> {
    build(&[
        ("@Alpha#1", 498_994, 498_994),
        ("@Bravo#2", 442_862, 442_862),
        ("@Charlie#3", 464_642, 464_642),
        ("@Delta#4", 443_604, 443_604),
        ("@Echo Player#5", 436_774, 436_774),
        ("@Foxtrot Ghost#6", 443_025, 443_025),
        ("@Golf Player#7", 443_782, 443_782),
        ("@Hotel#8", 443_426, 443_426),
    ])
}

fn row(index: usize, name: Option<&str>, hp: Option<u32>, pct: Option<u8>) -> RowObservation {
    RowObservation {
        row: index,
        name_text: name.map(str::to_string),
        hp_value: hp,
        hp_percent: pct,
    }
}

#[test]
fn clean_read_assigns_every_row() {
    let candidates = healer_pov_group();
    let rows = vec![
        row(0, Some("ALPHA"), Some(498_994), Some(100)),
        row(1, Some("BRAVO"), Some(442_862), Some(100)),
        row(2, Some("CHARLIE"), Some(464_642), Some(100)),
        row(3, Some("DELTA"), Some(443_604), Some(100)),
        row(4, Some("ECHO PLAYER"), Some(436_774), Some(100)),
        row(5, Some("FOXTROT"), Some(443_025), Some(100)),
        row(6, Some("GOLF PLAYER"), Some(443_782), Some(100)),
        row(7, Some("HOTEL"), Some(443_426), Some(100)),
    ];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert_eq!(result.len(), 8, "every row should resolve");
    let expected = [
        "@Alpha#1",
        "@Bravo#2",
        "@Charlie#3",
        "@Delta#4",
        "@Echo Player#5",
        "@Foxtrot Ghost#6",
        "@Golf Player#7",
        "@Hotel#8",
    ];
    for (assignment, want) in result.iter().zip(expected) {
        assert_eq!(assignment.name, want);
        assert!(
            assignment.confidence > 0.9,
            "corroborated read should be confident, got {}",
            assignment.confidence
        );
    }
}

#[test]
fn truncated_names_match_full_log_names() {
    let candidates = combat_group();
    // Exactly what the frame renders: clipped at the icon boundary.
    let rows = vec![
        row(0, Some("TEST CHARL"), Some(115_319), Some(26)),
        row(1, Some("TEST ECHO"), Some(114_221), Some(26)),
        row(2, Some("TEST HOTEL"), Some(246_734), Some(56)),
        row(3, Some("TEST BRAV"), Some(147_569), Some(40)),
    ];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert_eq!(result.len(), 4);
    assert_eq!(result[0].name, "@Test Charlie-Long#3");
    assert_eq!(result[1].name, "@Test Echo Player#5");
    assert_eq!(result[2].name, "@Test Hotel Player#8");
    assert_eq!(result[3].name, "@Tést Bravo#2");
}

#[test]
fn unidentified_middle_character_keeps_a_strong_match() {
    let candidates = build(&[("Catzoon", 100_000, 100_000)]);
    // The OCR placeholder survives normalization as a wildcard: the glyph's
    // presence is kept, only its identity is unknown.
    let rows = vec![row(0, Some("CA?ZOON"), None, None)];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "Catzoon");
    assert!(result[0].confidence > 0.8);
}

#[test]
fn unidentified_character_does_not_choose_between_lookalikes() {
    let candidates = build(&[("Catzoon", 100_000, 100_000), ("Carzoon", 100_000, 100_000)]);
    let rows = vec![row(0, Some("CA?ZOON"), None, None)];

    assert!(assign_rows(&rows, &candidates, &MatchConfig::default()).is_empty());
}

#[test]
fn a_full_read_beats_a_prefix_candidate() {
    // ALPHA also fits the reading if RHO were junk, but the exact match must
    // lead by enough to assign.
    let candidates = build(&[("Alpha", 100_000, 100_000), ("Alpharho", 100_000, 100_000)]);
    let rows = vec![row(0, Some("ALPHARHO"), None, None)];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "Alpharho");
}

#[test]
fn a_prefix_read_cannot_choose_between_a_name_and_its_extension() {
    // A frame showing ALPHA really could be Alpharho with its tail occluded.
    let candidates = build(&[("Alpha", 100_000, 100_000), ("Alpharho", 100_000, 100_000)]);
    let rows = vec![row(0, Some("ALPHA"), None, None)];

    assert!(assign_rows(&rows, &candidates, &MatchConfig::default()).is_empty());
}

#[test]
fn trailing_marker_noise_does_not_hide_a_name() {
    let candidates = build(&[("Alpha", 100_000, 100_000)]);
    let rows = vec![row(0, Some("ALPHA RXO"), None, None)];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "Alpha");
}

#[test]
fn returns_exact_log_spelling_not_ocr_output() {
    let candidates = build(&[("@Tést Alpha#1", 19_996, 400_000), ("@Brav'à#2", 31_922, 435_110)]);
    // OCR cannot emit the accented characters at all.
    let rows = vec![
        row(0, Some("TEST ALPHA"), Some(19_996), None),
        row(1, Some("BRAVA"), Some(31_922), None),
    ];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].name, "@Tést Alpha#1", "accent must come from the log");
    assert_eq!(result[1].name, "@Brav'à#2");
}

#[test]
fn health_value_alone_never_assigns_a_player() {
    // Health is stale as soon as the player does anything. Even an exact value
    // is not identity without a usable name read.
    let candidates = healer_pov_group();
    let rows = vec![
        row(0, None, Some(443_604), None),
        row(1, None, Some(443_025), None),
        row(2, None, Some(443_782), None),
        row(3, None, Some(443_426), None),
    ];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert!(result.is_empty());
}

#[test]
fn name_alone_resolves_when_health_is_hidden() {
    // The user has the numeric health display switched off.
    let candidates = healer_pov_group();
    let rows = vec![row(0, Some("CHARLIE"), None, None), row(1, Some("HOTEL"), None, None)];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].name, "@Charlie#3");
    assert_eq!(result[1].name, "@Hotel#8");
}

#[test]
fn one_misread_digit_still_matches() {
    let candidates = healer_pov_group();
    // 443,604 misread as 443,504.
    let rows = vec![row(0, Some("DELTA"), Some(443_504), Some(100))];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "@Delta#4");
}

#[test]
fn matching_health_can_raise_name_confidence() {
    let candidates = build(&[("Alpha", 271_245, 493_172)]);
    let without_health = assign_rows(
        &[row(0, Some("ALPNA"), None, None)],
        &candidates,
        &MatchConfig::default(),
    );
    let with_health = assign_rows(
        &[row(0, Some("ALPNA"), Some(271_245), None)],
        &candidates,
        &MatchConfig::default(),
    );

    assert_eq!(without_health.len(), 1);
    assert_eq!(with_health.len(), 1);
    assert!(with_health[0].confidence > without_health[0].confidence);
}

#[test]
fn a_player_cannot_occupy_two_rows() {
    let candidates = healer_pov_group();
    // Both rows read identically — a duplicate cannot be correct.
    let rows = vec![
        row(0, Some("HOTEL"), Some(443_426), Some(100)),
        row(1, Some("HOTEL"), Some(443_426), Some(100)),
    ];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert!(result.len() <= 1, "the same player must not fill two rows");
    if let Some(a) = result.first() {
        assert_eq!(a.name, "@Hotel#8");
    }
}

#[test]
fn missing_candidate_health_does_not_penalize_a_good_name() {
    // A player the log has not reported health for yet. Reading their health off
    // the screen must not count against them — absent is not contradicting.
    let candidates = build(&[("@Test Delta#4", 0, 0), ("@Bravo#2", 0, 0)]);
    let rows = vec![row(0, Some("TEST DELTA"), Some(322_802), Some(72))];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert_eq!(result.len(), 1, "name alone should still carry the match");
    assert_eq!(result[0].name, "@Test Delta#4");
}

#[test]
fn stale_health_does_not_veto_a_good_name() {
    let candidates = healer_pov_group();
    // The log still has Alpha's value while the frame already shows Hotel's
    // newer value. The name remains the identity signal.
    let rows = vec![row(0, Some("HOTEL"), Some(498_994), None)];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "@Hotel#8");
}

#[test]
fn ambiguous_read_leaves_the_row_unassigned() {
    let candidates = healer_pov_group();
    // Only a percentage, and everyone is at full health. Nothing distinguishes
    // the candidates, so guessing would be wrong seven times out of eight.
    let rows = vec![row(0, None, None, Some(100))];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert!(result.is_empty(), "a signal shared by all candidates decides nothing");
}

#[test]
fn unreadable_row_is_skipped_without_disturbing_others() {
    let candidates = healer_pov_group();
    let rows = vec![
        row(0, Some("CHARLIE"), Some(464_642), None),
        row(1, None, None, None),
        row(2, Some("BRAVO"), Some(442_862), None),
    ];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].row, 0);
    assert_eq!(result[1].row, 2, "row indices must survive the gap");
}

#[test]
fn garbage_text_does_not_match_anyone() {
    let candidates = healer_pov_group();
    let rows = vec![row(0, Some("|||^^~"), None, None)];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert!(result.is_empty());
}

#[test]
fn short_unique_prefix_is_not_enough() {
    let candidates = build(&[("TestCharlieLong", 100_000, 100_000)]);
    let rows = vec![RowObservation {
        row: 0,
        name_text: Some("TE".into()),
        ..Default::default()
    }];

    assert!(assign_rows(&rows, &candidates, &MatchConfig::default()).is_empty());
}

#[test]
fn three_character_name_is_valid() {
    let candidates = build(&[("Bõt", 100_000, 100_000)]);
    let rows = vec![row(0, Some("BOT"), None, None)];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "Bõt");
}

#[test]
fn three_character_prefix_is_valid() {
    let candidates = build(&[("TestCharlieLong", 100_000, 100_000)]);
    let rows = vec![row(0, Some("TES"), None, None)];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "TestCharlieLong");
}

#[test]
fn exact_two_character_name_is_too_short() {
    let candidates = build(&[("Ai", 100_000, 100_000)]);
    let rows = vec![row(0, Some("AI"), None, None)];

    assert!(assign_rows(&rows, &candidates, &MatchConfig::default()).is_empty());
}

#[test]
fn health_cannot_resolve_a_row_with_a_short_name_fragment() {
    let candidates = build(&[("TestCharlieLong", 73_456, 100_000)]);
    let rows = vec![RowObservation {
        row: 0,
        name_text: Some("TE".into()),
        hp_value: Some(73_456),
        ..Default::default()
    }];

    assert!(assign_rows(&rows, &candidates, &MatchConfig::default()).is_empty());
}

#[test]
fn empty_inputs_are_handled() {
    let candidates = healer_pov_group();
    assert!(assign_rows(&[], &candidates, &MatchConfig::default()).is_empty());
    assert!(assign_rows(&[row(0, Some("HOTEL"), None, None)], &[], &MatchConfig::default()).is_empty());
}

#[test]
fn candidate_set_expires_stale_players() {
    let mut set = CandidateSet::new();
    set.observe_raw(1, "@Old Player#1", (100, 100), ts(0));
    set.observe_raw(2, "@Current Player#2", (100, 100), ts(600));

    set.expire_before(ts(300));

    let remaining = set.candidates();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].name, "@Current Player#2");
}

#[test]
fn candidate_set_keeps_good_health_over_empty_readings() {
    let mut set = CandidateSet::new();
    set.observe_raw(1, "@Hotel#8", (443_426, 443_426), ts(0));
    // Effect lines routinely carry (0/0); that must not wipe the real reading.
    set.observe_raw(1, "@Hotel#8", (0, 0), ts(1));

    let c = &set.candidates()[0];
    assert_eq!(c.current_hp, 443_426);
    assert_eq!(c.max_hp, 443_426);
    assert_eq!(c.hp_percent(), Some(100));
}

#[test]
fn candidate_set_ignores_stale_health() {
    let mut set = CandidateSet::new();
    set.observe_raw(1, "@Hotel#8", (400_000, 443_426), ts(10));
    set.observe_raw(1, "@Hotel#8", (100_000, 443_426), ts(5));

    let c = &set.candidates()[0];
    assert_eq!(c.current_hp, 400_000);
    assert_eq!(c.last_seen, ts(10));
}

#[test]
fn two_near_ties_are_not_resolved_by_elimination() {
    let candidates = build(&[("@Alpha One#1", 100_000, 100_000), ("@Alpha Only#2", 100_000, 100_000)]);
    let rows = vec![
        row(0, Some("ALPHA ON"), None, None),
        row(1, Some("ALPHA ON"), None, None),
    ];

    assert!(assign_rows(&rows, &candidates, &MatchConfig::default()).is_empty());
}

#[test]
fn a_leading_edge_artifact_still_matches_the_right_player() {
    // The crop's left edge sometimes adds a glyph that normalization keeps
    // because it reads as a letter.
    let candidates = build(&[("@Soa#1", 274_808, 274_808), ("@Delta Four#2", 38_930, 38_930)]);
    let rows = vec![row(0, Some("ISOA"), Some(274_808), None)];

    let result = assign_rows(&rows, &candidates, &MatchConfig::default());

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "@Soa#1");
}

#[test]
fn dropping_a_leading_glyph_does_not_rescue_an_unrelated_read() {
    // The remainder has to match exactly; a near-miss must stay unassigned.
    let candidates = build(&[("@Echo Five#1", 397_263, 397_263)]);
    let rows = vec![row(0, Some("XQZRV"), None, None)];

    assert!(assign_rows(&rows, &candidates, &MatchConfig::default()).is_empty());
}
