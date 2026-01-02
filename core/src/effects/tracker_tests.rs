//! Tests for EffectTracker signal handling and entity filters
//!
//! Verifies that:
//! - Effects are correctly created/removed by signals
//! - EntityFilter conditions match correctly
//! - Edge cases like death, combat end, area change are handled

use chrono::{Local, NaiveDateTime};

use crate::combat_log::EntityType;
use crate::context::IStr;
use crate::encounter::{CombatEncounter, ProcessingMode};
use crate::signal_processor::{GameSignal, SignalHandler};

use super::{AbilitySelector, DefinitionSet, EffectCategory, EffectDefinition, EffectTracker, EntityFilter, EffectSelector};

// ═══════════════════════════════════════════════════════════════════════════
// Test Helpers
// ═══════════════════════════════════════════════════════════════════════════

fn now() -> NaiveDateTime {
    Local::now().naive_local()
}

/// Create a basic effect definition for testing
fn make_effect(id: &str, name: &str, effect_ids: Vec<u64>) -> EffectDefinition {
    EffectDefinition {
        id: id.to_string(),
        name: name.to_string(),
        display_text: None,
        enabled: true,
        effects: effect_ids.into_iter().map(EffectSelector::Id).collect(),
        trigger: crate::effects::EffectTriggerMode::default(),
        refresh_abilities: Vec::new(),
        source: EntityFilter::Any,
        target: EntityFilter::Any,
        duration_secs: Some(10.0),
        can_be_refreshed: true,
        category: EffectCategory::Hot,
        color: None,
        max_stacks: 0,
        show_on_raid_frames: true,
        show_on_effects_overlay: false,
        show_at_secs: 0.0,
        persist_past_death: false,
        track_outside_combat: true,
        audio: Default::default(),
        on_apply_trigger_timer: None,
        on_expire_trigger_timer: None,
        encounters: Vec::new(),
        alert_near_expiration: false,
        alert_threshold_secs: 3.0,
    }
}

/// Create a definition set with given effects
fn make_definitions(effects: Vec<EffectDefinition>) -> DefinitionSet {
    let mut set = DefinitionSet::new();
    set.add_definitions(effects, false);
    set
}

/// Create a tracker in live mode with local player set
fn make_tracker(effects: Vec<EffectDefinition>, local_player_id: i64) -> EffectTracker {
    let defs = make_definitions(effects);
    let mut tracker = EffectTracker::new(defs);
    tracker.set_live_mode(true);
    tracker.set_local_player(local_player_id);
    tracker
}

/// Create a mock encounter with the given entity IDs registered as bosses
fn make_encounter_with_bosses(boss_ids: &[i64]) -> CombatEncounter {
    let mut enc = CombatEncounter::new(1, ProcessingMode::Live);
    for &id in boss_ids {
        enc.hp_by_entity.insert(id, 100.0);
    }
    enc
}

// ═══════════════════════════════════════════════════════════════════════════
// Signal Handling Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_effect_applied_creates_active_effect() {
    let effect = make_effect("kolto_probe", "Kolto Probe", vec![12345]);
    let mut tracker = make_tracker(vec![effect], 1);

    let signal = GameSignal::EffectApplied {
        effect_id: 12345,
        effect_name: IStr::default(),
        action_id: 100,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 2,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: now(),
        charges: None,
    };
    tracker.handle_signal(&signal, None);

    assert!(tracker.has_active_effects(), "Effect should be active");
    let effects: Vec<_> = tracker.active_effects().collect();
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0].definition_id, "kolto_probe");
}

