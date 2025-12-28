//! Boss definition validation CLI
//!
//! Runs a combat log through boss definitions and reports:
//! - Entities seen (NPC IDs, names, deaths)
//! - Timers that fired
//! - Counter changes
//! - Phase transitions
//! - Untracked abilities from boss entities
//! - Untracked effects on players

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use chrono::NaiveDateTime;
use clap::Parser;

use baras_core::boss::{load_bosses_with_paths, BossEncounterDefinition, ChallengeContext, EntityInfo};
use baras_core::combat_log::{CombatEvent, EntityType, LogParser};
use baras_core::context::resolve;
use baras_core::encounter::ChallengeTracker;
use baras_core::events::{EventProcessor, GameSignal, SignalHandler};
use baras_core::game_data::{effect_id, effect_type_id};
use baras_core::state::SessionCache;
use baras_core::timers::{FiredAlert, TimerManager};

#[derive(Parser, Debug)]
#[command(name = "baras-validate")]
#[command(about = "Validate boss definitions against combat logs")]
struct Args {
    /// Path to combat log file
    #[arg(short, long)]
    log: PathBuf,

    /// Boss ID to validate (e.g., "sword_squadron")
    #[arg(short, long)]
    boss: String,

    /// Path to definitions directory (defaults to bundled)
    #[arg(short, long)]
    definitions: Option<PathBuf>,

    /// Show all abilities, not just untracked
    #[arg(long)]
    all_abilities: bool,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Tracking State
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Default)]
struct EntitySeen {
    npc_id: i64,
    name: String,
    first_seen: Option<NaiveDateTime>,
    last_seen: Option<NaiveDateTime>,
    death_count: u32,
    last_hp: Option<i64>,
    max_hp: Option<i64>,
}

#[derive(Debug)]
struct TimerFired {
    timestamp: NaiveDateTime,
    timer_id: String,
    timer_name: String,
    duration_secs: f32,
    trigger_info: String,
}

#[derive(Debug)]
struct CounterChange {
    timestamp: NaiveDateTime,
    counter_id: String,
    old_value: u32,
    new_value: u32,
    reason: String,
}

#[derive(Debug)]
struct PhaseChange {
    timestamp: NaiveDateTime,
    old_phase: Option<String>,
    new_phase: String,
}

#[derive(Debug, Default)]
struct AbilitySeen {
    ability_id: i64,
    name: String,
    count: u32,
    sources: HashSet<String>,
}

#[derive(Debug, Default)]
struct EffectSeen {
    effect_id: i64,
    name: String,
    apply_count: u32,
    remove_count: u32,
}

#[derive(Debug, Default)]
struct ValidationState {
    combat_start: Option<NaiveDateTime>,
    entities: HashMap<i64, EntitySeen>,
    timers_fired: Vec<TimerFired>,
    alerts_fired: Vec<FiredAlert>,
    counter_changes: Vec<CounterChange>,
    phase_changes: Vec<PhaseChange>,
    abilities_from_bosses: HashMap<i64, AbilitySeen>,
    effects_on_players: HashMap<i64, EffectSeen>,
    tracked_ability_ids: HashSet<u64>,
    tracked_effect_ids: HashSet<u64>,
    boss_entity_ids: HashSet<i64>,
}

