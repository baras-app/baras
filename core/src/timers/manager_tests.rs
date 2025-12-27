//! Tests for TimerManager signal handling
//!
//! Verifies that timers are correctly activated by various signals.

use chrono::Local;

use crate::events::{GameSignal, SignalHandler};
use crate::effects::EntityFilter;
use super::{TimerDefinition, TimerManager, TimerTrigger};

/// Create a test timer with the given trigger
fn make_timer(id: &str, name: &str, trigger: TimerTrigger, duration: f32) -> TimerDefinition {
    TimerDefinition {
        id: id.to_string(),
        name: name.to_string(),
        trigger,
        duration_secs: duration,
        color: [200, 200, 200, 255],
        enabled: true,
        can_be_refreshed: false,
        triggers_timer: None,
        cancel_on_timer: None,
        repeats: 0,
        alert_at_secs: None,
        alert_text: None,
        audio_file: None,
        show_on_raid_frames: false,
        source: EntityFilter::Any,
        target: EntityFilter::Any,
        encounters: Vec::new(),
        boss: None,
        difficulties: Vec::new(),
        phases: Vec::new(),
        counter_condition: None,
    }
}

fn now() -> chrono::NaiveDateTime {
    Local::now().naive_local()
}

#[test]
fn test_combat_start_triggers_timer() {
    let mut manager = TimerManager::new();

    let timer = make_timer(
        "enrage",
        "Enrage Timer",
        TimerTrigger::CombatStart,
        300.0,
    );
    manager.load_definitions(vec![timer]);

    // No timers active initially
    assert!(manager.active_timers().is_empty());

    // Send CombatStarted signal
    let signal = GameSignal::CombatStarted {
        timestamp: now(),
        encounter_id: 1,
    };
    manager.handle_signal(&signal);

    // Timer should now be active
    let active = manager.active_timers();
    assert_eq!(active.len(), 1, "Expected 1 active timer");
    assert_eq!(active[0].name, "Enrage Timer");
}

#[test]
fn test_ability_cast_triggers_timer() {
    let mut manager = TimerManager::new();

    let timer = make_timer(
        "dread_scream",
        "Dread Scream",
        TimerTrigger::AbilityCast { ability_ids: vec![3302391763959808] },
        15.0,
    );
    manager.load_definitions(vec![timer]);

    // Send AbilityActivated signal
    let signal = GameSignal::AbilityActivated {
        ability_id: 3302391763959808,
        source_id: 12345,
        source_npc_id: 0,
        target_id: 0,
        target_name: crate::context::IStr::default(),
        target_entity_type: crate::combat_log::EntityType::Player,
        target_npc_id: 0,
        timestamp: now(),
    };
    manager.handle_signal(&signal);

    let active = manager.active_timers();
    assert_eq!(active.len(), 1, "Expected 1 active timer");
    assert_eq!(active[0].name, "Dread Scream");
}

#[test]
fn test_effect_applied_triggers_timer() {
    let mut manager = TimerManager::new();

    let timer = make_timer(
        "debuff_tracker",
        "Debuff Active",
        TimerTrigger::EffectApplied { effect_ids: vec![999999] },
        10.0,
    );
    manager.load_definitions(vec![timer]);

    let signal = GameSignal::EffectApplied {
        effect_id: 999999,
        action_id: 0,
        source_id: 1,
        source_name: crate::context::IStr::default(),
        source_entity_type: crate::combat_log::EntityType::Npc,
        source_npc_id: 12345,
        target_id: 2,
        target_name: crate::context::IStr::default(),
        target_entity_type: crate::combat_log::EntityType::Player,
        target_npc_id: 0,
        timestamp: now(),
        charges: None,
    };
    manager.handle_signal(&signal);

    let active = manager.active_timers();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].name, "Debuff Active");
}

#[test]
fn test_npc_first_seen_triggers_timer() {
    let mut manager = TimerManager::new();

    // Timer for Dread Monster spawn
    let timer = make_timer(
        "monster_spawn",
        "Dread Monster Spawned",
        TimerTrigger::EntityFirstSeen { npc_id: 3291675820556288 },
        30.0,
    );
    manager.load_definitions(vec![timer]);

    let signal = GameSignal::NpcFirstSeen {
        entity_id: 12345,
        npc_id: 3291675820556288,
        entity_name: "Dread Monster".to_string(),
        timestamp: now(),
    };
    manager.handle_signal(&signal);

    let active = manager.active_timers();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].name, "Dread Monster Spawned");
}

