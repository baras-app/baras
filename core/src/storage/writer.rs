//! Parquet writer for encounter events.

use arrow::array::{
    ArrayRef, BooleanBuilder, Float32Builder, Int32Builder, Int64Builder, StringBuilder,
    TimestampMillisecondBuilder, UInt32Builder, UInt64Builder,
};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;

use crate::combat_log::CombatEvent;
use crate::context::resolve;

/// Flattened event row for parquet storage.
/// Contains event data + denormalized encounter metadata.
#[derive(Debug, Clone)]
pub struct EventRow {
    // ─── Core Event Identity ─────────────────────────────────────────────────
    pub timestamp_ms: i64,
    pub line_number: u64,

    // ─── Source Entity ───────────────────────────────────────────────────────
    pub source_id: i64,
    pub source_name: String,
    pub source_class_id: i64,
    pub source_entity_type: &'static str,
    pub source_hp: i32,
    pub source_max_hp: i32,

    // ─── Target Entity ───────────────────────────────────────────────────────
    pub target_id: i64,
    pub target_name: String,
    pub target_class_id: i64,
    pub target_entity_type: &'static str,
    pub target_hp: i32,
    pub target_max_hp: i32,

    // ─── Action ──────────────────────────────────────────────────────────────
    pub ability_id: i64,
    pub ability_name: String,

    // ─── Effect ──────────────────────────────────────────────────────────────
    pub effect_id: i64,
    pub effect_name: String,
    pub effect_type_id: i64,
    pub effect_type_name: String,

    // ─── Damage Details ──────────────────────────────────────────────────────
    pub dmg_amount: i32,
    pub dmg_effective: i32,
    pub dmg_absorbed: i32,
    pub dmg_type_id: i64,
    pub dmg_type: String,
    pub is_crit: bool,
    pub is_reflect: bool,
    pub defense_type_id: i64,

    // ─── Healing Details ─────────────────────────────────────────────────────
    pub heal_amount: i32,
    pub heal_effective: i32,

    // ─── Other Combat Values ─────────────────────────────────────────────────
    pub threat: f32,
    pub charges: i32,

    // ─── Denormalized Encounter Metadata ─────────────────────────────────────
    pub encounter_idx: u32,
    pub combat_time_secs: Option<f32>,
    pub phase_id: Option<String>,
    pub phase_name: Option<String>,
    pub area_name: String,
    pub boss_name: Option<String>,
    pub difficulty: Option<String>,
}

impl EventRow {
    pub fn from_event(event: &CombatEvent, metadata: &EventMetadata) -> Self {
        Self {
            // Core identity
            timestamp_ms: event.timestamp.and_utc().timestamp_millis(),
            line_number: event.line_number,

            // Source entity
            source_id: event.source_entity.log_id,
            source_name: resolve(event.source_entity.name).to_string(),
            source_class_id: event.source_entity.class_id,
            source_entity_type: entity_type_str(&event.source_entity.entity_type),
            source_hp: event.source_entity.health.0,
            source_max_hp: event.source_entity.health.1,

            // Target entity
            target_id: event.target_entity.log_id,
            target_name: resolve(event.target_entity.name).to_string(),
            target_class_id: event.target_entity.class_id,
            target_entity_type: entity_type_str(&event.target_entity.entity_type),
            target_hp: event.target_entity.health.0,
            target_max_hp: event.target_entity.health.1,

            // Action
            ability_id: event.action.action_id,
            ability_name: resolve(event.action.name).to_string(),

            // Effect
            effect_id: event.effect.effect_id,
            effect_name: resolve(event.effect.effect_name).to_string(),
            effect_type_id: event.effect.type_id,
            effect_type_name: resolve(event.effect.type_name).to_string(),

            // Damage details
            dmg_amount: event.details.dmg_amount,
            dmg_effective: event.details.dmg_effective,
            dmg_absorbed: event.details.dmg_absorbed,
            dmg_type_id: event.details.dmg_type_id,
            dmg_type: resolve(event.details.dmg_type).to_string(),
            is_crit: event.details.is_crit,
            is_reflect: event.details.is_reflect,
            defense_type_id: event.details.defense_type_id,

            // Healing details
            heal_amount: event.details.heal_amount,
            heal_effective: event.details.heal_effective,

            // Other combat values
            threat: event.details.threat,
            charges: event.details.charges,

            // Encounter metadata
            encounter_idx: metadata.encounter_idx,
            combat_time_secs: metadata.combat_time_secs,
            phase_id: metadata.phase_id.clone(),
            phase_name: metadata.phase_name.clone(),
            area_name: metadata.area_name.clone(),
            boss_name: metadata.boss_name.clone(),
            difficulty: metadata.difficulty.clone(),
        }
    }
}