#[test]
fn test_effect_removed_marks_inactive() {
    let effect = make_effect("debuff", "Debuff", vec![999]);
    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();

    // Apply effect
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 999,
        effect_name: IStr::default(),
        action_id: 100,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 2,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    assert!(tracker.has_active_effects());

    // Remove effect
    tracker.handle_signal(&GameSignal::EffectRemoved {
        effect_id: 999,
        effect_name: crate::context::IStr::default(),
        source_id: 1,
        source_entity_type: crate::combat_log::EntityType::Player,
        source_name: crate::context::IStr::default(),
        target_id: 2,
        target_entity_type: crate::combat_log::EntityType::Player,
        target_name: crate::context::IStr::default(),
        timestamp: ts,
    }, None);

    // Effect is marked removed but still in map until tick cleans it
    let effects: Vec<_> = tracker.active_effects().collect();
    assert_eq!(effects.len(), 1);
    assert!(effects[0].removed_at.is_some(), "Effect should be marked removed");
}

#[test]
fn test_charges_changed_updates_stacks() {
    let effect = make_effect("stacking_buff", "Stacking Buff", vec![555]);
    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();

    // Apply with initial charges
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 555,
        effect_name: IStr::default(),
        action_id: 100,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 2,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: Some(1),
    }, None);

    // Update charges
    tracker.handle_signal(&GameSignal::EffectChargesChanged {
        effect_id: 555,
        effect_name: IStr::default(),
        action_id: 100,
        action_name: IStr::default(),
        target_id: 2,
        timestamp: ts,
        charges: 3,
    }, None);

    let effects: Vec<_> = tracker.active_effects().collect();
    assert_eq!(effects[0].stacks, 3);
}

#[test]
fn test_entity_death_clears_effects() {
    let effect = make_effect("hot", "HoT", vec![111]);
    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();

    // Apply effect to target 2
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 111,
        effect_name: IStr::default(),
        action_id: 100,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 2,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    assert!(tracker.has_active_effects());

    // Target dies
    tracker.handle_signal(&GameSignal::EntityDeath {
        entity_id: 2,
        entity_type: EntityType::Player,
        npc_id: 0,
        entity_name: "Target".to_string(),
        timestamp: ts,
    }, None);

    let effects: Vec<_> = tracker.active_effects().collect();
    assert!(effects[0].removed_at.is_some(), "Effect should be marked removed on death");
}

#[test]
fn test_persist_past_death_keeps_effect() {
    let mut effect = make_effect("persistent", "Persistent Buff", vec![222]);
    effect.persist_past_death = true;
    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();

    // Apply effect
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 222,
        effect_name: IStr::default(),
        action_id: 100,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 2,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    // Target dies
    tracker.handle_signal(&GameSignal::EntityDeath {
        entity_id: 2,
        entity_type: EntityType::Player,
        npc_id: 0,
        entity_name: "Target".to_string(),
        timestamp: ts,
    }, None);

    let effects: Vec<_> = tracker.active_effects().collect();
    assert!(effects[0].removed_at.is_none(), "Persistent effect should survive death");
}

#[test]
fn test_combat_end_clears_combat_only_effects() {
    let mut combat_only = make_effect("combat_buff", "Combat Buff", vec![333]);
    combat_only.track_outside_combat = false;

    let outside_combat = make_effect("persistent_buff", "Persistent Buff", vec![444]);
    // track_outside_combat defaults to true

    let mut tracker = make_tracker(vec![combat_only, outside_combat], 1);
    let ts = now();

    // Start combat
    tracker.handle_signal(&GameSignal::CombatStarted {
        timestamp: ts,
        encounter_id: 1,
    }, None);

    // Apply both effects
    for effect_id in [333, 444] {
        tracker.handle_signal(&GameSignal::EffectApplied {
            effect_id,
            effect_name: IStr::default(),
            action_id: 100,
        action_name: IStr::default(),
            source_id: 1,
            source_name: IStr::default(),
            source_entity_type: EntityType::Player,
            source_npc_id: 0,
            target_id: 2,
            target_name: IStr::default(),
            target_entity_type: EntityType::Player,
            target_npc_id: 0,
            timestamp: ts,
            charges: None,
        }, None);
    }

    assert_eq!(tracker.active_effects().count(), 2);

    // End combat
    tracker.handle_signal(&GameSignal::CombatEnded {
        timestamp: ts,
        encounter_id: 1,
    }, None);

    let effects: Vec<_> = tracker.active_effects().collect();
    let combat_effect = effects.iter().find(|e| e.definition_id == "combat_buff");
    let persistent_effect = effects.iter().find(|e| e.definition_id == "persistent_buff");

    assert!(combat_effect.unwrap().removed_at.is_some(), "Combat-only effect should be removed");
    assert!(persistent_effect.unwrap().removed_at.is_none(), "Persistent effect should survive");
}

