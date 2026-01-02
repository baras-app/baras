//! Query module for analyzing encounter data with DataFusion.
//!
//! Provides SQL and DataFrame-based queries over:
//! - Live Arrow buffers (current encounter)
//! - Historical parquet files (completed encounters)

use std::path::Path;
use std::sync::Arc;

use arrow::record_batch::RecordBatch;
use datafusion::prelude::*;
use datafusion::datasource::MemTable;
use serde::Serialize;

use crate::storage::EncounterWriter;

/// Query result for damage/healing by ability.
#[derive(Debug, Clone, Serialize)]
pub struct AbilityBreakdown {
    pub ability_name: String,
    pub ability_id: i64,
    pub total_value: f64,
    pub hit_count: i64,
    pub crit_count: i64,
    pub crit_rate: f64,
    pub max_hit: f64,
    pub avg_hit: f64,
}

/// Query result for damage/healing by source entity.
#[derive(Debug, Clone, Serialize)]
pub struct EntityBreakdown {
    pub source_name: String,
    pub source_id: i64,
    pub total_value: f64,
    pub abilities_used: i64,
}

/// Query result for DPS/HPS over time (bucketed).
#[derive(Debug, Clone, Serialize)]
pub struct TimeSeriesPoint {
    pub bucket_start_ms: i64,
    pub total_value: f64,
}

/// Encounter query context - wraps DataFusion SessionContext.
pub struct EncounterQuery {
    ctx: SessionContext,
}

impl EncounterQuery {
    /// Create a new query context.
    pub fn new() -> Self {
        Self {
            ctx: SessionContext::new(),
        }
    }

