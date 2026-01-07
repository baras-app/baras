//! Integration tests for signal emission
//!
//! Uses fixture log files to verify signals are properly emitted.

use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use crate::combat_log::LogParser;
use crate::dsl::BossConfig;
use crate::state::SessionCache;

use super::{EventProcessor, GameSignal};

/// Load boss definitions from a TOML config file
fn load_boss_config(path: &Path) -> Option<BossConfig> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut config: BossConfig = toml::from_str(&content).ok()?;
    // Build indexes for NPC ID matching (not populated by serde)
    for boss in &mut config.bosses {
        boss.build_indexes();
    }
    Some(config)
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
        && let Some(config) = load_boss_config(config_path)
    {
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
                eprintln!(
                    "  source: {:?}",
                    crate::context::resolve(event.source_entity.name)
                );
            }
            let (signals, _event) = processor.process_event(event, &mut cache);
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
        GameSignal::DamageTaken { .. } => "DamageTaken",
        GameSignal::TargetChanged { .. } => "TargetChanged",
        GameSignal::TargetCleared { .. } => "TargetCleared",
        GameSignal::AreaEntered { .. } => "AreaEntered",
        GameSignal::PlayerInitialized { .. } => "PlayerInitialized",
        GameSignal::DisciplineChanged { .. } => "DisciplineChanged",
        GameSignal::BossEncounterDetected { .. } => "BossEncounterDetected",
        GameSignal::BossHpChanged { .. } => "BossHpChanged",
        GameSignal::PhaseChanged { .. } => "PhaseChanged",
        GameSignal::PhaseEndTriggered { .. } => "PhaseEndTriggered",
        GameSignal::CounterChanged { .. } => "CounterChanged",
    }
}

#[test]
fn test_bestia_pull_emits_expected_signals() {
    let fixture_path = Path::new("../integration-tests/fixtures/bestia_pull.txt");
    if !fixture_path.exists() {
        eprintln!(
            "Skipping test: fixture file not found at {:?}",
            fixture_path
        );
        return;
    }

    let signals = collect_signals_from_fixture(fixture_path);

    // Collect unique signal types
    let signal_types: HashSet<&str> = signals.iter().map(signal_type_name).collect();

    // Print what we got for debugging
    eprintln!(
        "Collected {} signals of {} unique types:",
        signals.len(),
        signal_types.len()
    );
    for signal_type in &signal_types {
        let count = signals
            .iter()
            .filter(|s| signal_type_name(s) == *signal_type)
            .count();
        eprintln!("  - {}: {}", signal_type, count);
    }

    // Assert expected signals are present
    assert!(
        signal_types.contains("CombatStarted"),
        "Missing CombatStarted signal"
    );
    assert!(
        signal_types.contains("DisciplineChanged"),
        "Missing DisciplineChanged signal"
    );
    assert!(
        signal_types.contains("EffectApplied"),
        "Missing EffectApplied signal"
    );
    assert!(
        signal_types.contains("EffectRemoved"),
        "Missing EffectRemoved signal"
    );
    assert!(
        signal_types.contains("AbilityActivated"),
        "Missing AbilityActivated signal"
    );
    assert!(
        signal_types.contains("TargetChanged"),
        "Missing TargetChanged signal"
    );

    // Count specific signal types
    let discipline_count = signals
        .iter()
        .filter(|s| matches!(s, GameSignal::DisciplineChanged { .. }))
        .count();
    assert!(
        discipline_count >= 8,
        "Expected at least 8 DisciplineChanged signals (one per player), got {}",
        discipline_count
    );

    // Verify combat started
    let combat_started = signals
        .iter()
        .find(|s| matches!(s, GameSignal::CombatStarted { .. }));
    assert!(combat_started.is_some(), "No CombatStarted signal found");
}

