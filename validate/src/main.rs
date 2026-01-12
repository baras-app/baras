//! Timer validation CLI for boss encounters
//!
//! Replays combat logs through boss definitions and validates timer behavior:
//! - Realtime mode (1x): Debug timer display issues
//! - Accelerated mode: Fast CI validation with checkpoints
//! - Visual mode: Display actual overlay window (requires --features visual)

mod output;
mod replay;
mod verification;

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use encoding_rs::WINDOWS_1252;

use chrono::NaiveDateTime;
use clap::{Parser, ValueEnum};

use baras_core::boss::{
    BossEncounterDefinition, ChallengeContext, EntityInfo, load_bosses_with_paths,
};
use baras_core::combat_log::{CombatEvent, EntityType, LogParser};
use baras_core::context::resolve;
use baras_core::encounter::ChallengeTracker;
use baras_core::encounter::combat::ActiveBoss;
use baras_core::game_data::{effect_id, effect_type_id};
use baras_core::signal_processor::{
    EventProcessor, GameSignal, SignalHandler, check_counter_timer_triggers,
};
use baras_core::state::SessionCache;
use baras_core::timers::TimerManager;

use crate::output::{CliOutput, OutputLevel};
use crate::replay::{LagSimulator, VirtualClock};
use crate::verification::{CheckpointVerifier, Expectations};

// ═══════════════════════════════════════════════════════════════════════════════
// CLI Arguments
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ReplayMode {
    /// 1x speed with actual timing delays
    Realtime,
    /// Fast replay with virtual time (default)
    Accelerated,
}

impl Default for ReplayMode {
    fn default() -> Self {
        Self::Accelerated
    }
}

