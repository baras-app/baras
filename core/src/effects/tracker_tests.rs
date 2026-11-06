//! Tests for effect tracker
//!
//! Verifies instant alert behavior, OnApply alert fixes, and multi-healer
//! effect tracking correctness (exclusivity, refresh-after-removal, charges).

use chrono::Local;

use super::definition::EffectDefinition;
use super::tracker::{DefinitionSet, EffectTracker};
use crate::combat_log::EntityType;
use crate::context::empty_istr;
use crate::dsl::{AudioConfig, EffectSelector, EntityFilter, Trigger};
use crate::signal_processor::{GameSignal, SignalHandler};
use baras_types::{AlertTrigger, RefreshAbility, RefreshScope};

fn now() -> chrono::NaiveDateTime {
    Local::now().naive_local()
}

/// Create a minimal effect definition for testing
fn make_effect(
    id: &str,
    name: &str,
    trigger: Trigger,
    duration_secs: Option<f32>,
) -> EffectDefinition {
    EffectDefinition {
        id: id.to_string(),
        name: name.to_string(),
        display_text: None,
        enabled: true,
        trigger,
        ignore_effect_removed: false,
        refresh_abilities: vec![],
        is_aoe_refresh: false,
        aoe_refresh_immediate: false,
        is_refreshed_on_modify: false,
        single_instance_per_target: false,
        default_charges: None,
        duration_secs,
        is_affected_by_alacrity: false,
        cooldown_ready_secs: 0.0,
        color: None,
        show_at_secs: 0.0,
        display_targets: vec![],
        icon_ability_id: None,
        show_icon: true,
        display_source: false,
        show_target: false,
        track_uptime: false,
        track_uptime_after_first: false,
        stack_priority: false,
        show_countdown: true,
        show_single_stack: false,
        disciplines: vec![],
        source_disciplines: vec![],
        target_disciplines: vec![],
        ignore_refreshes: false,
        refresh_scope: RefreshScope::Both,
        persist_past_death: false,
        track_outside_combat: true,
        on_apply_trigger_timer: None,
        on_expire_trigger_timer: None,
        is_alert: false,
        alert_text: None,
        alert_on: AlertTrigger::None,
        alert_countdown_secs: None,
        audio: AudioConfig::default(),
        modifiers: vec![],
    }
}

fn make_tracker(defs: Vec<EffectDefinition>) -> EffectTracker {
    let mut def_set = DefinitionSet::new();
    def_set.add_definitions(defs, true);
    EffectTracker::new(def_set)
}