#[test]
fn test_area_entered_clears_all_effects() {
    let effect = make_effect("buff", "Buff", vec![555]);
    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();

    // Apply effect
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 555,
        effect_name: IStr::default(),
        action_id: 100,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 2,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    // Change area
    tracker.handle_signal(&GameSignal::AreaEntered {
        area_id: 999,
        area_name: "New Zone".to_string(),
        difficulty_id: 0,
        difficulty_name: String::new(),
        timestamp: ts,
    }, None);

    let effects: Vec<_> = tracker.active_effects().collect();
    assert!(effects[0].removed_at.is_some(), "Effect should be cleared on area change");
}

#[test]
fn test_ability_activated_refreshes_effect() {
    let mut effect = make_effect("refreshable", "Refreshable Hot", vec![666]);
    effect.refresh_abilities = vec![AbilitySelector::Id(100)]; // Ability 100 can refresh this effect

    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();

    // Apply effect
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 666,
        effect_name: IStr::default(),
        action_id: 100,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 2,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    let effects: Vec<_> = tracker.active_effects().collect();
    let first_refreshed = effects[0].last_refreshed_at;

    // Use refresh ability
    let later = ts + chrono::Duration::seconds(5);
    tracker.handle_signal(&GameSignal::AbilityActivated {
        ability_id: 100,
        ability_name: IStr::default(),
        source_id: 1,
        source_entity_type: EntityType::Player,
        source_name: IStr::default(),
        source_npc_id: 0,
        target_id: 2,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: later,
    }, None);

    let effects: Vec<_> = tracker.active_effects().collect();
    assert!(effects[0].last_refreshed_at > first_refreshed, "Effect should be refreshed");
}

#[test]
fn test_player_initialized_sets_local_player() {
    let effect = make_effect("local_only", "Local Only", vec![777]);
    let defs = make_definitions(vec![effect]);
    let mut tracker = EffectTracker::new(defs);
    tracker.set_live_mode(true);
    // Don't set local player manually

    let ts = now();

    // Initialize player
    tracker.handle_signal(&GameSignal::PlayerInitialized {
        entity_id: 42,
        timestamp: ts,
    }, None);

    // Now effects from player 42 should be tracked as local
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 777,
        effect_name: IStr::default(),
        action_id: 100,
        action_name: IStr::default(),
        source_id: 42,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 42,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    let effects: Vec<_> = tracker.active_effects().collect();
    assert_eq!(effects.len(), 1);
    assert!(effects[0].is_from_local_player, "Effect should be marked as from local player");
}

#[test]
fn test_boss_filter_uses_encounter_context() {
    let mut effect = make_effect("boss_debuff", "Boss Debuff", vec![888]);
    effect.target = EntityFilter::Boss;

    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();
    let encounter = make_encounter_with_bosses(&[999]);

    // Effect on NPC won't match Boss filter without encounter context
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 888,
        effect_name: IStr::default(),
        action_id: 100,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 999,
        target_name: IStr::default(),
        target_entity_type: EntityType::Npc,
        target_npc_id: 999,
        timestamp: ts,
        charges: None,
    }, None);

    assert_eq!(tracker.active_effects().count(), 0, "Should not match - no encounter context");

    // Now try again with encounter context that knows about boss
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 888,
        effect_name: IStr::default(),
        action_id: 100,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 999,
        target_name: IStr::default(),
        target_entity_type: EntityType::Npc,
        target_npc_id: 999,
        timestamp: ts,
        charges: None,
    }, Some(&encounter));

    assert_eq!(tracker.active_effects().count(), 1, "Should match with encounter context");
}

