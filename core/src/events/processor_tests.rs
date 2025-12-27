//! Integration tests for signal emission
//!
//! Uses fixture log files to verify signals are properly emitted.

use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::boss::BossConfig;
use crate::combat_log::LogParser;
use crate::state::SessionCache;

use super::{EventProcessor, GameSignal};

/// Load boss definitions from a TOML config file
fn load_boss_config(path: &Path) -> Option<BossConfig> {
    let content = std::fs::read_to_string(path).ok()?;
    toml::from_str(&content).ok()
}

/// Parse a fixture file and collect all emitted signals
fn collect_signals_from_fixture(fixture_path: &Path) -> Vec<GameSignal> {
    collect_signals_from_fixture_ext(fixture_path, None, false)
}

/// Parse a fixture with boss definitions loaded
fn collect_signals_with_boss_defs(fixture_path: &Path, boss_config_path: &Path) -> Vec<GameSignal> {
    collect_signals_from_fixture_ext(fixture_path, Some(boss_config_path), false)
}

fn collect_signals_from_fixture_ext(
    fixture_path: &Path,
    boss_config_path: Option<&Path>,
    debug: bool,
) -> Vec<GameSignal> {
    let mut file = File::open(fixture_path).expect("Failed to open fixture file");

    // Read as bytes and convert with lossy UTF-8 (handles non-ASCII characters in player names)
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).expect("Failed to read file");
    let content = String::from_utf8_lossy(&bytes);

    let parser = LogParser::new(chrono::Local::now().naive_local());
    let mut processor = EventProcessor::new();
    let mut cache = SessionCache::default();

    // Load boss definitions if provided
    if let Some(config_path) = boss_config_path
        && let Some(config) = load_boss_config(config_path) {
            cache.load_boss_definitions(config.bosses);
    }

    let mut all_signals = Vec::new();

    for (line_num, line) in content.lines().enumerate() {
        if let Some(event) = parser.parse_line(line_num as u64, line) {
            // Debug: print EnterCombat events
            if debug && line.contains("EnterCombat") {
                eprintln!("Line {}: EnterCombat event parsed", line_num);
                eprintln!("  effect_id: {}", event.effect.effect_id);
                eprintln!("  type_id: {}", event.effect.type_id);
                eprintln!("  source: {:?}", crate::context::resolve(event.source_entity.name));
            }
            let signals = processor.process_event(event, &mut cache);
            if debug && !signals.is_empty() {
                for s in &signals {
                    eprintln!("  -> Signal: {}", signal_type_name(s));
                }
            }
            all_signals.extend(signals);
        }
    }

    all_signals
}

/// Get the discriminant name for a signal (for tracking which types were emitted)
fn signal_type_name(signal: &GameSignal) -> &'static str {
    match signal {
        GameSignal::CombatStarted { .. } => "CombatStarted",
        GameSignal::CombatEnded { .. } => "CombatEnded",
        GameSignal::EntityDeath { .. } => "EntityDeath",
        GameSignal::EntityRevived { .. } => "EntityRevived",
        GameSignal::NpcFirstSeen { .. } => "NpcFirstSeen",
        GameSignal::EffectApplied { .. } => "EffectApplied",
        GameSignal::EffectRemoved { .. } => "EffectRemoved",
        GameSignal::EffectChargesChanged { .. } => "EffectChargesChanged",
        GameSignal::AbilityActivated { .. } => "AbilityActivated",
        GameSignal::TargetChanged { .. } => "TargetChanged",
        GameSignal::TargetCleared { .. } => "TargetCleared",
        GameSignal::AreaEntered { .. } => "AreaEntered",
        GameSignal::PlayerInitialized { .. } => "PlayerInitialized",
        GameSignal::DisciplineChanged { .. } => "DisciplineChanged",
        GameSignal::BossEncounterDetected { .. } => "BossEncounterDetected",
        GameSignal::BossHpChanged { .. } => "BossHpChanged",
        GameSignal::PhaseChanged { .. } => "PhaseChanged",
        GameSignal::CounterChanged { .. } => "CounterChanged",
    }
}