fn effect_applied_signal(effect_id: i64, timestamp: chrono::NaiveDateTime) -> GameSignal {
    GameSignal::EffectApplied {
        effect_id,
        effect_name: empty_istr(),
        action_id: 0,
        action_name: empty_istr(),
        source_id: 1,
        source_name: empty_istr(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 2,
        target_name: empty_istr(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp,
        charges: None,
    }
}

fn ability_activated_signal(ability_id: i64, timestamp: chrono::NaiveDateTime) -> GameSignal {
    GameSignal::AbilityActivated {
        ability_id,
        ability_name: empty_istr(),
        source_id: 1,
        source_entity_type: EntityType::Player,
        source_name: empty_istr(),
        source_npc_id: 0,
        target_id: 2,
        target_name: empty_istr(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Instant Alert: EffectApplied trigger
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_instant_alert_effect_applied_fires_alert_no_active_effect() {
    let mut def = make_effect(
        "test_alert",
        "Test Alert",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(12345)],
            source: EntityFilter::Any,
            target: EntityFilter::Any,
            position: vec![],
        },
        None,
    );
    def.is_alert = true;
    def.alert_text = Some("Danger!".to_string());

    let mut tracker = make_tracker(vec![def]);
    let ts = now();

    tracker.handle_signal(&effect_applied_signal(12345, ts), None);

    // Should fire alert
    let alerts = tracker.take_fired_alerts();
    assert_eq!(alerts.len(), 1, "Expected 1 fired alert");
    assert_eq!(alerts[0].text, "Danger!");
    assert!(alerts[0].alert_text_enabled);

    // Should NOT create an active effect
    let active_count = tracker.active_effects().count();
    assert_eq!(
        active_count, 0,
        "Instant alert should not create active effect"
    );
}

#[test]
fn test_instant_alert_no_text_when_alert_text_is_none() {
    let mut def = make_effect(
        "test_alert",
        "My Alert Name",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(12345)],
            source: EntityFilter::Any,
            target: EntityFilter::Any,
            position: vec![],
        },
        None,
    );
    def.is_alert = true;
    def.audio.enabled = true;
    def.audio.file = Some("beep.mp3".to_string());
    // alert_text is None — text overlay should NOT fire, but audio should

    let mut tracker = make_tracker(vec![def]);
    tracker.handle_signal(&effect_applied_signal(12345, now()), None);

    let alerts = tracker.take_fired_alerts();
    assert_eq!(alerts.len(), 1);
    // Text field is populated (for TTS fallback) but alert_text_enabled is false
    assert_eq!(alerts[0].text, "My Alert Name");
    assert!(
        !alerts[0].alert_text_enabled,
        "No text overlay when alert_text is None"
    );
    // Audio still fires
    assert!(alerts[0].audio_enabled);
    assert_eq!(alerts[0].audio_file.as_deref(), Some("beep.mp3"));
}

#[test]
fn test_instant_alert_carries_audio_config() {
    let mut def = make_effect(
        "test_alert",
        "Test Alert",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(12345)],
            source: EntityFilter::Any,
            target: EntityFilter::Any,
            position: vec![],
        },
        None,
    );
    def.is_alert = true;
    def.alert_text = Some("Watch out!".to_string());
    def.audio.enabled = true;
    def.audio.file = Some("warning.mp3".to_string());

    let mut tracker = make_tracker(vec![def]);
    tracker.handle_signal(&effect_applied_signal(12345, now()), None);

    let alerts = tracker.take_fired_alerts();
    assert_eq!(alerts.len(), 1);
    assert!(alerts[0].audio_enabled);
    assert_eq!(alerts[0].audio_file.as_deref(), Some("warning.mp3"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Instant Alert: AbilityCast trigger
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_instant_alert_ability_cast_fires_alert_no_active_effect() {
    let mut def = make_effect(
        "test_ability_alert",
        "Ability Alert",
        Trigger::AbilityCast {
            abilities: vec![crate::dsl::AbilitySelector::Id(99999)],
            source: EntityFilter::Any,
            target: EntityFilter::Any,
            position: vec![],
        },
        None,
    );
    def.is_alert = true;
    def.alert_text = Some("Ability fired!".to_string());

    let mut tracker = make_tracker(vec![def]);
    tracker.handle_signal(&ability_activated_signal(99999, now()), None);

    let alerts = tracker.take_fired_alerts();
    assert_eq!(alerts.len(), 1, "Expected 1 fired alert for ability cast");
    assert_eq!(alerts[0].text, "Ability fired!");

    let active_count = tracker.active_effects().count();
    assert_eq!(
        active_count, 0,
        "Instant alert should not create active effect"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Non-instant (is_alert=false) — regression tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_non_instant_effect_creates_active_effect() {
    let def = make_effect(
        "normal_effect",
        "Normal Effect",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(12345)],
            source: EntityFilter::Any,
            target: EntityFilter::Any,
            position: vec![],
        },
        Some(15.0),
    );

    let mut tracker = make_tracker(vec![def]);
    tracker.handle_signal(&effect_applied_signal(12345, now()), None);

    // Should create active effect
    let active_count = tracker.active_effects().count();
    assert_eq!(
        active_count, 1,
        "Normal effect should create an active effect"
    );

    // Should NOT fire an alert (alert_on is None)
    let alerts = tracker.take_fired_alerts();
    assert!(alerts.is_empty(), "No alert expected for alert_on=None");
}

// ─────────────────────────────────────────────────────────────────────────────
// OnApply alert fix for AbilityCast triggers
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_on_apply_alert_fires_for_ability_cast_trigger() {
    let mut def = make_effect(
        "proc_with_alert",
        "Proc Alert",
        Trigger::AbilityCast {
            abilities: vec![crate::dsl::AbilitySelector::Id(99999)],
            source: EntityFilter::Any,
            target: EntityFilter::Any,
            position: vec![],
        },
        Some(10.0),
    );
    def.alert_on = AlertTrigger::OnApply;
    def.alert_text = Some("Proc activated!".to_string());

    let mut tracker = make_tracker(vec![def]);
    tracker.handle_signal(&ability_activated_signal(99999, now()), None);

    // Should create active effect AND fire alert
    let active_count = tracker.active_effects().count();
    assert_eq!(active_count, 1, "Should create active effect");

    let alerts = tracker.take_fired_alerts();
    assert_eq!(
        alerts.len(),
        1,
        "Expected OnApply alert for ability cast trigger"
    );
    assert_eq!(alerts[0].text, "Proc activated!");
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper: build signals with explicit source/target IDs
// ─────────────────────────────────────────────────────────────────────────────

fn effect_applied_signal_with_source(
    effect_id: i64,
    source_id: i64,
    target_id: i64,
    timestamp: chrono::NaiveDateTime,
) -> GameSignal {
    GameSignal::EffectApplied {
        effect_id,
        effect_name: empty_istr(),
        action_id: 0,
        action_name: empty_istr(),
        source_id,
        source_name: empty_istr(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id,
        target_name: empty_istr(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp,
        charges: None,
    }
}

fn effect_removed_signal_with_source(
    effect_id: i64,
    source_id: i64,
    target_id: i64,
    timestamp: chrono::NaiveDateTime,
) -> GameSignal {
    GameSignal::EffectRemoved {
        effect_id,
        effect_name: empty_istr(),
        source_id,
        source_entity_type: EntityType::Player,
        source_name: empty_istr(),
        source_npc_id: 0,
        target_id,
        target_entity_type: EntityType::Player,
        target_name: empty_istr(),
        target_npc_id: 0,
        timestamp,
    }
}

fn ability_activated_signal_with_source(
    ability_id: i64,
    source_id: i64,
    target_id: i64,
    timestamp: chrono::NaiveDateTime,
) -> GameSignal {
    GameSignal::AbilityActivated {
        ability_id,
        ability_name: empty_istr(),
        source_id,
        source_entity_type: EntityType::Player,
        source_name: empty_istr(),
        source_npc_id: 0,
        target_id,
        target_name: empty_istr(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp,
    }
}

fn charges_changed_signal(
    effect_id: i64,
    source_id: i64,
    target_id: i64,
    charges: u8,
    timestamp: chrono::NaiveDateTime,
) -> GameSignal {
    GameSignal::EffectChargesChanged {
        effect_id,
        effect_name: empty_istr(),
        action_id: 0,
        action_name: empty_istr(),
        source_id,
        source_entity_type: EntityType::Player,
        source_name: empty_istr(),
        source_npc_id: 0,
        target_id,
        target_entity_type: EntityType::Player,
        target_name: empty_istr(),
        target_npc_id: 0,
        timestamp,
        charges,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Bug 1: Phantom _others effects from same-class refresh
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_kolto_shell_others_does_not_create_phantom_when_local_active() {
    // Set up kolto_shell (local_player) and kolto_shell_others (other_players)
    // both triggered by the same effect ID
    let effect_id: u64 = 985226842996736;
    let local_player_id: i64 = 1;
    let other_player_id: i64 = 99;
    let target_id: i64 = 2;

    let mut kolto_shell = make_effect(
        "kolto_shell",
        "Kolto Shell",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(effect_id)],
            source: EntityFilter::LocalPlayer,
            target: EntityFilter::AnyPlayer,
            position: vec![],
        },
        Some(180.0),
    );
    kolto_shell.display_targets = vec![super::definition::DisplayTarget::RaidFrames];
    kolto_shell.default_charges = Some(7);
    kolto_shell.single_instance_per_target = true;
    kolto_shell.refresh_abilities = vec![RefreshAbility::Simple(baras_types::AbilitySelector::Id(
        effect_id,
    ))];

    let mut kolto_shell_others = make_effect(
        "kolto_shell_others",
        "Other's Kolto Shell",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(effect_id)],
            source: EntityFilter::OtherPlayers,
            target: EntityFilter::AnyPlayer,
            position: vec![],
        },
        Some(180.0),
    );
    kolto_shell_others.single_instance_per_target = true;

    let mut tracker = make_tracker(vec![kolto_shell, kolto_shell_others]);
    tracker.set_player_context(local_player_id, 0);

    let ts = now();

    // Local player applies Kolto Shell on target
    tracker.handle_signal(
        &effect_applied_signal_with_source(effect_id as i64, local_player_id, target_id, ts),
        None,
    );

    assert_eq!(
        tracker.active_effects().count(),
        1,
        "Should have 1 effect after local player applies"
    );

    // Another player of same class casts Kolto Shell on same target (refresh in game)
    let ts2 = ts + chrono::Duration::seconds(5);
    tracker.handle_signal(
        &effect_applied_signal_with_source(effect_id as i64, other_player_id, target_id, ts2),
        None,
    );

    // Should still have only 1 effect (the local player's, refreshed)
    // NOT 2 effects (phantom kolto_shell_others should not be created)
    assert_eq!(
        tracker.active_effects().count(),
        1,
        "Should still have 1 effect — no phantom _others created"
    );

    // The existing effect should be the local player's (kolto_shell, not kolto_shell_others)
    let effect = tracker.active_effects().next().unwrap();
    assert_eq!(effect.definition_id, "kolto_shell");
    assert_eq!(effect.source_entity_id, local_player_id);
}

#[test]
fn test_kolto_shell_others_creates_normally_when_no_local_active() {
    // When local player's shell is NOT active, _others should create normally
    let effect_id: u64 = 985226842996736;
    let local_player_id: i64 = 1;
    let other_player_id: i64 = 99;
    let target_id: i64 = 2;

    let mut kolto_shell = make_effect(
        "kolto_shell",
        "Kolto Shell",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(effect_id)],
            source: EntityFilter::LocalPlayer,
            target: EntityFilter::AnyPlayer,
            position: vec![],
        },
        Some(180.0),
    );
    kolto_shell.display_targets = vec![super::definition::DisplayTarget::RaidFrames];
    kolto_shell.single_instance_per_target = true;

    let mut kolto_shell_others = make_effect(
        "kolto_shell_others",
        "Other's Kolto Shell",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(effect_id)],
            source: EntityFilter::OtherPlayers,
            target: EntityFilter::AnyPlayer,
            position: vec![],
        },
        Some(180.0),
    );
    kolto_shell_others.single_instance_per_target = true;

    let mut tracker = make_tracker(vec![kolto_shell, kolto_shell_others]);
    tracker.set_player_context(local_player_id, 0);

    // Other player applies without local player having it first
    let ts = now();
    tracker.handle_signal(
        &effect_applied_signal_with_source(effect_id as i64, other_player_id, target_id, ts),
        None,
    );

    assert_eq!(
        tracker.active_effects().count(),
        1,
        "Should create kolto_shell_others when no local shell active"
    );
    let effect = tracker.active_effects().next().unwrap();
    assert_eq!(effect.definition_id, "kolto_shell_others");
}