    /// Register a live encounter buffer for querying.
    pub async fn register_live(&self, writer: &EncounterWriter) -> Result<(), String> {
        let batch = writer.to_record_batch()
            .ok_or("No data in live buffer")?;

        let schema = batch.schema();
        let mem_table = MemTable::try_new(schema, vec![vec![batch]])
            .map_err(|e| e.to_string())?;

        self.ctx.register_table("events", Arc::new(mem_table))
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Register a RecordBatch directly for querying.
    pub async fn register_batch(&self, batch: RecordBatch) -> Result<(), String> {
        let schema = batch.schema();
        let mem_table = MemTable::try_new(schema, vec![vec![batch]])
            .map_err(|e| e.to_string())?;

        self.ctx.register_table("events", Arc::new(mem_table))
            .map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Register a parquet file for querying.
    pub async fn register_parquet(&self, path: &Path) -> Result<(), String> {
        self.ctx.register_parquet(
            "events",
            path.to_string_lossy().as_ref(),
            ParquetReadOptions::default()
        ).await.map_err(|e| e.to_string())?;

        Ok(())
    }

    /// Execute a raw SQL query and return results as JSON-serializable rows.
    pub async fn sql(&self, query: &str) -> Result<Vec<RecordBatch>, String> {
        let df = self.ctx.sql(query).await.map_err(|e| e.to_string())?;
        df.collect().await.map_err(|e| e.to_string())
    }

    /// Get damage breakdown by ability for a specific source.
    pub async fn damage_by_ability(&self, source_name: Option<&str>) -> Result<Vec<AbilityBreakdown>, String> {
        let filter = source_name
            .map(|n| format!("WHERE source_name = '{}' AND value > 0", n))
            .unwrap_or_else(|| "WHERE value > 0".to_string());

        let query = format!(
            r#"
            SELECT
                ability_name,
                ability_id,
                SUM(value) as total_value,
                COUNT(*) as hit_count,
                SUM(CASE WHEN is_crit THEN 1 ELSE 0 END) as crit_count,
                MAX(value) as max_hit
            FROM events
            {}
            GROUP BY ability_name, ability_id
            ORDER BY total_value DESC
            "#,
            filter
        );

        let batches = self.sql(&query).await?;
        Self::parse_ability_breakdown(&batches)
    }

    /// Get total damage/healing by source entity.
    pub async fn breakdown_by_entity(&self) -> Result<Vec<EntityBreakdown>, String> {
        let query = r#"
            SELECT
                source_name,
                source_id,
                SUM(value) as total_value,
                COUNT(DISTINCT ability_id) as abilities_used
            FROM events
            WHERE value > 0
            GROUP BY source_name, source_id
            ORDER BY total_value DESC
        "#;

        let batches = self.sql(query).await?;
        Self::parse_entity_breakdown(&batches)
    }

    /// Get DPS over time with specified bucket size in milliseconds.
    pub async fn dps_over_time(&self, bucket_ms: i64, source_name: Option<&str>) -> Result<Vec<TimeSeriesPoint>, String> {
        let filter = source_name
            .map(|n| format!("AND source_name = '{}'", n))
            .unwrap_or_default();

        let query = format!(
            r#"
            SELECT
                (CAST(timestamp AS BIGINT) / {bucket}) * {bucket} as bucket_start_ms,
                SUM(value) as total_value
            FROM events
            WHERE value > 0 {filter}
            GROUP BY bucket_start_ms
            ORDER BY bucket_start_ms
            "#,
            bucket = bucket_ms,
            filter = filter
        );

        let batches = self.sql(&query).await?;
        Self::parse_time_series(&batches)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Result Parsing Helpers
    // ─────────────────────────────────────────────────────────────────────────

    /// Extract strings from any Arrow string array type (Utf8, LargeUtf8, Utf8View).
    /// DataFusion 45+ defaults to Utf8View for query results.
    fn extract_strings(array: &dyn arrow::array::Array) -> Result<Vec<String>, String> {
        use arrow::array::{Array, StringArray, LargeStringArray, StringViewArray};

        if let Some(arr) = array.as_any().downcast_ref::<StringViewArray>() {
            Ok((0..arr.len()).map(|i| arr.value(i).to_string()).collect())
        } else if let Some(arr) = array.as_any().downcast_ref::<StringArray>() {
            Ok((0..arr.len()).map(|i| arr.value(i).to_string()).collect())
        } else if let Some(arr) = array.as_any().downcast_ref::<LargeStringArray>() {
            Ok((0..arr.len()).map(|i| arr.value(i).to_string()).collect())
        } else {
            Err(format!("Expected string type, got {:?}", array.data_type()))
        }
    }

    fn parse_ability_breakdown(batches: &[RecordBatch]) -> Result<Vec<AbilityBreakdown>, String> {
        use arrow::array::{Array, Float64Array, Int64Array};

        let mut results = Vec::new();

        for batch in batches {
            let ability_names = Self::extract_strings(batch.column(0))?;

            let ability_id = batch.column(1).as_any().downcast_ref::<Int64Array>()
                .ok_or("Failed to read ability_id")?;
            let total_value = batch.column(2).as_any().downcast_ref::<Float64Array>()
                .ok_or("Failed to read total_value")?;
            let hit_count = batch.column(3).as_any().downcast_ref::<Int64Array>()
                .ok_or("Failed to read hit_count")?;
            let crit_count = batch.column(4).as_any().downcast_ref::<Int64Array>()
                .ok_or("Failed to read crit_count")?;
            let max_hit = batch.column(5).as_any().downcast_ref::<Float64Array>()
                .ok_or("Failed to read max_hit")?;

            for i in 0..batch.num_rows() {
                let hits = hit_count.value(i) as f64;
                let crits = crit_count.value(i) as f64;
                let total = total_value.value(i);

                results.push(AbilityBreakdown {
                    ability_name: ability_names[i].clone(),
                    ability_id: ability_id.value(i),
                    total_value: total,
                    hit_count: hit_count.value(i),
                    crit_count: crit_count.value(i),
                    crit_rate: if hits > 0.0 { crits / hits * 100.0 } else { 0.0 },
                    max_hit: max_hit.value(i),
                    avg_hit: if hits > 0.0 { total / hits } else { 0.0 },
                });
            }
        }

        Ok(results)
    }

    fn parse_entity_breakdown(batches: &[RecordBatch]) -> Result<Vec<EntityBreakdown>, String> {
        use arrow::array::{Array, Float64Array, Int64Array};

        let mut results = Vec::new();

        for batch in batches {
            let source_names = Self::extract_strings(batch.column(0))?;

            let source_id = batch.column(1).as_any().downcast_ref::<Int64Array>()
                .ok_or("Failed to read source_id")?;
            let total_value = batch.column(2).as_any().downcast_ref::<Float64Array>()
                .ok_or("Failed to read total_value")?;
            let abilities_used = batch.column(3).as_any().downcast_ref::<Int64Array>()
                .ok_or("Failed to read abilities_used")?;

            for i in 0..batch.num_rows() {
                results.push(EntityBreakdown {
                    source_name: source_names[i].clone(),
                    source_id: source_id.value(i),
                    total_value: total_value.value(i),
                    abilities_used: abilities_used.value(i),
                });
            }
        }

        Ok(results)
    }

    fn parse_time_series(batches: &[RecordBatch]) -> Result<Vec<TimeSeriesPoint>, String> {
        use arrow::array::{Array, Float64Array, Int64Array};

        let mut results = Vec::new();

        for batch in batches {
            let bucket_start = batch.column(0).as_any().downcast_ref::<Int64Array>()
                .ok_or("Failed to read bucket_start_ms")?;
            let total_value = batch.column(1).as_any().downcast_ref::<Float64Array>()
                .ok_or("Failed to read total_value")?;

            for i in 0..batch.num_rows() {
                results.push(TimeSeriesPoint {
                    bucket_start_ms: bucket_start.value(i),
                    total_value: total_value.value(i),
                });
            }
        }

        Ok(results)
    }
}

impl Default for EncounterQuery {
    fn default() -> Self {
        Self::new()
    }
}