#[test]
fn test_bestia_pull_emits_expected_signals() {
    let fixture_path = Path::new("../test-log-files/fixtures/bestia_pull.txt");
    if !fixture_path.exists() {
        eprintln!("Skipping test: fixture file not found at {:?}", fixture_path);
        return;
    }

    let signals = collect_signals_from_fixture(fixture_path);

    // Collect unique signal types
    let signal_types: HashSet<&str> = signals.iter().map(signal_type_name).collect();

    // Print what we got for debugging
    eprintln!("Collected {} signals of {} unique types:", signals.len(), signal_types.len());
    for signal_type in &signal_types {
        let count = signals.iter().filter(|s| signal_type_name(s) == *signal_type).count();
        eprintln!("  - {}: {}", signal_type, count);
    }

    // Assert expected signals are present
    assert!(signal_types.contains("CombatStarted"), "Missing CombatStarted signal");
    assert!(signal_types.contains("DisciplineChanged"), "Missing DisciplineChanged signal");
    assert!(signal_types.contains("EffectApplied"), "Missing EffectApplied signal");
    assert!(signal_types.contains("EffectRemoved"), "Missing EffectRemoved signal");
    assert!(signal_types.contains("AbilityActivated"), "Missing AbilityActivated signal");
    assert!(signal_types.contains("TargetChanged"), "Missing TargetChanged signal");

    // Count specific signal types
    let discipline_count = signals
        .iter()
        .filter(|s| matches!(s, GameSignal::DisciplineChanged { .. }))
        .count();
    assert!(discipline_count >= 8, "Expected at least 8 DisciplineChanged signals (one per player), got {}", discipline_count);

    // Verify combat started
    let combat_started = signals
        .iter()
        .find(|s| matches!(s, GameSignal::CombatStarted { .. }));
    assert!(combat_started.is_some(), "No CombatStarted signal found");
}

#[test]
fn test_effect_applied_has_source_info() {
    let fixture_path = Path::new("../test-log-files/fixtures/bestia_pull.txt");
    if !fixture_path.exists() {
        eprintln!("Skipping test: fixture file not found at {:?}", fixture_path);
        return;
    }

    let signals = collect_signals_from_fixture(fixture_path);

    // Find any EffectApplied and verify it has source info
    let effect_applied = signals.iter().find(|s| matches!(s, GameSignal::EffectApplied { .. }));

    if let Some(GameSignal::EffectApplied {
        source_id,
        source_name,
        source_entity_type,
        target_id,
        target_name,
        ..
    }) = effect_applied
    {
        // Source should have valid data
        assert!(*source_id != 0, "source_id should not be 0");
        assert!(!crate::context::resolve(*source_name).is_empty(), "source_name should not be empty");
        eprintln!(
            "EffectApplied: source={} ({:?}), target={} ({:?})",
            crate::context::resolve(*source_name),
            source_entity_type,
            crate::context::resolve(*target_name),
            target_id
        );
    } else {
        panic!("No EffectApplied signal found");
    }
}

#[test]
fn test_target_changed_signals() {
    let fixture_path = Path::new("../test-log-files/fixtures/bestia_pull.txt");
    if !fixture_path.exists() {
        eprintln!("Skipping test: fixture file not found");
        return;
    }

    let signals = collect_signals_from_fixture(fixture_path);

    // Find TargetChanged signals
    let target_signals: Vec<_> = signals
        .iter()
        .filter_map(|s| {
            if let GameSignal::TargetChanged {
                source_id,
                target_id,
                target_name,
                target_entity_type,
                ..
            } = s
            {
                Some((source_id, target_id, target_name, target_entity_type))
            } else {
                None
            }
        })
        .collect();

    assert!(!target_signals.is_empty(), "Expected at least one TargetChanged signal");
    eprintln!("Found {} TargetChanged signals", target_signals.len());

    // Verify NPC targets exist (players targeting boss/adds)
    let npc_targets: Vec<_> = target_signals
        .iter()
        .filter(|(_, _, _, entity_type)| matches!(entity_type, crate::combat_log::EntityType::Npc))
        .collect();
    assert!(!npc_targets.is_empty(), "Expected at least one target to be an NPC");
    eprintln!("  - {} targets are NPCs", npc_targets.len());
}

