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
        cache.load_boss_definitions(config.bosses, false);
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
            let (signals, _event, _) = processor.process_event(event, &mut cache);
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
        GameSignal::HealingDone { .. } => "HealingDone",
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
        GameSignal::ThreatModified { .. } => "ThreatModified",
        GameSignal::TimerStarted { .. } => "TimerStarted",
        GameSignal::TimerExpired { .. } => "TimerExpired",
        GameSignal::TimerCanceled { .. } => "TimerCanceled",
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
        cache.load_boss_definitions(config.bosses, false);
    }

    // Pre-initialize encounter to InCombat state (since fixture lacks EnterCombat)
    if let Some(enc) = cache.current_encounter_mut() {
        enc.state = EncounterState::InCombat;
        enc.enter_combat_time = Some(chrono::Local::now().naive_local());
    }

    let mut signals = Vec::new();
    for (line_num, line) in content.lines().enumerate() {
        if let Some(event) = parser.parse_line(line_num as u64, line) {
            let (sigs, _, _) = processor.process_event(event, &mut cache);
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
            alert_on: bt.alert_on,
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
            boss_definition_id: None,
            display_target: Default::default(),
            difficulties: Vec::new(),
            group_size: None,
            conditions: Vec::new(),
            phases: Vec::new(),
            counter_condition: None,
            per_target: bt.per_target,
            icon_ability_id: None,
            gcd_secs: None,
            queue_on_expire: false,
            queue_priority: 0,
            queue_remove_trigger: None,
            queue_blocking_timers: Vec::new(),
        })
        .collect();

    // Setup processor and timer manager
    let parser = crate::combat_log::LogParser::new(chrono::Local::now().naive_local());
    let mut processor = super::EventProcessor::new();
    let mut cache = SessionCache::default();
    cache.load_boss_definitions(config.bosses, false);

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
            let (signals, _, _) = processor.process_event(event, &mut cache);

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
            timer_manager.tick(cache.current_encounter());
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