// ─────────────────────────────────────────────────────────────────────────────
// Bug 2: Refresh abilities resurrecting removed effects
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_refresh_does_not_resurrect_removed_effect() {
    // Simulate: Kolto Probes applied, then removed, then refresh ability cast.
    // The refresh should NOT bring back the removed effect.
    let effect_id: u64 = 814832605462528;
    let local_player_id: i64 = 1;
    let target_id: i64 = 2;

    let mut kolto_probe = make_effect(
        "kolto_probe",
        "Kolto Probe",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(effect_id)],
            source: EntityFilter::LocalPlayer,
            target: EntityFilter::AnyPlayer,
            position: vec![],
        },
        Some(21.0),
    );
    kolto_probe.display_targets = vec![super::definition::DisplayTarget::RaidFrames];
    kolto_probe.refresh_abilities = vec![RefreshAbility::Simple(baras_types::AbilitySelector::Id(
        effect_id,
    ))];

    let mut tracker = make_tracker(vec![kolto_probe]);
    tracker.set_player_context(local_player_id, 0);

    let ts = now();

    // Step 1: Apply effect
    tracker.handle_signal(
        &effect_applied_signal_with_source(effect_id as i64, local_player_id, target_id, ts),
        None,
    );
    assert_eq!(
        tracker.active_effects().count(),
        1,
        "Effect should be active"
    );

    // Step 2: Remove effect (authoritative game signal, well after application)
    let ts_remove = ts + chrono::Duration::seconds(22);
    tracker.handle_signal(
        &effect_removed_signal_with_source(effect_id as i64, local_player_id, target_id, ts_remove),
        None,
    );

    // The effect is marked removed but still in HashMap (tick() hasn't run yet to GC)
    // Verify it's marked removed
    let effect = tracker.active_effects().next().unwrap();
    assert!(
        effect.removed_at.is_some(),
        "Effect should be marked as removed"
    );

    // Step 3: Cast the refresh ability (slightly too late — probes already fell off)
    let ts_recast = ts_remove + chrono::Duration::milliseconds(500);
    tracker.handle_signal(
        &ability_activated_signal_with_source(
            effect_id as i64,
            local_player_id,
            target_id,
            ts_recast,
        ),
        None,
    );

    // The removed effect should NOT be resurrected by the refresh
    let effect = tracker.active_effects().next().unwrap();
    assert!(
        effect.removed_at.is_some(),
        "Removed effect should NOT be resurrected by refresh ability"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Bug 3: Charges from another player updating local effect
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_charges_from_other_player_do_not_update_local_effect() {
    // Two operative healers: both have Kolto Probes on the same target.
    // Charges changed from the other player should NOT update the local player's effect.
    let effect_id: u64 = 814832605462528;
    let local_player_id: i64 = 1;
    let other_player_id: i64 = 99;
    let target_id: i64 = 2;

    let kolto_probe = make_effect(
        "kolto_probe",
        "Kolto Probe",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(effect_id)],
            source: EntityFilter::LocalPlayer,
            target: EntityFilter::AnyPlayer,
            position: vec![],
        },
        Some(21.0),
    );

    let mut tracker = make_tracker(vec![kolto_probe]);
    tracker.set_player_context(local_player_id, 0);

    let ts = now();

    // Local player applies 2 stacks
    tracker.handle_signal(
        &effect_applied_signal_with_source(effect_id as i64, local_player_id, target_id, ts),
        None,
    );
    let ts2 = ts + chrono::Duration::seconds(1);
    tracker.handle_signal(
        &charges_changed_signal(effect_id as i64, local_player_id, target_id, 2, ts2),
        None,
    );

    let effect = tracker.active_effects().next().unwrap();
    assert_eq!(effect.stacks, 2, "Should have 2 stacks from local player");

    // Other player's probes cause a ModifyCharges event (from their source)
    let ts3 = ts + chrono::Duration::seconds(3);
    tracker.handle_signal(
        &charges_changed_signal(effect_id as i64, other_player_id, target_id, 4, ts3),
        None,
    );

    // Local player's effect should still show 2 stacks, NOT 4
    let effect = tracker.active_effects().next().unwrap();
    assert_eq!(
        effect.stacks, 2,
        "Charges from other player should NOT update local effect"
    );
}

#[test]
fn test_charges_from_local_player_do_update_local_effect() {
    // Sanity check: charges from the local player should still update
    let effect_id: u64 = 814832605462528;
    let local_player_id: i64 = 1;
    let target_id: i64 = 2;

    let kolto_probe = make_effect(
        "kolto_probe",
        "Kolto Probe",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(effect_id)],
            source: EntityFilter::LocalPlayer,
            target: EntityFilter::AnyPlayer,
            position: vec![],
        },
        Some(21.0),
    );

    let mut tracker = make_tracker(vec![kolto_probe]);
    tracker.set_player_context(local_player_id, 0);

    let ts = now();

    // Apply effect
    tracker.handle_signal(
        &effect_applied_signal_with_source(effect_id as i64, local_player_id, target_id, ts),
        None,
    );

    // Charges changed from local player
    let ts2 = ts + chrono::Duration::seconds(1);
    tracker.handle_signal(
        &charges_changed_signal(effect_id as i64, local_player_id, target_id, 2, ts2),
        None,
    );

    let effect = tracker.active_effects().next().unwrap();
    assert_eq!(
        effect.stacks, 2,
        "Charges from local player should update normally"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Source-agnostic refresh: other players' effects refresh via AbilityActivated
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_kolto_shell_others_refreshes_via_ability_activated() {
    // When another player recasts Kolto Shell, the existing kolto_shell_others
    // effect should get its timer refreshed via the AbilityActivated signal.
    let effect_id: u64 = 985226842996736;
    let local_player_id: i64 = 1;
    let other_player_id: i64 = 99;
    let target_id: i64 = 2;

    let mut kolto_shell = make_effect(
        "kolto_shell",
        "Kolto Shell",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(effect_id)],
            source: EntityFilter::LocalPlayer,
            target: EntityFilter::AnyPlayer,
            position: vec![],
        },
        Some(180.0),
    );
    kolto_shell.single_instance_per_target = true;

    let mut kolto_shell_others = make_effect(
        "kolto_shell_others",
        "Other's Kolto Shell",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(effect_id)],
            source: EntityFilter::OtherPlayers,
            target: EntityFilter::AnyPlayer,
            position: vec![],
        },
        Some(180.0),
    );
    kolto_shell_others.display_targets = vec![super::definition::DisplayTarget::RaidFrames];
    kolto_shell_others.single_instance_per_target = true;
    kolto_shell_others.refresh_abilities = vec![RefreshAbility::Simple(
        baras_types::AbilitySelector::Id(effect_id),
    )];

    let mut tracker = make_tracker(vec![kolto_shell, kolto_shell_others]);
    tracker.set_player_context(local_player_id, 0);

    let ts = now();

    // Other player applies Kolto Shell on target (creates kolto_shell_others)
    tracker.handle_signal(
        &effect_applied_signal_with_source(effect_id as i64, other_player_id, target_id, ts),
        None,
    );

    assert_eq!(tracker.active_effects().count(), 1);
    let effect = tracker.active_effects().next().unwrap();
    assert_eq!(effect.definition_id, "kolto_shell_others");
    let original_expires = effect.expires_at;

    // 60 seconds later, other player recasts (AbilityActivated signal)
    let ts2 = ts + chrono::Duration::seconds(60);
    tracker.handle_signal(
        &ability_activated_signal_with_source(effect_id as i64, other_player_id, target_id, ts2),
        None,
    );

    // The effect should still exist and its timer should be refreshed
    assert_eq!(tracker.active_effects().count(), 1);
    let effect = tracker.active_effects().next().unwrap();
    assert_eq!(effect.definition_id, "kolto_shell_others");
    assert!(
        effect.expires_at > original_expires,
        "Timer should be refreshed — expires_at should be later than original"
    );
    assert_eq!(
        effect.last_refreshed_at, ts2,
        "last_refreshed_at should match the recast timestamp"
    );
}