#[test]
fn test_live_mode_required_for_tracking() {
    let effect = make_effect("test", "Test", vec![111]);
    let defs = make_definitions(vec![effect]);
    let mut tracker = EffectTracker::new(defs);
    // Don't enable live mode

    let signal = GameSignal::EffectApplied {
        effect_id: 111,
        effect_name: IStr::default(),
        action_id: 100,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 2,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: now(),
        charges: None,
    };
    tracker.handle_signal(&signal, None);

    assert!(!tracker.has_active_effects(), "Should not track in historical mode");
}

// ═══════════════════════════════════════════════════════════════════════════
// EntityFilter Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_filter_local_player() {
    let mut effect = make_effect("local", "Local Only", vec![100]);
    effect.source = EntityFilter::LocalPlayer;

    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();

    // From local player - should match
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 1, // Local player
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 2,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    assert_eq!(tracker.active_effects().count(), 1, "Local player source should match");
}

#[test]
fn test_filter_local_player_rejects_other() {
    let mut effect = make_effect("local", "Local Only", vec![100]);
    effect.source = EntityFilter::LocalPlayer;

    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();

    // From other player - should NOT match
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 2, // Other player
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 1,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    assert_eq!(tracker.active_effects().count(), 0, "Other player source should not match LocalPlayer filter");
}

#[test]
fn test_filter_other_players() {
    let mut effect = make_effect("others", "From Others", vec![100]);
    effect.source = EntityFilter::OtherPlayers;

    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();

    // From other player - should match
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 2,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 1,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    assert_eq!(tracker.active_effects().count(), 1);

    // From local - should NOT match
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 3,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    // Still only 1 effect
    assert_eq!(tracker.active_effects().count(), 1, "Local player should not match OtherPlayers");
}

#[test]
fn test_filter_any_player() {
    let mut effect = make_effect("any_player", "Any Player", vec![100]);
    effect.source = EntityFilter::AnyPlayer;

    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();

    // From local
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 2,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    // From other
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 3,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 4,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    assert_eq!(tracker.active_effects().count(), 2, "Both players should match AnyPlayer");
}

#[test]
fn test_filter_any_npc() {
    let mut effect = make_effect("npc", "NPC Effect", vec![100]);
    effect.target = EntityFilter::AnyNpc;

    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();

    // On NPC - should match
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 999,
        target_name: IStr::default(),
        target_entity_type: EntityType::Npc,
        target_npc_id: 999,
        timestamp: ts,
        charges: None,
    }, None);

    assert_eq!(tracker.active_effects().count(), 1);

    // On Player - should NOT match
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 2,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    assert_eq!(tracker.active_effects().count(), 1, "Player should not match AnyNpc");
}

#[test]
fn test_filter_npc_except_boss() {
    let mut effect = make_effect("trash", "Trash Mob Debuff", vec![100]);
    effect.target = EntityFilter::NpcExceptBoss;

    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();
    let encounter = make_encounter_with_bosses(&[999]);

    // On boss - should NOT match
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 999,
        target_name: IStr::default(),
        target_entity_type: EntityType::Npc,
        target_npc_id: 999,
        timestamp: ts,
        charges: None,
    }, Some(&encounter));

    assert_eq!(tracker.active_effects().count(), 0, "Boss should not match NpcExceptBoss");

    // On non-boss NPC - should match
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 888,
        target_name: IStr::default(),
        target_entity_type: EntityType::Npc,
        target_npc_id: 888,
        timestamp: ts,
        charges: None,
    }, Some(&encounter));

    assert_eq!(tracker.active_effects().count(), 1, "Non-boss NPC should match");
}

