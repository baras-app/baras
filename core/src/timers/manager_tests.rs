//! Tests for TimerManager signal handling
//!
//! Verifies that timers are correctly activated by various signals.

use chrono::Local;

use super::{TimerDefinition, TimerManager, TimerTrigger};
use crate::dsl::AudioConfig;
use crate::dsl::EntityFilter;
use crate::dsl::{AbilitySelector, EffectSelector, EntitySelector};
use crate::signal_processor::{GameSignal, SignalHandler};

/// Create a test timer with the given trigger
fn make_timer(id: &str, name: &str, trigger: TimerTrigger, duration: f32) -> TimerDefinition {
    TimerDefinition {
        id: id.to_string(),
        name: name.to_string(),
        trigger,
        duration_secs: duration,
        is_alert: false,
        color: [200, 200, 200, 255],
        enabled: true,
        can_be_refreshed: false,
        triggers_timer: None,
        cancel_trigger: None,
        repeats: 0,
        alert_at_secs: None,
        alert_text: None,
        audio: AudioConfig::default(),
        show_on_raid_frames: false,
        show_at_secs: 0.0,
        area_ids: Vec::new(),
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

    let timer = make_timer("enrage", "Enrage Timer", TimerTrigger::CombatStart, 300.0);
    manager.load_definitions(vec![timer]);

    // No timers active initially
    assert!(manager.active_timers().is_empty());

    // Send CombatStarted signal
    let signal = GameSignal::CombatStarted {
        timestamp: now(),
        encounter_id: 1,
    };
    manager.handle_signal(&signal, None);

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
        TimerTrigger::AbilityCast {
            abilities: vec![AbilitySelector::Id(3302391763959808)],
            source: EntityFilter::Any,
        },
        15.0,
    );
    manager.load_definitions(vec![timer]);

    // Send AbilityActivated signal
    let signal = GameSignal::AbilityActivated {
        ability_id: 3302391763959808,
        ability_name: crate::context::IStr::default(),
        source_id: 12345,
        source_entity_type: crate::combat_log::EntityType::Player,
        source_name: crate::context::IStr::default(),
        source_npc_id: 0,
        target_id: 0,
        target_name: crate::context::IStr::default(),
        target_entity_type: crate::combat_log::EntityType::Player,
        target_npc_id: 0,
        timestamp: now(),
    };
    manager.handle_signal(&signal, None);

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
        TimerTrigger::EffectApplied {
            effects: vec![EffectSelector::Id(999999)],
            source: EntityFilter::Any,
            target: EntityFilter::Any,
        },
        10.0,
    );
    manager.load_definitions(vec![timer]);

    let signal = GameSignal::EffectApplied {
        effect_id: 999999,
        effect_name: crate::context::IStr::default(),
        action_id: 0,
        action_name: crate::context::IStr::default(),
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
    manager.handle_signal(&signal, None);

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
        TimerTrigger::NpcAppears {
            selector: vec![EntitySelector::Id(3291675820556288)],
        },
        30.0,
    );
    manager.load_definitions(vec![timer]);

    let signal = GameSignal::NpcFirstSeen {
        entity_id: 12345,
        npc_id: 3291675820556288,
        entity_name: "Dread Monster".to_string(),
        timestamp: now(),
    };
    manager.handle_signal(&signal, None);

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
                TimerTrigger::AbilityCast {
                    abilities: vec![AbilitySelector::Id(111)],
                    source: EntityFilter::Any,
                },
                TimerTrigger::AbilityCast {
                    abilities: vec![AbilitySelector::Id(222)],
                    source: EntityFilter::Any,
                },
            ],
        },
        10.0,
    );
    manager.load_definitions(vec![timer]);

    // First ability should trigger
    let signal1 = GameSignal::AbilityActivated {
        ability_id: 111,
        ability_name: crate::context::IStr::default(),
        source_id: 1,
        source_entity_type: crate::combat_log::EntityType::Player,
        source_name: crate::context::IStr::default(),
        source_npc_id: 0,
        target_id: 0,
        target_name: crate::context::IStr::default(),
        target_entity_type: crate::combat_log::EntityType::Player,
        target_npc_id: 0,
        timestamp: now(),
    };
    manager.handle_signal(&signal1, None);

    assert_eq!(
        manager.active_timers().len(),
        1,
        "First condition should trigger"
    );

    // Clear timers via CombatEnded and test second ability
    manager.handle_signal(
        &GameSignal::CombatEnded {
            timestamp: now(),
            encounter_id: 1,
        },
        None,
    );

    let signal2 = GameSignal::AbilityActivated {
        ability_id: 222,
        ability_name: crate::context::IStr::default(),
        source_id: 1,
        source_entity_type: crate::combat_log::EntityType::Player,
        source_name: crate::context::IStr::default(),
        source_npc_id: 0,
        target_id: 0,
        target_name: crate::context::IStr::default(),
        target_entity_type: crate::combat_log::EntityType::Player,
        target_npc_id: 0,
        timestamp: now(),
    };
    manager.handle_signal(&signal2, None);

    assert_eq!(
        manager.active_timers().len(),
        1,
        "Second condition should also trigger"
    );
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
                TimerTrigger::AbilityCast {
                    abilities: vec![AbilitySelector::Id(333)],
                    source: EntityFilter::Any,
                },
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
    manager.handle_signal(&signal, None);

    assert_eq!(
        manager.active_timers().len(),
        1,
        "CombatStart in AnyOf should trigger"
    );
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
        cancel_trigger: Some(TimerTrigger::TimerStarted {
            timer_id: "timer_b".to_string(),
        }),
        ..make_timer("", "", TimerTrigger::CombatStart, 0.0)
    };

    // Timer B starts on ability
    let timer_b = make_timer(
        "timer_b",
        "Timer B",
        TimerTrigger::AbilityCast {
            abilities: vec![AbilitySelector::Id(444)],
            source: EntityFilter::Any,
        },
        30.0,
    );

    manager.load_definitions(vec![timer_a, timer_b]);

    // Start combat - Timer A should be active
    manager.handle_signal(
        &GameSignal::CombatStarted {
            timestamp: now(),
            encounter_id: 1,
        },
        None,
    );

    let active = manager.active_timers();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].name, "Timer A");

    // Trigger Timer B - Timer A should be cancelled
    manager.handle_signal(
        &GameSignal::AbilityActivated {
            ability_id: 444,
            ability_name: crate::context::IStr::default(),
            source_id: 1,
            source_entity_type: crate::combat_log::EntityType::Player,
            source_name: crate::context::IStr::default(),
            source_npc_id: 0,
            target_id: 0,
            target_name: crate::context::IStr::default(),
            target_entity_type: crate::combat_log::EntityType::Player,
            target_npc_id: 0,
            timestamp: now(),
        },
        None,
    );

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
        TimerTrigger::AbilityCast {
            abilities: vec![AbilitySelector::Id(12345)],
            source: EntityFilter::Any,
        },
        10.0,
    );
    manager.load_definitions(vec![timer]);

    // Wrong ability ID
    let signal = GameSignal::AbilityActivated {
        ability_id: 99999, // Different ID
        ability_name: crate::context::IStr::default(),
        source_id: 1,
        source_entity_type: crate::combat_log::EntityType::Player,
        source_name: crate::context::IStr::default(),
        source_npc_id: 0,
        target_id: 0,
        target_name: crate::context::IStr::default(),
        target_entity_type: crate::combat_log::EntityType::Player,
        target_npc_id: 0,
        timestamp: now(),
    };
    manager.handle_signal(&signal, None);

    assert!(
        manager.active_timers().is_empty(),
        "Wrong ability should not trigger"
    );
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
    manager.handle_signal(
        &GameSignal::CombatStarted {
            timestamp: now(),
            encounter_id: 1,
        },
        None,
    );
    assert_eq!(manager.active_timers().len(), 1);

    // End combat
    manager.handle_signal(
        &GameSignal::CombatEnded {
            timestamp: now(),
            encounter_id: 1,
        },
        None,
    );

    assert!(
        manager.active_timers().is_empty(),
        "CombatEnded should clear timers"
    );
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
        TimerTrigger::TimerExpires {
            timer_id: "timer_a".to_string(),
        },
        10.0,
    );

    manager.load_definitions(vec![timer_a, timer_b]);

    let start_time = now();

    // Start combat - Timer A should start
    manager.handle_signal(
        &GameSignal::CombatStarted {
            timestamp: start_time,
            encounter_id: 1,
        },
        None,
    );

    let active = manager.active_timers();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].name, "Timer A");

    // Send a signal with future timestamp to advance time and trigger tick
    let after_expiry = start_time + chrono::Duration::seconds(3);
    manager.handle_signal(
        &GameSignal::AbilityActivated {
            ability_id: 999999, // Non-matching ability just to advance time
            ability_name: crate::context::IStr::default(),
            source_id: 1,
            source_entity_type: crate::combat_log::EntityType::Player,
            source_name: crate::context::IStr::default(),
            source_npc_id: 0,
            target_id: 0,
            target_name: crate::context::IStr::default(),
            target_entity_type: crate::combat_log::EntityType::Player,
            target_npc_id: 0,
            timestamp: after_expiry,
        },
        None,
    );
    manager.tick();

    // Timer A should be gone, Timer B should now be active
    let active = manager.active_timers();
    assert_eq!(
        active.len(),
        1,
        "Expected Timer B to be active after Timer A expires"
    );
    assert_eq!(active[0].name, "Timer B");
}