#[test]
fn test_single_instance_activation_does_not_late_register() {
    // A cast with no tracked instance may be silently refreshing an instance
    // whose apply predates the log (the true caster is unknown). No instance
    // should be fabricated — only the target gets registered for raid frames.
    // The real instance arrives via ApplyEffect (fresh cast) or ModifyCharges.
    let effect_id: u64 = 985226842996736;
    let local_player_id: i64 = 1;
    let other_player_id: i64 = 99;
    let target_id: i64 = 2;

    let mut kolto_shell_others = make_effect(
        "kolto_shell_others",
        "Other's Kolto Shell",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(effect_id)],
            source: EntityFilter::OtherPlayers,
            target: EntityFilter::AnyPlayer,
            position: vec![],
        },
        Some(180.0),
    );
    kolto_shell_others.display_targets = vec![super::definition::DisplayTarget::RaidFrames];
    kolto_shell_others.single_instance_per_target = true;
    kolto_shell_others.refresh_abilities = vec![RefreshAbility::Simple(
        baras_types::AbilitySelector::Id(effect_id),
    )];

    let mut tracker = make_tracker(vec![kolto_shell_others]);
    tracker.set_player_context(local_player_id, 0);

    // No EffectApplied — go straight to AbilityActivated
    let ts = now();
    tracker.handle_signal(
        &ability_activated_signal_with_source(effect_id as i64, other_player_id, target_id, ts),
        None,
    );

    assert_eq!(
        tracker.active_effects().count(),
        0,
        "Activation alone must not fabricate a single-instance effect"
    );
    let targets = tracker.take_new_targets();
    assert!(
        targets.iter().any(|t| t.entity_id == target_id),
        "Target should still be registered for raid frames"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Single-instance buffs: imperfect information (zoned in after apply)
// ─────────────────────────────────────────────────────────────────────────────

/// Kolto Shell pair with single-instance semantics, mirroring the bundled defs
fn make_single_instance_pair(effect_id: u64) -> (EffectDefinition, EffectDefinition) {
    let mut local = make_effect(
        "kolto_shell",
        "Kolto Shell",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(effect_id)],
            source: EntityFilter::LocalPlayer,
            target: EntityFilter::AnyPlayer,
            position: vec![],
        },
        Some(180.0),
    );
    local.display_targets = vec![super::definition::DisplayTarget::RaidFrames];
    local.default_charges = Some(7);
    local.single_instance_per_target = true;
    local.refresh_abilities = vec![RefreshAbility::Simple(baras_types::AbilitySelector::Id(
        effect_id,
    ))];

    let mut others = make_effect(
        "kolto_shell_others",
        "Other's Kolto Shell",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(effect_id)],
            source: EntityFilter::OtherPlayers,
            target: EntityFilter::AnyPlayer,
            position: vec![],
        },
        Some(180.0),
    );
    others.display_targets = vec![super::definition::DisplayTarget::RaidFrames];
    others.single_instance_per_target = true;
    others.refresh_abilities = vec![RefreshAbility::Simple(baras_types::AbilitySelector::Id(
        effect_id,
    ))];

    (local, others)
}

#[test]
fn test_single_instance_local_apply_refreshes_others_instance() {
    // Reverse direction of the phantom test: another player's instance is
    // active and the local player applies — the existing instance refreshes
    // and the original caster keeps credit; no second instance appears.
    let effect_id: u64 = 985226842996736;
    let local_player_id: i64 = 1;
    let other_player_id: i64 = 99;
    let target_id: i64 = 2;

    let (local, others) = make_single_instance_pair(effect_id);
    let mut tracker = make_tracker(vec![local, others]);
    tracker.set_player_context(local_player_id, 0);

    let ts = now();
    tracker.handle_signal(
        &effect_applied_signal_with_source(effect_id as i64, other_player_id, target_id, ts),
        None,
    );
    let original_expires = tracker.active_effects().next().unwrap().expires_at;

    let ts2 = ts + chrono::Duration::seconds(5);
    tracker.handle_signal(
        &effect_applied_signal_with_source(effect_id as i64, local_player_id, target_id, ts2),
        None,
    );

    assert_eq!(
        tracker.active_effects().count(),
        1,
        "Local apply over other's instance must refresh, not duplicate"
    );
    let effect = tracker.active_effects().next().unwrap();
    assert_eq!(effect.definition_id, "kolto_shell_others");
    assert_eq!(
        effect.source_entity_id, other_player_id,
        "Original caster keeps credit"
    );
    assert!(effect.expires_at > original_expires, "Timer should refresh");
}

#[test]
fn test_single_instance_local_activation_registers_target_without_instance() {
    // Local player casts on a target with no tracked instance (the other
    // healer applied before we zoned in): no instance is created, but the
    // target is registered for raid frames.
    let effect_id: u64 = 985226842996736;
    let local_player_id: i64 = 1;
    let target_id: i64 = 2;

    let (local, others) = make_single_instance_pair(effect_id);
    let mut tracker = make_tracker(vec![local, others]);
    tracker.set_player_context(local_player_id, 0);

    let ts = now();
    tracker.handle_signal(
        &ability_activated_signal_with_source(effect_id as i64, local_player_id, target_id, ts),
        None,
    );

    assert_eq!(
        tracker.active_effects().count(),
        0,
        "No instance without ApplyEffect/ModifyCharges — credit is unknown"
    );
    let targets = tracker.take_new_targets();
    assert!(
        targets.iter().any(|t| t.entity_id == target_id),
        "Target should be registered for raid frames"
    );
}