#[test]
fn test_filter_companion() {
    let mut effect = make_effect("companion", "Companion Buff", vec![100]);
    effect.target = EntityFilter::AnyCompanion;

    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();

    // On companion - should match
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 50,
        target_name: IStr::default(),
        target_entity_type: EntityType::Companion,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    assert_eq!(tracker.active_effects().count(), 1);

    // On player - should NOT match
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 2,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    assert_eq!(tracker.active_effects().count(), 1, "Player should not match AnyCompanion");
}

#[test]
fn test_filter_any_player_or_companion() {
    let mut effect = make_effect("friendly", "Friendly Buff", vec![100]);
    effect.target = EntityFilter::AnyPlayerOrCompanion;

    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();

    // On player
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 2,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    // On companion
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 50,
        target_name: IStr::default(),
        target_entity_type: EntityType::Companion,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    assert_eq!(tracker.active_effects().count(), 2, "Both player and companion should match");

    // On NPC - should NOT match
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 999,
        target_name: IStr::default(),
        target_entity_type: EntityType::Npc,
        target_npc_id: 999,
        timestamp: ts,
        charges: None,
    }, None);

    assert_eq!(tracker.active_effects().count(), 2, "NPC should not match");
}

#[test]
fn test_filter_any_matches_everything() {
    let mut effect = make_effect("universal", "Universal", vec![100]);
    effect.source = EntityFilter::Any;
    effect.target = EntityFilter::Any;

    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();

    // Player to Player
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 2,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    // NPC to NPC
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 888,
        source_name: IStr::default(),
        source_entity_type: EntityType::Npc,
        source_npc_id: 888,
        target_id: 999,
        target_name: IStr::default(),
        target_entity_type: EntityType::Npc,
        target_npc_id: 999,
        timestamp: ts,
        charges: None,
    }, None);

    // Player to Companion
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 50,
        target_name: IStr::default(),
        target_entity_type: EntityType::Companion,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    assert_eq!(tracker.active_effects().count(), 3, "Any filter should match all");
}

#[test]
fn test_non_matching_effect_id_ignored() {
    let effect = make_effect("specific", "Specific Effect", vec![12345]);
    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();

    // Wrong effect ID
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 99999,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 2,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    assert_eq!(tracker.active_effects().count(), 0, "Non-matching effect ID should be ignored");
}

#[test]
fn test_target_tracking_for_ability_refresh() {
    let mut effect = make_effect("healing", "Healing Effect", vec![100]);
    effect.refresh_abilities = vec![AbilitySelector::Id(200)];

    let mut tracker = make_tracker(vec![effect], 1);
    let ts = now();

    // Track target
    tracker.handle_signal(&GameSignal::TargetChanged {
        source_id: 1,
        source_npc_id: 0,
        source_name: IStr::default(),
        target_id: 99,
        target_name: IStr::default(),
        target_npc_id: 0,
        target_entity_type: EntityType::Player,
        timestamp: ts,
    }, None);

    // Apply effect to target
    tracker.handle_signal(&GameSignal::EffectApplied {
        effect_id: 100,
        effect_name: IStr::default(),
        action_id: 1,
        action_name: IStr::default(),
        source_id: 1,
        source_name: IStr::default(),
        source_entity_type: EntityType::Player,
        source_npc_id: 0,
        target_id: 99,
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: ts,
        charges: None,
    }, None);

    // Use ability with self-target (should resolve to tracked target)
    let later = ts + chrono::Duration::seconds(2);
    tracker.handle_signal(&GameSignal::AbilityActivated {
        ability_id: 200,
        ability_name: IStr::default(),
        source_id: 1,
        source_entity_type: EntityType::Player,
        source_name: IStr::default(),
        source_npc_id: 0,
        target_id: 1, // Self-target
        target_name: IStr::default(),
        target_entity_type: EntityType::Player,
        target_npc_id: 0,
        timestamp: later,
    }, None);

    let effects: Vec<_> = tracker.active_effects().collect();
    assert!(effects[0].last_refreshed_at > ts, "Effect should be refreshed via tracked target");

    // Clear target
    tracker.handle_signal(&GameSignal::TargetCleared {
        source_id: 1,
        timestamp: later,
    }, None);
}