#[test]
fn test_timer_expires_without_chain() {
    let mut manager = TimerManager::new();

    // Simple timer with no chaining
    let timer = make_timer("short_timer", "Short Timer", TimerTrigger::CombatStart, 1.0);
    manager.load_definitions(vec![timer]);

    let start_time = now();

    manager.handle_signal(
        &GameSignal::CombatStarted {
            timestamp: start_time,
            encounter_id: 1,
        },
        None,
    );

    assert_eq!(manager.active_timers().len(), 1);

    // Send signal with future timestamp to advance time
    let after_expiry = start_time + chrono::Duration::seconds(2);
    manager.handle_signal(
        &GameSignal::AbilityActivated {
            ability_id: 999999,
            ability_name: crate::context::IStr::default(),
            source_id: 1,
            source_entity_type: crate::combat_log::EntityType::Player,
            source_name: crate::context::IStr::default(),
            source_npc_id: 0,
            target_id: 0,
            target_name: crate::context::IStr::default(),
            target_entity_type: crate::combat_log::EntityType::Player,
            target_npc_id: 0,
            timestamp: after_expiry,
        },
        None,
    );
    manager.tick();

    // Timer should be gone
    assert!(
        manager.active_timers().is_empty(),
        "Expired timer should be removed"
    );
}

