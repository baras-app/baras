//! Parquet writer for encounter events.

use arrow::array::{
    ArrayRef, Float64Builder, Int64Builder, StringBuilder, TimestampMillisecondBuilder,
    UInt32Builder,
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
    // Event data
    pub timestamp_ms: i64,
    pub line_number: u64,
    pub effect_id: i64,
    pub effect_type_id: i64,
    pub source_id: i64,
    pub source_name: String,
    pub target_id: i64,
    pub target_name: String,
    pub ability_id: i64,
    pub ability_name: String,
    pub value: f64,
    pub threat: f64,
    pub is_crit: bool,

    // Denormalized metadata
    pub encounter_idx: u32,
    pub phase_id: Option<String>,
    pub phase_name: Option<String>,
    pub area_name: String,
    pub boss_name: Option<String>,
    pub difficulty: Option<String>,
}

impl EventRow {
    pub fn from_event(event: &CombatEvent, metadata: &EventMetadata) -> Self {
        Self {
            timestamp_ms: event.timestamp.and_utc().timestamp_millis(),
            line_number: event.line_number,
            effect_id: event.effect.effect_id,
            effect_type_id: event.effect.type_id,
            source_id: event.source_entity.log_id,
            source_name: resolve(event.source_entity.name).to_string(),
            target_id: event.target_entity.log_id,
            target_name: resolve(event.target_entity.name).to_string(),
            ability_id: event.action.action_id,
            ability_name: resolve(event.action.name).to_string(),
            value: event.details.dmg_amount as f64,
            threat: event.details.threat as f64,
            is_crit: event.details.is_crit,
            encounter_idx: metadata.encounter_idx,
            phase_id: metadata.phase_id.clone(),
            phase_name: metadata.phase_name.clone(),
            area_name: metadata.area_name.clone(),
            boss_name: metadata.boss_name.clone(),
            difficulty: metadata.difficulty.clone(),
        }
    }
}

/// Metadata for denormalizing into event rows.
#[derive(Debug, Clone, Default)]
pub struct EventMetadata {
    pub encounter_idx: u32,
    pub phase_id: Option<String>,
    pub phase_name: Option<String>,
    pub area_name: String,
    pub boss_name: Option<String>,
    pub difficulty: Option<String>,
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

    fn schema() -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new(
                "timestamp",
                DataType::Timestamp(TimeUnit::Millisecond, None),
                false,
            ),
            Field::new("line_number", DataType::UInt64, false),
            Field::new("effect_id", DataType::Int64, false),
            Field::new("effect_type_id", DataType::Int64, false),
            Field::new("source_id", DataType::Int64, false),
            Field::new("source_name", DataType::Utf8, false),
            Field::new("target_id", DataType::Int64, false),
            Field::new("target_name", DataType::Utf8, false),
            Field::new("ability_id", DataType::Int64, false),
            Field::new("ability_name", DataType::Utf8, false),
            Field::new("value", DataType::Float64, false),
            Field::new("threat", DataType::Float64, false),
            Field::new("is_crit", DataType::Boolean, false),
            Field::new("encounter_idx", DataType::UInt32, false),
            Field::new("phase_id", DataType::Utf8, true),
            Field::new("phase_name", DataType::Utf8, true),
            Field::new("area_name", DataType::Utf8, false),
            Field::new("boss_name", DataType::Utf8, true),
            Field::new("difficulty", DataType::Utf8, true),
        ]))
    }

    fn build_record_batch(
        &self,
        schema: &Arc<Schema>,
    ) -> Result<RecordBatch, Box<dyn std::error::Error>> {
        let len = self.rows.len();

        let mut timestamp = TimestampMillisecondBuilder::with_capacity(len);
        let mut line_number = arrow::array::UInt64Builder::with_capacity(len);
        let mut effect_id = Int64Builder::with_capacity(len);
        let mut effect_type_id = Int64Builder::with_capacity(len);
        let mut source_id = Int64Builder::with_capacity(len);
        let mut source_name = StringBuilder::with_capacity(len, len * 20);
        let mut target_id = Int64Builder::with_capacity(len);
        let mut target_name = StringBuilder::with_capacity(len, len * 20);
        let mut ability_id = Int64Builder::with_capacity(len);
        let mut ability_name = StringBuilder::with_capacity(len, len * 30);
        let mut value = Float64Builder::with_capacity(len);
        let mut threat = Float64Builder::with_capacity(len);
        let mut is_crit = arrow::array::BooleanBuilder::with_capacity(len);
        let mut encounter_idx = UInt32Builder::with_capacity(len);
        let mut phase_id = StringBuilder::with_capacity(len, len * 10);
        let mut phase_name = StringBuilder::with_capacity(len, len * 20);
        let mut area_name = StringBuilder::with_capacity(len, len * 30);
        let mut boss_name = StringBuilder::with_capacity(len, len * 30);
        let mut difficulty = StringBuilder::with_capacity(len, len * 10);

        for row in &self.rows {
            timestamp.append_value(row.timestamp_ms);
            line_number.append_value(row.line_number);
            effect_id.append_value(row.effect_id);
            effect_type_id.append_value(row.effect_type_id);
            source_id.append_value(row.source_id);
            source_name.append_value(&row.source_name);
            target_id.append_value(row.target_id);
            target_name.append_value(&row.target_name);
            ability_id.append_value(row.ability_id);
            ability_name.append_value(&row.ability_name);
            value.append_value(row.value);
            threat.append_value(row.threat);
            is_crit.append_value(row.is_crit);
            encounter_idx.append_value(row.encounter_idx);
            phase_id.append_option(row.phase_id.as_deref());
            phase_name.append_option(row.phase_name.as_deref());
            area_name.append_value(&row.area_name);
            boss_name.append_option(row.boss_name.as_deref());
            difficulty.append_option(row.difficulty.as_deref());
        }

        let columns: Vec<ArrayRef> = vec![
            Arc::new(timestamp.finish()),
            Arc::new(line_number.finish()),
            Arc::new(effect_id.finish()),
            Arc::new(effect_type_id.finish()),
            Arc::new(source_id.finish()),
            Arc::new(source_name.finish()),
            Arc::new(target_id.finish()),
            Arc::new(target_name.finish()),
            Arc::new(ability_id.finish()),
            Arc::new(ability_name.finish()),
            Arc::new(value.finish()),
            Arc::new(threat.finish()),
            Arc::new(is_crit.finish()),
            Arc::new(encounter_idx.finish()),
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