// ═══════════════════════════════════════════════════════════════════════════════
// Main
// ═══════════════════════════════════════════════════════════════════════════════

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Load boss definitions
    let def_path = args.definitions.clone().unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("core/definitions/encounters")
    });

    let bosses_with_paths = load_bosses_with_paths(&def_path)?;
    let bosses: Vec<&BossEncounterDefinition> = bosses_with_paths.iter().map(|b| &b.boss).collect();

    // Find the requested boss
    let boss_def = bosses
        .iter()
        .find(|b| b.id.eq_ignore_ascii_case(&args.boss))
        .ok_or_else(|| format!("Boss '{}' not found. Available: {}",
            args.boss,
            bosses.iter().map(|b| b.id.as_str()).collect::<Vec<_>>().join(", ")
        ))?;

    println!("Validating: {} ({})", boss_def.name, boss_def.area_name);

    // Build tracking state
    let mut state = ValidationState::default();
    populate_tracked_ids(&mut state, boss_def);

    // Parse and process log (handle non-UTF8 gracefully)
    let file = File::open(&args.log)?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader
        .lines()
        .filter_map(|l| l.ok()) // Skip lines with encoding errors
        .collect();

    if lines.is_empty() {
        return Err("Log file is empty or unreadable".into());
    }

    // Extract session date from first line timestamp
    let session_date = extract_session_date(&lines[0])?;
    let parser = LogParser::new(session_date);

    let mut processor = EventProcessor::new();
    let mut cache = SessionCache::default();
    let mut timer_manager = TimerManager::new();

    // Load boss definitions into both cache (for phase detection) and timer manager
    let boss_defs = vec![(*boss_def).clone()];

    // Debug: show phase and counter info
    eprintln!("Boss {} has {} phases, {} counters:", boss_def.name, boss_def.phases.len(), boss_def.counters.len());
    for phase in &boss_def.phases {
        eprintln!("  Phase: {} ({:?})", phase.id, phase.start_trigger);
    }
    for counter in &boss_def.counters {
        eprintln!("  Counter: {} ({:?})", counter.id, counter.increment_on);
    }

    cache.load_boss_definitions(boss_defs.clone());
    timer_manager.load_boss_definitions(boss_defs);

    // Disable live mode to process historical events (bypass recency threshold)
    timer_manager.set_live_mode(false);

    // Initialize challenge tracker
    let mut challenge_tracker = ChallengeTracker::new();
    let boss_npc_ids: Vec<i64> = boss_def.entities.iter()
        .filter(|e| e.is_boss)
        .flat_map(|e| e.ids.iter().copied())
        .collect();
    challenge_tracker.start(boss_def.challenges.clone(), boss_npc_ids.clone());

    // Debug: show challenge info
    if !boss_def.challenges.is_empty() {
        eprintln!("Challenges: {}", boss_def.challenges.len());
        for c in &boss_def.challenges {
            eprintln!("  Challenge: {} ({:?})", c.id, c.metric);
        }
    }

    // Build challenge context (updated as we process)
    let mut challenge_ctx = ChallengeContext::default();
    challenge_ctx.boss_npc_ids = boss_npc_ids;

    // Track player names by entity_id for challenge breakdown
    let mut player_names: HashMap<i64, String> = HashMap::new();

    // Debug: Track damage events by phase for challenge debugging
    let mut damage_by_phase: HashMap<String, (u32, i64)> = HashMap::new(); // phase -> (count, total)
    let mut machine_core_damage_total: u32 = 0;
    let mut kill_target_death_time: Option<NaiveDateTime> = None; // Track when kill target dies (fight end)
    const MACHINE_CORE_NPC_ID: i64 = 3447583133401088;

    let mut event_count = 0;
    let mut local_player_id: i64 = 0;
    for (line_num, line) in lines.iter().enumerate() {
        if let Some(event) = parser.parse_line(line_num as u64, line) {
            event_count += 1;
            let signals = processor.process_event(event.clone(), &mut cache);

            // Detect local player early (first player entity we see as source or target)
            // This must happen before signal processing for local_player filters to work
            if local_player_id == 0 {
                if event.source_entity.entity_type == EntityType::Player {
                    local_player_id = event.source_entity.log_id;
                    timer_manager.set_local_player_id(local_player_id);
                } else if event.target_entity.entity_type == EntityType::Player {
                    local_player_id = event.target_entity.log_id;
                    timer_manager.set_local_player_id(local_player_id);
                }
            }

            // Track timers before processing
            let timers_before: HashSet<String> = timer_manager
                .active_timers()
                .iter()
                .map(|t| t.definition_id.clone())
                .collect();

            // Process signals through timer manager
            for signal in &signals {
                timer_manager.handle_signal(signal);
            }

            // Capture fired alerts (ephemeral notifications)
            state.alerts_fired.extend(timer_manager.take_fired_alerts());

            // Detect newly started timers (countdown timers, not alerts)
            for timer in timer_manager.active_timers() {
                if !timers_before.contains(&timer.definition_id) {
                    state.timers_fired.push(TimerFired {
                        timestamp: event.timestamp,
                        timer_id: timer.definition_id.clone(),
                        timer_name: timer.name.clone(),
                        duration_secs: timer.duration.as_secs_f32(),
                        trigger_info: String::new(),
                    });
                }
            }

            // Track entities, abilities, effects
            track_event(&mut state, &event, boss_def);

            // Track signals and update challenge context
            for signal in &signals {
                match signal {
                    GameSignal::PhaseChanged { new_phase, timestamp, .. } => {
                        challenge_ctx.current_phase = Some(new_phase.clone());
                        challenge_tracker.set_phase(new_phase, *timestamp);
                    }
                    GameSignal::CounterChanged { counter_id, new_value, .. } => {
                        challenge_ctx.counters.insert(counter_id.clone(), *new_value);
                    }
                    GameSignal::EntityDeath { npc_id, timestamp, .. } => {
                        // Track kill target death (Machine Core) as fight end
                        if *npc_id == MACHINE_CORE_NPC_ID {
                            kill_target_death_time = Some(*timestamp);
                        }
                    }
                    _ => {}
                }
                track_signal(&mut state, signal);
            }

            // Track player names for challenge breakdown
            if event.source_entity.entity_type == EntityType::Player {
                player_names.entry(event.source_entity.log_id)
                    .or_insert_with(|| resolve(event.source_entity.name).to_string());
            }

            // Process damage events through challenge tracker
            if event.effect.effect_id == effect_id::DAMAGE {
                let source = entity_to_info(&event.source_entity, local_player_id);
                let target = entity_to_info(&event.target_entity, local_player_id);
                let damage = event.details.dmg_effective as i64;

                // Debug: Track Machine Core damage by phase
                if target.npc_id == Some(MACHINE_CORE_NPC_ID) && damage > 0 {
                    machine_core_damage_total += 1;
                    let phase = challenge_ctx.current_phase.clone().unwrap_or_else(|| "none".to_string());
                    let entry = damage_by_phase.entry(phase).or_insert((0, 0));
                    entry.0 += 1;
                    entry.1 += damage;
                }

                // Update boss HP in context
                if target.npc_id.is_some() && event.target_entity.health.1 > 0 {
                    let hp_pct = (event.target_entity.health.0 as f32 / event.target_entity.health.1 as f32) * 100.0;
                    challenge_ctx.hp_by_npc_id.insert(target.npc_id.unwrap(), hp_pct);
                }

                challenge_tracker.process_damage(
                    &challenge_ctx,
                    &source,
                    &target,
                    event.action.action_id as u64,
                    damage,
                );
            }
        }
    }

    // Finalize challenge tracker - must call finalize() to record final phase duration
    // Use kill target death time if available (fight end), otherwise last entity seen
    let end_time = kill_target_death_time
        .or_else(|| state.entities.values().filter_map(|e| e.last_seen).max())
        .or_else(|| state.phase_changes.last().map(|p| p.timestamp));

    let combat_duration = if let (Some(start), Some(end)) = (state.combat_start, end_time) {
        (end - start).num_milliseconds() as f32 / 1000.0
    } else {
        0.0
    };

    // finalize() ends the current phase and records its duration
    if let Some(end) = end_time {
        challenge_tracker.finalize(end, combat_duration);
    } else {
        challenge_tracker.set_duration(combat_duration);
    }

    // Debug: Print Machine Core damage breakdown by phase
    if machine_core_damage_total > 0 {
        eprintln!();
        eprintln!("=== DEBUG: Machine Core Damage by Phase ===");
        eprintln!("Total Machine Core damage events: {}", machine_core_damage_total);
        let mut phases: Vec<_> = damage_by_phase.iter().collect();
        phases.sort_by_key(|(phase, _)| phase.clone());
        for (phase, (count, total)) in phases {
            eprintln!("  {:20} {:6} events  {:>12} damage", phase, count, total);
        }
        eprintln!();
    }

    // Output report
    print_report(&args, &state, boss_def, event_count, &challenge_tracker, &player_names);

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn extract_session_date(first_line: &str) -> Result<NaiveDateTime, Box<dyn std::error::Error>> {
    // Combat log first line should have timestamp like [HH:MM:SS.mmm]
    // We'll use today's date as base (logs don't include date)
    let today = chrono::Local::now().naive_local().date();
    Ok(today.and_hms_opt(0, 0, 0).unwrap())
}