// ═══════════════════════════════════════════════════════════════════════════
// Integration Tests with Real Log Data
// ═══════════════════════════════════════════════════════════════════════════

use std::fs::File;
use std::io::Read as _;
use std::path::Path;
use crate::combat_log::LogParser;
use crate::signal_processor::EventProcessor;
use crate::state::SessionCache;

/// Parse a fixture file and pipe signals through an EffectTracker
fn run_effect_integration(fixture_path: &Path, effect: EffectDefinition) -> (usize, usize) {
    let mut file = File::open(fixture_path).expect("Failed to open fixture file");
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).expect("Failed to read file");
    let content = String::from_utf8_lossy(&bytes);

    let parser = LogParser::new(Local::now().naive_local());
    let mut processor = EventProcessor::new();
    let mut cache = SessionCache::default();

    let defs = make_definitions(vec![effect]);
    let mut tracker = EffectTracker::new(defs);
    tracker.set_live_mode(true);

    let mut total_applied = 0;
    let mut total_removed = 0;

    for (line_num, line) in content.lines().enumerate() {
        if let Some(event) = parser.parse_line(line_num as u64, line) {
            let signals = processor.process_event(event, &mut cache);
            for signal in signals {
                let before = tracker.active_effects().count();
                tracker.handle_signal(&signal, None);
                let after = tracker.active_effects().count();

                if after > before {
                    total_applied += after - before;
                } else if after < before {
                    total_removed += before - after;
                }
            }
        }
    }

    (total_applied, total_removed)
}

#[test]
fn test_integration_effect_tracker_with_real_log() {
    let fixture_path = Path::new("../test-log-files/fixtures/bestia_pull.txt");
    if !fixture_path.exists() {
        eprintln!("Skipping test: fixture file not found at {:?}", fixture_path);
        return;
    }

    // Track a common buff/debuff - this tests the full pipeline works
    let effect = make_effect("test_effect", "Test Effect", vec![3283085219856384]);

    let (applied, removed) = run_effect_integration(fixture_path, effect);

    // The pipeline should process without crashing
    // Actual counts depend on what's in the log
    eprintln!("Effects applied: {}, removed: {}", applied, removed);
}

#[test]
fn test_integration_combat_clears_effects() {
    let fixture_path = Path::new("../test-log-files/fixtures/bestia_pull.txt");
    if !fixture_path.exists() {
        eprintln!("Skipping test: fixture file not found at {:?}", fixture_path);
        return;
    }

    let mut file = File::open(fixture_path).expect("Failed to open fixture file");
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).expect("Failed to read file");
    let content = String::from_utf8_lossy(&bytes);

    let parser = LogParser::new(Local::now().naive_local());
    let mut processor = EventProcessor::new();
    let mut cache = SessionCache::default();

    let effect = make_effect("combat_effect", "Combat Effect", vec![3283085219856384]);
    let defs = make_definitions(vec![effect]);
    let mut tracker = EffectTracker::new(defs);
    tracker.set_live_mode(true);

    let mut saw_combat_end = false;
    let mut effects_at_combat_end = 0;

    for (line_num, line) in content.lines().enumerate() {
        if let Some(event) = parser.parse_line(line_num as u64, line) {
            let signals = processor.process_event(event, &mut cache);
            for signal in &signals {
                tracker.handle_signal(signal, None);

                if matches!(signal, GameSignal::CombatEnded { .. }) {
                    saw_combat_end = true;
                    effects_at_combat_end = tracker.active_effects().count();
                }
            }
        }
    }

    if saw_combat_end {
        assert_eq!(
            effects_at_combat_end, 0,
            "CombatEnded should clear all non-persistent effects"
        );
    }
}