#[test]
fn test_non_raid_frame_effect_does_not_register_target() {
    // A tracked cooldown/buff (e.g. a stim re-applied at zone-in) matches a
    // definition, but only raid-frame definitions may queue the target for
    // raid frame registration.
    let local_player_id: i64 = 1;

    let mut stim = make_effect(
        "stim",
        "Stim",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(4256329770205184)],
            source: EntityFilter::LocalPlayer,
            target: EntityFilter::Any,
            position: vec![],
        },
        None,
    );
    stim.display_targets = vec![super::definition::DisplayTarget::Cooldowns];

    let mut hot = make_effect(
        "hot",
        "HOT",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(814832605462528)],
            source: EntityFilter::LocalPlayer,
            target: EntityFilter::AnyPlayer,
            position: vec![],
        },
        Some(21.0),
    );
    hot.display_targets = vec![super::definition::DisplayTarget::RaidFrames];

    let mut tracker = make_tracker(vec![stim, hot]);
    tracker.set_player_context(local_player_id, 0);

    // Self-applied stim: tracked, but must not claim a raid-frame slot
    tracker.handle_signal(
        &effect_applied_signal_with_source(
            4256329770205184,
            local_player_id,
            local_player_id,
            now(),
        ),
        None,
    );
    assert_eq!(tracker.active_effects().count(), 1, "Stim is still tracked");
    assert!(
        tracker.take_new_targets().is_empty(),
        "Non-raid-frame effects must not register their target"
    );

    // A raid-frame HOT on self still registers
    tracker.handle_signal(
        &effect_applied_signal_with_source(
            814832605462528,
            local_player_id,
            local_player_id,
            now(),
        ),
        None,
    );
    assert!(
        tracker
            .take_new_targets()
            .iter()
            .any(|t| t.entity_id == local_player_id),
        "Raid-frame effects register their target"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// PvP: only classified teammates may register to raid frames
// ─────────────────────────────────────────────────────────────────────────────

/// Warzone encounter (The Pit) with an empty faction map
fn warzone_encounter() -> crate::encounter::CombatEncounter {
    let mut enc =
        crate::encounter::CombatEncounter::new(1, crate::encounter::ProcessingMode::Live);
    enc.set_area(Some(137438988866), Some("The Pit".to_string()), None);
    enc
}

#[test]
fn test_pvp_other_source_registers_only_classified_teammates() {
    let effect_id: u64 = 985226842996736;
    let local_player_id: i64 = 1;
    let other_healer_id: i64 = 50;
    let target_id: i64 = 51;

    let (local, others) = make_single_instance_pair(effect_id);
    let mut tracker = make_tracker(vec![local, others]);
    tracker.set_player_context(local_player_id, 0);
    let mut enc = warzone_encounter();

    // Another player's shell on an unclassified target: the effect is tracked,
    // but the target must not claim a raid-frame slot — not via the fresh apply...
    let ts = now();
    tracker.handle_signal(
        &effect_applied_signal_with_source(effect_id as i64, other_healer_id, target_id, ts),
        Some(&enc),
    );
    // ...the single-instance re-apply...
    tracker.handle_signal(
        &effect_applied_signal_with_source(
            effect_id as i64,
            other_healer_id,
            target_id,
            ts + chrono::Duration::seconds(5),
        ),
        Some(&enc),
    );
    // ...or the recast/refresh path.
    tracker.handle_signal(
        &ability_activated_signal_with_source(
            effect_id as i64,
            other_healer_id,
            target_id,
            ts + chrono::Duration::seconds(10),
        ),
        Some(&enc),
    );
    assert!(
        tracker.take_new_targets().is_empty(),
        "A cast with no known-friendly endpoint must not register in PvP"
    );

    // Once the SOURCE is classified as a teammate (they heal the local player),
    // their next recast proves the target is friendly too and registers them.
    enc.pvp_factions
        .observe_pair(other_healer_id, local_player_id, false, local_player_id);
    tracker.handle_signal(
        &ability_activated_signal_with_source(
            effect_id as i64,
            other_healer_id,
            target_id,
            ts + chrono::Duration::seconds(15),
        ),
        Some(&enc),
    );
    assert!(
        tracker
            .take_new_targets()
            .iter()
            .any(|t| t.entity_id == target_id),
        "A friendly healer's cast registers its (still unclassified) target"
    );
}

#[test]
fn test_pvp_local_source_registers_unclassified_target() {
    // The local player can only land beneficial effects on teammates, so a
    // local cast registers its target even before faction classification.
    let effect_id: u64 = 985226842996736;
    let local_player_id: i64 = 1;
    let target_id: i64 = 2;

    let (local, others) = make_single_instance_pair(effect_id);
    let mut tracker = make_tracker(vec![local, others]);
    tracker.set_player_context(local_player_id, 0);
    let enc = warzone_encounter();

    tracker.handle_signal(
        &effect_applied_signal_with_source(effect_id as i64, local_player_id, target_id, now()),
        Some(&enc),
    );
    assert!(
        tracker
            .take_new_targets()
            .iter()
            .any(|t| t.entity_id == target_id),
        "Local casts register their target without classification"
    );
}

#[test]
fn test_single_instance_charges_create_missing_instance() {
    // ModifyCharges reveals the true caster of an instance whose apply
    // predates the log: it creates the instance with correct credit.
    let effect_id: u64 = 985226842996736;
    let local_player_id: i64 = 1;
    let other_player_id: i64 = 99;
    let target_id: i64 = 2;

    let (local, others) = make_single_instance_pair(effect_id);
    let mut tracker = make_tracker(vec![local, others]);
    tracker.set_player_context(local_player_id, 0);

    let ts = now();
    tracker.handle_signal(
        &charges_changed_signal(effect_id as i64, other_player_id, target_id, 6, ts),
        None,
    );

    assert_eq!(
        tracker.active_effects().count(),
        1,
        "ModifyCharges should create the missing instance"
    );
    let effect = tracker.active_effects().next().unwrap();
    assert_eq!(effect.definition_id, "kolto_shell_others");
    assert_eq!(effect.source_entity_id, other_player_id);
    assert!(!effect.is_from_local_player);
    assert_eq!(effect.stacks, 6, "Stacks come from the event");
    assert!(
        effect.expires_at > Some(ts + chrono::Duration::seconds(179)),
        "Starts a full duration (real remaining time is unknown)"
    );

    // Local variant creates when the charges come from the local player
    let mut tracker = {
        let (local, others) = make_single_instance_pair(effect_id);
        make_tracker(vec![local, others])
    };
    tracker.set_player_context(local_player_id, 0);
    tracker.handle_signal(
        &charges_changed_signal(effect_id as i64, local_player_id, target_id, 5, ts),
        None,
    );
    let effect = tracker.active_effects().next().unwrap();
    assert_eq!(effect.definition_id, "kolto_shell");
    assert!(effect.is_from_local_player);
    assert_eq!(effect.stacks, 5);
}

#[test]
fn test_single_instance_merged_charges_update() {
    // Local instance refreshed by another player's apply (merged): subsequent
    // ModifyCharges from that other player must still update the stacks —
    // the merged instance is keyed under the local source.
    let effect_id: u64 = 985226842996736;
    let local_player_id: i64 = 1;
    let other_player_id: i64 = 99;
    let target_id: i64 = 2;

    let (local, others) = make_single_instance_pair(effect_id);
    let mut tracker = make_tracker(vec![local, others]);
    tracker.set_player_context(local_player_id, 0);

    let ts = now();
    tracker.handle_signal(
        &effect_applied_signal_with_source(effect_id as i64, local_player_id, target_id, ts),
        None,
    );
    let ts2 = ts + chrono::Duration::seconds(5);
    tracker.handle_signal(
        &effect_applied_signal_with_source(effect_id as i64, other_player_id, target_id, ts2),
        None,
    );
    assert_eq!(tracker.active_effects().count(), 1, "Merged into one instance");

    let ts3 = ts + chrono::Duration::seconds(10);
    tracker.handle_signal(
        &charges_changed_signal(effect_id as i64, other_player_id, target_id, 5, ts3),
        None,
    );

    let effect = tracker.active_effects().next().unwrap();
    assert_eq!(effect.definition_id, "kolto_shell");
    assert_eq!(
        effect.stacks, 5,
        "Charges from the other source must reach the merged instance"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Modifier refill_duration must work without is_refreshed_on_modify
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_charges_modifier_refills_duration_without_refresh_on_modify() {
    // Tactical Advantage style: duration refills only when charges INCREASE,
    // so is_refreshed_on_modify stays off and a SelfChargesChanged(Increased)
    // modifier with refill_duration handles the refresh instead.
    let effect_id: u64 = 12345;
    let local_player_id: i64 = 1;

    let mut def = make_effect(
        "tactical_advantage",
        "Tactical Advantage",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(effect_id)],
            source: EntityFilter::LocalPlayer,
            target: EntityFilter::LocalPlayer,
            position: vec![],
        },
        Some(10.0),
    );
    def.modifiers = vec![baras_types::EffectModifier {
        trigger: baras_types::Trigger::SelfChargesChanged {
            direction: Some(baras_types::ChargeDirection::Increased),
        },
        adjust_duration_secs: 0.0,
        requires_crit: false,
        refill_duration: true,
        icd_secs: None,
        icd_from_application: false,
        icd_affected_by_alacrity: false,
        max_duration_secs: None,
        cancel: false,
    }];

    let mut tracker = make_tracker(vec![def]);
    tracker.set_player_context(local_player_id, 0);

    let ts = now();
    tracker.handle_signal(
        &effect_applied_signal_with_source(effect_id as i64, local_player_id, local_player_id, ts),
        None,
    );
    let original_expires = tracker.active_effects().next().unwrap().expires_at;

    // Charges increase 4s later — duration should refill from that timestamp
    let ts2 = ts + chrono::Duration::seconds(4);
    tracker.handle_signal(
        &charges_changed_signal(effect_id as i64, local_player_id, local_player_id, 2, ts2),
        None,
    );
    let effect = tracker.active_effects().next().unwrap();
    assert_eq!(effect.stacks, 2);
    assert!(
        effect.expires_at > original_expires,
        "refill_duration modifier should extend expires_at on charge increase"
    );
    let refilled_expires = effect.expires_at;

    // Charges decrease — Increased direction filter must NOT refill
    let ts3 = ts + chrono::Duration::seconds(6);
    tracker.handle_signal(
        &charges_changed_signal(effect_id as i64, local_player_id, local_player_id, 1, ts3),
        None,
    );
    let effect = tracker.active_effects().next().unwrap();
    assert_eq!(effect.stacks, 1);
    assert_eq!(
        effect.expires_at, refilled_expires,
        "charge decrease should not refill duration"
    );
}

#[test]
fn test_local_player_cast_does_not_create_phantom_others_via_refresh() {
    // When the LOCAL player casts Kolto Shell, the AbilityActivated signal should
    // NOT create a phantom kolto_shell_others via late registration in the refresh path.
    // The source filter (source = "other_players") must reject the local player.
    let effect_id: u64 = 985226842996736;
    let local_player_id: i64 = 1;
    let target_id: i64 = 2;

    let mut kolto_shell = make_effect(
        "kolto_shell",
        "Kolto Shell",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(effect_id)],
            source: EntityFilter::LocalPlayer,
            target: EntityFilter::AnyPlayer,
            position: vec![],
        },
        Some(180.0),
    );
    kolto_shell.display_targets = vec![super::definition::DisplayTarget::RaidFrames];
    kolto_shell.refresh_abilities = vec![RefreshAbility::Simple(baras_types::AbilitySelector::Id(
        effect_id,
    ))];

    let mut kolto_shell_others = make_effect(
        "kolto_shell_others",
        "Other's Kolto Shell",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(effect_id)],
            source: EntityFilter::OtherPlayers,
            target: EntityFilter::AnyPlayer,
            position: vec![],
        },
        Some(180.0),
    );
    kolto_shell_others.display_targets = vec![super::definition::DisplayTarget::RaidFrames];
    kolto_shell_others.refresh_abilities = vec![RefreshAbility::Simple(
        baras_types::AbilitySelector::Id(effect_id),
    )];

    let mut tracker = make_tracker(vec![kolto_shell, kolto_shell_others]);
    tracker.set_player_context(local_player_id, 0);

    let ts = now();

    // Local player applies Kolto Shell (EffectApplied creates kolto_shell)
    tracker.handle_signal(
        &effect_applied_signal_with_source(effect_id as i64, local_player_id, target_id, ts),
        None,
    );

    assert_eq!(
        tracker.active_effects().count(),
        1,
        "Should have exactly 1 effect (kolto_shell)"
    );
    assert_eq!(
        tracker.active_effects().next().unwrap().definition_id,
        "kolto_shell"
    );

    // Local player recasts (AbilityActivated) — should refresh kolto_shell,
    // NOT create a phantom kolto_shell_others
    let ts2 = ts + chrono::Duration::seconds(30);
    tracker.handle_signal(
        &ability_activated_signal_with_source(effect_id as i64, local_player_id, target_id, ts2),
        None,
    );

    assert_eq!(
        tracker.active_effects().count(),
        1,
        "Should still have exactly 1 effect — no phantom kolto_shell_others"
    );
    let effect = tracker.active_effects().next().unwrap();
    assert_eq!(effect.definition_id, "kolto_shell");
    assert_eq!(
        effect.last_refreshed_at, ts2,
        "kolto_shell should be refreshed"
    );
}