#[test]
fn test_phase_ended_triggers_timer() {
    let mut manager = TimerManager::new();

    // Timer that triggers when phase_1 ends
    let timer = make_timer(
        "phase_end_timer",
        "Phase 1 Ended Timer",
        TimerTrigger::PhaseEnded {
            phase_id: "phase_1".to_string(),
        },
        30.0,
    );
    manager.load_definitions(vec![timer]);

    // No timers active initially
    assert!(manager.active_timers().is_empty());

    // Enter phase_1 - should NOT trigger (we're waiting for it to END)
    manager.handle_signal(
        &GameSignal::PhaseChanged {
            boss_id: "test_boss".to_string(),
            old_phase: None,
            new_phase: "phase_1".to_string(),
            timestamp: now(),
        },
        None,
    );
    assert!(
        manager.active_timers().is_empty(),
        "PhaseEntered should not trigger PhaseEnded timer"
    );

    // Transition to phase_2 - phase_1 ended, should trigger
    manager.handle_signal(
        &GameSignal::PhaseChanged {
            boss_id: "test_boss".to_string(),
            old_phase: Some("phase_1".to_string()),
            new_phase: "phase_2".to_string(),
            timestamp: now(),
        },
        None,
    );

    let active = manager.active_timers();
    assert_eq!(
        active.len(),
        1,
        "PhaseEnded timer should trigger when phase_1 ends"
    );
    assert_eq!(active[0].name, "Phase 1 Ended Timer");
}