/// Convert EntityType enum to string for storage
fn entity_type_str(entity_type: &crate::combat_log::EntityType) -> &'static str {
    use crate::combat_log::EntityType;
    match entity_type {
        EntityType::Player => "Player",
        EntityType::Npc => "Npc",
        EntityType::Companion => "Companion",
        EntityType::Empty => "",
        EntityType::SelfReference => "Self",
    }
}

/// Metadata for denormalizing into event rows.
#[derive(Debug, Clone, Default)]
pub struct EventMetadata {
    pub encounter_idx: u32,
    pub combat_time_secs: Option<f32>,
    pub phase_id: Option<String>,
    pub phase_name: Option<String>,
    pub area_name: String,
    pub boss_name: Option<String>,
    pub difficulty: Option<String>,
}

impl EventMetadata {
    /// Build metadata from session cache state at the time of an event.
    pub fn from_cache(
        cache: &crate::state::SessionCache,
        encounter_idx: u32,
        event_timestamp: chrono::NaiveDateTime,
    ) -> Self {
        let enc = cache.current_encounter();
        let boss_def = enc.and_then(|e| e.active_boss_definition());
        let current_phase = enc.and_then(|e| e.current_phase.clone());

        // Calculate elapsed combat time from combat start
        let combat_time_secs = enc.and_then(|e| {
            e.enter_combat_time.map(|start| {
                let duration = event_timestamp - start;
                duration.num_milliseconds() as f32 / 1000.0
            })
        });

        Self {
            encounter_idx,
            combat_time_secs,
            phase_id: current_phase.clone(),
            phase_name: current_phase.as_ref().and_then(|phase_id| {
                boss_def.and_then(|def| {
                    def.phases
                        .iter()
                        .find(|p| &p.id == phase_id)
                        .map(|p| p.name.clone())
                })
            }),
            area_name: cache.current_area.area_name.clone(),
            boss_name: boss_def.map(|d| d.name.clone()),
            difficulty: if cache.current_area.difficulty_name.is_empty() {
                None
            } else {
                Some(cache.current_area.difficulty_name.clone())
            },
        }
    }
}

/// Writer for a single encounter's events to parquet.
pub struct EncounterWriter {
    rows: Vec<EventRow>,
}

impl EncounterWriter {
    pub fn new() -> Self {
        Self {
            rows: Vec::with_capacity(10_000),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            rows: Vec::with_capacity(capacity),
        }
    }

    /// Add an event row to the buffer.
    pub fn push(&mut self, row: EventRow) {
        self.rows.push(row);
    }

    /// Add an event with metadata.
    pub fn push_event(&mut self, event: &CombatEvent, metadata: &EventMetadata) {
        self.rows.push(EventRow::from_event(event, metadata));
    }