/// Test that counters with PhaseEntered triggers fire correctly for non-HP phase transitions.
///
/// This verifies the two-pass counter evaluation fix: counters are first evaluated before
/// phase transitions (so counter_condition guards work), then re-evaluated after phase
/// transitions so PhaseEntered/PhaseEnded/AnyPhaseChange triggers see the new phases.
#[test]
fn test_counter_phase_entered_trigger() {
    use crate::combat_log::{Action, CombatEvent, Details, Effect, Entity, EntityType};
    use crate::context::intern;
    use crate::dsl::{
        BossEncounterDefinition, CounterDefinition, EntityDefinition, PhaseDefinition, Trigger,
    };
    use crate::encounter::EncounterState;
    use crate::encounter::entity_info::NpcInfo;
    use crate::game_data::effect_id;

    let ts = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();

    const ADD_NPC_CLASS_ID: i64 = 999001;
    const ADD_NPC_LOG_ID: i64 = 888001;

    // Build a boss definition with:
    // - Phase p1 (initial, starts on CombatStart)
    // - Phase p2 (starts on EntityDeath of add NPC)
    // - Counter that increments on PhaseEntered(p2)
    // - Counter that resets on PhaseEnded(p1)
    let mut boss_def = BossEncounterDefinition {
        id: "test_phase_counter".to_string(),
        name: "Test Phase Counter".to_string(),
        entities: vec![
            EntityDefinition {
                name: "Boss".to_string(),
                ids: vec![100001],
                is_boss: true,
                is_kill_target: true,
                triggers_encounter: None,
                show_on_hp_overlay: None,
                hp_markers: vec![],
                shields: vec![],
                pushes_at: None,
            },
            EntityDefinition {
                name: "Add".to_string(),
                ids: vec![ADD_NPC_CLASS_ID],
                is_boss: false,
                is_kill_target: false,
                triggers_encounter: None,
                show_on_hp_overlay: None,
                hp_markers: vec![],
                shields: vec![],
                pushes_at: None,
            },
        ],
        phases: vec![
            PhaseDefinition {
                id: "p1".to_string(),
                name: "Phase 1".to_string(),
                enabled: true,
                display_text: None,
                start_trigger: Trigger::CombatStart,
                end_trigger: None,
                preceded_by: None,
                conditions: vec![],
                counter_condition: None,
                resets_counters: vec![],
                difficulties: vec![],
            },
            PhaseDefinition {
                id: "p2".to_string(),
                name: "Phase 2".to_string(),
                enabled: true,
                display_text: None,
                start_trigger: Trigger::EntityDeath {
                    selector: vec![crate::dsl::triggers::EntitySelector::Name(
                        "Add".to_string(),
                    )],
                },
                end_trigger: None,
                preceded_by: None,
                conditions: vec![],
                counter_condition: None,
                resets_counters: vec![],
                difficulties: vec![],
            },
        ],
        counters: vec![
            CounterDefinition {
                id: "phase_enter_ct".to_string(),
                name: "Phase Enter Counter".to_string(),
                enabled: true,
                display_text: None,
                increment_on: Trigger::PhaseEntered {
                    phase_id: "p2".to_string(),
                },
                decrement_on: None,
                reset_on: Trigger::CombatEnd,
                initial_value: 0,
                decrement: false,
                set_value: None,
                track_effect_stacks: None,
            },
            CounterDefinition {
                id: "phase_end_ct".to_string(),
                name: "Phase End Counter".to_string(),
                enabled: true,
                display_text: None,
                increment_on: Trigger::PhaseEnded {
                    phase_id: "p1".to_string(),
                },
                decrement_on: None,
                reset_on: Trigger::CombatEnd,
                initial_value: 0,
                decrement: false,
                set_value: None,
                track_effect_stacks: None,
            },
        ],
        ..Default::default()
    };
    boss_def.build_indexes();

    // Setup cache
    let mut cache = SessionCache::default();
    cache.load_boss_definitions(vec![boss_def], true);

    // Get encounter into InCombat state with active boss and initial phase
    if let Some(enc) = cache.current_encounter_mut() {
        enc.state = EncounterState::InCombat;
        enc.enter_combat_time = Some(ts);
        enc.set_active_boss_idx(Some(0));
        enc.set_phase("p1", ts);

        // Register the add NPC so entity lifecycle can track it
        enc.npcs.insert(
            ADD_NPC_LOG_ID,
            NpcInfo {
                name: intern("Add"),
                log_id: ADD_NPC_LOG_ID,
                class_id: ADD_NPC_CLASS_ID,
                current_hp: 1000,
                max_hp: 1000,
                ..Default::default()
            },
        );
    }

    let mut processor = EventProcessor::new();

    // Mark the add NPC instance as seen (required for NpcFirstSeen dedup)
    // We do this by processing a dummy event from the NPC first
    let npc_seen_event = CombatEvent {
        line_number: 1,
        timestamp: ts,
        source_entity: Entity {
            name: intern("Add"),
            class_id: ADD_NPC_CLASS_ID,
            log_id: ADD_NPC_LOG_ID,
            entity_type: EntityType::Npc,
            health: (1000, 1000),
        },
        target_entity: Entity::default(),
        action: Action::default(),
        effect: Effect::default(),
        details: Details::default(),
    };
    let _ = processor.process_event(npc_seen_event, &mut cache);

    // Now send a death event for the add NPC — this should:
    // 1. Emit EntityDeath signal
    // 2. check_entity_phase_transitions matches EntityDeath -> starts p2
    // 3. Second-pass counter eval sees PhaseChanged(p2) -> increments phase_enter_ct
    let death_event = CombatEvent {
        line_number: 2,
        timestamp: ts + chrono::Duration::seconds(10),
        source_entity: Entity::default(),
        target_entity: Entity {
            name: intern("Add"),
            class_id: ADD_NPC_CLASS_ID,
            log_id: ADD_NPC_LOG_ID,
            entity_type: EntityType::Npc,
            health: (0, 1000),
        },
        action: Action::default(),
        effect: Effect {
            effect_id: effect_id::DEATH,
            ..Default::default()
        },
        details: Details::default(),
    };
    let (signals, _, _) = processor.process_event(death_event, &mut cache);

    // Debug: print all signals
    for s in &signals {
        eprintln!("Signal: {}", signal_type_name(s));
        if let GameSignal::PhaseChanged {
            old_phase,
            new_phase,
            ..
        } = s
        {
            eprintln!("  PhaseChanged: {:?} -> {}", old_phase, new_phase);
        }
        if let GameSignal::CounterChanged {
            counter_id,
            old_value,
            new_value,
            ..
        } = s
        {
            eprintln!(
                "  CounterChanged: {} {} -> {}",
                counter_id, old_value, new_value
            );
        }
    }

    // Verify EntityDeath signal was emitted
    let has_entity_death = signals
        .iter()
        .any(|s| matches!(s, GameSignal::EntityDeath { .. }));
    assert!(has_entity_death, "Expected EntityDeath signal");

    // Verify phase changed to p2
    let has_phase_change = signals.iter().any(
        |s| matches!(s, GameSignal::PhaseChanged { new_phase, .. } if new_phase == "p2"),
    );
    assert!(
        has_phase_change,
        "Expected PhaseChanged to p2 on EntityDeath"
    );

    // Verify counter with PhaseEntered(p2) trigger incremented (THE KEY FIX)
    let phase_enter_counter = signals.iter().find(
        |s| matches!(s, GameSignal::CounterChanged { counter_id, .. } if counter_id == "phase_enter_ct"),
    );
    assert!(
        phase_enter_counter.is_some(),
        "Expected CounterChanged for phase_enter_ct (PhaseEntered trigger). \
         This tests the two-pass counter evaluation fix."
    );
    if let Some(GameSignal::CounterChanged {
        old_value,
        new_value,
        ..
    }) = phase_enter_counter
    {
        assert_eq!(*old_value, 0, "Counter should start at 0");
        assert_eq!(*new_value, 1, "Counter should increment to 1");
    }

    // Verify the encounter's counter state is correct
    let enc = cache.current_encounter().unwrap();
    assert_eq!(
        enc.get_counter("phase_enter_ct"),
        1,
        "phase_enter_ct should be 1 after PhaseEntered(p2)"
    );
    assert_eq!(
        enc.phase(),
        Some("p2"),
        "Current phase should be p2 after entity death transition"
    );
}

