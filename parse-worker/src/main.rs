//! baras-parse-worker - Subprocess for parsing combat log files.
//!
//! This binary is spawned by the main BARAS app to parse historical files.
//! It runs in a separate process so memory fragmentation doesn't affect the main app.
//!
//! Usage: baras-parse-worker <file_path> <session_id> <output_dir>
//!
//! Output: JSON to stdout with encounter summaries and final byte position.

use baras_core::combat_log::{CombatEvent, LogParser};
use baras_core::context::{parse_log_filename, resolve};
use baras_core::encounter::summary::EncounterSummary;
use baras_core::signal_processor::{EventProcessor, GameSignal};
use baras_core::state::SessionCache;
use baras_core::storage::{encounter_filename, EncounterWriter, EventMetadata};
use encoding_rs::WINDOWS_1252;
use memchr::memchr_iter;
use memmap2::Mmap;
use rayon::prelude::*;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// Player session info for main process.
#[derive(Debug, Serialize)]
struct PlayerInfo {
    name: String,
    class_name: String,
    discipline_name: String,
    entity_id: i64,
}

/// Area info for main process.
#[derive(Debug, Serialize)]
struct AreaInfoOutput {
    area_name: String,
    area_id: i64,
    difficulty_name: String,
}

/// Output sent to main process via stdout.
#[derive(Debug, Serialize)]
struct ParseOutput {
    /// Final byte position in the file (for tailing).
    end_pos: u64,
    /// Number of events parsed.
    event_count: usize,
    /// Number of encounters written.
    encounter_count: usize,
    /// Encounter summaries for the main process.
    encounters: Vec<EncounterSummary>,
    /// Player info at end of file.
    player: PlayerInfo,
    /// Area info at end of file.
    area: AreaInfoOutput,
    /// Elapsed time in milliseconds.
    elapsed_ms: u128,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        eprintln!("Usage: baras-parse-worker <file_path> <session_id> <output_dir>");
        std::process::exit(1);
    }

    let file_path = PathBuf::from(&args[1]);
    let session_id = &args[2];
    let output_dir = PathBuf::from(&args[3]);

    // Ensure output directory exists
    if let Err(e) = fs::create_dir_all(&output_dir) {
        eprintln!("Failed to create output dir: {}", e);
        std::process::exit(1);
    }

    let timer = std::time::Instant::now();

    match parse_file(&file_path, session_id, &output_dir) {
        Ok(output) => {
            let mut output = output;
            output.elapsed_ms = timer.elapsed().as_millis();

            // Output JSON to stdout for main process
            if let Ok(json) = serde_json::to_string(&output) {
                println!("{}", json);
            }
        }
        Err(e) => {
            eprintln!("Parse error: {}", e);
            std::process::exit(1);
        }
    }
}

fn parse_file(
    file_path: &Path,
    _session_id: &str,
    output_dir: &Path,
) -> Result<ParseOutput, String> {
    // Extract session date from filename
    let date_stamp = file_path
        .file_name()
        .and_then(|f| f.to_str())
        .and_then(parse_log_filename)
        .map(|(_, dt)| dt)
        .ok_or("Invalid log filename")?;

    // Memory-map the file
    let file = fs::File::open(file_path).map_err(|e| format!("Failed to open file: {}", e))?;
    let mmap = unsafe { Mmap::map(&file).map_err(|e| format!("Failed to mmap: {}", e))? };
    let bytes = mmap.as_ref();
    let end_pos = bytes.len() as u64;

    // Find line boundaries
    let mut line_ranges: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    for end in memchr_iter(b'\n', bytes) {
        if end > start {
            line_ranges.push((start, end));
        }
        start = end + 1;
    }
    if start < bytes.len() {
        line_ranges.push((start, bytes.len()));
    }

    // Parallel parse
    let parser = LogParser::new(date_stamp);
    let events: Vec<CombatEvent> = line_ranges
        .par_iter()
        .enumerate()
        .filter_map(|(idx, &(start, end))| {
            let (line, _, _) = WINDOWS_1252.decode(&bytes[start..end]);
            parser.parse_line(idx as u64 + 1, &line)
        })
        .collect();

    let event_count = events.len();

    // Process events and write encounters
    let (encounters, player, area) = process_and_write_encounters(events, output_dir)?;

    Ok(ParseOutput {
        end_pos,
        event_count,
        encounter_count: encounters.len(),
        encounters,
        player,
        area,
        elapsed_ms: 0, // Filled in by caller
    })
}

fn process_and_write_encounters(
    events: Vec<CombatEvent>,
    output_dir: &Path,
) -> Result<(Vec<EncounterSummary>, PlayerInfo, AreaInfoOutput), String> {
    let mut cache = SessionCache::new();
    let mut processor = EventProcessor::new();
    let mut writer = EncounterWriter::with_capacity(50_000);
    let mut current_encounter_idx: u32 = 0;
    let mut pending_write = false;

    for event in events {
        // Build metadata for this event
        let metadata = EventMetadata::from_cache(&cache, current_encounter_idx, event.timestamp);
        writer.push_event(&event, &metadata);

        // Process through state machine
        // Note: CombatEnded triggers push_new_encounter which calls finalize_current_encounter
        // which creates the summary and adds it to encounter_history
        let signals = processor.process_event(event, &mut cache);

        // Check for combat end signal
        for signal in &signals {
            if let GameSignal::CombatEnded { .. } = signal {
                pending_write = true;
            }
        }

        // Write encounter when combat ends
        if pending_write {
            if !writer.is_empty() {
                let filename = encounter_filename(current_encounter_idx);
                let path = output_dir.join(&filename);
                writer
                    .write_to_file(&path)
                    .map_err(|e| format!("Failed to write parquet: {}", e))?;

                writer.clear();
                current_encounter_idx += 1;
            }
            pending_write = false;
        }
    }

    // Write any remaining events (final incomplete encounter)
    if !writer.is_empty() {
        let filename = encounter_filename(current_encounter_idx);
        let path = output_dir.join(&filename);
        let _ = writer.write_to_file(&path);
    }

    // Get all summaries that were created during processing
    // (push_new_encounter -> finalize_current_encounter -> create_summary)
    let encounter_summaries: Vec<EncounterSummary> = cache.encounter_history
        .summaries()
        .to_vec();

    // Extract player info (name is IStr, needs resolve)
    let player = PlayerInfo {
        name: resolve(cache.player.name).to_string(),
        class_name: cache.player.class_name.clone(),
        discipline_name: cache.player.discipline_name.clone(),
        entity_id: cache.player.id,
    };

    // Extract area info
    let area = AreaInfoOutput {
        area_name: cache.current_area.area_name.clone(),
        area_id: cache.current_area.area_id,
        difficulty_name: cache.current_area.difficulty_name.clone(),
    };

    Ok((encounter_summaries, player, area))
}