#[test]
fn test_effect_applied_has_source_info() {
    let fixture_path = Path::new("../integration-tests/fixtures/bestia_pull.txt");
    if !fixture_path.exists() {
        eprintln!(
            "Skipping test: fixture file not found at {:?}",
            fixture_path
        );
        return;
    }

    let signals = collect_signals_from_fixture(fixture_path);

    // Find any EffectApplied and verify it has source info
    let effect_applied = signals
        .iter()
        .find(|s| matches!(s, GameSignal::EffectApplied { .. }));

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
        assert!(
            !crate::context::resolve(*source_name).is_empty(),
            "source_name should not be empty"
        );
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
    let fixture_path = Path::new("../integration-tests/fixtures/bestia_pull.txt");
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

    assert!(
        !target_signals.is_empty(),
        "Expected at least one TargetChanged signal"
    );
    eprintln!("Found {} TargetChanged signals", target_signals.len());

    // Verify NPC targets exist (players targeting boss/adds)
    let npc_targets: Vec<_> = target_signals
        .iter()
        .filter(|(_, _, _, entity_type)| matches!(entity_type, crate::combat_log::EntityType::Npc))
        .collect();
    assert!(
        !npc_targets.is_empty(),
        "Expected at least one target to be an NPC"
    );
    eprintln!("  - {} targets are NPCs", npc_targets.len());
}