/// Test the fixed-point loop: phase cascade (A → B → C) in a single event.
///
/// Phase A ends via EntityDeath → Phase B starts (PhaseEnded(A) trigger) →
/// Phase B's end_trigger fires immediately → Phase C starts (PhaseEnded(B) trigger).
/// All three phase transitions happen within a single process_event call.
#[test]
fn test_phase_cascade_in_single_event() {
    use crate::combat_log::{Action, CombatEvent, Details, Effect, Entity, EntityType};
    use crate::context::intern;
    use crate::dsl::{
        BossEncounterDefinition, CounterDefinition, EntityDefinition, PhaseDefinition, Trigger,
    };
    use crate::encounter::EncounterState;
    use crate::encounter::entity_info::NpcInfo;
    use crate::game_data::effect_id;

    let ts = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();

    const ADD_NPC_CLASS_ID: i64 = 999001;
    const ADD_NPC_LOG_ID: i64 = 888001;

    // Phase chain: p1 → p2 (on EntityDeath) → p3 (on PhaseEnded(p2), immediate end_trigger)
    // Phase p2 has an end_trigger that fires immediately (PhaseEntered(p2) — fires the moment p2 starts)
    // Counter tracks how many phase entries happened
    let mut boss_def = BossEncounterDefinition {
        id: "test_cascade".to_string(),
        name: "Test Cascade".to_string(),
        entities: vec![
            EntityDefinition {
                name: "Boss".to_string(),
                ids: vec![100001],
                is_boss: true,
                is_kill_target: true,
                triggers_encounter: None,
                show_on_hp_overlay: None,
                hp_markers: vec![],
                shields: vec![],
                pushes_at: None,
            },
            EntityDefinition {
                name: "Add".to_string(),
                ids: vec![ADD_NPC_CLASS_ID],
                is_boss: false,
                is_kill_target: false,
                triggers_encounter: None,
                show_on_hp_overlay: None,
                hp_markers: vec![],
                shields: vec![],
                pushes_at: None,
            },
        ],
        phases: vec![
            PhaseDefinition {
                id: "p1".to_string(),
                name: "Phase 1".to_string(),
                enabled: true,
                display_text: None,
                start_trigger: Trigger::CombatStart,
                end_trigger: Some(Trigger::EntityDeath {
                    selector: vec![crate::dsl::triggers::EntitySelector::Name("Add".to_string())],
                }),
                preceded_by: None,
                conditions: vec![],
                counter_condition: None,
                resets_counters: vec![],
                difficulties: vec![],
            },
            PhaseDefinition {
                id: "p2".to_string(),
                name: "Phase 2 (transient)".to_string(),
                enabled: true,
                display_text: None,
                start_trigger: Trigger::PhaseEnded {
                    phase_id: "p1".to_string(),
                },
                // p2's end trigger fires on EntityDeath too (same signal that started the chain)
                end_trigger: Some(Trigger::EntityDeath {
                    selector: vec![crate::dsl::triggers::EntitySelector::Name("Add".to_string())],
                }),
                preceded_by: Some("p1".to_string()),
                conditions: vec![],
                counter_condition: None,
                resets_counters: vec![],
                difficulties: vec![],
            },
            PhaseDefinition {
                id: "p3".to_string(),
                name: "Phase 3 (final)".to_string(),
                enabled: true,
                display_text: None,
                start_trigger: Trigger::PhaseEnded {
                    phase_id: "p2".to_string(),
                },
                end_trigger: None,
                preceded_by: Some("p2".to_string()),
                conditions: vec![],
                counter_condition: None,
                resets_counters: vec![],
                difficulties: vec![],
            },
        ],
        counters: vec![CounterDefinition {
            id: "cascade_ct".to_string(),
            name: "Cascade Counter".to_string(),
            enabled: true,
            display_text: None,
            increment_on: Trigger::AnyPhaseChange,
            decrement_on: None,
            reset_on: Trigger::CombatEnd,
            initial_value: 0,
            decrement: false,
            set_value: None,
            track_effect_stacks: None,
        }],
        ..Default::default()
    };
    boss_def.build_indexes();

    let mut cache = SessionCache::default();
    cache.load_boss_definitions(vec![boss_def], true);

    if let Some(enc) = cache.current_encounter_mut() {
        enc.state = EncounterState::InCombat;
        enc.enter_combat_time = Some(ts);
        enc.set_active_boss_idx(Some(0));
        enc.set_phase("p1", ts);
        enc.npcs.insert(
            ADD_NPC_LOG_ID,
            NpcInfo {
                name: intern("Add"),
                log_id: ADD_NPC_LOG_ID,
                class_id: ADD_NPC_CLASS_ID,
                current_hp: 1000,
                max_hp: 1000,
                ..Default::default()
            },
        );
    }

    let mut processor = EventProcessor::new();

    // Register NPC
    let npc_event = CombatEvent {
        line_number: 1,
        timestamp: ts,
        source_entity: Entity {
            name: intern("Add"),
            class_id: ADD_NPC_CLASS_ID,
            log_id: ADD_NPC_LOG_ID,
            entity_type: EntityType::Npc,
            health: (1000, 1000),
        },
        target_entity: Entity::default(),
        action: Action::default(),
        effect: Effect::default(),
        details: Details::default(),
    };
    let _ = processor.process_event(npc_event, &mut cache);

    // Kill the add — should cascade: p1→p2→p3 in a single event
    let death_event = CombatEvent {
        line_number: 2,
        timestamp: ts + chrono::Duration::seconds(10),
        source_entity: Entity::default(),
        target_entity: Entity {
            name: intern("Add"),
            class_id: ADD_NPC_CLASS_ID,
            log_id: ADD_NPC_LOG_ID,
            entity_type: EntityType::Npc,
            health: (0, 1000),
        },
        action: Action::default(),
        effect: Effect {
            effect_id: effect_id::DEATH,
            ..Default::default()
        },
        details: Details::default(),
    };
    let (signals, _, _) = processor.process_event(death_event, &mut cache);

    // Debug output
    for s in &signals {
        match s {
            GameSignal::PhaseChanged { old_phase, new_phase, .. } => {
                eprintln!("PhaseChanged: {:?} -> {}", old_phase, new_phase);
            }
            GameSignal::CounterChanged { counter_id, old_value, new_value, .. } => {
                eprintln!("CounterChanged: {} {} -> {}", counter_id, old_value, new_value);
            }
            GameSignal::PhaseEndTriggered { phase_id, .. } => {
                eprintln!("PhaseEndTriggered: {}", phase_id);
            }
            _ => {}
        }
    }

    // Verify full cascade happened
    let phase_changes: Vec<_> = signals
        .iter()
        .filter_map(|s| match s {
            GameSignal::PhaseChanged { new_phase, .. } => Some(new_phase.as_str()),
            _ => None,
        })
        .collect();

    assert!(
        phase_changes.contains(&"p2"),
        "Expected PhaseChanged to p2. Got: {:?}",
        phase_changes
    );
    assert!(
        phase_changes.contains(&"p3"),
        "Expected PhaseChanged to p3 (cascade). Got: {:?}",
        phase_changes
    );

    // Final state should be p3
    let enc = cache.current_encounter().unwrap();
    assert_eq!(enc.phase(), Some("p3"), "Final phase should be p3");

    // Counter should have incremented for each phase change (p1→p2, p2→p3)
    let cascade_count = enc.get_counter("cascade_ct");
    assert!(
        cascade_count >= 2,
        "cascade_ct should be >= 2 (one per phase change), got {}",
        cascade_count
    );
}