/// Convert a parsed Entity to EntityInfo for challenge matching
fn entity_to_info(entity: &baras_core::combat_log::Entity, local_player_id: i64) -> EntityInfo {
    match entity.entity_type {
        EntityType::Player => EntityInfo {
            entity_id: entity.log_id,
            name: resolve(entity.name).to_string(),
            is_player: true,
            is_local_player: entity.log_id == local_player_id,
            npc_id: None,
        },
        EntityType::Npc | EntityType::Companion => EntityInfo {
            entity_id: entity.log_id,
            name: resolve(entity.name).to_string(),
            is_player: false,
            is_local_player: false,
            npc_id: Some(entity.class_id),
        },
        _ => EntityInfo::default(),
    }
}

fn populate_tracked_ids(state: &mut ValidationState, boss: &BossEncounterDefinition) {
    // Collect all ability IDs from timers
    for timer in &boss.timers {
        match &timer.trigger {
            baras_core::timers::TimerTrigger::AbilityCast { ability_ids } => {
                state.tracked_ability_ids.extend(ability_ids.iter().copied());
            }
            baras_core::timers::TimerTrigger::EffectApplied { effect_ids } => {
                state.tracked_effect_ids.extend(effect_ids.iter().copied());
            }
            baras_core::timers::TimerTrigger::EffectRemoved { effect_ids } => {
                state.tracked_effect_ids.extend(effect_ids.iter().copied());
            }
            _ => {}
        }
    }

    // Collect boss entity NPC IDs
    for entity in &boss.entities {
        if entity.is_boss {
            state.boss_entity_ids.extend(entity.ids.iter().copied());
        }
    }
}