    /// Number of buffered rows.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.rows.clear();
    }

    /// Get a RecordBatch snapshot of buffered rows for querying.
    /// Returns None if buffer is empty.
    pub fn to_record_batch(&self) -> Option<RecordBatch> {
        if self.rows.is_empty() {
            return None;
        }
        self.build_record_batch(&Self::schema()).ok()
    }

    /// Get the schema for encounter event data.
    pub fn schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            // ─── Core Event Identity ─────────────────────────────────────────
            Field::new(
                "timestamp",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new("line_number", DataType::UInt64, false),
            // ─── Source Entity ───────────────────────────────────────────────
            Field::new("source_id", DataType::Int64, false),
            Field::new("source_name", DataType::Utf8, false),
            Field::new("source_class_id", DataType::Int64, false),
            Field::new("source_entity_type", DataType::Utf8, false),
            Field::new("source_hp", DataType::Int32, false),
            Field::new("source_max_hp", DataType::Int32, false),
            // ─── Target Entity ───────────────────────────────────────────────
            Field::new("target_id", DataType::Int64, false),
            Field::new("target_name", DataType::Utf8, false),
            Field::new("target_class_id", DataType::Int64, false),
            Field::new("target_entity_type", DataType::Utf8, false),
            Field::new("target_hp", DataType::Int32, false),
            Field::new("target_max_hp", DataType::Int32, false),
            // ─── Action ──────────────────────────────────────────────────────
            Field::new("ability_id", DataType::Int64, false),
            Field::new("ability_name", DataType::Utf8, false),
            // ─── Effect ──────────────────────────────────────────────────────
            Field::new("effect_id", DataType::Int64, false),
            Field::new("effect_name", DataType::Utf8, false),
            Field::new("effect_type_id", DataType::Int64, false),
            Field::new("effect_type_name", DataType::Utf8, false),
            // ─── Damage Details ──────────────────────────────────────────────
            Field::new("dmg_amount", DataType::Int32, false),
            Field::new("dmg_effective", DataType::Int32, false),
            Field::new("dmg_absorbed", DataType::Int32, false),
            Field::new("dmg_type_id", DataType::Int64, false),
            Field::new("dmg_type", DataType::Utf8, false),
            Field::new("is_crit", DataType::Boolean, false),
            Field::new("is_reflect", DataType::Boolean, false),
            Field::new("dmg_type_id", DataType::Int64, false),
            // ─── Healing Details ─────────────────────────────────────────────
            Field::new("heal_amount", DataType::Int32, false),
            Field::new("heal_effective", DataType::Int32, false),
            // ─── Other Combat Values ─────────────────────────────────────────
            Field::new("threat", DataType::Float32, false),
            Field::new("charges", DataType::Int32, false),
            // ─── Denormalized Encounter Metadata ─────────────────────────────
            Field::new("encounter_idx", DataType::UInt32, false),
            Field::new("combat_time_secs", DataType::Float32, true),
            Field::new("phase_id", DataType::Utf8, true),
            Field::new("phase_name", DataType::Utf8, true),
            Field::new("area_name", DataType::Utf8, false),
            Field::new("boss_name", DataType::Utf8, true),
            Field::new("difficulty", DataType::Utf8, true),
        ]))
    }

    /// Write buffered rows to a parquet file.
    pub fn write_to_file(&self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        if self.rows.is_empty() {
            return Ok(());
        }

        let schema = Self::schema();
        let batch = self.build_record_batch(&schema)?;

        let file = File::create(path)?;
        let props = WriterProperties::builder()
            .set_compression(Compression::ZSTD(Default::default()))
            .build();

        let mut writer = ArrowWriter::try_new(file, schema, Some(props))?;
        writer.write(&batch)?;
        writer.close()?;

        Ok(())
    }

    fn build_record_batch(
        &self,
        schema: &Arc<Schema>,
    ) -> Result<RecordBatch, Box<dyn std::error::Error>> {
        let len = self.rows.len();

        // ─── Core Event Identity ─────────────────────────────────────────────
        let mut timestamp = TimestampMillisecondBuilder::with_capacity(len);
        let mut line_number = UInt64Builder::with_capacity(len);

        // ─── Source Entity ───────────────────────────────────────────────────
        let mut source_id = Int64Builder::with_capacity(len);
        let mut source_name = StringBuilder::with_capacity(len, len * 20);
        let mut source_class_id = Int64Builder::with_capacity(len);
        let mut source_entity_type = StringBuilder::with_capacity(len, len * 10);
        let mut source_hp = Int32Builder::with_capacity(len);
        let mut source_max_hp = Int32Builder::with_capacity(len);

        // ─── Target Entity ───────────────────────────────────────────────────
        let mut target_id = Int64Builder::with_capacity(len);
        let mut target_name = StringBuilder::with_capacity(len, len * 20);
        let mut target_class_id = Int64Builder::with_capacity(len);
        let mut target_entity_type = StringBuilder::with_capacity(len, len * 10);
        let mut target_hp = Int32Builder::with_capacity(len);
        let mut target_max_hp = Int32Builder::with_capacity(len);

        // ─── Action ──────────────────────────────────────────────────────────
        let mut ability_id = Int64Builder::with_capacity(len);
        let mut ability_name = StringBuilder::with_capacity(len, len * 30);

        // ─── Effect ──────────────────────────────────────────────────────────
        let mut effect_id = Int64Builder::with_capacity(len);
        let mut effect_name = StringBuilder::with_capacity(len, len * 30);
        let mut effect_type_id = Int64Builder::with_capacity(len);
        let mut effect_type_name = StringBuilder::with_capacity(len, len * 20);

        // ─── Damage Details ──────────────────────────────────────────────────
        let mut dmg_amount = Int32Builder::with_capacity(len);
        let mut dmg_effective = Int32Builder::with_capacity(len);
        let mut dmg_absorbed = Int32Builder::with_capacity(len);
        let mut dmg_type_id = Int64Builder::with_capacity(len);
        let mut dmg_type = StringBuilder::with_capacity(len, len * 15);
        let mut is_crit = BooleanBuilder::with_capacity(len);
        let mut is_reflect = BooleanBuilder::with_capacity(len);
        let mut defense_type_id = Int64Builder::with_capacity(len);

        // ─── Healing Details ─────────────────────────────────────────────────
        let mut heal_amount = Int32Builder::with_capacity(len);
        let mut heal_effective = Int32Builder::with_capacity(len);

        // ─── Other Combat Values ─────────────────────────────────────────────
        let mut threat = Float32Builder::with_capacity(len);
        let mut charges = Int32Builder::with_capacity(len);

        // ─── Denormalized Encounter Metadata ─────────────────────────────────
        let mut encounter_idx = UInt32Builder::with_capacity(len);
        let mut combat_time_secs = Float32Builder::with_capacity(len);
        let mut phase_id = StringBuilder::with_capacity(len, len * 10);
        let mut phase_name = StringBuilder::with_capacity(len, len * 20);
        let mut area_name = StringBuilder::with_capacity(len, len * 30);
        let mut boss_name = StringBuilder::with_capacity(len, len * 30);
        let mut difficulty = StringBuilder::with_capacity(len, len * 10);

        for row in &self.rows {
            // Core identity
            timestamp.append_value(row.timestamp_ms);
            line_number.append_value(row.line_number);

            // Source entity
            source_id.append_value(row.source_id);
            source_name.append_value(&row.source_name);
            source_class_id.append_value(row.source_class_id);
            source_entity_type.append_value(row.source_entity_type);
            source_hp.append_value(row.source_hp);
            source_max_hp.append_value(row.source_max_hp);

            // Target entity
            target_id.append_value(row.target_id);
            target_name.append_value(&row.target_name);
            target_class_id.append_value(row.target_class_id);
            target_entity_type.append_value(row.target_entity_type);
            target_hp.append_value(row.target_hp);
            target_max_hp.append_value(row.target_max_hp);

            // Action
            ability_id.append_value(row.ability_id);
            ability_name.append_value(&row.ability_name);

            // Effect
            effect_id.append_value(row.effect_id);
            effect_name.append_value(&row.effect_name);
            effect_type_id.append_value(row.effect_type_id);
            effect_type_name.append_value(&row.effect_type_name);

            // Damage details
            dmg_amount.append_value(row.dmg_amount);
            dmg_effective.append_value(row.dmg_effective);
            dmg_absorbed.append_value(row.dmg_absorbed);
            dmg_type_id.append_value(row.dmg_type_id);
            dmg_type.append_value(&row.dmg_type);
            is_crit.append_value(row.is_crit);
            is_reflect.append_value(row.is_reflect);
            defense_type_id.append_value(row.defense_type_id);

            // Healing details
            heal_amount.append_value(row.heal_amount);
            heal_effective.append_value(row.heal_effective);

            // Other combat values
            threat.append_value(row.threat);
            charges.append_value(row.charges);

            // Encounter metadata
            encounter_idx.append_value(row.encounter_idx);
            combat_time_secs.append_option(row.combat_time_secs);
            phase_id.append_option(row.phase_id.as_deref());
            phase_name.append_option(row.phase_name.as_deref());
            area_name.append_value(&row.area_name);
            boss_name.append_option(row.boss_name.as_deref());
            difficulty.append_option(row.difficulty.as_deref());
        }

        let columns: Vec<ArrayRef> = vec![
            // Core identity
            Arc::new(timestamp.finish()),
            Arc::new(line_number.finish()),
            // Source entity
            Arc::new(source_id.finish()),
            Arc::new(source_name.finish()),
            Arc::new(source_class_id.finish()),
            Arc::new(source_entity_type.finish()),
            Arc::new(source_hp.finish()),
            Arc::new(source_max_hp.finish()),
            // Target entity
            Arc::new(target_id.finish()),
            Arc::new(target_name.finish()),
            Arc::new(target_class_id.finish()),
            Arc::new(target_entity_type.finish()),
            Arc::new(target_hp.finish()),
            Arc::new(target_max_hp.finish()),
            // Action
            Arc::new(ability_id.finish()),
            Arc::new(ability_name.finish()),
            // Effect
            Arc::new(effect_id.finish()),
            Arc::new(effect_name.finish()),
            Arc::new(effect_type_id.finish()),
            Arc::new(effect_type_name.finish()),
            // Damage details
            Arc::new(dmg_amount.finish()),
            Arc::new(dmg_effective.finish()),
            Arc::new(dmg_absorbed.finish()),
            Arc::new(dmg_type_id.finish()),
            Arc::new(dmg_type.finish()),
            Arc::new(is_crit.finish()),
            Arc::new(is_reflect.finish()),
            Arc::new(defense_type_id.finish()),
            // Healing details
            Arc::new(heal_amount.finish()),
            Arc::new(heal_effective.finish()),
            // Other combat values
            Arc::new(threat.finish()),
            Arc::new(charges.finish()),
            // Encounter metadata
            Arc::new(encounter_idx.finish()),
            Arc::new(combat_time_secs.finish()),
            Arc::new(phase_id.finish()),
            Arc::new(phase_name.finish()),
            Arc::new(area_name.finish()),
            Arc::new(boss_name.finish()),
            Arc::new(difficulty.finish()),
        ];

        Ok(RecordBatch::try_new(schema.clone(), columns)?)
    }
}

impl Default for EncounterWriter {
    fn default() -> Self {
        Self::new()
    }
}