// ─────────────────────────────────────────────────────────────────────────
// ResourceSpent modifier + cancel
// ─────────────────────────────────────────────────────────────────────────

fn resource_spent_signal(source_id: i64, amount: f32, timestamp: chrono::NaiveDateTime) -> GameSignal {
    GameSignal::ResourceSpent {
        source_id,
        source_entity_type: EntityType::Player,
        amount,
        timestamp,
    }
}

fn spend_modifier_def(modifier: baras_types::EffectModifier) -> (u64, i64, EffectTracker) {
    let effect_id: u64 = 4242;
    let local_player_id: i64 = 1;
    let mut def = make_effect(
        "spend_defensive",
        "Spend Defensive",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(effect_id)],
            source: EntityFilter::LocalPlayer,
            target: EntityFilter::LocalPlayer,
            position: vec![],
        },
        Some(10.0),
    );
    def.modifiers = vec![modifier];
    let mut tracker = make_tracker(vec![def]);
    tracker.set_player_context(local_player_id, 0);
    (effect_id, local_player_id, tracker)
}

#[test]
fn test_resource_spent_modifier_scales_by_amount() {
    let (effect_id, player, mut tracker) = spend_modifier_def(baras_types::EffectModifier {
        trigger: baras_types::Trigger::ResourceSpent { per_amount: 10.0 },
        adjust_duration_secs: 1.0,
        requires_crit: false,
        refill_duration: false,
        icd_secs: None,
        icd_from_application: false,
        icd_affected_by_alacrity: false,
        max_duration_secs: None,
        cancel: false,
    });

    let ts = now();
    tracker.handle_signals(
        &[effect_applied_signal_with_source(effect_id as i64, player, player, ts)],
        None,
    );
    let original = tracker.active_effects().next().unwrap().expires_at.unwrap();

    // 44 spent at 1s/10 → +4.4s
    let ts2 = ts + chrono::Duration::seconds(1);
    tracker.handle_signals(&[resource_spent_signal(player, 44.0, ts2)], None);
    let extended = tracker.active_effects().next().unwrap().expires_at.unwrap();
    assert_eq!((extended - original).num_milliseconds(), 4400);

    // Spend by a different entity must not proc
    tracker.handle_signals(&[resource_spent_signal(99, 100.0, ts2)], None);
    assert_eq!(tracker.active_effects().next().unwrap().expires_at.unwrap(), extended);
}

#[test]
fn test_resource_spent_modifier_cancels_effect() {
    let (effect_id, player, mut tracker) = spend_modifier_def(baras_types::EffectModifier {
        trigger: baras_types::Trigger::ResourceSpent { per_amount: 0.0 },
        adjust_duration_secs: 0.0,
        requires_crit: false,
        refill_duration: false,
        icd_secs: None,
        icd_from_application: false,
        icd_affected_by_alacrity: false,
        max_duration_secs: None,
        cancel: true,
    });

    let ts = now();
    tracker.handle_signals(
        &[effect_applied_signal_with_source(effect_id as i64, player, player, ts)],
        None,
    );
    assert!(tracker.active_effects().next().unwrap().removed_at.is_none());

    tracker.handle_signals(&[resource_spent_signal(player, 5.0, ts + chrono::Duration::seconds(1))], None);
    assert!(
        tracker.active_effects().next().unwrap().removed_at.is_some(),
        "cancel modifier should mark the effect removed"
    );
}

#[test]
fn test_killing_blow_modifier_extends_effect() {
    let (effect_id, player, mut tracker) = spend_modifier_def(baras_types::EffectModifier {
        trigger: baras_types::Trigger::KillingBlow,
        adjust_duration_secs: 3.0,
        requires_crit: false,
        refill_duration: false,
        icd_secs: None,
        icd_from_application: false,
        icd_affected_by_alacrity: false,
        max_duration_secs: None,
        cancel: false,
    });
    let ts = now();
    tracker.handle_signals(
        &[effect_applied_signal_with_source(effect_id as i64, player, player, ts)],
        None,
    );
    let original = tracker.active_effects().next().unwrap().expires_at.unwrap();

    let death = |killer_id: i64, t| GameSignal::EntityDeath {
        entity_id: 500,
        entity_type: EntityType::Npc,
        npc_id: 123,
        entity_name: "Add".to_string(),
        killer_id,
        timestamp: t,
    };
    // Kill by someone else: no proc
    tracker.handle_signals(&[death(99, ts + chrono::Duration::seconds(1))], None);
    assert_eq!(tracker.active_effects().next().unwrap().expires_at.unwrap(), original);
    // Kill by the holder: +3s
    tracker.handle_signals(&[death(player, ts + chrono::Duration::seconds(2))], None);
    let extended = tracker.active_effects().next().unwrap().expires_at.unwrap();
    assert_eq!((extended - original).num_milliseconds(), 3000);
}