#[test]
fn test_npc_first_seen_for_all_npcs() {
    // NpcFirstSeen should fire for ANY NPC, not just bosses
    let fixture_path = Path::new("../integration-tests/fixtures/bestia_pull.txt");
    if !fixture_path.exists() {
        eprintln!("Skipping test: fixture file not found");
        return;
    }

    // Test WITHOUT boss definitions - should still get NpcFirstSeen for all NPCs
    let signals = collect_signals_from_fixture(fixture_path);

    let npc_signals: Vec<_> = signals
        .iter()
        .filter_map(|s| {
            if let GameSignal::NpcFirstSeen {
                npc_id,
                entity_name,
                ..
            } = s
            {
                Some((*npc_id, entity_name.clone()))
            } else {
                None
            }
        })
        .collect();

    assert!(
        !npc_signals.is_empty(),
        "Expected NpcFirstSeen signals for NPCs"
    );
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
    let fixture_path = Path::new("../integration-tests/fixtures/death_and_revive.txt");
    if !fixture_path.exists() {
        eprintln!("Skipping test: fixture not found");
        return;
    }

    let signals = collect_signals_from_fixture(fixture_path);

    let signal_types: HashSet<&str> = signals.iter().map(signal_type_name).collect();
    eprintln!("Death/revive fixture signals:");
    for signal_type in &signal_types {
        let count = signals
            .iter()
            .filter(|s| signal_type_name(s) == *signal_type)
            .count();
        eprintln!("  - {}: {}", signal_type, count);
    }

    // EntityDeath should fire when NPCs die
    assert!(
        signal_types.contains("EntityDeath"),
        "Expected EntityDeath signals"
    );
    let death_signals: Vec<_> = signals
        .iter()
        .filter_map(|s| {
            if let GameSignal::EntityDeath {
                entity_name,
                entity_type,
                npc_id,
                ..
            } = s
            {
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
    assert!(
        signal_types.contains("TargetCleared"),
        "Expected TargetCleared signals"
    );
    let cleared_count = signals
        .iter()
        .filter(|s| matches!(s, GameSignal::TargetCleared { .. }))
        .count();
    eprintln!("Found {} TargetCleared signals", cleared_count);
    assert!(cleared_count >= 4, "Expected at least 4 TargetCleared");

    // EntityRevived should fire when players revive
    assert!(
        signal_types.contains("EntityRevived"),
        "Expected EntityRevived signals"
    );
    let revive_count = signals
        .iter()
        .filter(|s| matches!(s, GameSignal::EntityRevived { .. }))
        .count();
    eprintln!("Found {} EntityRevived signals", revive_count);
    assert!(revive_count >= 2, "Expected at least 2 revives");
}

#[test]
fn test_boss_signals_with_definitions() {
    let fixture_path = Path::new("../integration-tests/fixtures/bestia_pull.txt");
    let config_path = Path::new("../integration-tests/fixtures/config/dread_palace.toml");

    if !fixture_path.exists() || !config_path.exists() {
        eprintln!("Skipping test: fixture files not found");
        return;
    }

    let signals = collect_signals_with_boss_defs(fixture_path, config_path);

    let signal_types: HashSet<&str> = signals.iter().map(signal_type_name).collect();

    eprintln!("With boss definitions loaded:");
    for signal_type in &signal_types {
        let count = signals
            .iter()
            .filter(|s| signal_type_name(s) == *signal_type)
            .count();
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
    eprintln!(
        "Note: BossHpChanged requires HP change - not expected in pull fixture with immune damage"
    );
}

#[test]
fn test_boss_hp_and_phase_signals() {
    use crate::encounter::EncounterState;

    // Use burn phase fixture which has active combat with HP changes
    // NOTE: This fixture is a mid-fight snippet without EnterCombat, so we manually
    // initialize the encounter to InCombat state and detect the boss
    let fixture_path = Path::new("../integration-tests/fixtures/bestia_burn_phase.txt");
    let config_path = Path::new("../integration-tests/fixtures/config/dread_palace.toml");

    if !fixture_path.exists() || !config_path.exists() {
        eprintln!("Skipping test: fixture files not found");
        return;
    }

    // Custom processing with pre-initialized combat state
    let mut file = File::open(fixture_path).expect("Failed to open fixture file");
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).expect("Failed to read file");
    let content = String::from_utf8_lossy(&bytes);

    let parser = LogParser::new(chrono::Local::now().naive_local());
    let mut processor = EventProcessor::new();
    let mut cache = SessionCache::default();

    // Load boss definitions
    if let Some(config) = load_boss_config(config_path) {
        cache.load_boss_definitions(config.bosses);
    }

    // Pre-initialize encounter to InCombat state (since fixture lacks EnterCombat)
    if let Some(enc) = cache.current_encounter_mut() {
        enc.state = EncounterState::InCombat;
        enc.enter_combat_time = Some(chrono::Local::now().naive_local());
    }

    let mut signals = Vec::new();
    for (line_num, line) in content.lines().enumerate() {
        if let Some(event) = parser.parse_line(line_num as u64, line) {
            let (sigs, _) = processor.process_event(event, &mut cache);
            signals.extend(sigs);
        }
    }

    let signal_types: HashSet<&str> = signals.iter().map(signal_type_name).collect();
    eprintln!("Burn phase fixture signals:");
    for signal_type in &signal_types {
        let count = signals
            .iter()
            .filter(|s| signal_type_name(s) == *signal_type)
            .count();
        eprintln!("  - {}: {}", signal_type, count);
    }

    // Validate BossHpChanged signals
    assert!(
        signal_types.contains("BossHpChanged"),
        "Expected BossHpChanged signals"
    );
    let hp_signals: Vec<_> = signals
        .iter()
        .filter(|s| matches!(s, GameSignal::BossHpChanged { .. }))
        .collect();
    eprintln!("Found {} BossHpChanged signals", hp_signals.len());

    // Verify HP data is valid
    if let Some(GameSignal::BossHpChanged {
        current_hp,
        max_hp,
        entity_name,
        ..
    }) = hp_signals.first()
    {
        assert!(*max_hp > 0, "Boss max_hp should be > 0");
        assert!(*current_hp >= 0, "current_hp should be >= 0");
        eprintln!(
            "Boss HP sample: {}/{} for {}",
            current_hp, max_hp, entity_name
        );
    }

    // Validate NpcFirstSeen for boss
    assert!(
        signal_types.contains("NpcFirstSeen"),
        "Expected NpcFirstSeen signal"
    );
    let bestia_npc_id: i64 = 3273941900591104;
    let bestia_seen = signals
        .iter()
        .any(|s| matches!(s, GameSignal::NpcFirstSeen { npc_id, .. } if *npc_id == bestia_npc_id));
    assert!(bestia_seen, "Expected NpcFirstSeen for Dread Master Bestia");

    // Check for PhaseChanged to burn phase (boss HP drops below 32% - config threshold)
    let burn_phase = signals
        .iter()
        .find(|s| matches!(s, GameSignal::PhaseChanged { new_phase, .. } if new_phase == "burn"));
    assert!(
        burn_phase.is_some(),
        "Expected PhaseChanged to 'burn' phase"
    );

    // Validate CounterChanged signals (counter increments from events)
    if signal_types.contains("CounterChanged") {
        let counter_signals: Vec<_> = signals
            .iter()
            .filter_map(|s| {
                if let GameSignal::CounterChanged {
                    counter_id,
                    new_value,
                    ..
                } = s
                {
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

/// Comprehensive Bestia encounter test using complete pull fixture.
/// Tests phases, timers, and the full combat lifecycle.
#[test]
fn test_bestia_complete_encounter() {
    use crate::signal_processor::handler::SignalHandler;
    use crate::timers::{TimerDefinition, TimerManager};

    let fixture_path = Path::new("../integration-tests/fixtures/bestia_complete_pull.txt");
    let config_path = Path::new("../integration-tests/fixtures/config/dread_palace.toml");

    if !fixture_path.exists() {
        eprintln!("Skipping test: bestia_complete_pull.txt not found");
        return;
    }
    if !config_path.exists() {
        eprintln!("Skipping test: dread_palace.toml not found");
        return;
    }

    // Load fixture
    let mut file = File::open(fixture_path).expect("Failed to open fixture");
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).expect("Failed to read file");
    let content = String::from_utf8_lossy(&bytes);

    // Load boss config
    let config = load_boss_config(config_path).expect("Failed to load boss config");
    let bestia_def = &config.bosses[0];

    // Convert BossTimerDefinitions to TimerDefinitions
    let timer_defs: Vec<TimerDefinition> = bestia_def
        .timers
        .iter()
        .map(|bt| TimerDefinition {
            id: bt.id.clone(),
            name: bt.name.clone(),
            enabled: bt.enabled,
            trigger: bt.trigger.clone(),
            duration_secs: bt.duration_secs,
            is_alert: bt.is_alert,
            can_be_refreshed: bt.can_be_refreshed,
            triggers_timer: bt.chains_to.clone(),
            cancel_trigger: bt.cancel_trigger.clone(),
            color: bt.color,
            alert_at_secs: None,
            alert_text: None,
            audio: Default::default(),
            repeats: 0,
            show_on_raid_frames: false,
            show_at_secs: 0.0,
            area_ids: Vec::new(),
            encounters: Vec::new(),
            boss: None,
            difficulties: Vec::new(),
            phases: Vec::new(),
            counter_condition: None,
        })
        .collect();

    // Setup processor and timer manager
    let parser = crate::combat_log::LogParser::new(chrono::Local::now().naive_local());
    let mut processor = super::EventProcessor::new();
    let mut cache = SessionCache::default();
    cache.load_boss_definitions(config.bosses);

    let mut timer_manager = TimerManager::new();
    timer_manager.load_definitions(timer_defs);

    // Track what we observe
    let mut phase_changes: Vec<(String, String)> = Vec::new(); // (old, new)
    let mut combat_started = false;
    let mut combat_ended = false;
    let mut boss_detected = false;
    let mut timers_activated: HashSet<String> = HashSet::new();
    let mut timer_chains_triggered: Vec<String> = Vec::new();
    let mut ability_timer_triggers = 0;

    // Process all events
    for (line_num, line) in content.lines().enumerate() {
        if let Some(event) = parser.parse_line(line_num as u64, line) {
            let (signals, _) = processor.process_event(event, &mut cache);

            for signal in &signals {
                // Track phase/boss signals
                match signal {
                    GameSignal::CombatStarted { .. } => combat_started = true,
                    GameSignal::CombatEnded { .. } => combat_ended = true,
                    GameSignal::BossEncounterDetected { definition_id, .. } => {
                        boss_detected = true;
                        eprintln!("Boss detected: {}", definition_id);
                    }
                    GameSignal::PhaseChanged {
                        old_phase,
                        new_phase,
                        ..
                    } => {
                        let old = old_phase.clone().unwrap_or_else(|| "none".to_string());
                        eprintln!("Phase: {} -> {}", old, new_phase);
                        phase_changes.push((old, new_phase.clone()));
                    }
                    GameSignal::AbilityActivated { ability_id, .. } => {
                        // Track ability-triggered timer activations
                        let swelling_despair: i64 = 3294098182111232;
                        let dread_strike: i64 = 3294841211453440;
                        let combusting_seed: i64 = 3294102477078528;
                        if ability_id == &swelling_despair
                            || ability_id == &dread_strike
                            || ability_id == &combusting_seed
                        {
                            ability_timer_triggers += 1;
                        }
                    }
                    _ => {}
                }

                // Feed to timer manager
                timer_manager.handle_signal(signal, cache.current_encounter());
            }

            // Tick timers and check active state
            timer_manager.tick();
            for timer in timer_manager.active_timers() {
                if !timers_activated.contains(&timer.name) {
                    eprintln!("Timer activated: {}", timer.name);
                    timers_activated.insert(timer.name.clone());

                    // Track chains
                    if timer.name.starts_with("A2") || timer.name.starts_with("A3") {
                        timer_chains_triggered.push(timer.name.clone());
                    }
                }
            }
        }
    }

    // ─── Assertions ────────────────────────────────────────────────────────────

    // Combat lifecycle
    assert!(combat_started, "Expected CombatStarted signal");
    assert!(combat_ended, "Expected CombatEnded signal");
    eprintln!("\n✓ Combat lifecycle: Started and Ended");

    // Boss detection
    assert!(boss_detected, "Expected BossEncounterDetected for Bestia");
    eprintln!("✓ Boss detected: Dread Master Bestia");

    // Phase transitions
    assert!(
        !phase_changes.is_empty(),
        "Expected at least one phase change"
    );
    let has_monsters = phase_changes.iter().any(|(_, new)| new == "monsters");
    let has_burn = phase_changes.iter().any(|(_, new)| new == "burn");
    assert!(
        has_monsters,
        "Expected phase change to 'monsters' (combat start)"
    );
    assert!(has_burn, "Expected phase change to 'burn' (boss HP < 50%)");
    eprintln!("✓ Phase transitions: monsters -> burn");

    // Combat start timers
    assert!(
        timers_activated.contains("Soft Enrage"),
        "Expected Soft Enrage timer to activate on combat start"
    );
    assert!(
        timers_activated.contains("A1: Tentacle"),
        "Expected A1: Tentacle timer to activate on combat start"
    );
    eprintln!("✓ Combat start timers: Soft Enrage, A1: Tentacle");

    // Timer chains (A1 -> A2 -> A3)
    // Note: Timer chains depend on timing - the 15s timers should chain
    // during the 6+ minute fight
    assert!(
        timers_activated.contains("A2: Monster")
            || timer_chains_triggered.contains(&"A2: Monster".to_string()),
        "Expected A2: Monster timer to chain from A1. Activated timers: {:?}",
        timers_activated
    );
    eprintln!("✓ Timer chain: A1 -> A2 triggered");

    // Ability-based timer triggers exist in the log
    assert!(
        ability_timer_triggers > 0,
        "Expected ability timer triggers (Swelling Despair, Dread Strike, or Combusting Seed)"
    );
    eprintln!(
        "✓ Ability timer triggers: {} events",
        ability_timer_triggers
    );

    // Check if ability timers activated
    let ability_timers_activated = timers_activated.contains("Swelling Despair")
        || timers_activated.contains("Dread Strike")
        || timers_activated.contains("Combusting Seed");
    if ability_timers_activated {
        eprintln!("✓ Ability-triggered timers activated");
    } else {
        eprintln!("Note: Ability timers may not have activated (source filter)");
    }

    // ─── Challenge Tracking ───────────────────────────────────────────────────
    eprintln!("\n=== Challenge Metrics ===");

    // Access the encounter's challenge tracker
    let encounter = cache
        .current_encounter()
        .expect("Expected active encounter");
    let tracker = &encounter.challenge_tracker;

    // Boss damage challenge
    if let Some(boss_dmg) = tracker.get_value("boss_damage") {
        eprintln!(
            "boss_damage: {} total ({} events)",
            boss_dmg.value, boss_dmg.event_count
        );
        assert!(boss_dmg.value > 0, "Expected boss damage to be tracked");
        assert!(boss_dmg.event_count > 0, "Expected boss damage events");
        eprintln!("✓ boss_damage challenge tracked");
    } else {
        panic!("Expected boss_damage challenge to exist");
    }

    // Add damage challenge (Larva + Monster)
    if let Some(add_dmg) = tracker.get_value("add_damage") {
        eprintln!(
            "add_damage: {} total ({} events)",
            add_dmg.value, add_dmg.event_count
        );
        assert!(add_dmg.value > 0, "Expected add damage to be tracked");
        assert!(add_dmg.event_count > 0, "Expected add damage events");
        eprintln!("✓ add_damage challenge tracked");
    } else {
        panic!("Expected add_damage challenge to exist");
    }

    // Burn phase DPS challenge
    if let Some(burn_dps) = tracker.get_value("burn_phase_dps") {
        eprintln!(
            "burn_phase_dps: {} total ({} events)",
            burn_dps.value, burn_dps.event_count
        );
        // Should have damage during burn phase
        assert!(
            burn_dps.value > 0,
            "Expected burn phase damage (boss was below 50% HP)"
        );
        eprintln!("✓ burn_phase_dps challenge tracked");
    } else {
        panic!("Expected burn_phase_dps challenge to exist");
    }

    // Boss damage taken challenge
    if let Some(dmg_taken) = tracker.get_value("boss_damage_taken") {
        eprintln!(
            "boss_damage_taken: {} total ({} events)",
            dmg_taken.value, dmg_taken.event_count
        );
        assert!(
            dmg_taken.value > 0,
            "Expected damage taken from boss to be tracked"
        );
        eprintln!("✓ boss_damage_taken challenge tracked");
    } else {
        panic!("Expected boss_damage_taken challenge to exist");
    }

    // Local player boss damage (depends on having a local player set)
    if let Some(local_dmg) = tracker.get_value("local_player_boss_damage") {
        eprintln!(
            "local_player_boss_damage: {} total ({} events)",
            local_dmg.value, local_dmg.event_count
        );
        // May be 0 if no local player is set in test context
        eprintln!("✓ local_player_boss_damage challenge exists");
    }

    // Per-player breakdown for boss damage
    if let Some(boss_dmg) = tracker.get_value("boss_damage") {
        if !boss_dmg.by_player.is_empty() {
            eprintln!("\n  Per-player boss damage:");
            for (player, value) in &boss_dmg.by_player {
                eprintln!("    {}: {}", player, value);
            }
        }
    }

    eprintln!("\n=== Summary ===");
    eprintln!("Total phase changes: {}", phase_changes.len());
    eprintln!("Total timers activated: {}", timers_activated.len());
    eprintln!("Activated timers: {:?}", timers_activated);
}