#[test]
fn test_anyof_condition_triggers_on_either() {
    let mut manager = TimerManager::new();

    // Timer that triggers on EITHER ability
    let timer = make_timer(
        "multi_trigger",
        "Multi Trigger",
        TimerTrigger::AnyOf {
            conditions: vec![
                TimerTrigger::AbilityCast { ability_ids: vec![111] },
                TimerTrigger::AbilityCast { ability_ids: vec![222] },
            ],
        },
        10.0,
    );
    manager.load_definitions(vec![timer]);

    // First ability should trigger
    let signal1 = GameSignal::AbilityActivated {
        ability_id: 111,
        source_id: 1,
        source_npc_id: 0,
        target_id: 0,
        target_name: crate::context::IStr::default(),
        target_entity_type: crate::combat_log::EntityType::Player,
        target_npc_id: 0,
        timestamp: now(),
    };
    manager.handle_signal(&signal1);

    assert_eq!(manager.active_timers().len(), 1, "First condition should trigger");

    // Clear timers via CombatEnded and test second ability
    manager.handle_signal(&GameSignal::CombatEnded {
        timestamp: now(),
        encounter_id: 1,
    });

    let signal2 = GameSignal::AbilityActivated {
        ability_id: 222,
        source_id: 1,
        source_npc_id: 0,
        target_id: 0,
        target_name: crate::context::IStr::default(),
        target_entity_type: crate::combat_log::EntityType::Player,
        target_npc_id: 0,
        timestamp: now(),
    };
    manager.handle_signal(&signal2);

    assert_eq!(manager.active_timers().len(), 1, "Second condition should also trigger");
}

#[test]
fn test_anyof_mixed_trigger_types() {
    let mut manager = TimerManager::new();

    // Timer that triggers on combat start OR specific ability
    let timer = make_timer(
        "mixed_trigger",
        "Mixed Trigger",
        TimerTrigger::AnyOf {
            conditions: vec![
                TimerTrigger::CombatStart,
                TimerTrigger::AbilityCast { ability_ids: vec![333] },
            ],
        },
        20.0,
    );
    manager.load_definitions(vec![timer]);

    // Combat start should trigger
    let signal = GameSignal::CombatStarted {
        timestamp: now(),
        encounter_id: 1,
    };
    manager.handle_signal(&signal);

    assert_eq!(manager.active_timers().len(), 1, "CombatStart in AnyOf should trigger");
}

#[test]
fn test_cancel_on_timer() {
    let mut manager = TimerManager::new();

    // Timer A starts on combat, cancelled when B starts
    let timer_a = TimerDefinition {
        id: "timer_a".to_string(),
        name: "Timer A".to_string(),
        trigger: TimerTrigger::CombatStart,
        duration_secs: 60.0,
        cancel_on_timer: Some("timer_b".to_string()),
        ..make_timer("", "", TimerTrigger::CombatStart, 0.0)
    };

    // Timer B starts on ability
    let timer_b = make_timer(
        "timer_b",
        "Timer B",
        TimerTrigger::AbilityCast { ability_ids: vec![444] },
        30.0,
    );

    manager.load_definitions(vec![timer_a, timer_b]);

    // Start combat - Timer A should be active
    manager.handle_signal(&GameSignal::CombatStarted {
        timestamp: now(),
        encounter_id: 1,
    });

    let active = manager.active_timers();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].name, "Timer A");

    // Trigger Timer B - Timer A should be cancelled
    manager.handle_signal(&GameSignal::AbilityActivated {
        ability_id: 444,
        source_id: 1,
        source_npc_id: 0,
        target_id: 0,
        target_name: crate::context::IStr::default(),
        target_entity_type: crate::combat_log::EntityType::Player,
        target_npc_id: 0,
        timestamp: now(),
    });

    let active = manager.active_timers();
    assert_eq!(active.len(), 1, "Only Timer B should remain");
    assert_eq!(active[0].name, "Timer B");
}