#[derive(Parser, Debug)]
#[command(name = "baras-validate")]
#[command(about = "Validate timer definitions against combat logs")]
#[command(version)]
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

    // ─────────────────────────────────────────────────────────────────────────
    // Replay Mode
    // ─────────────────────────────────────────────────────────────────────────
    /// Replay mode
    #[arg(long, value_enum, default_value_t = ReplayMode::Accelerated)]
    mode: ReplayMode,

    /// Custom speed multiplier (overrides --mode)
    #[arg(long)]
    speed: Option<f32>,

    /// Simulate file I/O lag for realistic timing
    #[arg(long)]
    simulate_lag: bool,

    // ─────────────────────────────────────────────────────────────────────────
    // Output Mode
    // ─────────────────────────────────────────────────────────────────────────
    /// Quiet mode: summary only
    #[arg(short, long)]
    quiet: bool,

    /// Verbose mode: show all signals including counters
    #[arg(short, long)]
    verbose: bool,

    /// Show all abilities from boss entities (not just untracked)
    #[arg(long)]
    all_abilities: bool,

    /// Show all entities seen in the log
    #[arg(long)]
    all_entities: bool,

    // ─────────────────────────────────────────────────────────────────────────
    // Verification
    // ─────────────────────────────────────────────────────────────────────────
    /// Path to expectations TOML file for checkpoint verification
    #[arg(long)]
    expect: Option<PathBuf>,

    // ─────────────────────────────────────────────────────────────────────────
    // Debug
    // ─────────────────────────────────────────────────────────────────────────
    /// Start at specific combat time (MM:SS or seconds)
    #[arg(long)]
    start_at: Option<String>,

    /// Stop at specific combat time (MM:SS or seconds)
    #[arg(long)]
    stop_at: Option<String>,
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
    last_death: Option<NaiveDateTime>,
    death_count: u32,
    last_hp: Option<i64>,
    max_hp: Option<i64>,
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

    // Determine output level
    let output_level = if args.quiet {
        OutputLevel::Quiet
    } else if args.verbose {
        OutputLevel::Verbose
    } else {
        OutputLevel::Normal
    };
    let mut cli = CliOutput::new(output_level);

    // Determine speed multiplier
    let speed = args.speed.unwrap_or(match args.mode {
        ReplayMode::Realtime => 1.0,
        ReplayMode::Accelerated => 0.0, // Instant
    });

    // Parse time bounds
    let start_at_secs = args
        .start_at
        .as_ref()
        .map(|s| parse_time_arg(s))
        .transpose()?;
    let stop_at_secs = args
        .stop_at
        .as_ref()
        .map(|s| parse_time_arg(s))
        .transpose()?;

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
        .ok_or_else(|| {
            format!(
                "Boss '{}' not found. Available: {}",
                args.boss,
                bosses
                    .iter()
                    .map(|b| b.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;

    eprintln!("Validating: {} ({})", boss_def.name, boss_def.area_name);
    eprintln!(
        "Mode: {} (speed: {}x)",
        if speed == 0.0 {
            "accelerated"
        } else {
            "realtime"
        },
        if speed == 0.0 {
            "instant".to_string()
        } else {
            format!("{:.1}", speed)
        }
    );

    // Load expectations for verification (if provided)
    let mut verifier = if let Some(expect_path) = &args.expect {
        let expectations = Expectations::load(expect_path)?;
        if expectations.meta.boss_id != boss_def.id {
            eprintln!(
                "Warning: expectations file is for '{}' but validating '{}'",
                expectations.meta.boss_id, boss_def.id
            );
        }
        Some(CheckpointVerifier::new(expectations))
    } else {
        None
    };

    // Build tracking state
    let mut state = ValidationState::default();
    populate_tracked_ids(&mut state, boss_def);

    // Parse log file with Windows-1252 encoding (SWTOR uses this for non-ASCII characters)
    let mut file = File::open(&args.log)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let (content, _, _) = WINDOWS_1252.decode(&bytes);
    let lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() {
        return Err("Log file is empty or unreadable".into());
    }

    let session_date = extract_session_date(&lines[0])?;
    let parser = LogParser::new(session_date);

    // Initialize processing components
    let mut processor = EventProcessor::new();
    let mut cache = SessionCache::default();
    let mut timer_manager = TimerManager::new();

    let boss_defs = vec![(*boss_def).clone()];
    cache.load_boss_definitions(boss_defs.clone());
    timer_manager.load_boss_definitions(boss_defs);

    // IMPORTANT: Disable live mode to process historical events
    timer_manager.set_live_mode(false);

    // Initialize challenge tracker
    let mut challenge_tracker = ChallengeTracker::new();
    let boss_npc_ids: Vec<i64> = boss_def
        .entities
        .iter()
        .filter(|e| e.is_boss)
        .flat_map(|e| e.ids.iter().copied())
        .collect();
    challenge_tracker.start(
        boss_def.challenges.clone(),
        boss_def.entities.clone(),
        boss_npc_ids.clone(),
        session_date,
    );

    let mut challenge_ctx = ChallengeContext::default();
    challenge_ctx.boss_npc_ids = boss_npc_ids.clone();

    // Track player names for challenge breakdown
    let mut player_names: HashMap<i64, String> = HashMap::new();

    // Timing components
    let mut clock: Option<VirtualClock> = None;
    let mut lag_sim = if args.simulate_lag {
        LagSimulator::new()
    } else {
        LagSimulator::disabled()
    };

    // Track active timers for expiration detection
    let mut prev_timer_ids: HashSet<String> = HashSet::new();

    let mut event_count = 0;
    let mut local_player_id: i64 = 0;
    let mut kill_target_death_time: Option<NaiveDateTime> = None;

    for (line_num, line) in lines.iter().enumerate() {
        let Some(event) = parser.parse_line(line_num as u64, line) else {
            continue;
        };

        event_count += 1;

        // Initialize clock on first event (or first combat start)
        if clock.is_none() {
            clock = Some(VirtualClock::new(event.timestamp, speed));
        }
        let clock = clock.as_mut().unwrap();

        // Calculate combat time for filtering
        let combat_time_secs = if let Some(start) = state.combat_start {
            (event.timestamp - start).num_milliseconds() as f32 / 1000.0
        } else {
            0.0
        };

        // Apply time bounds
        if let Some(start) = start_at_secs {
            if combat_time_secs < start {
                continue;
            }
        }
        if let Some(stop) = stop_at_secs {
            if combat_time_secs > stop {
                break;
            }
        }

        // Advance virtual clock (sleeps in realtime mode)
        clock.advance_to(event.timestamp);

        // Apply simulated lag
        let _lag = lag_sim.next_lag();

        // Process event
        let (signals, _processed_event) = processor.process_event(event.clone(), &mut cache);

        // Detect local player
        if local_player_id == 0 {
            if event.source_entity.entity_type == EntityType::Player {
                local_player_id = event.source_entity.log_id;
                timer_manager.set_local_player_id(local_player_id);
            } else if event.target_entity.entity_type == EntityType::Player {
                local_player_id = event.target_entity.log_id;
                timer_manager.set_local_player_id(local_player_id);
            }
        }

        // Set active_boss on encounter before timer processing
        // (AreaEntered is now handled by the processor automatically)
        for signal in &signals {
            if let GameSignal::BossEncounterDetected {
                definition_id,
                boss_name,
                entity_id,
                ..
            } = signal
            {
                if let Some(enc) = cache.current_encounter_mut() {
                    enc.set_boss(ActiveBoss {
                        definition_id: definition_id.clone(),
                        name: boss_name.clone(),
                        entity_id: *entity_id,
                        max_hp: 0,
                        current_hp: 0,
                    });
                }
            }
        }

        // Process signals through timer manager, accumulating IDs across all signals
        let encounter = cache.current_encounter();
        let mut expired_timer_ids: Vec<String> = Vec::new();
        let mut cancelled_timer_ids: Vec<String> = Vec::new();
        let mut started_timer_ids: Vec<String> = Vec::new();

        for signal in &signals {
            timer_manager.handle_signal(signal, encounter);
            // Accumulate IDs after each signal (vectors are cleared per-signal)
            expired_timer_ids.extend(timer_manager.expired_timer_ids());
            cancelled_timer_ids.extend(timer_manager.cancelled_timer_ids());
            started_timer_ids.extend(timer_manager.started_timer_ids());
        }

        // Track active timer IDs for prev comparison
        let current_timer_ids: HashSet<String> = timer_manager
            .active_timers()
            .iter()
            .map(|t| t.definition_id.clone())
            .collect();

        // Log new/restarted timers
        for timer in timer_manager.active_timers() {
            if started_timer_ids.contains(&timer.definition_id) {
                cli.timer_start(
                    event.timestamp,
                    &timer.name,
                    timer.duration.as_secs_f32(),
                    &timer.definition_id,
                );

                if let Some(ref mut v) = verifier {
                    v.record_timer_start(&timer.definition_id, combat_time_secs);
                }
            }
        }

        // Log expired timers (may include timers that immediately restarted)
        for expired_id in &expired_timer_ids {
            cli.timer_expire(event.timestamp, expired_id, expired_id);
        }

        // DEBUG: Check fingers timer state around expected 3rd expiration (05:28 = 328s)
        if combat_time_secs > 325.0 && combat_time_secs < 350.0 {
            // 325s = 05:25, 350s = 05:50
            for timer in timer_manager.active_timers() {
                if timer.definition_id.contains("fingers_knock") {
                    let remaining = timer.remaining_secs(event.timestamp);
                    eprintln!(
                        "[DEBUG {:.2}s] Timer '{}' active, remaining: {:.2}s",
                        combat_time_secs, timer.definition_id, remaining
                    );
                }
            }
            if expired_timer_ids.iter().any(|id| id.contains("fingers")) {
                eprintln!("[DEBUG {:.2}s] Fingers timer expired!", combat_time_secs);
            }
            if started_timer_ids.iter().any(|id| id.contains("fingers")) {
                eprintln!("[DEBUG {:.2}s] Fingers timer started!", combat_time_secs);
            }
        }

        // Log cancelled timers
        for cancelled_id in &cancelled_timer_ids {
            cli.timer_cancel(event.timestamp, cancelled_id, cancelled_id);
        }

        prev_timer_ids = current_timer_ids;

        // Process alerts
        for alert in timer_manager.take_fired_alerts() {
            cli.alert(event.timestamp, &alert.name, &alert.text);

            if let Some(ref mut v) = verifier {
                v.record_alert(&alert.id);
            }
        }

        // Process counter triggers from timer events (expires and starts)
        let timer_counter_signals = check_counter_timer_triggers(
            &expired_timer_ids,
            &started_timer_ids,
            &mut cache,
            event.timestamp,
        );
        for signal in &timer_counter_signals {
            if let GameSignal::CounterChanged {
                counter_id,
                old_value,
                new_value,
                timestamp,
            } = signal
            {
                cli.counter_change(*timestamp, counter_id, *old_value, *new_value);
            }
        }

        // Track entities, abilities, effects
        track_event(&mut state, &event, boss_def);

        // Update boss HP for CLI display (per-encounter)
        for entity in [&event.source_entity, &event.target_entity] {
            if entity.entity_type == EntityType::Npc
                && state.boss_entity_ids.contains(&entity.class_id)
                && entity.health.1 > 0
            {
                let name = resolve(entity.name).to_string();
                cli.update_boss_hp(
                    &name,
                    entity.class_id,
                    entity.health.0 as i64,
                    entity.health.1 as i64,
                );
            }
        }

        // Process signals for output and state updates
        for signal in &signals {
            match signal {
                GameSignal::AreaEntered { difficulty_id, .. } => {
                    // Set difficulty from the actual log file's AreaEntered event
                    if let Some(enc) = cache.current_encounter_mut() {
                        enc.difficulty = baras_core::Difficulty::from_difficulty_id(*difficulty_id);
                    }
                }
                GameSignal::CombatStarted { timestamp, .. } => {
                    state.combat_start = Some(*timestamp);
                    cli.combat_start(*timestamp);
                    // Reset challenge tracker for new encounter
                    challenge_tracker.start(
                        boss_def.challenges.clone(),
                        boss_def.entities.clone(),
                        boss_npc_ids.clone(),
                        *timestamp,
                    );
                    // Set encounter context for timer matching
                    if let Some(enc) = cache.current_encounter_mut() {
                        enc.area_id = Some(boss_def.area_id);
                        enc.area_name = Some(boss_def.area_name.clone());
                        // Difficulty is now set from AreaEntered signal above
                    }
                }
                GameSignal::CombatEnded { timestamp, .. } => {
                    // Calculate duration from combat start
                    let duration = if let Some(start) = state.combat_start {
                        (*timestamp - start).num_milliseconds() as f32 / 1000.0
                    } else {
                        0.0
                    };
                    // Finalize and get challenge snapshot for this encounter
                    challenge_tracker.set_duration(duration);
                    let challenge_snapshot = challenge_tracker.snapshot();
                    cli.combat_end(*timestamp, duration, &challenge_snapshot);
                }
                GameSignal::BossEncounterDetected {
                    definition_id,
                    boss_name,
                    entity_id,
                    timestamp,
                    ..
                } => {
                    cli.boss_detected(*timestamp, boss_name);

                    // CRITICAL: Set active boss for timer context
                    if let Some(enc) = cache.current_encounter_mut() {
                        enc.set_boss(ActiveBoss {
                            definition_id: definition_id.clone(),
                            name: boss_name.clone(),
                            entity_id: *entity_id,
                            max_hp: 0,
                            current_hp: 0,
                        });
                    }
                }
                GameSignal::PhaseChanged {
                    old_phase,
                    new_phase,
                    timestamp,
                    ..
                } => {
                    cli.phase_change(*timestamp, old_phase.as_deref(), new_phase);
                    challenge_ctx.current_phase = Some(new_phase.clone());
                    challenge_tracker.set_phase(new_phase, *timestamp);
                }
                GameSignal::PhaseEndTriggered {
                    phase_id,
                    timestamp,
                } => {
                    cli.phase_end_triggered(*timestamp, phase_id);
                }
                GameSignal::CounterChanged {
                    counter_id,
                    old_value,
                    new_value,
                    timestamp,
                    ..
                } => {
                    cli.counter_change(*timestamp, counter_id, *old_value, *new_value);
                    challenge_ctx
                        .counters
                        .insert(counter_id.clone(), *new_value);
                }
                GameSignal::EntityDeath {
                    npc_id,
                    entity_name,
                    timestamp,
                    ..
                } => {
                    // Check if this is a kill target
                    let is_kill_target = boss_def
                        .entities
                        .iter()
                        .any(|e| e.is_kill_target && e.ids.contains(npc_id));

                    if is_kill_target {
                        kill_target_death_time = Some(*timestamp);
                    }

                    // Output death event
                    cli.entity_death(*timestamp, entity_name, *npc_id, is_kill_target);

                    // Update entity tracking
                    if let Some(entity) = state.entities.get_mut(npc_id) {
                        entity.death_count += 1;
                        entity.last_seen = Some(*timestamp);
                        entity.last_death = Some(*timestamp);
                        entity.last_hp = Some(0);
                    } else {
                        state.entities.insert(
                            *npc_id,
                            EntitySeen {
                                npc_id: *npc_id,
                                name: entity_name.clone(),
                                first_seen: None,
                                last_seen: Some(*timestamp),
                                last_death: Some(*timestamp),
                                death_count: 1,
                                last_hp: Some(0),
                                max_hp: None,
                            },
                        );
                    }
                }
                _ => {}
            }
        }

        // Track player names
        if event.source_entity.entity_type == EntityType::Player {
            player_names
                .entry(event.source_entity.log_id)
                .or_insert_with(|| resolve(event.source_entity.name).to_string());
        }

        // Process damage events through challenge tracker
        if event.effect.effect_id == effect_id::DAMAGE {
            let source = entity_to_info(&event.source_entity, local_player_id);
            let target = entity_to_info(&event.target_entity, local_player_id);
            let damage = event.details.dmg_effective as i64;

            if target.npc_id.is_some() && event.target_entity.health.1 > 0 {
                let hp_pct = (event.target_entity.health.0 as f32
                    / event.target_entity.health.1 as f32)
                    * 100.0;
                challenge_ctx
                    .hp_by_npc_id
                    .insert(target.npc_id.unwrap(), hp_pct);
            }

            challenge_tracker.process_damage(
                &challenge_ctx,
                &source,
                &target,
                event.action.action_id as u64,
                damage,
                event.timestamp,
            );
        }

        // Check verification checkpoints
        if let Some(ref mut v) = verifier {
            let active_timers: Vec<(String, f32)> = timer_manager
                .active_timers()
                .iter()
                .map(|t| (t.definition_id.clone(), t.remaining_secs(event.timestamp)))
                .collect();

            if let Some(result) = v.check_time(combat_time_secs, &active_timers) {
                if result.passed {
                    eprintln!("  [PASS] Checkpoint at {:.1}s", result.at_secs);
                } else {
                    eprintln!("  [FAIL] Checkpoint at {:.1}s:", result.at_secs);
                    for failure in &result.failures {
                        eprintln!("         - {}", failure);
                    }
                }
            }
        }
    }

    // Finalize challenge tracker
    let end_time = kill_target_death_time
        .or_else(|| state.entities.values().filter_map(|e| e.last_seen).max());

    let combat_duration = if let (Some(start), Some(end)) = (state.combat_start, end_time) {
        (end - start).num_milliseconds() as f32 / 1000.0
    } else {
        0.0
    };

    if let Some(end) = end_time {
        challenge_tracker.finalize(end, combat_duration);
    } else {
        challenge_tracker.set_duration(combat_duration);
    }

    // Print verification summary
    let checkpoint_result = verifier.map(|v| {
        let result = v.finalize();
        (result.checkpoints_passed, result.checkpoints_total)
    });

    cli.print_summary(checkpoint_result);

    // Print detailed report (unless quiet)
    if !args.quiet {
        print_detailed_report(
            &args,
            &state,
            boss_def,
            event_count,
            &challenge_tracker,
            &player_names,
        );
    }

    // Exit with error code if verification failed
    if let Some((passed, total)) = checkpoint_result {
        if passed != total {
            std::process::exit(1);
        }
    }

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════════

fn parse_time_arg(s: &str) -> Result<f32, Box<dyn std::error::Error>> {
    if s.contains(':') {
        // Parse MM:SS format
        let parts: Vec<&str> = s.split(':').collect();
        if parts.len() != 2 {
            return Err(format!("Invalid time format '{}', expected MM:SS or seconds", s).into());
        }
        let mins: f32 = parts[0].parse()?;
        let secs: f32 = parts[1].parse()?;
        Ok(mins * 60.0 + secs)
    } else {
        // Parse as seconds
        Ok(s.parse()?)
    }
}

fn extract_session_date(_first_line: &str) -> Result<NaiveDateTime, Box<dyn std::error::Error>> {
    let today = chrono::Local::now().naive_local().date();
    Ok(today.and_hms_opt(0, 0, 0).unwrap())
}

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
    for timer in &boss.timers {
        match &timer.trigger {
            baras_core::timers::TimerTrigger::AbilityCast { abilities, .. } => {
                for selector in abilities {
                    if let baras_core::AbilitySelector::Id(id) = selector {
                        state.tracked_ability_ids.insert(*id);
                    }
                }
            }
            baras_core::timers::TimerTrigger::EffectApplied { effects, .. } => {
                for selector in effects {
                    if let baras_core::EffectSelector::Id(id) = selector {
                        state.tracked_effect_ids.insert(*id);
                    }
                }
            }
            baras_core::timers::TimerTrigger::EffectRemoved { effects, .. } => {
                for selector in effects {
                    if let baras_core::EffectSelector::Id(id) = selector {
                        state.tracked_effect_ids.insert(*id);
                    }
                }
            }
            _ => {}
        }
    }

    for entity in &boss.entities {
        if entity.is_boss {
            state.boss_entity_ids.extend(entity.ids.iter().copied());
        }
    }
}

fn track_event(state: &mut ValidationState, event: &CombatEvent, boss: &BossEncounterDefinition) {
    let source_name = resolve(event.source_entity.name).to_string();

    // Track NPC entities
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
            last_death: None,
            death_count: 0,
            last_hp: None,
            max_hp: None,
        });

        entry.last_seen = Some(event.timestamp);
        if max_hp > 0 {
            entry.last_hp = Some(hp);
            entry.max_hp = Some(max_hp);
        }
        if entry.first_seen.is_none() {
            entry.first_seen = Some(event.timestamp);
        }
    }

    // Track abilities from boss entities
    let is_boss_source = boss
        .entities
        .iter()
        .any(|e| e.is_boss && e.name.eq_ignore_ascii_case(&source_name))
        || state
            .boss_entity_ids
            .contains(&event.source_entity.class_id);

    if is_boss_source && event.action.action_id != 0 {
        let ability_id = event.action.action_id;
        let ability_name = resolve(event.action.name).to_string();

        let entry = state
            .abilities_from_bosses
            .entry(ability_id)
            .or_insert_with(|| AbilitySeen {
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

        let entry = state
            .effects_on_players
            .entry(effect_id)
            .or_insert_with(|| EffectSeen {
                effect_id,
                name: effect_name,
                apply_count: 0,
                remove_count: 0,
            });

        let type_id = event.effect.type_id;
        if type_id == effect_type_id::APPLYEFFECT {
            entry.apply_count += 1;
        } else if type_id == effect_type_id::REMOVEEFFECT {
            entry.remove_count += 1;
        }
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

fn print_detailed_report(
    args: &Args,
    state: &ValidationState,
    boss: &BossEncounterDefinition,
    event_count: usize,
    challenges: &ChallengeTracker,
    player_names: &HashMap<i64, String>,
) {
    let log_name = args.log.file_name().unwrap_or_default().to_string_lossy();

    println!();
    println!("══════════════════════════════════════════════════════════════════════");
    println!("  VALIDATION DETAILS: {}", boss.name);
    println!("  Log: {} ({} events)", log_name, event_count);
    println!("══════════════════════════════════════════════════════════════════════");

    // Entities (only show with --all-entities flag)
    if args.all_entities {
        println!();
        println!("ENTITIES SEEN:");
        println!(
            "  {:20} {:30} {:12} {:6} {:12}",
            "NPC ID", "Name", "First Seen", "Deaths", "Last Death"
        );
        println!("  {}", "─".repeat(86));

        let mut entities: Vec<_> = state.entities.values().collect();
        entities.sort_by_key(|e| e.first_seen);

        for entity in &entities {
            let first_seen = entity
                .first_seen
                .map(|ts| format_combat_time(state.combat_start, ts))
                .unwrap_or_else(|| "?".to_string());
            let last_death = entity
                .last_death
                .map(|ts| format_combat_time(state.combat_start, ts))
                .unwrap_or_else(|| "-".to_string());
            println!(
                "  {:20} {:30} {:12} {:6} {:12}",
                entity.npc_id,
                truncate(&entity.name, 30),
                first_seen,
                entity.death_count,
                last_death
            );
        }
    }

    // Challenges
    let challenge_values = challenges.snapshot();
    if !challenge_values.is_empty() {
        println!();
        println!("CHALLENGES:");
        println!(
            "  {:25} {:>15} {:>10} {:>12}",
            "Name", "Value", "Events", "DPS"
        );
        println!("  {}", "─".repeat(65));

        for cv in &challenge_values {
            let phase_ids: Option<Vec<String>> = boss
                .challenges
                .iter()
                .find(|c| c.id == cv.id)
                .and_then(|c| c.phase_ids().map(|ids| ids.to_vec()));

            let duration = if let Some(ref phases) = phase_ids {
                phases
                    .iter()
                    .map(|p| challenges.phase_duration(p))
                    .sum::<f32>()
            } else {
                challenges.total_duration()
            };

            let dps = if duration > 0.0 {
                cv.value as f32 / duration
            } else {
                0.0
            };
            let dps_str = if dps > 0.0 {
                format!("{:.1}/s", dps)
            } else {
                "-".to_string()
            };

            println!(
                "  {:25} {:>15} {:>10} {:>12}",
                truncate(&cv.name, 25),
                format_number(cv.value),
                cv.event_count,
                dps_str
            );

            if let Some(phases) = phase_ids {
                let phase_str = phases.join(", ");
                println!("    └─ phases: {} ({:.1}s)", phase_str, duration);
            }

            if !cv.by_player.is_empty() {
                let mut players: Vec<_> = cv.by_player.iter().collect();
                players.sort_by(|a, b| b.1.cmp(a.1));

                for (entity_id, value) in players {
                    let name = player_names
                        .get(entity_id)
                        .map(|s| s.as_str())
                        .unwrap_or("Unknown");
                    let player_dps = if duration > 0.0 {
                        *value as f32 / duration
                    } else {
                        0.0
                    };
                    println!(
                        "      {:20} {:>12} {:>12.1}/s",
                        truncate(name, 20),
                        format_number(*value),
                        player_dps
                    );
                }
            }
        }
    }

    // Untracked abilities (only show with --all-abilities flag)
    if args.all_abilities {
        let untracked_abilities: Vec<_> = state.abilities_from_bosses.values().collect();

        if !untracked_abilities.is_empty() {
            println!();
            println!("ALL ABILITIES FROM BOSS ENTITIES:");
            println!("  {:20} {:30} {:6} {:20}", "ID", "Name", "Count", "Source");
            println!("  {}", "─".repeat(80));

            let mut abilities: Vec<_> = untracked_abilities;
            abilities.sort_by(|a, b| b.count.cmp(&a.count));

            for ability in abilities {
                let sources: Vec<_> = ability.sources.iter().take(2).cloned().collect();
                let sources_str = sources.join(", ");
                let tracked = if state
                    .tracked_ability_ids
                    .contains(&(ability.ability_id as u64))
                {
                    " ✓"
                } else {
                    ""
                };
                println!(
                    "  {:20} {:30} {:6} {:20}{}",
                    ability.ability_id,
                    truncate(&ability.name, 30),
                    ability.count,
                    truncate(&sources_str, 20),
                    tracked
                );
            }
        }
    }

    println!();
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
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