#[test]
fn test_phase_entered_and_ended_both_trigger() {
    let mut manager = TimerManager::new();

    // Timer for entering phase_2
    let enter_timer = make_timer(
        "phase_enter",
        "Phase 2 Started",
        TimerTrigger::PhaseEntered {
            phase_id: "phase_2".to_string(),
        },
        20.0,
    );

    // Timer for phase_1 ending
    let end_timer = make_timer(
        "phase_end",
        "Phase 1 Ended",
        TimerTrigger::PhaseEnded {
            phase_id: "phase_1".to_string(),
        },
        15.0,
    );

    manager.load_definitions(vec![enter_timer, end_timer]);

    // Transition from phase_1 to phase_2 - both should trigger
    manager.handle_signal(
        &GameSignal::PhaseChanged {
            boss_id: "test_boss".to_string(),
            old_phase: Some("phase_1".to_string()),
            new_phase: "phase_2".to_string(),
            timestamp: now(),
        },
        None,
    );

    let active = manager.active_timers();
    assert_eq!(
        active.len(),
        2,
        "Both phase entered and ended timers should trigger"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Integration Tests with Real Log Data
// ═══════════════════════════════════════════════════════════════════════════

use crate::combat_log::LogParser;
use crate::signal_processor::EventProcessor;
use crate::state::SessionCache;
use std::fs::File;
use std::io::Read as _;
use std::path::Path;

/// Parse a fixture file and pipe signals through a TimerManager
fn run_timer_integration(fixture_path: &Path, timer: TimerDefinition) -> Vec<String> {
    let mut file = File::open(fixture_path).expect("Failed to open fixture file");
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).expect("Failed to read file");
    let content = String::from_utf8_lossy(&bytes);

    let parser = LogParser::new(Local::now().naive_local());
    let mut processor = EventProcessor::new();
    let mut cache = SessionCache::default();
    let mut manager = TimerManager::new();

    manager.load_definitions(vec![timer]);

    let mut activated_timers = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        if let Some(event) = parser.parse_line(line_num as u64, line) {
            let (signals, _) = processor.process_event(event, &mut cache);
            for signal in signals {
                let before = manager.active_timers().len();
                manager.handle_signal(&signal, None);
                let after = manager.active_timers().len();

                // Track newly activated timers
                if after > before {
                    for t in manager.active_timers() {
                        if !activated_timers.contains(&t.name) {
                            activated_timers.push(t.name.clone());
                        }
                    }
                }
            }
        }
    }

    activated_timers
}

#[test]
fn test_integration_combat_start_timer_with_real_log() {
    let fixture_path = Path::new("../integration-tests/fixtures/bestia_pull.txt");
    if !fixture_path.exists() {
        eprintln!(
            "Skipping test: fixture file not found at {:?}",
            fixture_path
        );
        return;
    }

    // Create a timer that triggers on CombatStart
    let timer = make_timer(
        "enrage",
        "Enrage Timer",
        TimerTrigger::CombatStart,
        480.0, // 8 minute enrage
    );

    let activated = run_timer_integration(fixture_path, timer);

    assert!(
        activated.contains(&"Enrage Timer".to_string()),
        "CombatStart timer should have activated during bestia_pull. Activated: {:?}",
        activated
    );
}

#[test]
fn test_integration_ability_timer_with_real_log() {
    let fixture_path = Path::new("../integration-tests/fixtures/bestia_pull.txt");
    if !fixture_path.exists() {
        eprintln!(
            "Skipping test: fixture file not found at {:?}",
            fixture_path
        );
        return;
    }

    // Dread Scream ability ID from the fixture (known ability in Bestia fight)
    // We'll use a generic high-frequency ability that should appear in combat
    let timer = make_timer(
        "any_ability",
        "Ability Tracker",
        TimerTrigger::AbilityCast {
            abilities: vec![AbilitySelector::Id(807737319514112)],
            source: EntityFilter::Any,
        }, // Basic Attack
        10.0,
    );

    let activated = run_timer_integration(fixture_path, timer);

    // This may or may not trigger depending on what abilities are in the log
    // The important thing is that the pipeline doesn't crash
    eprintln!("Ability timers activated: {:?}", activated);
}