fn track_event(state: &mut ValidationState, event: &CombatEvent, boss: &BossEncounterDefinition) {
    let source_name = resolve(event.source_entity.name).to_string();
    let target_name = resolve(event.target_entity.name).to_string();

    // Track NPC entities (source and target)
    for entity in [&event.source_entity, &event.target_entity] {
        if entity.entity_type != EntityType::Npc || entity.class_id == 0 {
            continue;
        }
        let npc_id = entity.class_id;
        let name = resolve(entity.name).to_string();
        let (hp, max_hp) = (entity.health.0 as i64, entity.health.1 as i64);

        let entry = state.entities.entry(npc_id).or_insert_with(|| EntitySeen {
            npc_id,
            name: name.clone(),
            first_seen: Some(event.timestamp),
            last_seen: Some(event.timestamp),
            death_count: 0,
            last_hp: None,
            max_hp: None,
        });

        // Update last seen and HP
        entry.last_seen = Some(event.timestamp);
        if max_hp > 0 {
            entry.last_hp = Some(hp);
            entry.max_hp = Some(max_hp);
        }
        if entry.first_seen.is_none() {
            entry.first_seen = Some(event.timestamp);
        }
    }

    // Track abilities from boss entities (by name match since we may not have NPC IDs)
    let is_boss_source = boss.entities.iter().any(|e|
        e.is_boss && e.name.eq_ignore_ascii_case(&source_name)
    ) || state.boss_entity_ids.contains(&event.source_entity.class_id);

    if is_boss_source && event.action.action_id != 0 {
        let ability_id = event.action.action_id;
        let ability_name = resolve(event.action.name).to_string();

        let entry = state.abilities_from_bosses.entry(ability_id).or_insert_with(|| AbilitySeen {
            ability_id,
            name: ability_name,
            count: 0,
            sources: HashSet::new(),
        });
        entry.count += 1;
        entry.sources.insert(source_name.clone());
    }

    // Track effects on players
    if event.target_entity.entity_type == EntityType::Player && event.effect.effect_id != 0 {
        let effect_id = event.effect.effect_id;
        let effect_name = resolve(event.effect.effect_name).to_string();

        let entry = state.effects_on_players.entry(effect_id).or_insert_with(|| EffectSeen {
            effect_id,
            name: effect_name,
            apply_count: 0,
            remove_count: 0,
        });

        // Check if it's an apply or remove based on effect type
        let type_id = event.effect.type_id;
        if type_id == effect_type_id::APPLYEFFECT {
            entry.apply_count += 1;
        } else if type_id == effect_type_id::REMOVEEFFECT {
            entry.remove_count += 1;
        }
    }
}

fn track_signal(state: &mut ValidationState, signal: &GameSignal) {
    match signal {
        GameSignal::CombatStarted { timestamp, .. } => {
            state.combat_start = Some(*timestamp);
        }
        GameSignal::EntityDeath { npc_id, entity_name, timestamp, .. } => {
            if let Some(entity) = state.entities.get_mut(npc_id) {
                entity.death_count += 1;
                entity.last_seen = Some(*timestamp);
                entity.last_hp = Some(0);
            } else {
                state.entities.insert(*npc_id, EntitySeen {
                    npc_id: *npc_id,
                    name: entity_name.clone(),
                    first_seen: None,
                    last_seen: Some(*timestamp),
                    death_count: 1,
                    last_hp: Some(0),
                    max_hp: None,
                });
            }
        }
        GameSignal::PhaseChanged { old_phase, new_phase, timestamp, .. } => {
            state.phase_changes.push(PhaseChange {
                timestamp: *timestamp,
                old_phase: old_phase.clone(),
                new_phase: new_phase.clone(),
            });
        }
        GameSignal::CounterChanged { counter_id, old_value, new_value, timestamp, .. } => {
            state.counter_changes.push(CounterChange {
                timestamp: *timestamp,
                counter_id: counter_id.clone(),
                old_value: *old_value,
                new_value: *new_value,
                reason: String::new(), // Could extract from signal if available
            });
        }
        _ => {}
    }
}

