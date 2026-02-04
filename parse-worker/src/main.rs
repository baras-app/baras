//! baras-parse-worker - Subprocess for parsing combat log files.
//!
//! This binary is spawned by the main BARAS app to parse historical files.
//! It runs in a separate process so memory fragmentation doesn't affect the main app.
//!
//! Usage: baras-parse-worker <file_path> <session_id> <output_dir> [definitions_dir]
//!
//! Output: JSON to stdout with encounter summaries and final byte position.

#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use arrow::array::{
    ArrayBuilder, ArrayRef, BooleanBuilder, Float32Builder, Int32Builder, Int64Builder, ListArray,
    StringBuilder, StructArray, TimestampMillisecondBuilder, UInt32Builder, UInt64Builder,
    UInt8Builder,
};
use arrow::buffer::{NullBuffer, OffsetBuffer};
use arrow::datatypes::{DataType, Field, Fields, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use baras_core::combat_log::{CombatEvent, EntityType, LogParser};
use baras_core::context::{parse_log_filename, resolve};
use baras_core::dsl::{build_area_index, load_bosses_with_custom};
use baras_core::game_data::defense_type;
use baras_core::signal_processor::{EventProcessor, GameSignal};
use baras_core::state::{ParseWorkerOutput, SessionCache};
use baras_core::storage::encounter_filename;
use encoding_rs::WINDOWS_1252;
use memchr::memchr_iter;
use memmap2::Mmap;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use rayon::prelude::*;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use tracing_subscriber::filter::EnvFilter;

// ─────────────────────────────────────────────────────────────────────────────
// Fast Encounter Writer - writes directly to Arrow builders, no intermediate allocs
// ─────────────────────────────────────────────────────────────────────────────

struct FastEncounterWriter {
    // Core event identity
    timestamp: TimestampMillisecondBuilder,
    line_number: UInt64Builder,
    // Source entity
    source_id: Int64Builder,
    source_name: StringBuilder,
    source_class_id: Int64Builder,
    source_entity_type: StringBuilder,
    source_hp: Int32Builder,
    source_max_hp: Int32Builder,
    // Target entity
    target_id: Int64Builder,
    target_name: StringBuilder,
    target_class_id: Int64Builder,
    target_entity_type: StringBuilder,
    target_hp: Int32Builder,
    target_max_hp: Int32Builder,
    // Action
    ability_id: Int64Builder,
    ability_name: StringBuilder,
    // Effect
    effect_id: Int64Builder,
    effect_name: StringBuilder,
    effect_type_id: Int64Builder,
    effect_type_name: StringBuilder,
    // Damage details
    dmg_amount: Int32Builder,
    dmg_effective: Int32Builder,
    dmg_absorbed: Int32Builder,
    dmg_type_id: Int64Builder,
    dmg_type: StringBuilder,
    is_crit: BooleanBuilder,
    is_reflect: BooleanBuilder,
    defense_type_id: Int64Builder,
    // Healing details
    heal_amount: Int32Builder,
    heal_effective: Int32Builder,
    // Other combat values
    threat: Float32Builder,
    charges: Int32Builder,
    // Encounter metadata
    encounter_idx: UInt32Builder,
    combat_time_secs: Float32Builder,
    phase_id: StringBuilder,
    phase_name: StringBuilder,
    area_name: StringBuilder,
    boss_name: StringBuilder,
    difficulty: StringBuilder,
    // Shield attribution context
    shield_effect_ids: Int64Builder,
    shield_source_ids: Int64Builder,
    shield_positions: UInt8Builder,
    shield_estimated_maxes: Int64Builder,
    shield_list_offsets: Vec<Option<(usize, usize)>>,
    // Row count
    len: usize,
}

impl FastEncounterWriter {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            timestamp: TimestampMillisecondBuilder::with_capacity(capacity),
            line_number: UInt64Builder::with_capacity(capacity),
            source_id: Int64Builder::with_capacity(capacity),
            source_name: StringBuilder::with_capacity(capacity, capacity * 16),
            source_class_id: Int64Builder::with_capacity(capacity),
            source_entity_type: StringBuilder::with_capacity(capacity, capacity * 8),
            source_hp: Int32Builder::with_capacity(capacity),
            source_max_hp: Int32Builder::with_capacity(capacity),
            target_id: Int64Builder::with_capacity(capacity),
            target_name: StringBuilder::with_capacity(capacity, capacity * 16),
            target_class_id: Int64Builder::with_capacity(capacity),
            target_entity_type: StringBuilder::with_capacity(capacity, capacity * 8),
            target_hp: Int32Builder::with_capacity(capacity),
            target_max_hp: Int32Builder::with_capacity(capacity),
            ability_id: Int64Builder::with_capacity(capacity),
            ability_name: StringBuilder::with_capacity(capacity, capacity * 24),
            effect_id: Int64Builder::with_capacity(capacity),
            effect_name: StringBuilder::with_capacity(capacity, capacity * 24),
            effect_type_id: Int64Builder::with_capacity(capacity),
            effect_type_name: StringBuilder::with_capacity(capacity, capacity * 16),
            dmg_amount: Int32Builder::with_capacity(capacity),
            dmg_effective: Int32Builder::with_capacity(capacity),
            dmg_absorbed: Int32Builder::with_capacity(capacity),
            dmg_type_id: Int64Builder::with_capacity(capacity),
            dmg_type: StringBuilder::with_capacity(capacity, capacity * 12),
            is_crit: BooleanBuilder::with_capacity(capacity),
            is_reflect: BooleanBuilder::with_capacity(capacity),
            defense_type_id: Int64Builder::with_capacity(capacity),
            heal_amount: Int32Builder::with_capacity(capacity),
            heal_effective: Int32Builder::with_capacity(capacity),
            threat: Float32Builder::with_capacity(capacity),
            charges: Int32Builder::with_capacity(capacity),
            encounter_idx: UInt32Builder::with_capacity(capacity),
            combat_time_secs: Float32Builder::with_capacity(capacity),
            phase_id: StringBuilder::with_capacity(capacity, capacity * 8),
            phase_name: StringBuilder::with_capacity(capacity, capacity * 16),
            area_name: StringBuilder::with_capacity(capacity, capacity * 24),
            boss_name: StringBuilder::with_capacity(capacity, capacity * 24),
            difficulty: StringBuilder::with_capacity(capacity, capacity * 8),
            shield_effect_ids: Int64Builder::with_capacity(capacity),
            shield_source_ids: Int64Builder::with_capacity(capacity),
            shield_positions: UInt8Builder::with_capacity(capacity),
            shield_estimated_maxes: Int64Builder::with_capacity(capacity),
            shield_list_offsets: Vec::with_capacity(capacity),
            len: 0,
        }
    }

    #[inline]
    fn append_event(&mut self, event: &CombatEvent, cache: &SessionCache, encounter_idx: u32) {
        // Core identity
        self.timestamp
            .append_value(event.timestamp.and_utc().timestamp_millis());
        self.line_number.append_value(event.line_number);

        // Source entity
        self.source_id.append_value(event.source_entity.log_id);
        self.source_name
            .append_value(resolve(event.source_entity.name));
        self.source_class_id
            .append_value(event.source_entity.class_id);
        self.source_entity_type
            .append_value(entity_type_str(&event.source_entity.entity_type));
        self.source_hp.append_value(event.source_entity.health.0);
        self.source_max_hp
            .append_value(event.source_entity.health.1);

        // Target entity
        self.target_id.append_value(event.target_entity.log_id);
        self.target_name
            .append_value(resolve(event.target_entity.name));
        self.target_class_id
            .append_value(event.target_entity.class_id);
        self.target_entity_type
            .append_value(entity_type_str(&event.target_entity.entity_type));
        self.target_hp.append_value(event.target_entity.health.0);
        self.target_max_hp
            .append_value(event.target_entity.health.1);

        // Action
        self.ability_id.append_value(event.action.action_id);
        self.ability_name.append_value(resolve(event.action.name));

        // Effect
        self.effect_id.append_value(event.effect.effect_id);
        self.effect_name
            .append_value(resolve(event.effect.effect_name));
        self.effect_type_id.append_value(event.effect.type_id);
        self.effect_type_name
            .append_value(resolve(event.effect.type_name));

        // Damage details
        self.dmg_amount.append_value(event.details.dmg_amount);
        self.dmg_effective.append_value(event.details.dmg_effective);
        self.dmg_absorbed.append_value(event.details.dmg_absorbed);
        self.dmg_type_id.append_value(event.details.dmg_type_id);
        self.dmg_type.append_value(resolve(event.details.dmg_type));
        self.is_crit.append_value(event.details.is_crit);
        self.is_reflect.append_value(event.details.is_reflect);
        self.defense_type_id
            .append_value(event.details.defense_type_id);

        // Healing details
        self.heal_amount.append_value(event.details.heal_amount);
        self.heal_effective
            .append_value(event.details.heal_effective);

        // Other combat values
        self.threat.append_value(event.details.threat);
        self.charges.append_value(event.details.charges);

        // Encounter metadata - computed inline, no intermediate struct
        let enc = cache.current_encounter();
        let boss_def = enc.and_then(|e| e.active_boss_definition());
        // Borrow phase reference to avoid cloning on every event
        let current_phase = enc.and_then(|e| e.current_phase.as_deref());

        let combat_time = enc.and_then(|e| {
            e.enter_combat_time.map(|start| {
                let duration = event.timestamp - start;
                duration.num_milliseconds() as f32 / 1000.0
            })
        });

        self.encounter_idx.append_value(encounter_idx);
        self.combat_time_secs.append_option(combat_time);
        self.phase_id.append_option(current_phase);
        self.phase_name
            .append_option(current_phase.and_then(|phase_id| {
                boss_def.and_then(|def| {
                    def.phases
                        .iter()
                        .find(|p| p.id == phase_id)
                        .map(|p| p.name.as_str())
                })
            }));
        self.area_name.append_value(&cache.current_area.area_name);
        self.boss_name
            .append_option(boss_def.map(|d| d.name.as_str()));
        self.difficulty
            .append_option(if cache.current_area.difficulty_name.is_empty() {
                None
            } else {
                Some(cache.current_area.difficulty_name.as_str())
            });

        // Shield attribution context - capture active shields for damage events with absorption
        let is_natural_shield = event.details.defense_type_id == defense_type::SHIELD
            && event.details.dmg_effective == event.details.dmg_amount;

        if event.details.dmg_absorbed > 0 && !is_natural_shield {
            if let Some(enc) = cache.current_encounter() {
                let shields = enc.get_shield_context(event.target_entity.log_id, event.timestamp);
                if !shields.is_empty() {
                    let start = self.shield_effect_ids.len();
                    for s in &shields {
                        self.shield_effect_ids.append_value(s.effect_id);
                        self.shield_source_ids.append_value(s.source_id);
                        self.shield_positions.append_value(s.position);
                        self.shield_estimated_maxes.append_value(s.estimated_max);
                    }
                    self.shield_list_offsets
                        .push(Some((start, self.shield_effect_ids.len())));
                } else {
                    self.shield_list_offsets.push(None);
                }
            } else {
                self.shield_list_offsets.push(None);
            }
        } else {
            self.shield_list_offsets.push(None);
        }

        self.len += 1;
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Finish builders and return a RecordBatch. Builders are left empty and ready for reuse.
    fn take_batch(&mut self) -> Option<RecordBatch> {
        if self.is_empty() {
            return None;
        }

        let schema = ENCOUNTER_SCHEMA.clone();

        // Build the active_shields List<Struct> array
        let active_shields_array = {
            let struct_fields = Fields::from(vec![
                Field::new("effect_id", DataType::Int64, false),
                Field::new("source_id", DataType::Int64, false),
                Field::new("position", DataType::UInt8, false),
                Field::new("estimated_max", DataType::Int64, false),
            ]);
            let struct_array = StructArray::try_new(
                struct_fields.clone(),
                vec![
                    Arc::new(self.shield_effect_ids.finish()) as ArrayRef,
                    Arc::new(self.shield_source_ids.finish()) as ArrayRef,
                    Arc::new(self.shield_positions.finish()) as ArrayRef,
                    Arc::new(self.shield_estimated_maxes.finish()) as ArrayRef,
                ],
                None,
            )
            .ok()?;

            // Build offsets and nulls for the list
            let mut offsets: Vec<i32> = Vec::with_capacity(self.shield_list_offsets.len() + 1);
            let mut nulls: Vec<bool> = Vec::with_capacity(self.shield_list_offsets.len());
            offsets.push(0);
            for offset in &self.shield_list_offsets {
                match offset {
                    Some((_, end)) => {
                        offsets.push(*end as i32);
                        nulls.push(true);
                    }
                    None => {
                        offsets.push(*offsets.last().unwrap());
                        nulls.push(false);
                    }
                }
            }
            self.shield_list_offsets.clear();

            let list_field = Field::new("item", DataType::Struct(struct_fields), true);
            ListArray::try_new(
                Arc::new(list_field),
                OffsetBuffer::new(offsets.into()),
                Arc::new(struct_array),
                Some(NullBuffer::from(nulls)),
            )
            .ok()?
        };

        let columns: Vec<ArrayRef> = vec![
            Arc::new(self.timestamp.finish()),
            Arc::new(self.line_number.finish()),
            Arc::new(self.source_id.finish()),
            Arc::new(self.source_name.finish()),
            Arc::new(self.source_class_id.finish()),
            Arc::new(self.source_entity_type.finish()),
            Arc::new(self.source_hp.finish()),
            Arc::new(self.source_max_hp.finish()),
            Arc::new(self.target_id.finish()),
            Arc::new(self.target_name.finish()),
            Arc::new(self.target_class_id.finish()),
            Arc::new(self.target_entity_type.finish()),
            Arc::new(self.target_hp.finish()),
            Arc::new(self.target_max_hp.finish()),
            Arc::new(self.ability_id.finish()),
            Arc::new(self.ability_name.finish()),
            Arc::new(self.effect_id.finish()),
            Arc::new(self.effect_name.finish()),
            Arc::new(self.effect_type_id.finish()),
            Arc::new(self.effect_type_name.finish()),
            Arc::new(self.dmg_amount.finish()),
            Arc::new(self.dmg_effective.finish()),
            Arc::new(self.dmg_absorbed.finish()),
            Arc::new(self.dmg_type_id.finish()),
            Arc::new(self.dmg_type.finish()),
            Arc::new(self.is_crit.finish()),
            Arc::new(self.is_reflect.finish()),
            Arc::new(self.defense_type_id.finish()),
            Arc::new(self.heal_amount.finish()),
            Arc::new(self.heal_effective.finish()),
            Arc::new(self.threat.finish()),
            Arc::new(self.charges.finish()),
            Arc::new(self.encounter_idx.finish()),
            Arc::new(self.combat_time_secs.finish()),
            Arc::new(self.phase_id.finish()),
            Arc::new(self.phase_name.finish()),
            Arc::new(self.area_name.finish()),
            Arc::new(self.boss_name.finish()),
            Arc::new(self.difficulty.finish()),
            Arc::new(active_shields_array),
        ];

        self.len = 0;
        RecordBatch::try_new(schema, columns).ok()
    }

    /// Write a RecordBatch to a parquet file (can be called from any thread)
    fn write_batch_to_file(
        batch: RecordBatch,
        path: PathBuf,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let schema = batch.schema();
        let file = File::create(&path)?;
        let props = WriterProperties::builder()
            .set_compression(Compression::LZ4)
            .build();

        let mut writer = ArrowWriter::try_new(file, schema, Some(props))?;
        writer.write(&batch)?;
        writer.close()?;
        Ok(())
    }

    fn build_schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new(
                "timestamp",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new("line_number", DataType::UInt64, false),
            Field::new("source_id", DataType::Int64, false),
            Field::new("source_name", DataType::Utf8, false),
            Field::new("source_class_id", DataType::Int64, false),
            Field::new("source_entity_type", DataType::Utf8, false),
            Field::new("source_hp", DataType::Int32, false),
            Field::new("source_max_hp", DataType::Int32, false),
            Field::new("target_id", DataType::Int64, false),
            Field::new("target_name", DataType::Utf8, false),
            Field::new("target_class_id", DataType::Int64, false),
            Field::new("target_entity_type", DataType::Utf8, false),
            Field::new("target_hp", DataType::Int32, false),
            Field::new("target_max_hp", DataType::Int32, false),
            Field::new("ability_id", DataType::Int64, false),
            Field::new("ability_name", DataType::Utf8, false),
            Field::new("effect_id", DataType::Int64, false),
            Field::new("effect_name", DataType::Utf8, false),
            Field::new("effect_type_id", DataType::Int64, false),
            Field::new("effect_type_name", DataType::Utf8, false),
            Field::new("dmg_amount", DataType::Int32, false),
            Field::new("dmg_effective", DataType::Int32, false),
            Field::new("dmg_absorbed", DataType::Int32, false),
            Field::new("dmg_type_id", DataType::Int64, false),
            Field::new("dmg_type", DataType::Utf8, false),
            Field::new("is_crit", DataType::Boolean, false),
            Field::new("is_reflect", DataType::Boolean, false),
            Field::new("defense_type_id", DataType::Int64, false),
            Field::new("heal_amount", DataType::Int32, false),
            Field::new("heal_effective", DataType::Int32, false),
            Field::new("threat", DataType::Float32, false),
            Field::new("charges", DataType::Int32, false),
            Field::new("encounter_idx", DataType::UInt32, false),
            Field::new("combat_time_secs", DataType::Float32, true),
            Field::new("phase_id", DataType::Utf8, true),
            Field::new("phase_name", DataType::Utf8, true),
            Field::new("area_name", DataType::Utf8, false),
            Field::new("boss_name", DataType::Utf8, true),
            Field::new("difficulty", DataType::Utf8, true),
            // Shield attribution context
            Field::new(
                "active_shields",
                DataType::List(Arc::new(Field::new(
                    "item",
                    DataType::Struct(Fields::from(vec![
                        Field::new("effect_id", DataType::Int64, false),
                        Field::new("source_id", DataType::Int64, false),
                        Field::new("position", DataType::UInt8, false),
                        Field::new("estimated_max", DataType::Int64, false),
                    ])),
                    true,
                ))),
                true,
            ),
        ]))
    }
}