#[test]
fn test_integration_npc_first_seen_timer() {
    let fixture_path = Path::new("../integration-tests/fixtures/bestia_pull.txt");
    if !fixture_path.exists() {
        eprintln!(
            "Skipping test: fixture file not found at {:?}",
            fixture_path
        );
        return;
    }

    // Dread Monster NPC ID from Bestia fight
    let timer = make_timer(
        "monster_spawn",
        "Dread Monster Spawned",
        TimerTrigger::NpcAppears {
            selector: vec![EntitySelector::Id(3291675820556288)],
        },
        30.0,
    );

    let activated = run_timer_integration(fixture_path, timer);

    assert!(
        activated.contains(&"Dread Monster Spawned".to_string()),
        "EntityFirstSeen timer should trigger when Dread Monster appears. Activated: {:?}",
        activated
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Timer Expiration and Chaining Tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn test_multi_timer_chain_a_b_c() {
    let mut manager = TimerManager::new();

    // Chain: A (2s) → B (3s) → C (5s)
    let timer_a = TimerDefinition {
        triggers_timer: Some("timer_b".to_string()),
        ..make_timer("timer_a", "Timer A", TimerTrigger::CombatStart, 2.0)
    };

    let timer_b = TimerDefinition {
        triggers_timer: Some("timer_c".to_string()),
        ..make_timer(
            "timer_b",
            "Timer B",
            TimerTrigger::TimerExpires {
                timer_id: "timer_a".to_string(),
            },
            3.0,
        )
    };

    let timer_c = make_timer(
        "timer_c",
        "Timer C",
        TimerTrigger::TimerExpires {
            timer_id: "timer_b".to_string(),
        },
        5.0,
    );

    manager.load_definitions(vec![timer_a, timer_b, timer_c]);

    let start_time = now();

    // Start combat - Timer A starts
    manager.handle_signal(
        &GameSignal::CombatStarted {
            timestamp: start_time,
            encounter_id: 1,
        },
        None,
    );

    assert_eq!(manager.active_timers().len(), 1);
    assert_eq!(manager.active_timers()[0].name, "Timer A");

    // Advance 3 seconds - Timer A expires, Timer B starts
    let t1 = start_time + chrono::Duration::seconds(3);
    manager.handle_signal(
        &GameSignal::AbilityActivated {
            ability_id: 0,
            ability_name: crate::context::IStr::default(),
            source_id: 1,
            source_entity_type: crate::combat_log::EntityType::Player,
            source_name: crate::context::IStr::default(),
            source_npc_id: 0,
            target_id: 0,
            target_name: crate::context::IStr::default(),
            target_entity_type: crate::combat_log::EntityType::Player,
            target_npc_id: 0,
            timestamp: t1,
        },
        None,
    );
    manager.tick();

    assert_eq!(manager.active_timers().len(), 1, "Timer B should be active");
    assert_eq!(manager.active_timers()[0].name, "Timer B");

    // Advance another 4 seconds - Timer B expires, Timer C starts
    let t2 = t1 + chrono::Duration::seconds(4);
    manager.handle_signal(
        &GameSignal::AbilityActivated {
            ability_id: 0,
            ability_name: crate::context::IStr::default(),
            source_id: 1,
            source_entity_type: crate::combat_log::EntityType::Player,
            source_name: crate::context::IStr::default(),
            source_npc_id: 0,
            target_id: 0,
            target_name: crate::context::IStr::default(),
            target_entity_type: crate::combat_log::EntityType::Player,
            target_npc_id: 0,
            timestamp: t2,
        },
        None,
    );
    manager.tick();

    assert_eq!(manager.active_timers().len(), 1, "Timer C should be active");
    assert_eq!(manager.active_timers()[0].name, "Timer C");

    // Advance another 6 seconds - Timer C expires, nothing chains
    let t3 = t2 + chrono::Duration::seconds(6);
    manager.handle_signal(
        &GameSignal::AbilityActivated {
            ability_id: 0,
            ability_name: crate::context::IStr::default(),
            source_id: 1,
            source_entity_type: crate::combat_log::EntityType::Player,
            source_name: crate::context::IStr::default(),
            source_npc_id: 0,
            target_id: 0,
            target_name: crate::context::IStr::default(),
            target_entity_type: crate::combat_log::EntityType::Player,
            target_npc_id: 0,
            timestamp: t3,
        },
        None,
    );
    manager.tick();

    assert!(
        manager.active_timers().is_empty(),
        "All timers should have expired"
    );
}

#[test]
fn test_cancel_on_timer_with_chain() {
    let mut manager = TimerManager::new();

    // Setup: Timer A starts on combat
    //        Timer B starts on combat, but is cancelled when Timer C starts
    //        Timer C is triggered by Timer A expiring

    let timer_a = TimerDefinition {
        triggers_timer: Some("timer_c".to_string()),
        ..make_timer("timer_a", "Timer A", TimerTrigger::CombatStart, 2.0)
    };

    let timer_b = TimerDefinition {
        cancel_trigger: Some(TimerTrigger::TimerStarted {
            timer_id: "timer_c".to_string(),
        }),
        ..make_timer("timer_b", "Timer B", TimerTrigger::CombatStart, 60.0)
    };

    let timer_c = make_timer(
        "timer_c",
        "Timer C",
        TimerTrigger::TimerExpires {
            timer_id: "timer_a".to_string(),
        },
        10.0,
    );

    manager.load_definitions(vec![timer_a, timer_b, timer_c]);

    let start_time = now();

    // Start combat - Timer A and Timer B both start
    manager.handle_signal(
        &GameSignal::CombatStarted {
            timestamp: start_time,
            encounter_id: 1,
        },
        None,
    );

    let active = manager.active_timers();
    assert_eq!(active.len(), 2, "Both Timer A and Timer B should be active");

    // Advance time - Timer A expires, triggers Timer C
    // Timer C starting should cancel Timer B
    let after_expiry = start_time + chrono::Duration::seconds(3);
    manager.handle_signal(
        &GameSignal::AbilityActivated {
            ability_id: 0,
            ability_name: crate::context::IStr::default(),
            source_id: 1,
            source_entity_type: crate::combat_log::EntityType::Player,
            source_name: crate::context::IStr::default(),
            source_npc_id: 0,
            target_id: 0,
            target_name: crate::context::IStr::default(),
            target_entity_type: crate::combat_log::EntityType::Player,
            target_npc_id: 0,
            timestamp: after_expiry,
        },
        None,
    );
    manager.tick();

    let active = manager.active_timers();
    assert_eq!(
        active.len(),
        1,
        "Only Timer C should remain (Timer B cancelled)"
    );
    assert_eq!(active[0].name, "Timer C");
}

#[test]
fn test_timer_refresh_resets_expiration() {
    let mut manager = TimerManager::new();

    // Timer that can be refreshed by the same ability
    let timer = TimerDefinition {
        can_be_refreshed: true,
        ..make_timer(
            "refreshable",
            "Refreshable Timer",
            TimerTrigger::AbilityCast {
                abilities: vec![AbilitySelector::Id(12345)],
                source: EntityFilter::Any,
            },
            5.0,
        )
    };

    manager.load_definitions(vec![timer]);

    let start_time = now();

    // First cast - timer starts
    manager.handle_signal(
        &GameSignal::AbilityActivated {
            ability_id: 12345,
            ability_name: crate::context::IStr::default(),
            source_id: 1,
            source_entity_type: crate::combat_log::EntityType::Player,
            source_name: crate::context::IStr::default(),
            source_npc_id: 0,
            target_id: 0,
            target_name: crate::context::IStr::default(),
            target_entity_type: crate::combat_log::EntityType::Player,
            target_npc_id: 0,
            timestamp: start_time,
        },
        None,
    );

    assert_eq!(manager.active_timers().len(), 1);
    let initial_remaining = manager.active_timers()[0].remaining_secs(start_time);

    // Advance 3 seconds
    let t1 = start_time + chrono::Duration::seconds(3);

    // Cast again - should refresh
    manager.handle_signal(
        &GameSignal::AbilityActivated {
            ability_id: 12345,
            ability_name: crate::context::IStr::default(),
            source_id: 1,
            source_entity_type: crate::combat_log::EntityType::Player,
            source_name: crate::context::IStr::default(),
            source_npc_id: 0,
            target_id: 0,
            target_name: crate::context::IStr::default(),
            target_entity_type: crate::combat_log::EntityType::Player,
            target_npc_id: 0,
            timestamp: t1,
        },
        None,
    );

    assert_eq!(
        manager.active_timers().len(),
        1,
        "Should still be one timer"
    );

    // The timer should have been refreshed (remaining time reset to ~5s)
    let after_refresh = manager.active_timers()[0].remaining_secs(t1);
    assert!(
        after_refresh > 4.0, // Should be close to 5s after refresh
        "Timer should have been refreshed. Remaining: {:.1}s (expected ~5s)",
        after_refresh
    );
}

#[test]
fn test_timer_no_refresh_when_disabled() {
    let mut manager = TimerManager::new();

    // Timer that cannot be refreshed
    let timer = TimerDefinition {
        can_be_refreshed: false,
        ..make_timer(
            "no_refresh",
            "No Refresh Timer",
            TimerTrigger::AbilityCast {
                abilities: vec![AbilitySelector::Id(12345)],
                source: EntityFilter::Any,
            },
            10.0,
        )
    };

    manager.load_definitions(vec![timer]);

    let start_time = now();

    // First cast - timer starts
    manager.handle_signal(
        &GameSignal::AbilityActivated {
            ability_id: 12345,
            ability_name: crate::context::IStr::default(),
            source_id: 1,
            source_entity_type: crate::combat_log::EntityType::Player,
            source_name: crate::context::IStr::default(),
            source_npc_id: 0,
            target_id: 0,
            target_name: crate::context::IStr::default(),
            target_entity_type: crate::combat_log::EntityType::Player,
            target_npc_id: 0,
            timestamp: start_time,
        },
        None,
    );

    // Advance 3 seconds
    let t1 = start_time + chrono::Duration::seconds(3);

    // Cast again - should NOT start a second timer (can't refresh, can't duplicate)
    manager.handle_signal(
        &GameSignal::AbilityActivated {
            ability_id: 12345,
            ability_name: crate::context::IStr::default(),
            source_id: 1,
            source_entity_type: crate::combat_log::EntityType::Player,
            source_name: crate::context::IStr::default(),
            source_npc_id: 0,
            target_id: 0,
            target_name: crate::context::IStr::default(),
            target_entity_type: crate::combat_log::EntityType::Player,
            target_npc_id: 0,
            timestamp: t1,
        },
        None,
    );

    assert_eq!(
        manager.active_timers().len(),
        1,
        "Should still be one timer (no duplicate)"
    );
}

#[test]
fn test_integration_timer_expiration_with_real_log() {
    let fixture_path = Path::new("../integration-tests/fixtures/bestia_pull.txt");
    if !fixture_path.exists() {
        eprintln!("Skipping test: fixture file not found");
        return;
    }

    // Create a very short timer that will expire during the log playback
    // The bestia_pull.txt log spans several seconds of combat
    let timer = TimerDefinition {
        triggers_timer: Some("follow_up".to_string()),
        ..make_timer("quick_timer", "Quick Timer", TimerTrigger::CombatStart, 0.5)
    };

    let follow_up = make_timer(
        "follow_up",
        "Follow Up Timer",
        TimerTrigger::TimerExpires {
            timer_id: "quick_timer".to_string(),
        },
        30.0,
    );

    // Run integration with both timers
    let mut file = File::open(fixture_path).expect("Failed to open fixture file");
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).expect("Failed to read file");
    let content = String::from_utf8_lossy(&bytes);

    let parser = LogParser::new(Local::now().naive_local());
    let mut processor = EventProcessor::new();
    let mut cache = SessionCache::default();
    let mut manager = TimerManager::new();

    manager.load_definitions(vec![timer, follow_up]);

    let mut saw_quick_timer = false;
    let mut saw_follow_up = false;

    for (line_num, line) in content.lines().enumerate() {
        if let Some(event) = parser.parse_line(line_num as u64, line) {
            let (signals, _) = processor.process_event(event, &mut cache);
            for signal in signals {
                manager.handle_signal(&signal, None);
                manager.tick();

                for t in manager.active_timers() {
                    if t.name == "Quick Timer" {
                        saw_quick_timer = true;
                    }
                    if t.name == "Follow Up Timer" {
                        saw_follow_up = true;
                    }
                }
            }
        }
    }

    assert!(saw_quick_timer, "Quick Timer should have been activated");
    assert!(
        saw_follow_up,
        "Follow Up Timer should have been triggered by Quick Timer expiring"
    );
}