fn format_combat_time(start: Option<NaiveDateTime>, ts: NaiveDateTime) -> String {
    if let Some(start) = start {
        let secs = (ts - start).num_milliseconds() as f32 / 1000.0;
        let mins = (secs / 60.0).floor() as u32;
        let secs = secs % 60.0;
        format!("{:02}:{:05.2}", mins, secs)
    } else {
        format!("{}", ts.format("%H:%M:%S"))
    }
}

fn print_report(args: &Args, state: &ValidationState, boss: &BossEncounterDefinition, event_count: usize, challenges: &ChallengeTracker, player_names: &HashMap<i64, String>) {
    let log_name = args.log.file_name().unwrap_or_default().to_string_lossy();

    println!();
    println!("══════════════════════════════════════════════════════════════════════");
    println!("  BOSS VALIDATION: {}", boss.name);
    println!("  Log: {} ({} events)", log_name, event_count);
    println!("══════════════════════════════════════════════════════════════════════");

    // Entities
    println!();
    println!("ENTITIES SEEN:");
    println!("  {:20} {:30} {:12} {:6}", "NPC ID", "Name", "First Seen", "Deaths");
    println!("  {}", "─".repeat(72));

    let mut entities: Vec<_> = state.entities.values().collect();
    entities.sort_by_key(|e| e.first_seen);

    for entity in &entities {
        let first_seen = entity.first_seen
            .map(|ts| format_combat_time(state.combat_start, ts))
            .unwrap_or_else(|| "?".to_string());
        println!("  {:20} {:30} {:12} {:6}",
            entity.npc_id,
            truncate(&entity.name, 30),
            first_seen,
            entity.death_count
        );
    }

    // Boss HP (only for boss entities with HP data)
    let boss_npc_ids: HashSet<i64> = boss.entities.iter()
        .filter(|e| e.is_boss)
        .flat_map(|e| e.ids.iter().copied())
        .collect();

    let boss_entities: Vec<_> = entities.iter()
        .filter(|e| boss_npc_ids.contains(&e.npc_id) && e.max_hp.is_some())
        .collect();

    if !boss_entities.is_empty() {
        println!();
        println!("BOSS HP (last known):");
        println!("  {:30} {:>15} {:>10} {:12}", "Name", "HP", "%", "Last Seen");
        println!("  {}", "─".repeat(70));

        for entity in boss_entities {
            let hp = entity.last_hp.unwrap_or(0);
            let max_hp = entity.max_hp.unwrap_or(1);
            let pct = if max_hp > 0 { (hp as f64 / max_hp as f64) * 100.0 } else { 0.0 };
            let last_seen = entity.last_seen
                .map(|ts| format_combat_time(state.combat_start, ts))
                .unwrap_or_else(|| "?".to_string());

            println!("  {:30} {:>10}/{:<10} {:>6.1}% {:12}",
                truncate(&entity.name, 30),
                hp,
                max_hp,
                pct,
                last_seen
            );
        }
    }

    // Alerts fired (ephemeral notifications)
    if !state.alerts_fired.is_empty() {
        println!();
        println!("ALERTS:");
        println!("  {:10} {:25} {:30}", "Time", "Name", "Text");
        println!("  {}", "─".repeat(68));
        for alert in &state.alerts_fired {
            let time = format_combat_time(state.combat_start, alert.timestamp);
            println!("  {:10} {:25} {:30}",
                time,
                truncate(&alert.name, 25),
                truncate(&alert.text, 30)
            );
        }
    } else {
        println!();
        println!("ALERTS: (none)");
    }

    // Timers fired (countdown timers)
    if !state.timers_fired.is_empty() {
        println!();
        println!("TIMERS:");
        println!("  {:10} {:30} {:10} {:20}", "Time", "Name", "Duration", "Timer ID");
        println!("  {}", "─".repeat(74));
        for timer in &state.timers_fired {
            let time = format_combat_time(state.combat_start, timer.timestamp);
            println!("  {:10} {:30} {:10.1}s {:20}",
                time,
                truncate(&timer.timer_name, 30),
                timer.duration_secs,
                truncate(&timer.timer_id, 20)
            );
        }
    } else {
        println!();
        println!("TIMERS: (none)");
    }

    // Phase changes
    println!();
    if !state.phase_changes.is_empty() {
        println!("PHASES:");
        for phase in &state.phase_changes {
            let time = format_combat_time(state.combat_start, phase.timestamp);
            if let Some(old) = &phase.old_phase {
                println!("  {}  {} → {}", time, old, phase.new_phase);
            } else {
                println!("  {}  → {} (initial)", time, phase.new_phase);
            }
        }
    } else {
        println!("PHASES: (none)");
    }

    // Counter changes
    println!();
    if !state.counter_changes.is_empty() {
        println!("COUNTERS:");
        for change in &state.counter_changes {
            let time = format_combat_time(state.combat_start, change.timestamp);
            println!("  {}  {}: {} → {}", time, change.counter_id, change.old_value, change.new_value);
        }
    } else {
        println!("COUNTERS: (none)");
    }

    // Challenges
    let challenge_values = challenges.snapshot();
    if !challenge_values.is_empty() {
        println!();
        println!("CHALLENGES:");
        println!("  {:25} {:>15} {:>10} {:>12}", "Name", "Value", "Events", "DPS");
        println!("  {}", "─".repeat(65));

        for cv in &challenge_values {
            // Try to find the phase this challenge is restricted to for DPS calculation
            let phase_ids: Option<Vec<String>> = boss.challenges.iter()
                .find(|c| c.id == cv.id)
                .and_then(|c| c.phase_ids().map(|ids| ids.to_vec()));

            let duration = if let Some(ref phases) = phase_ids {
                // Sum durations of all phases this challenge tracks
                phases.iter().map(|p| challenges.phase_duration(p)).sum::<f32>()
            } else {
                challenges.total_duration()
            };

            let dps = if duration > 0.0 { cv.value as f32 / duration } else { 0.0 };
            let dps_str = if dps > 0.0 { format!("{:.1}/s", dps) } else { "-".to_string() };

            println!("  {:25} {:>15} {:>10} {:>12}",
                truncate(&cv.name, 25),
                format_number(cv.value),
                cv.event_count,
                dps_str
            );

            // Show phase duration if phase-restricted
            if let Some(phases) = phase_ids {
                let phase_str = phases.join(", ");
                println!("    └─ phases: {} ({:.1}s)", phase_str, duration);
            }

            // Show per-player breakdown
            if !cv.by_player.is_empty() {
                let mut players: Vec<_> = cv.by_player.iter().collect();
                players.sort_by(|a, b| b.1.cmp(a.1)); // Sort by damage descending

                for (entity_id, value) in players {
                    let name = player_names.get(entity_id)
                        .map(|s| s.as_str())
                        .unwrap_or("Unknown");
                    let player_dps = if duration > 0.0 { *value as f32 / duration } else { 0.0 };
                    println!("      {:20} {:>12} {:>12.1}/s",
                        truncate(name, 20),
                        format_number(*value),
                        player_dps
                    );
                }
            }
        }
    }

    // Untracked abilities from bosses
    let untracked_abilities: Vec<_> = state.abilities_from_bosses.values()
        .filter(|a| !state.tracked_ability_ids.contains(&(a.ability_id as u64)) || args.all_abilities)
        .collect();

    if !untracked_abilities.is_empty() {
        println!();
        if args.all_abilities {
            println!("ALL ABILITIES FROM BOSS ENTITIES:");
        } else {
            println!("UNTRACKED ABILITIES FROM BOSS ENTITIES:");
        }
        println!("  {:20} {:30} {:6} {:20}", "ID", "Name", "Count", "Source");
        println!("  {}", "─".repeat(80));

        let mut abilities: Vec<_> = untracked_abilities;
        abilities.sort_by(|a, b| b.count.cmp(&a.count));

        for ability in abilities {
            let sources: Vec<_> = ability.sources.iter().take(2).cloned().collect();
            let sources_str = sources.join(", ");
            let tracked = if state.tracked_ability_ids.contains(&(ability.ability_id as u64)) { " ✓" } else { "" };
            println!("  {:20} {:30} {:6} {:20}{}",
                ability.ability_id,
                truncate(&ability.name, 30),
                ability.count,
                truncate(&sources_str, 20),
                tracked
            );
        }
    }


    println!();
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max-1])
    }
}

fn format_number(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