/// Test counter → phase chain: counter reaches threshold → phase transition.
///
/// Multiple EntityDeath events increment a counter. When it reaches 3, a phase
/// transition with counter_condition fires. All within the fixed-point loop.
#[test]
fn test_counter_reaches_enables_phase() {
    use crate::combat_log::{Action, CombatEvent, Details, Effect, Entity, EntityType};
    use crate::context::intern;
    use crate::dsl::CounterCondition;
    use crate::dsl::{
        BossEncounterDefinition, CounterDefinition, EntityDefinition, PhaseDefinition, Trigger,
    };
    use crate::encounter::EncounterState;
    use crate::encounter::entity_info::NpcInfo;
    use crate::game_data::effect_id;

    let ts = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();

    const ADD_NPC_CLASS_ID: i64 = 999001;

    // Phase p2 requires kill_count >= 3
    let mut boss_def = BossEncounterDefinition {
        id: "test_counter_phase".to_string(),
        name: "Test Counter Phase".to_string(),
        entities: vec![
            EntityDefinition {
                name: "Boss".to_string(),
                ids: vec![100001],
                is_boss: true,
                is_kill_target: true,
                triggers_encounter: None,
                show_on_hp_overlay: None,
                hp_markers: vec![],
                shields: vec![],
                pushes_at: None,
            },
            EntityDefinition {
                name: "Add".to_string(),
                ids: vec![ADD_NPC_CLASS_ID],
                is_boss: false,
                is_kill_target: false,
                triggers_encounter: None,
                show_on_hp_overlay: None,
                hp_markers: vec![],
                shields: vec![],
                pushes_at: None,
            },
        ],
        phases: vec![
            PhaseDefinition {
                id: "p1".to_string(),
                name: "Phase 1".to_string(),
                enabled: true,
                display_text: None,
                start_trigger: Trigger::CombatStart,
                end_trigger: None,
                preceded_by: None,
                conditions: vec![],
                counter_condition: None,
                resets_counters: vec![],
                difficulties: vec![],
            },
            PhaseDefinition {
                id: "p2".to_string(),
                name: "Phase 2 (after 3 kills)".to_string(),
                enabled: true,
                display_text: None,
                start_trigger: Trigger::EntityDeath {
                    selector: vec![crate::dsl::triggers::EntitySelector::Name("Add".to_string())],
                },
                end_trigger: None,
                preceded_by: None,
                conditions: vec![],
                counter_condition: Some(CounterCondition {
                    counter_id: "kill_count".to_string(),
                    operator: crate::dsl::ComparisonOp::Gte,
                    value: 3,
                }),
                resets_counters: vec![],
                difficulties: vec![],
            },
        ],
        counters: vec![CounterDefinition {
            id: "kill_count".to_string(),
            name: "Kill Count".to_string(),
            enabled: true,
            display_text: None,
            increment_on: Trigger::EntityDeath {
                selector: vec![crate::dsl::triggers::EntitySelector::Name("Add".to_string())],
            },
            decrement_on: None,
            reset_on: Trigger::CombatEnd,
            initial_value: 0,
            decrement: false,
            set_value: None,
            track_effect_stacks: None,
        }],
        ..Default::default()
    };
    boss_def.build_indexes();

    let mut cache = SessionCache::default();
    cache.load_boss_definitions(vec![boss_def], true);

    if let Some(enc) = cache.current_encounter_mut() {
        enc.state = EncounterState::InCombat;
        enc.enter_combat_time = Some(ts);
        enc.set_active_boss_idx(Some(0));
        enc.set_phase("p1", ts);
    }

    let mut processor = EventProcessor::new();

    // Kill 3 adds (different log_ids so each registers as a fresh death)
    for i in 0..3u32 {
        let log_id = 888000 + i as i64;
        let event_ts = ts + chrono::Duration::seconds(i as i64 + 1);

        // Register NPC
        if let Some(enc) = cache.current_encounter_mut() {
            enc.npcs.insert(
                log_id,
                NpcInfo {
                    name: intern("Add"),
                    log_id,
                    class_id: ADD_NPC_CLASS_ID,
                    current_hp: 1000,
                    max_hp: 1000,
                    ..Default::default()
                },
            );
        }

        // NPC seen event
        let seen = CombatEvent {
            line_number: i as u64 * 2,
            timestamp: event_ts,
            source_entity: Entity {
                name: intern("Add"),
                class_id: ADD_NPC_CLASS_ID,
                log_id,
                entity_type: EntityType::Npc,
                health: (1000, 1000),
            },
            target_entity: Entity::default(),
            action: Action::default(),
            effect: Effect::default(),
            details: Details::default(),
        };
        let _ = processor.process_event(seen, &mut cache);

        // Kill event
        let death = CombatEvent {
            line_number: i as u64 * 2 + 1,
            timestamp: event_ts,
            source_entity: Entity::default(),
            target_entity: Entity {
                name: intern("Add"),
                class_id: ADD_NPC_CLASS_ID,
                log_id,
                entity_type: EntityType::Npc,
                health: (0, 1000),
            },
            action: Action::default(),
            effect: Effect {
                effect_id: effect_id::DEATH,
                ..Default::default()
            },
            details: Details::default(),
        };
        let (signals, _, _) = processor.process_event(death, &mut cache);

        let kill_num = i + 1;
        eprintln!("Kill #{}: counter={}", kill_num, cache.current_encounter().unwrap().get_counter("kill_count"));

        if kill_num < 3 {
            // Phase should NOT have transitioned yet
            let has_p2 = signals.iter().any(
                |s| matches!(s, GameSignal::PhaseChanged { new_phase, .. } if new_phase == "p2"),
            );
            assert!(
                !has_p2,
                "Phase should NOT transition to p2 after only {} kills",
                kill_num
            );
        } else {
            // Kill 3: counter_condition (kill_count >= 3) should be satisfied
            // AND EntityDeath trigger matches → phase transitions to p2
            let has_p2 = signals.iter().any(
                |s| matches!(s, GameSignal::PhaseChanged { new_phase, .. } if new_phase == "p2"),
            );
            assert!(
                has_p2,
                "Phase SHOULD transition to p2 on kill #{} (counter_condition: kill_count >= 3)",
                kill_num
            );
        }
    }

    let enc = cache.current_encounter().unwrap();
    assert_eq!(enc.phase(), Some("p2"), "Final phase should be p2");
    assert_eq!(enc.get_counter("kill_count"), 3, "kill_count should be 3");
}