const fn entity_type_str(entity_type: &EntityType) -> &'static str {
    match entity_type {
        EntityType::Player => "Player",
        EntityType::Npc => "Npc",
        EntityType::Companion => "Companion",
        EntityType::Empty => "",
        EntityType::SelfReference => "Self",
    }
}

/// Static schema to avoid recreation on every batch write.
static ENCOUNTER_SCHEMA: LazyLock<Arc<Schema>> = LazyLock::new(FastEncounterWriter::build_schema);

/// Initialize logging, writing to BARAS_LOG_PATH if set, otherwise stderr.
fn init_logging() {
    let filter = EnvFilter::builder()
        .with_default_directive(tracing::Level::INFO.into())
        .from_env_lossy();

    // If BARAS_LOG_PATH is set, append to that file (shared with main app)
    if let Ok(path) = std::env::var("BARAS_LOG_PATH") {
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_target(true)
                .with_ansi(false)
                .with_writer(file)
                .init();
            return;
        }
    }

    // Fallback to stderr
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_writer(std::io::stderr)
        .init();
}

fn main() {
    // Initialize tracing subscriber (parse-worker is separate process, needs its own)
    init_logging();

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        tracing::error!(
            "Usage: baras-parse-worker <file_path> <session_id> <output_dir> [definitions_dir]"
        );
        std::process::exit(1);
    }

    let file_path = PathBuf::from(&args[1]);
    let session_id = &args[2];
    let output_dir = PathBuf::from(&args[3]);
    let definitions_dir = args.get(4).map(PathBuf::from);

    // Ensure output directory exists
    if let Err(e) = fs::create_dir_all(&output_dir) {
        tracing::error!(error = %e, "Failed to create output dir");
        std::process::exit(1);
    }

    // User custom directory for definitions
    let user_dir =
        dirs::config_dir().map(|p| p.join("baras").join("definitions").join("encounters"));

    // Build area index from both bundled and custom definitions (lightweight - only reads headers)
    // Custom definitions can override bundled ones (e.g., for new areas like flashpoints)
    let area_index = {
        let mut index = baras_core::dsl::AreaIndex::new();

        // Index bundled definitions
        if let Some(ref dir) = definitions_dir {
            match build_area_index(dir) {
                Ok(bundled_index) => {
                    tracing::debug!(count = bundled_index.len(), "Built bundled area index");
                    index.extend(bundled_index);
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to build bundled area index");
                }
            }
        }

        // Index custom definitions (can add new areas or override bundled)
        if let Some(ref dir) = user_dir {
            if dir.exists() {
                match build_area_index(dir) {
                    Ok(custom_index) => {
                        tracing::debug!(count = custom_index.len(), "Built custom area index");
                        index.extend(custom_index);
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to build custom area index");
                    }
                }
            }
        }

        if index.is_empty() {
            None
        } else {
            Some(index)
        }
    };

    let timer = std::time::Instant::now();

    match parse_file(&file_path, session_id, &output_dir, area_index, user_dir) {
        Ok(output) => {
            let mut output = output;
            output.elapsed_ms = timer.elapsed().as_millis();

            // Output JSON to stdout for main process
            if let Ok(json) = serde_json::to_string(&output) {
                println!("{}", json);
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "Parse error");
            std::process::exit(1);
        }
    }
}