#[test]
fn test_any_of_modifier_shares_icd_across_trigger_kinds() {
    let (effect_id, player, mut tracker) = spend_modifier_def(baras_types::EffectModifier {
        trigger: baras_types::Trigger::AnyOf {
            conditions: vec![
                baras_types::Trigger::ResourceSpent { per_amount: 0.0 },
                baras_types::Trigger::KillingBlow,
            ],
        },
        adjust_duration_secs: 2.0,
        requires_crit: false,
        refill_duration: false,
        icd_secs: Some(5.0),
        icd_from_application: false,
        icd_affected_by_alacrity: false,
        max_duration_secs: None,
        cancel: false,
    });
    let ts = now();
    tracker.handle_signals(
        &[effect_applied_signal_with_source(effect_id as i64, player, player, ts)],
        None,
    );
    let original = tracker.active_effects().next().unwrap().expires_at.unwrap();

    // Resource spend procs (+2s)
    tracker.handle_signals(&[resource_spent_signal(player, 5.0, ts + chrono::Duration::seconds(1))], None);
    let after_spend = tracker.active_effects().next().unwrap().expires_at.unwrap();
    assert_eq!((after_spend - original).num_milliseconds(), 2000);

    // Killing blow 1s later is blocked by the ICD shared with the spend
    let death = GameSignal::EntityDeath {
        entity_id: 500,
        entity_type: EntityType::Npc,
        npc_id: 123,
        entity_name: "Add".to_string(),
        killer_id: player,
        timestamp: ts + chrono::Duration::seconds(2),
    };
    tracker.handle_signals(&[death], None);
    assert_eq!(tracker.active_effects().next().unwrap().expires_at.unwrap(), after_spend);

    // After the ICD elapses, the killing blow procs
    let death = GameSignal::EntityDeath {
        entity_id: 501,
        entity_type: EntityType::Npc,
        npc_id: 123,
        entity_name: "Add".to_string(),
        killer_id: player,
        timestamp: ts + chrono::Duration::seconds(7),
    };
    tracker.handle_signals(&[death], None);
    let after_kill = tracker.active_effects().next().unwrap().expires_at.unwrap();
    assert_eq!((after_kill - after_spend).num_milliseconds(), 2000);
}

#[test]
fn test_icd_from_application_gates_first_proc() {
    let (effect_id, player, mut tracker) = spend_modifier_def(baras_types::EffectModifier {
        trigger: baras_types::Trigger::ResourceSpent { per_amount: 0.0 },
        adjust_duration_secs: 2.0,
        requires_crit: false,
        refill_duration: false,
        icd_secs: Some(3.0),
        icd_from_application: true,
        icd_affected_by_alacrity: false,
        max_duration_secs: None,
        cancel: false,
    });
    let ts = now();
    tracker.handle_signals(
        &[effect_applied_signal_with_source(effect_id as i64, player, player, ts)],
        None,
    );
    let original = tracker.active_effects().next().unwrap().expires_at.unwrap();

    // 1s after application: still inside the ICD window counted from applied_at
    tracker.handle_signals(&[resource_spent_signal(player, 5.0, ts + chrono::Duration::seconds(1))], None);
    assert_eq!(tracker.active_effects().next().unwrap().expires_at.unwrap(), original);

    // 3s after application: allowed
    tracker.handle_signals(&[resource_spent_signal(player, 5.0, ts + chrono::Duration::seconds(3))], None);
    let extended = tracker.active_effects().next().unwrap().expires_at.unwrap();
    assert_eq!((extended - original).num_milliseconds(), 2000);
}

#[test]
fn test_modifier_revives_interpolation_expired_effect() {
    let (effect_id, player, mut tracker) = spend_modifier_def(baras_types::EffectModifier {
        trigger: baras_types::Trigger::ResourceSpent { per_amount: 0.0 },
        adjust_duration_secs: 5.0,
        requires_crit: false,
        refill_duration: false,
        icd_secs: None,
        icd_from_application: false,
        icd_affected_by_alacrity: false,
        max_duration_secs: None,
        cancel: false,
    });
    let ts = now();
    tracker.handle_signals(
        &[effect_applied_signal_with_source(effect_id as i64, player, player, ts)],
        None,
    );
    let original = tracker.active_effects().next().unwrap().expires_at.unwrap();

    // Simulate tick() having expired the effect on interpolated time while
    // the log (e.g. a delayed flush) hadn't caught up yet.
    tracker.active_effects_mut().next().unwrap().timer_expired = true;

    // A log event from BEFORE expires_at proves the effect was still alive.
    tracker.handle_signals(&[resource_spent_signal(player, 5.0, ts + chrono::Duration::seconds(8))], None);
    let e = tracker.active_effects().next().unwrap();
    assert!(!e.timer_expired, "modifier should revive an interpolation-expired effect");
    assert_eq!((e.expires_at.unwrap() - original).num_milliseconds(), 5000);

    // A log event from AFTER expires_at must not resurrect a genuinely expired effect.
    tracker.active_effects_mut().next().unwrap().timer_expired = true;
    let late = tracker.active_effects().next().unwrap().expires_at.unwrap() + chrono::Duration::seconds(1);
    tracker.handle_signals(&[resource_spent_signal(player, 5.0, late)], None);
    assert!(tracker.active_effects().next().unwrap().timer_expired);
}

#[test]
fn test_icd_affected_by_alacrity_shortens_cooldown() {
    // 25% alacrity: 10s ICD → 8s
    let (effect_id, player, mut tracker) = spend_modifier_def(baras_types::EffectModifier {
        trigger: baras_types::Trigger::ResourceSpent { per_amount: 0.0 },
        adjust_duration_secs: 1.0,
        requires_crit: false,
        refill_duration: false,
        icd_secs: Some(10.0),
        icd_from_application: true,
        icd_affected_by_alacrity: true,
        max_duration_secs: Some(60.0),
        cancel: false,
    });
    tracker.set_alacrity(25.0);
    let ts = now();
    tracker.handle_signals(
        &[effect_applied_signal_with_source(effect_id as i64, player, player, ts)],
        None,
    );
    let original = tracker.active_effects().next().unwrap().expires_at.unwrap();

    // 7s: inside the 8s scaled ICD
    tracker.handle_signals(&[resource_spent_signal(player, 1.0, ts + chrono::Duration::seconds(7))], None);
    assert_eq!(tracker.active_effects().next().unwrap().expires_at.unwrap(), original);

    // 8s: scaled ICD elapsed (would still be blocked by the raw 10s)
    tracker.handle_signals(&[resource_spent_signal(player, 1.0, ts + chrono::Duration::seconds(8))], None);
    let extended = tracker.active_effects().next().unwrap().expires_at.unwrap();
    assert_eq!((extended - original).num_milliseconds(), 1000);
}

// ─────────────────────────────────────────────────────────────────────────────
// Area change handling
// ─────────────────────────────────────────────────────────────────────────────

fn area_entered_signal(area_id: i64, timestamp: chrono::NaiveDateTime) -> GameSignal {
    GameSignal::AreaEntered {
        area_id,
        area_name: String::new(),
        difficulty_id: 0,
        difficulty_name: String::new(),
        timestamp,
    }
}