/// Regression test: counter with `increment_on = phase_ended("X")` should only
/// increment once when phase X has an `end_trigger` and another phase starts on
/// `phase_ended("X")`.
///
/// Previously, both `PhaseEndTriggered` and `PhaseChanged { old_phase }` would
/// match the `PhaseEnded` trigger, causing a double-increment.
#[test]
fn test_phase_ended_counter_no_double_increment() {
    use crate::combat_log::{Action, CombatEvent, Details, Effect, Entity, EntityType};
    use crate::context::intern;
    use crate::dsl::{
        BossEncounterDefinition, CounterDefinition, EntityDefinition, PhaseDefinition, Trigger,
    };
    use crate::dsl::triggers::EffectSelector;
    use crate::encounter::EncounterState;
    use crate::encounter::entity_info::NpcInfo;
    use crate::game_data::effect_type_id;

    let ts = chrono::NaiveDate::from_ymd_opt(2026, 1, 1)
        .unwrap()
        .and_hms_opt(12, 0, 0)
        .unwrap();

    // Setup: mimics the fs_shield → fs_tanks cycle from Explosive Conflict.
    //
    // - "shield" phase: starts when a specific effect is applied, ends when it's removed
    // - "tanks" phase: starts on phase_ended("shield")
    // - counter: increments on phase_ended("shield")
    //
    // When the shield effect is removed:
    //   1. shield's end_trigger fires → PhaseEndTriggered { "shield" }
    //   2. tanks' start_trigger matches phase_ended("shield") → PhaseChanged { old: "shield", new: "tanks" }
    //   3. Counter should increment exactly ONCE (not twice)

    const BOSS_NPC_CLASS_ID: i64 = 100001;
    const BOSS_NPC_LOG_ID: i64 = 200001;
    const SHIELD_EFFECT_ID: i64 = 9999;

    let mut boss_def = BossEncounterDefinition {
        id: "test_dedup".to_string(),
        name: "Test PhaseEnded Dedup".to_string(),
        entities: vec![EntityDefinition {
            name: "Boss".to_string(),
            ids: vec![BOSS_NPC_CLASS_ID],
            is_boss: true,
            is_kill_target: true,
            triggers_encounter: None,
            show_on_hp_overlay: None,
            hp_markers: vec![],
            shields: vec![],
                pushes_at: None,
        }],
        phases: vec![
            PhaseDefinition {
                id: "tanks".to_string(),
                name: "Tanks".to_string(),
                enabled: true,
                display_text: None,
                start_trigger: Trigger::PhaseEnded {
                    phase_id: "shield".to_string(),
                },
                end_trigger: None,
                preceded_by: None,
                conditions: vec![],
                counter_condition: None,
                resets_counters: vec![],
                difficulties: vec![],
            },
            PhaseDefinition {
                id: "shield".to_string(),
                name: "Shield".to_string(),
                enabled: true,
                display_text: None,
                start_trigger: Trigger::EffectApplied {
                    effects: vec![EffectSelector::Id(SHIELD_EFFECT_ID as u64)],
                    source: baras_types::EntityFilter::Any,
                    target: baras_types::EntityFilter::Any,
                },
                end_trigger: Some(Trigger::EffectRemoved {
                    effects: vec![EffectSelector::Id(SHIELD_EFFECT_ID as u64)],
                    source: baras_types::EntityFilter::Any,
                    target: baras_types::EntityFilter::Any,
                }),
                preceded_by: None,
                conditions: vec![],
                counter_condition: None,
                resets_counters: vec![],
                difficulties: vec![],
            },
        ],
        counters: vec![CounterDefinition {
            id: "shield_count".to_string(),
            name: "Shield Count".to_string(),
            enabled: true,
            display_text: None,
            increment_on: Trigger::PhaseEnded {
                phase_id: "shield".to_string(),
            },
            decrement_on: None,
            reset_on: Trigger::CombatEnd,
            initial_value: 0,
            decrement: false,
            set_value: None,
            track_effect_stacks: None,
        }],
        ..Default::default()
    };
    boss_def.build_indexes();

    let mut cache = SessionCache::default();
    cache.load_boss_definitions(vec![boss_def], true);

    if let Some(enc) = cache.current_encounter_mut() {
        enc.state = EncounterState::InCombat;
        enc.enter_combat_time = Some(ts);
        enc.set_active_boss_idx(Some(0));
        enc.set_phase("shield", ts);
        enc.npcs.insert(
            BOSS_NPC_LOG_ID,
            NpcInfo {
                name: intern("Boss"),
                log_id: BOSS_NPC_LOG_ID,
                class_id: BOSS_NPC_CLASS_ID,
                current_hp: 100_000,
                max_hp: 100_000,
                ..Default::default()
            },
        );
    }

    let mut processor = EventProcessor::new();

    // Register NPC
    let npc_event = CombatEvent {
        line_number: 1,
        timestamp: ts,
        source_entity: Entity {
            name: intern("Boss"),
            class_id: BOSS_NPC_CLASS_ID,
            log_id: BOSS_NPC_LOG_ID,
            entity_type: EntityType::Npc,
            health: (100_000, 100_000),
        },
        target_entity: Entity::default(),
        action: Action::default(),
        effect: Effect::default(),
        details: Details::default(),
    };
    let _ = processor.process_event(npc_event, &mut cache);

    // Shield effect removed → should trigger shield's end_trigger → tanks starts
    let shield_removed = CombatEvent {
        line_number: 2,
        timestamp: ts + chrono::Duration::seconds(25),
        source_entity: Entity::default(),
        target_entity: Entity {
            name: intern("Boss"),
            class_id: BOSS_NPC_CLASS_ID,
            log_id: BOSS_NPC_LOG_ID,
            entity_type: EntityType::Npc,
            health: (100_000, 100_000),
        },
        action: Action::default(),
        effect: Effect {
            type_id: effect_type_id::REMOVEEFFECT,
            effect_id: SHIELD_EFFECT_ID,
            ..Default::default()
        },
        details: Details::default(),
    };
    let (signals, _, _) = processor.process_event(shield_removed, &mut cache);

    // Debug output
    for s in &signals {
        match s {
            GameSignal::PhaseChanged {
                old_phase,
                new_phase,
                ..
            } => {
                eprintln!("PhaseChanged: {:?} -> {}", old_phase, new_phase);
            }
            GameSignal::CounterChanged {
                counter_id,
                old_value,
                new_value,
                ..
            } => {
                eprintln!("CounterChanged: {} {} -> {}", counter_id, old_value, new_value);
            }
            GameSignal::PhaseEndTriggered { phase_id, .. } => {
                eprintln!("PhaseEndTriggered: {}", phase_id);
            }
            _ => {}
        }
    }

    // Verify phase transitioned to tanks
    let has_tanks = signals.iter().any(
        |s| matches!(s, GameSignal::PhaseChanged { new_phase, .. } if new_phase == "tanks"),
    );
    assert!(has_tanks, "Expected PhaseChanged to tanks");

    // KEY ASSERTION: counter should have incremented exactly once
    let counter_changes: Vec<_> = signals
        .iter()
        .filter_map(|s| match s {
            GameSignal::CounterChanged {
                counter_id,
                old_value,
                new_value,
                ..
            } if counter_id == "shield_count" => Some((*old_value, *new_value)),
            _ => None,
        })
        .collect();

    assert_eq!(
        counter_changes.len(),
        1,
        "shield_count should increment exactly once, got {} changes: {:?}",
        counter_changes.len(),
        counter_changes
    );
    assert_eq!(
        counter_changes[0],
        (0, 1),
        "shield_count should go from 0 to 1, got {:?}",
        counter_changes[0]
    );

    // Verify final state
    let enc = cache.current_encounter().unwrap();
    assert_eq!(enc.phase(), Some("tanks"), "Final phase should be tanks");
    assert_eq!(
        enc.get_counter("shield_count"),
        1,
        "shield_count should be 1 (not 2)"
    );
}