fn parse_file(
    file_path: &Path,
    _session_id: &str,
    output_dir: &Path,
    area_index: Option<baras_core::dsl::AreaIndex>,
    user_dir: Option<PathBuf>,
) -> Result<ParseWorkerOutput, String> {
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

    // Process events and write encounters (definitions loaded lazily on AreaEntered)
    let (cache, incomplete_line) =
        process_and_write_encounters(events, output_dir, area_index, user_dir)?;

    // If there's an incomplete encounter, set end_pos to the byte position of its first line
    // and line_count to match that line number for correct tailing
    let (final_end_pos, final_line_count) = if let Some(line_num) = incomplete_line {
        // line_num is 1-based, line_ranges is 0-based
        let line_idx = (line_num - 1) as usize;
        let byte_pos = if line_idx < line_ranges.len() {
            line_ranges[line_idx].0 as u64
        } else {
            end_pos
        };
        // Reader uses current_line as the line number for the FIRST line it reads
        // So we pass the line number of the incomplete encounter's first line
        tracing::debug!(
            incomplete_line = line_num,
            byte_pos,
            "Handing off to live parser at incomplete encounter start"
        );
        (byte_pos, line_num)
    } else {
        // No incomplete encounter - start at end of file
        // Next line to be read would be line_count + 1, but reader increments after use
        // So we pass total lines; next read will be parsed as line (total + 1) after increment
        (end_pos, line_ranges.len() as u64)
    };

    let mut output = cache.to_worker_output(final_end_pos, final_line_count, event_count);
    output.elapsed_ms = 0; // Filled in by caller
    Ok(output)
}