/// Instanced areas log AreaEntered twice (plain, then difficulty-qualified)
/// with the game's effect restatement in between. The duplicate must not
/// wipe the just-restated effects.
#[test]
fn test_duplicate_area_entered_keeps_restated_effects() {
    let def = make_effect(
        "stim",
        "Stim",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(999)],
            source: EntityFilter::Any,
            target: EntityFilter::Any,
            position: vec![],
        },
        None,
    );
    let mut tracker = make_tracker(vec![def]);
    let ts = now();

    tracker.handle_signal(&effect_applied_signal(999, ts), None);
    assert_eq!(tracker.active_effects().filter(|e| e.removed_at.is_none()).count(), 1);

    // Zone into an instance: wipe, restatement, then duplicate AreaEntered
    tracker.handle_signal(&area_entered_signal(100, ts + chrono::Duration::milliseconds(10)), None);
    tracker.handle_signal(&effect_applied_signal(999, ts + chrono::Duration::milliseconds(30)), None);
    tracker.handle_signal(&area_entered_signal(100, ts + chrono::Duration::milliseconds(50)), None);

    assert_eq!(
        tracker.active_effects().filter(|e| e.removed_at.is_none()).count(),
        1,
        "duplicate AreaEntered for the same area must not wipe restated effects"
    );

    // A genuinely different area still wipes
    tracker.handle_signal(&area_entered_signal(200, ts + chrono::Duration::seconds(5)), None);
    assert_eq!(
        tracker.active_effects().filter(|e| e.removed_at.is_none()).count(),
        0,
        "entering a different area must clear active effects"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Deferred DOT refresh: target snapshot
// ═══════════════════════════════════════════════════════════════════════════

fn target_changed_signal(source_id: i64, target_id: i64, timestamp: chrono::NaiveDateTime) -> GameSignal {
    GameSignal::TargetChanged {
        source_id,
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        source_name: empty_istr(),
        target_id,
        target_entity_type: EntityType::Npc,
        target_name: empty_istr(),
        target_npc_id: 0,
        timestamp,
    }
}

fn damage_taken_signal(ability_id: i64, source_id: i64, target_id: i64, timestamp: chrono::NaiveDateTime) -> GameSignal {
    GameSignal::DamageTaken {
        ability_id,
        ability_name: empty_istr(),
        source_id,
        source_entity_type: EntityType::Player,
        source_name: empty_istr(),
        source_npc_id: 0,
        target_id,
        target_entity_type: EntityType::Npc,
        target_name: empty_istr(),
        target_npc_id: 0,
        timestamp,
        absorbed: 0,
        defense_type_id: 0,
        is_crit: false,
    }
}

/// Scorch-like DOT: silent refreshes confirmed by the next damage event from the ability.
fn dot_tracker_with_two_targets(
    scorch: u64,
    local: i64,
    dummy_a: i64,
    dummy_b: i64,
    ts: chrono::NaiveDateTime,
) -> EffectTracker {
    let mut def = make_effect(
        "scorch",
        "Scorch",
        Trigger::EffectApplied {
            effects: vec![EffectSelector::Id(scorch)],
            source: EntityFilter::LocalPlayer,
            target: EntityFilter::Any,
            position: vec![],
        },
        Some(30.0),
    );
    def.display_targets = vec![super::definition::DisplayTarget::DotTracker];
    def.refresh_abilities = vec![RefreshAbility::Simple(baras_types::AbilitySelector::Id(scorch))];

    let mut tracker = make_tracker(vec![def]);
    tracker.set_player_context(local, 0);
    tracker.handle_signal(&effect_applied_signal_with_source(scorch as i64, local, dummy_a, ts), None);
    tracker.handle_signal(
        &effect_applied_signal_with_source(scorch as i64, local, dummy_b, ts + chrono::Duration::seconds(3)),
        None,
    );
    assert_eq!(tracker.active_effects().count(), 2);
    tracker
}

fn expires_at(tracker: &EffectTracker, target_id: i64) -> chrono::NaiveDateTime {
    tracker
        .active_effects()
        .find(|e| e.target_entity_id == target_id)
        .and_then(|e| e.expires_at)
        .expect("effect with expiry")
}

#[test]
fn test_deferred_dot_refresh_ignores_bystander_tick() {
    // Mirrors the multi-dummy Scorch log: the first damage event after a refresh cast
    // is usually another target's periodic tick, not the hit on the cast target.
    let (scorch, local, a, b) = (3384812186370048u64, 1i64, 10i64, 20i64);
    let ts = now();
    let mut tracker = dot_tracker_with_two_targets(scorch, local, a, b, ts);
    let b_expiry = expires_at(&tracker, b);

    let s = |secs_x10: i64| ts + chrono::Duration::milliseconds(secs_x10 * 100);
    tracker.handle_signal(&target_changed_signal(local, a, s(100)), None);
    tracker.handle_signal(&ability_activated_signal_with_source(scorch as i64, local, local, s(110)), None);
    // Bystander B ticks 0.3s after the cast
    tracker.handle_signal(&damage_taken_signal(scorch as i64, local, b, s(113)), None);
    assert_eq!(expires_at(&tracker, b), b_expiry, "bystander tick must not claim the refresh");
    // Real hit on A lands 1.1s after the cast
    tracker.handle_signal(&damage_taken_signal(scorch as i64, local, a, s(121)), None);
    assert_eq!(expires_at(&tracker, a), s(121) + chrono::Duration::seconds(30), "A refreshed on its own hit");
    assert_eq!(expires_at(&tracker, b), b_expiry);
}

#[test]
fn test_deferred_dot_refresh_falls_back_when_target_is_self() {
    // A TargetSet on self carries no information: keep legacy first-damage behaviour.
    let (scorch, local, a, b) = (3384812186370048u64, 1i64, 10i64, 20i64);
    let ts = now();
    let mut tracker = dot_tracker_with_two_targets(scorch, local, a, b, ts);

    let s = |secs_x10: i64| ts + chrono::Duration::milliseconds(secs_x10 * 100);
    tracker.handle_signal(&target_changed_signal(local, local, s(100)), None);
    tracker.handle_signal(&ability_activated_signal_with_source(scorch as i64, local, local, s(110)), None);
    tracker.handle_signal(&damage_taken_signal(scorch as i64, local, b, s(113)), None);
    assert_eq!(expires_at(&tracker, b), s(113) + chrono::Duration::seconds(30));
}

#[test]
fn test_logged_apply_clears_deferred_refresh() {
    // A fresh application that the game logs is authoritative; a bystander's tick
    // right after it must not receive a phantom refresh.
    let (scorch, local, a, b) = (3384812186370048u64, 1i64, 10i64, 20i64);
    let ts = now();
    let mut tracker = dot_tracker_with_two_targets(scorch, local, a, b, ts);
    let b_expiry = expires_at(&tracker, b);

    let s = |secs_x10: i64| ts + chrono::Duration::milliseconds(secs_x10 * 100);
    // A's DOT expires naturally, then is re-cast with a logged ApplyEffect
    tracker.handle_signal(&effect_removed_signal_with_source(scorch as i64, local, a, s(300)), None);
    tracker.handle_signal(&target_changed_signal(local, a, s(302)), None);
    tracker.handle_signal(&ability_activated_signal_with_source(scorch as i64, local, local, s(305)), None);
    let mut apply = effect_applied_signal_with_source(scorch as i64, local, a, s(305));
    if let GameSignal::EffectApplied { action_id, .. } = &mut apply {
        *action_id = scorch as i64;
    }
    tracker.handle_signal(&apply, None);
    tracker.handle_signal(&damage_taken_signal(scorch as i64, local, b, s(311)), None);
    assert_eq!(expires_at(&tracker, b), b_expiry, "B must not get a phantom refresh");
    assert_eq!(expires_at(&tracker, a), s(305) + chrono::Duration::seconds(30));
}