#[test]
fn test_npc_first_seen_for_all_npcs() {
    // NpcFirstSeen should fire for ANY NPC, not just bosses
    let fixture_path = Path::new("../test-log-files/fixtures/bestia_pull.txt");
    if !fixture_path.exists() {
        eprintln!("Skipping test: fixture file not found");
        return;
    }

    // Test WITHOUT boss definitions - should still get NpcFirstSeen for all NPCs
    let signals = collect_signals_from_fixture(fixture_path);

    let npc_signals: Vec<_> = signals
        .iter()
        .filter_map(|s| {
            if let GameSignal::NpcFirstSeen { npc_id, entity_name, .. } = s {
                Some((*npc_id, entity_name.clone()))
            } else {
                None
            }
        })
        .collect();

    assert!(!npc_signals.is_empty(), "Expected NpcFirstSeen signals for NPCs");
    eprintln!("Found {} NpcFirstSeen signals:", npc_signals.len());
    for (npc_id, name) in &npc_signals {
        eprintln!("  - {} (npc_id={})", name, npc_id);
    }

    // Verify we see all NPC types from the fixture:
    // - Dread Master Bestia (boss)
    // - Dread Monster (add)
    // - Dread Larva (add)
    let bestia_id: i64 = 3273941900591104;
    let monster_id: i64 = 3291675820556288;
    let larva_id: i64 = 3292079547482112;

    assert!(
        npc_signals.iter().any(|(id, _)| *id == bestia_id),
        "Expected NpcFirstSeen for Dread Master Bestia"
    );
    assert!(
        npc_signals.iter().any(|(id, _)| *id == monster_id),
        "Expected NpcFirstSeen for Dread Monster"
    );
    assert!(
        npc_signals.iter().any(|(id, _)| *id == larva_id),
        "Expected NpcFirstSeen for Dread Larva"
    );
}

#[test]
fn test_entity_death_target_cleared_and_revive() {
    // Fixture with death, target cleared, and revive events
    let fixture_path = Path::new("../test-log-files/fixtures/death_and_revive.txt");
    if !fixture_path.exists() {
        eprintln!("Skipping test: fixture not found");
        return;
    }

    let signals = collect_signals_from_fixture(fixture_path);

    let signal_types: HashSet<&str> = signals.iter().map(signal_type_name).collect();
    eprintln!("Death/revive fixture signals:");
    for signal_type in &signal_types {
        let count = signals.iter().filter(|s| signal_type_name(s) == *signal_type).count();
        eprintln!("  - {}: {}", signal_type, count);
    }

    // EntityDeath should fire when NPCs die
    assert!(signal_types.contains("EntityDeath"), "Expected EntityDeath signals");
    let death_signals: Vec<_> = signals
        .iter()
        .filter_map(|s| {
            if let GameSignal::EntityDeath { entity_name, entity_type, npc_id, .. } = s {
                Some((entity_name.clone(), *entity_type, *npc_id))
            } else {
                None
            }
        })
        .collect();
    eprintln!("Found {} EntityDeath signals:", death_signals.len());
    for (name, etype, npc_id) in &death_signals {
        eprintln!("  - {} ({:?}, npc_id={})", name, etype, npc_id);
    }
    assert!(death_signals.len() >= 3, "Expected at least 3 deaths");

    // TargetCleared should fire when entities clear their target
    assert!(signal_types.contains("TargetCleared"), "Expected TargetCleared signals");
    let cleared_count = signals
        .iter()
        .filter(|s| matches!(s, GameSignal::TargetCleared { .. }))
        .count();
    eprintln!("Found {} TargetCleared signals", cleared_count);
    assert!(cleared_count >= 4, "Expected at least 4 TargetCleared");

    // EntityRevived should fire when players revive
    assert!(signal_types.contains("EntityRevived"), "Expected EntityRevived signals");
    let revive_count = signals
        .iter()
        .filter(|s| matches!(s, GameSignal::EntityRevived { .. }))
        .count();
    eprintln!("Found {} EntityRevived signals", revive_count);
    assert!(revive_count >= 2, "Expected at least 2 revives");
}