#[test]
fn test_wrong_ability_does_not_trigger() {
    let mut manager = TimerManager::new();

    let timer = make_timer(
        "specific",
        "Specific Ability",
        TimerTrigger::AbilityCast { ability_ids: vec![12345] },
        10.0,
    );
    manager.load_definitions(vec![timer]);

    // Wrong ability ID
    let signal = GameSignal::AbilityActivated {
        ability_id: 99999, // Different ID
        source_id: 1,
        source_npc_id: 0,
        target_id: 0,
        target_name: crate::context::IStr::default(),
        target_entity_type: crate::combat_log::EntityType::Player,
        target_npc_id: 0,
        timestamp: now(),
    };
    manager.handle_signal(&signal);

    assert!(manager.active_timers().is_empty(), "Wrong ability should not trigger");
}

#[test]
fn test_combat_end_clears_timers() {
    let mut manager = TimerManager::new();

    let timer = make_timer(
        "combat_timer",
        "Combat Timer",
        TimerTrigger::CombatStart,
        300.0,
    );
    manager.load_definitions(vec![timer]);

    // Start combat
    manager.handle_signal(&GameSignal::CombatStarted {
        timestamp: now(),
        encounter_id: 1,
    });
    assert_eq!(manager.active_timers().len(), 1);

    // End combat
    manager.handle_signal(&GameSignal::CombatEnded {
        timestamp: now(),
        encounter_id: 1,
    });

    assert!(manager.active_timers().is_empty(), "CombatEnded should clear timers");
}

#[test]
fn test_timer_expires_triggers_chain() {
    let mut manager = TimerManager::new();

    // Timer A expires after 2 seconds and triggers Timer B
    let timer_a = TimerDefinition {
        triggers_timer: Some("timer_b".to_string()),
        ..make_timer("timer_a", "Timer A", TimerTrigger::CombatStart, 2.0)
    };

    // Timer B is triggered by Timer A expiring
    let timer_b = make_timer(
        "timer_b",
        "Timer B",
        TimerTrigger::TimerExpires { timer_id: "timer_a".to_string() },
        10.0,
    );

    manager.load_definitions(vec![timer_a, timer_b]);

    let start_time = now();

    // Start combat - Timer A should start
    manager.handle_signal(&GameSignal::CombatStarted {
        timestamp: start_time,
        encounter_id: 1,
    });

    let active = manager.active_timers();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].name, "Timer A");

    // Send a signal with future timestamp to advance time and trigger tick
    let after_expiry = start_time + chrono::Duration::seconds(3);
    manager.handle_signal(&GameSignal::AbilityActivated {
        ability_id: 999999, // Non-matching ability just to advance time
        source_id: 1,
        source_npc_id: 0,
        target_id: 0,
        target_name: crate::context::IStr::default(),
        target_entity_type: crate::combat_log::EntityType::Player,
        target_npc_id: 0,
        timestamp: after_expiry,
    });
    manager.tick();

    // Timer A should be gone, Timer B should now be active
    let active = manager.active_timers();
    assert_eq!(active.len(), 1, "Expected Timer B to be active after Timer A expires");
    assert_eq!(active[0].name, "Timer B");
}

#[test]
fn test_timer_expires_without_chain() {
    let mut manager = TimerManager::new();

    // Simple timer with no chaining
    let timer = make_timer(
        "short_timer",
        "Short Timer",
        TimerTrigger::CombatStart,
        1.0,
    );
    manager.load_definitions(vec![timer]);

    let start_time = now();

    manager.handle_signal(&GameSignal::CombatStarted {
        timestamp: start_time,
        encounter_id: 1,
    });

    assert_eq!(manager.active_timers().len(), 1);

    // Send signal with future timestamp to advance time
    let after_expiry = start_time + chrono::Duration::seconds(2);
    manager.handle_signal(&GameSignal::AbilityActivated {
        ability_id: 999999,
        source_id: 1,
        source_npc_id: 0,
        target_id: 0,
        target_name: crate::context::IStr::default(),
        target_entity_type: crate::combat_log::EntityType::Player,
        target_npc_id: 0,
        timestamp: after_expiry,
    });
    manager.tick();

    // Timer should be gone
    assert!(manager.active_timers().is_empty(), "Expired timer should be removed");
}