fn process_and_write_encounters(
    events: Vec<CombatEvent>,
    output_dir: &Path,
    area_index: Option<baras_core::dsl::AreaIndex>,
    user_dir: Option<PathBuf>,
) -> Result<
    (
        SessionCache,
        Option<u64>, // First line of incomplete encounter (if any)
    ),
    String,
> {
    use std::sync::mpsc;

    // Spawn background writer thread
    let (tx, rx) = mpsc::channel::<(RecordBatch, PathBuf)>();
    let writer_thread = std::thread::spawn(move || {
        for (batch, path) in rx {
            let _ = FastEncounterWriter::write_batch_to_file(batch, path);
        }
    });

    let mut cache = SessionCache::new();
    let mut processor = EventProcessor::new();
    let mut writer = FastEncounterWriter::with_capacity(50_000);
    let mut current_encounter_idx: u32 = 0;
    let mut pending_write = false;
    let output_dir = output_dir.to_path_buf();

    // Track the first line number of the current incomplete encounter
    let mut incomplete_encounter_first_line: Option<u64> = None;

    for event in events {
        // Load boss definitions lazily when we see AreaEntered for an area we have definitions for
        use baras_core::game_data::effect_type_id;
        if event.effect.type_id == effect_type_id::AREAENTERED {
            let area_id = event.effect.effect_id;
            tracing::info!(area_id, "Detected AreaEntered event");

            if let Some(ref index) = area_index {
                if let Some(entry) = index.get(&area_id) {
                    tracing::info!(area_id, area_name = %entry.name, "Found matching area in index, loading definitions");
                    match load_bosses_with_custom(&entry.file_path, user_dir.as_deref()) {
                        Ok(bosses) => {
                            tracing::info!(
                                area_id,
                                count = bosses.len(),
                                "Loaded boss definitions for area"
                            );
                            cache.load_boss_definitions(bosses);
                        }
                        Err(e) => {
                            tracing::warn!(area_id, error = %e, "Failed to load definitions for area");
                        }
                    }
                } else {
                    tracing::info!(area_id, "No matching area found in index");
                }
            } else {
                tracing::warn!("Area index not available");
            }
        }

        let (signals, event, was_accumulated) = processor.process_event(event, &mut cache);

        // Track first line of current encounter (reset on combat end)
        if incomplete_encounter_first_line.is_none() {
            incomplete_encounter_first_line = Some(event.line_number);
        }

        // Only write events that were accumulated (same filtering as live parquet)
        if was_accumulated {
            writer.append_event(&event, &cache, current_encounter_idx);
        }

        for signal in &signals {
            if let GameSignal::CombatEnded { .. } = signal {
                pending_write = true;
            }
        }

        if pending_write {
            if let Some(batch) = writer.take_batch() {
                let filename = encounter_filename(current_encounter_idx);
                let path = output_dir.join(&filename);
                let _ = tx.send((batch, path));
                current_encounter_idx += 1;
            }
            pending_write = false;
            // Reset for next encounter
            incomplete_encounter_first_line = None;
        }
    }

    // Write any remaining events (final incomplete encounter) to parquet
    // The main process will handle this correctly by continuing with the same encounter ID
    if let Some(batch) = writer.take_batch() {
        let filename = encounter_filename(current_encounter_idx);
        let path = output_dir.join(&filename);
        let _ = tx.send((batch, path));
    } else {
        // No incomplete encounter, so clear the tracking
        incomplete_encounter_first_line = None;
    }

    // Close channel and wait for writer thread to finish
    drop(tx);
    let _ = writer_thread.join();

    Ok((cache, incomplete_encounter_first_line))
}