#[test]
fn test_boss_signals_with_definitions() {
    let fixture_path = Path::new("../test-log-files/fixtures/bestia_pull.txt");
    let config_path = Path::new("../test-log-files/fixtures/config/dread_palace.toml");

    if !fixture_path.exists() || !config_path.exists() {
        eprintln!("Skipping test: fixture files not found");
        return;
    }

    let signals = collect_signals_with_boss_defs(fixture_path, config_path);

    let signal_types: HashSet<&str> = signals.iter().map(signal_type_name).collect();

    eprintln!("With boss definitions loaded:");
    for signal_type in &signal_types {
        let count = signals.iter().filter(|s| signal_type_name(s) == *signal_type).count();
        eprintln!("  - {}: {}", signal_type, count);
    }

    // With boss definitions, we expect boss encounter detection
    assert!(
        signal_types.contains("BossEncounterDetected"),
        "Expected BossEncounterDetected signal when boss definitions are loaded"
    );

    // PhaseChanged should fire for initial phase (CombatStart trigger)
    assert!(
        signal_types.contains("PhaseChanged"),
        "Expected PhaseChanged signal for initial phase (p1)"
    );

    // Note: BossHpChanged/NpcFirstSeen only fire when HP actually changes.
    // In bestia_pull.txt, early attacks are immune (0 damage), so HP doesn't change.
    // These signals are tested in test_phase_changed_signal with burn_phase fixture.
    eprintln!("Note: BossHpChanged requires HP change - not expected in pull fixture with immune damage");
}

#[test]
fn test_boss_hp_and_phase_signals() {
    // Use burn phase fixture which has active combat with HP changes
    let fixture_path = Path::new("../test-log-files/fixtures/bestia_burn_phase.txt");
    let config_path = Path::new("../test-log-files/fixtures/config/dread_palace.toml");

    if !fixture_path.exists() || !config_path.exists() {
        eprintln!("Skipping test: fixture files not found");
        return;
    }

    let signals = collect_signals_with_boss_defs(fixture_path, config_path);

    let signal_types: HashSet<&str> = signals.iter().map(signal_type_name).collect();
    eprintln!("Burn phase fixture signals:");
    for signal_type in &signal_types {
        let count = signals.iter().filter(|s| signal_type_name(s) == *signal_type).count();
        eprintln!("  - {}: {}", signal_type, count);
    }

    // Validate BossHpChanged signals
    assert!(signal_types.contains("BossHpChanged"), "Expected BossHpChanged signals");
    let hp_signals: Vec<_> = signals
        .iter()
        .filter(|s| matches!(s, GameSignal::BossHpChanged { .. }))
        .collect();
    eprintln!("Found {} BossHpChanged signals", hp_signals.len());

    // Verify HP data is valid
    if let Some(GameSignal::BossHpChanged { current_hp, max_hp, entity_name, .. }) = hp_signals.first() {
        assert!(*max_hp > 0, "Boss max_hp should be > 0");
        assert!(*current_hp >= 0, "current_hp should be >= 0");
        eprintln!("Boss HP sample: {}/{} for {}", current_hp, max_hp, entity_name);
    }

    // Validate NpcFirstSeen for boss
    assert!(signal_types.contains("NpcFirstSeen"), "Expected NpcFirstSeen signal");
    let bestia_npc_id: i64 = 3273941900591104;
    let bestia_seen = signals.iter().any(|s| {
        matches!(s, GameSignal::NpcFirstSeen { npc_id, .. } if *npc_id == bestia_npc_id)
    });
    assert!(bestia_seen, "Expected NpcFirstSeen for Dread Master Bestia");

    // Check for PhaseChanged to burn phase (boss HP drops below 30%)
    let burn_phase = signals.iter().find(|s| {
        matches!(s, GameSignal::PhaseChanged { new_phase, .. } if new_phase == "burn")
    });
    assert!(burn_phase.is_some(), "Expected PhaseChanged to 'burn' phase");
    eprintln!("PhaseChanged to 'burn' phase detected!");

    // Validate CounterChanged signals (counter increments from events)
    if signal_types.contains("CounterChanged") {
        let counter_signals: Vec<_> = signals
            .iter()
            .filter_map(|s| {
                if let GameSignal::CounterChanged { counter_id, new_value, .. } = s {
                    Some((counter_id, new_value))
                } else {
                    None
                }
            })
            .collect();
        eprintln!("Found {} CounterChanged signals:", counter_signals.len());
        for (id, value) in &counter_signals {
            eprintln!("  - {}: {}", id, value);
        }
    }
}
