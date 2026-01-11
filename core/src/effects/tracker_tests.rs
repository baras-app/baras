//! Integration tests for EffectTracker with real log data

use chrono::Local;
use std::fs::File;
use std::io::Read as _;
use std::path::Path;

use crate::combat_log::LogParser;
use crate::dsl::Trigger;
use crate::signal_processor::{EventProcessor, GameSignal, SignalHandler};
use crate::state::SessionCache;

use super::{
    DefinitionSet, EffectCategory, EffectDefinition, EffectSelector, EffectTracker, EntityFilter,
};

// ═══════════════════════════════════════════════════════════════════════════
// Test Helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Create a basic effect definition for testing
fn make_effect(id: &str, name: &str, effect_ids: Vec<u64>) -> EffectDefinition {
    EffectDefinition {
        id: id.to_string(),
        name: name.to_string(),
        display_text: None,
        enabled: true,
        trigger: Trigger::EffectApplied {
            effects: effect_ids.into_iter().map(EffectSelector::Id).collect(),
            source: EntityFilter::Any,
            target: EntityFilter::Any,
        },
        fixed_duration: false,
        refresh_abilities: Vec::new(),
        duration_secs: Some(10.0),
        is_refreshed_on_modify: false,
        category: EffectCategory::Hot,
        color: None,
        show_on_raid_frames: true,
        show_on_effects_overlay: false,
        show_at_secs: 0.0,
        persist_past_death: false,
        track_outside_combat: true,
        audio: Default::default(),
        on_apply_trigger_timer: None,
        on_expire_trigger_timer: None,
    }
}

/// Create a definition set with given effects
fn make_definitions(effects: Vec<EffectDefinition>) -> DefinitionSet {
    let mut set = DefinitionSet::new();
    set.add_definitions(effects, false);
    set
}

// ═══════════════════════════════════════════════════════════════════════════
// Integration Tests with Real Log Data
// ═══════════════════════════════════════════════════════════════════════════

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
            let (signals, _) = processor.process_event(event, &mut cache);
            for signal in signals {
                let before = tracker.active_effects().count();
                tracker.handle_signal(&signal, cache.current_encounter());
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
    let fixture_path = Path::new("../integration-tests/fixtures/bestia_pull.txt");
    if !fixture_path.exists() {
        eprintln!(
            "Skipping test: fixture file not found at {:?}",
            fixture_path
        );
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
    let fixture_path = Path::new("../integration-tests/fixtures/bestia_pull.txt");
    if !fixture_path.exists() {
        eprintln!(
            "Skipping test: fixture file not found at {:?}",
            fixture_path
        );
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
            let (signals, _) = processor.process_event(event, &mut cache);
            for signal in &signals {
                tracker.handle_signal(signal, cache.current_encounter());

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
