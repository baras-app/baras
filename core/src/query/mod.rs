//! Query module for analyzing encounter data with DataFusion.
//!
//! Provides SQL queries over:
//! - Live Arrow buffers (current encounter)
//! - Historical parquet files (completed encounters)
mod column_helpers;

use std::path::Path;
use std::sync::Arc;

use datafusion::arrow::array::Array;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::config::ConfigOptions;
use datafusion::datasource::MemTable;
use datafusion::prelude::*;

use crate::game_data::effect_id;
use column_helpers::*;

// Re-export query types from shared types crate
pub use baras_types::{
    AbilityBreakdown, BreakdownMode, CombatLogRow, DataTab, EffectChartData, EffectWindow,
    EncounterTimeline, EntityBreakdown, PhaseSegment, PlayerDeath, RaidOverviewRow, TimeRange,
    TimeSeriesPoint,
};

/// Escape single quotes for SQL string literals (O'Brien -> O''Brien)
fn sql_escape(s: &str) -> String {
    s.replace('\'', "''")
}

// ─────────────────────────────────────────────────────────────────────────────
// Query Context (shared across queries to avoid repeated allocation)
// ─────────────────────────────────────────────────────────────────────────────

/// Identifies what data source is currently registered
#[derive(Debug, Clone, PartialEq, Eq)]
enum RegisteredSource {
    /// No table registered
    None,
    /// Parquet file at the given path
    Parquet(std::path::PathBuf),
    /// Live in-memory batch (changes frequently, always re-register)
    Live,
}

/// Internal state protected by the lock
struct QueryContextState {
    ctx: SessionContext,
    current_source: RegisteredSource,
}

/// Shared query context that manages DataFusion SessionContext lifecycle.
///
/// Key design decisions to minimize memory growth:
/// - Creates a FRESH SessionContext when switching to a different parquet file
///   (this clears all internal DataFusion caches and state)
/// - Reuses context for repeated queries on the same file
/// - Always re-registers for live data (it changes frequently)
pub struct QueryContext {
    /// Lock protecting the session context and current source tracking
    state: tokio::sync::RwLock<QueryContextState>,
}

impl Default for QueryContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a fresh SessionContext with our optimized config
fn create_session_context() -> SessionContext {
    let mut config = ConfigOptions::new();
    config.execution.target_partitions = 2; // Default is num_cpus, way too high for small files
    config.execution.batch_size = 4096; // Default is 8192
    SessionContext::new_with_config(config.into())
}

impl QueryContext {
    pub fn new() -> Self {
        Self {
            state: tokio::sync::RwLock::new(QueryContextState {
                ctx: create_session_context(),
                current_source: RegisteredSource::None,
            }),
        }
    }

    /// Register a parquet file for querying.
    /// - If same file is already registered: no-op (fast path)
    /// - If different file: creates a FRESH SessionContext to clear all caches
    pub async fn register_parquet(&self, path: &Path) -> Result<(), String> {
        // Fast path: check if already registered (read lock only)
        {
            let state = self.state.read().await;
            if let RegisteredSource::Parquet(ref registered_path) = state.current_source {
                if registered_path == path {
                    return Ok(());
                }
            }
        }

        // Slow path: need to register new file
        let mut state = self.state.write().await;

        // Double-check after acquiring write lock
        if let RegisteredSource::Parquet(ref registered_path) = state.current_source {
            if registered_path == path {
                return Ok(());
            }
        }

        // Create a FRESH SessionContext to clear all internal DataFusion state
        // This prevents memory accumulation from cached query plans, statistics, etc.
        state.ctx = create_session_context();

        state
            .ctx
            .register_parquet(
                "events",
                path.to_string_lossy().as_ref(),
                ParquetReadOptions::default(),
            )
            .await
            .map_err(|e| e.to_string())?;

        state.current_source = RegisteredSource::Parquet(path.to_path_buf());
        Ok(())
    }

    /// Register a RecordBatch for querying (live data).
    /// Always re-registers since live data changes frequently.
    pub async fn register_batch(&self, batch: RecordBatch) -> Result<(), String> {
        let mut state = self.state.write().await;

        // For live data, just deregister and re-register (don't create fresh context
        // since this happens frequently during combat)
        let _ = state.ctx.deregister_table("events");

        let schema = batch.schema();
        let mem_table = MemTable::try_new(schema, vec![vec![batch]]).map_err(|e| e.to_string())?;
        state
            .ctx
            .register_table("events", Arc::new(mem_table))
            .map_err(|e| e.to_string())?;

        state.current_source = RegisteredSource::Live;
        Ok(())
    }

    /// Clear all state and create a fresh SessionContext.
    /// Call this when closing the data explorer or switching log directories.
    pub async fn clear(&self) {
        let mut state = self.state.write().await;
        state.ctx = create_session_context();
        state.current_source = RegisteredSource::None;
    }

    /// Create an EncounterQuery that uses the current context.
    /// Acquires a read lock for the duration of the query.
    pub async fn query(&self) -> QueryContextGuard<'_> {
        QueryContextGuard {
            guard: self.state.read().await,
        }
    }
}

/// RAII guard that holds a read lock on the QueryContext
pub struct QueryContextGuard<'a> {
    guard: tokio::sync::RwLockReadGuard<'a, QueryContextState>,
}

impl QueryContextGuard<'_> {
    /// Get an EncounterQuery for executing SQL
    pub fn query(&self) -> EncounterQuery<'_> {
        EncounterQuery { ctx: &self.guard.ctx }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Query Executor
// ─────────────────────────────────────────────────────────────────────────────

pub struct EncounterQuery<'a> {
    ctx: &'a SessionContext,
}

impl EncounterQuery<'_> {
    /// Execute SQL query, returning empty results if table doesn't exist.
    /// This prevents panics when queries are made before parquet data is loaded.
    async fn sql(&self, query: &str) -> Result<Vec<RecordBatch>, String> {
        match self.ctx.sql(query).await {
            Ok(df) => df.collect().await.map_err(|e| e.to_string()),
            Err(e) => {
                let msg = e.to_string();
                // Return empty results for missing table (common during startup or empty encounters)
                if msg.contains("not found") || msg.contains("does not exist") {
                    Ok(vec![])
                } else {
                    Err(msg)
                }
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Query Methods
    // ─────────────────────────────────────────────────────────────────────────

    /// Query ability breakdown for any data tab.
    /// - entity_name: For outgoing tabs (Damage/Healing), filters by source_name.
    ///                For incoming tabs (DamageTaken/HealingTaken), filters by target_name.
    /// - entity_types: Filters by source_entity_type for outgoing, target_entity_type for incoming.
    pub async fn query_breakdown(
        &self,
        tab: DataTab,
        entity_name: Option<&str>,
        time_range: Option<&TimeRange>,
        entity_types: Option<&[&str]>,
        breakdown_mode: Option<&BreakdownMode>,
        duration_secs: Option<f32>,
    ) -> Result<Vec<AbilityBreakdown>, String> {
        let mode = breakdown_mode
            .copied()
            .unwrap_or(BreakdownMode::ability_only());
        let value_col = tab.value_column();
        let is_outgoing = tab.is_outgoing();

        // For outgoing (Damage/Healing): filter/group by source, breakdown by target
        // For incoming (DamageTaken/HealingTaken): filter/group by target, breakdown by source
        let (
            entity_col,
            entity_type_col,
            breakdown_name_col,
            breakdown_class_col,
            breakdown_id_col,
        ) = if is_outgoing {
            (
                "source_name",
                "source_entity_type",
                "target_name",
                "target_class_id",
                "target_id",
            )
        } else {
            (
                "target_name",
                "target_entity_type",
                "source_name",
                "source_class_id",
                "source_id",
            )
        };

        // Build WHERE conditions
        let mut conditions = vec![format!("{} > 0", value_col)];
        if let Some(n) = entity_name {
            conditions.push(format!("{} = '{}'", entity_col, sql_escape(n)));
        }
        if let Some(tr) = time_range {
            conditions.push(tr.sql_filter());
        }
        if let Some(types) = entity_types {
            let type_list = types
                .iter()
                .map(|t| format!("'{}'", sql_escape(t)))
                .collect::<Vec<_>>()
                .join(", ");
            conditions.push(format!("{} IN ({})", entity_type_col, type_list));
        }
        let filter = format!("WHERE {}", conditions.join(" AND "));

        // Build dynamic SELECT and GROUP BY based on breakdown mode
        let mut select_cols = Vec::new();
        let mut group_cols = Vec::new();

        // Ability columns (can be toggled off if grouping by target)
        if mode.by_ability {
            select_cols.push("ability_name".to_string());
            select_cols.push("ability_id".to_string());
            group_cols.push("ability_name".to_string());
            group_cols.push("ability_id".to_string());
        } else {
            // When ability is off, use placeholder values
            select_cols.push("'' as ability_name".to_string());
            select_cols.push("0 as ability_id".to_string());
        }

        // Add breakdown columns (target for outgoing, source for incoming)
        if mode.by_target_type || mode.by_target_instance {
            select_cols.push(breakdown_name_col.to_string());
            group_cols.push(breakdown_name_col.to_string());
        }
        if mode.by_target_type {
            select_cols.push(breakdown_class_col.to_string());
            group_cols.push(breakdown_class_col.to_string());
        }
        if mode.by_target_instance {
            select_cols.push(breakdown_id_col.to_string());
            group_cols.push(breakdown_id_col.to_string());
        }

        // Ensure we have at least one grouping column
        if group_cols.is_empty() {
            // Fallback to ability grouping if nothing selected
            select_cols.clear();
            select_cols.push("ability_name".to_string());
            select_cols.push("ability_id".to_string());
            group_cols.push("ability_name".to_string());
            group_cols.push("ability_id".to_string());
        }

        let select_str = select_cols.join(", ");
        let group_str = group_cols.join(", ");

        // Add first_hit_secs when grouping by instance
        let first_hit_col = if mode.by_target_instance {
            ", MIN(combat_time_secs) as first_hit_secs"
        } else {
            ""
        };

        // Query with window function for percent calculation
        let batches = self
            .sql(&format!(
                r#"
            SELECT {select_str},
                   SUM({value_col}) as total_value,
                   COUNT(*) as hit_count,
                   SUM(CASE WHEN is_crit THEN 1 ELSE 0 END) as crit_count,
                   MAX({value_col}) as max_hit,
                   SUM({value_col}) * 100.0 / SUM(SUM({value_col})) OVER () as percent_of_total
                   {first_hit_col}
            FROM events {filter}
            GROUP BY {group_str}
            ORDER BY total_value DESC
        "#
            ))
            .await?;

        // Use time range duration if provided, otherwise fall back to full fight duration
        let duration = if let Some(tr) = time_range {
            (tr.end - tr.start).max(0.001) as f64
        } else {
            duration_secs.unwrap_or(1.0).max(0.001) as f64
        };

        let mut results = Vec::new();
        for batch in &batches {
            let mut col_idx = 0;
            let names = col_strings(batch, col_idx)?;
            col_idx += 1;
            let ids = col_i64(batch, col_idx)?;
            col_idx += 1;

            // Extract target columns if present
            let target_names = if mode.by_target_type || mode.by_target_instance {
                let v = col_strings(batch, col_idx)?;
                col_idx += 1;
                Some(v)
            } else {
                None
            };
            let target_class_ids = if mode.by_target_type {
                let v = col_i64(batch, col_idx)?;
                col_idx += 1;
                Some(v)
            } else {
                None
            };
            let target_log_ids = if mode.by_target_instance {
                let v = col_i64(batch, col_idx)?;
                col_idx += 1;
                Some(v)
            } else {
                None
            };

            let totals = col_f64(batch, col_idx)?;
            col_idx += 1;
            let hits = col_i64(batch, col_idx)?;
            col_idx += 1;
            let crits = col_i64(batch, col_idx)?;
            col_idx += 1;
            let maxes = col_f64(batch, col_idx)?;
            col_idx += 1;
            let percents = col_f64(batch, col_idx)?;
            col_idx += 1;

            // Extract first_hit_secs if grouping by target instance
            let first_hit_times = if mode.by_target_instance {
                Some(col_f32(batch, col_idx)?)
            } else {
                None
            };

            for i in 0..batch.num_rows() {
                let h = hits[i] as f64;
                results.push(AbilityBreakdown {
                    ability_name: names[i].clone(),
                    ability_id: ids[i],
                    target_name: target_names.as_ref().map(|v| v[i].clone()),
                    target_class_id: target_class_ids.as_ref().map(|v| v[i]),
                    target_log_id: target_log_ids.as_ref().map(|v| v[i]),
                    target_first_hit_secs: first_hit_times.as_ref().map(|v| v[i]),
                    total_value: totals[i],
                    hit_count: hits[i],
                    crit_count: crits[i],
                    crit_rate: if h > 0.0 {
                        crits[i] as f64 / h * 100.0
                    } else {
                        0.0
                    },
                    max_hit: maxes[i],
                    avg_hit: if h > 0.0 { totals[i] / h } else { 0.0 },
                    dps: totals[i] / duration,
                    percent_of_total: percents[i],
                });
            }
        }
        Ok(results)
    }

    /// Query entity breakdown for any data tab.
    /// - For outgoing tabs (Damage/Healing): groups by source entity.
    /// - For incoming tabs (DamageTaken/HealingTaken): groups by target entity (who received).
    pub async fn breakdown_by_entity(
        &self,
        tab: DataTab,
        time_range: Option<&TimeRange>,
    ) -> Result<Vec<EntityBreakdown>, String> {
        let value_col = tab.value_column();
        let is_outgoing = tab.is_outgoing();

        // For outgoing: group by source (who dealt)
        // For incoming: group by target (who received)
        let (name_col, id_col, type_col) = if is_outgoing {
            ("source_name", "source_id", "source_entity_type")
        } else {
            ("target_name", "target_id", "target_entity_type")
        };

        let mut conditions = vec![format!("{} > 0", value_col)];
        if let Some(tr) = time_range {
            conditions.push(tr.sql_filter());
        }
        let filter = format!("WHERE {}", conditions.join(" AND "));

        let batches = self
            .sql(&format!(
                r#"
            SELECT {name_col}, {id_col}, MIN({type_col}) as entity_type,
                   SUM({value_col}) as total_value,
                   COUNT(DISTINCT ability_id) as abilities_used
            FROM events {filter}
            GROUP BY {name_col}, {id_col}
            ORDER BY total_value DESC
        "#
            ))
            .await?;

        let mut results = Vec::new();
        for batch in &batches {
            let names = col_strings(batch, 0)?;
            let ids = col_i64(batch, 1)?;
            let entity_types = col_strings(batch, 2)?;
            let totals = col_f64(batch, 3)?;
            let abilities = col_i64(batch, 4)?;

            for i in 0..batch.num_rows() {
                results.push(EntityBreakdown {
                    source_name: names[i].clone(),
                    source_id: ids[i],
                    entity_type: entity_types[i].clone(),
                    total_value: totals[i],
                    abilities_used: abilities[i],
                });
            }
        }
        Ok(results)
    }

    /// Query raid overview - aggregated stats per player across all metrics.
    /// Returns damage dealt, threat, damage taken, absorbed, and healing for each player.
    pub async fn query_raid_overview(
        &self,
        time_range: Option<&TimeRange>,
        duration_secs: Option<f32>,
    ) -> Result<Vec<RaidOverviewRow>, String> {
        let time_filter = time_range
            .map(|tr| format!("AND {}", tr.sql_filter()))
            .unwrap_or_default();
        let duration = duration_secs.unwrap_or(1.0).max(0.001) as f64;

        // CTE-based query to aggregate multiple metrics per player
        // participants: all unique source names (players who did anything)
        // damage_dealt: sum of dmg_amount WHERE source = player
        // threat: sum of threat WHERE source = player
        // damage_taken: sum of dmg_amount WHERE target = player
        // absorbed: sum of dmg_absorbed WHERE target = player
        // healing: sum of heal_amount WHERE source = player
        let batches = self
            .sql(&format!(
                r#"
            WITH participants AS (
                SELECT DISTINCT source_name as name, source_entity_type as entity_type
                FROM events
                WHERE 1=1 {time_filter}
            ),
            damage_dealt AS (
                SELECT source_name as name,
                       SUM(dmg_amount) as damage_total,
                FROM events
                WHERE dmg_amount > 0 {time_filter}
                GROUP BY source_name
            ),
            damage_taken AS (
                SELECT target_name as name,
                       SUM(dmg_amount) as damage_taken_total,
                       SUM(dmg_absorbed) as absorbed_total
                FROM events
                WHERE dmg_amount > 0 {time_filter}
                GROUP BY target_name
            ),
            healing_done AS (
                SELECT source_name as name,
                       SUM(heal_amount) as healing_total,
                       SUM(heal_effective) as healing_effective
                FROM events
                WHERE heal_amount > 0 {time_filter}
                GROUP BY source_name
            ),
            threat AS (
                SELECT source_name as name,
                    SUM(threat) as threat_total
                FROM events
                WHERE threat > 0 {time_filter}
                GROUP BY source_name
            )
            SELECT
                p.name,
                p.entity_type,
                COALESCE(d.damage_total, 0) as damage_total,
                COALESCE(th.threat_total, 0) as threat_total,
                COALESCE(t.damage_taken_total, 0) as damage_taken_total,
                COALESCE(t.absorbed_total, 0) as absorbed_total,
                COALESCE(h.healing_total, 0) as healing_total,
                COALESCE(h.healing_effective, 0) as healing_effective,
                COALESCE(h.healing_effective * 100.0 / NULLIF(h.healing_total, 0), 0) as healing_pct
            FROM participants p
            LEFT JOIN damage_dealt d ON p.name = d.name
            LEFT JOIN damage_taken t ON p.name = t.name
            LEFT JOIN healing_done h ON p.name = h.name
            LEFT JOIN threat as th ON p.name = th.name
            ORDER BY damage_total DESC
        "#
            ))
            .await?;

        let mut results = Vec::new();
        for batch in &batches {
            let names = col_strings(batch, 0)?;
            let entity_types = col_strings(batch, 1)?;
            let damage_totals = col_f64(batch, 2)?;
            let threat_totals = col_f64(batch, 3)?;
            let damage_taken_totals = col_f64(batch, 4)?;
            let absorbed_totals = col_f64(batch, 5)?;
            let healing_totals = col_f64(batch, 6)?;
            let healing_effectives = col_f64(batch, 7)?;
            let healing_pcts = col_f64(batch, 8)?;

            for i in 0..batch.num_rows() {
                results.push(RaidOverviewRow {
                    name: names[i].clone(),
                    entity_type: entity_types[i].clone(),
                    class_name: None,
                    discipline_name: None,
                    class_icon: None,
                    role_icon: None,
                    damage_total: damage_totals[i],
                    dps: damage_totals[i] / duration,
                    threat_total: threat_totals[i],
                    tps: threat_totals[i] / duration,
                    damage_taken_total: damage_taken_totals[i],
                    dtps: damage_taken_totals[i] / duration,
                    aps: absorbed_totals[i] / duration,
                    healing_total: healing_totals[i],
                    hps: healing_totals[i] / duration,
                    healing_effective: healing_effectives[i],
                    ehps: healing_effectives[i] / duration,
                    healing_pct: healing_pcts[i],
                });
            }
        }
        Ok(results)
    }

    pub async fn dps_over_time(
        &self,
        bucket_ms: i64,
        source_name: Option<&str>,
        time_range: Option<&TimeRange>,
    ) -> Result<Vec<TimeSeriesPoint>, String> {
        let bucket_secs = (bucket_ms as f64 / 1000.0).max(1.0);
        let mut conditions = vec!["combat_time_secs IS NOT NULL".to_string()];
        if let Some(tr) = time_range {
            conditions.push(tr.sql_filter());
        }
        let tr_filter = format!("WHERE {}", conditions.join(" AND "));

        if let Some(n) = source_name {
            conditions.push(format!("source_name = '{}'", sql_escape(n)));
        }
        let filter = format!("WHERE {}", conditions.join(" AND "));

        let batches = self.sql(&format!(r#"
  WITH bounds AS (
      SELECT
          CAST(MIN(FLOOR(combat_time_secs / {bucket_secs})) as BIGINT) as min_bucket,
          CAST(MAX(FLOOR(combat_time_secs / {bucket_secs})) as BIGINT) as max_bucket
      FROM events
      {tr_filter}
  ),
  time_series AS (
      SELECT
        unnest(generate_series(bounds.min_bucket, bounds.max_bucket, 1)) * {bucket_secs} * 1000 AS bucket_start_ms
      FROM bounds
  ),
  entity_ts AS (
      SELECT CAST(FLOOR(combat_time_secs / {bucket_secs}) * {bucket_secs} * 1000 AS BIGINT) as bucket_start_ms,
             SUM(dmg_amount) as total_value
      FROM events
      {filter}
      GROUP BY bucket_start_ms
  )
  SELECT
      time_series.bucket_start_ms,
      COALESCE(entity_ts.total_value, 0) as total_value
  FROM time_series
  LEFT JOIN entity_ts ON time_series.bucket_start_ms = entity_ts.bucket_start_ms
  ORDER BY time_series.bucket_start_ms
          "#)).await?;

        let mut results = Vec::new();
        for batch in &batches {
            let buckets = col_i64(batch, 0)?;
            let values = col_f64(batch, 1)?;
            for i in 0..batch.num_rows() {
                results.push(TimeSeriesPoint {
                    bucket_start_ms: buckets[i],
                    total_value: values[i],
                });
            }
        }
        Ok(results)
    }

    /// Query HPS (healing per second) over time, bucketed by time interval.
    pub async fn hps_over_time(
        &self,
        bucket_ms: i64,
        source_name: Option<&str>,
        time_range: Option<&TimeRange>,
    ) -> Result<Vec<TimeSeriesPoint>, String> {
        let bucket_secs = (bucket_ms as f64 / 1000.0).max(1.0);
        let mut conditions = vec!["combat_time_secs IS NOT NULL".to_string()];
        if let Some(tr) = time_range {
            conditions.push(tr.sql_filter());
        }
        let tr_filter = format!("WHERE {}", conditions.join(" AND "));

        if let Some(n) = source_name {
            conditions.push(format!("source_name = '{}'", sql_escape(n)));
        }
        let filter = format!("WHERE {}", conditions.join(" AND "));

        let batches = self.sql(&format!(r#"
  WITH bounds AS (
      SELECT
          CAST(MIN(FLOOR(combat_time_secs / {bucket_secs})) as BIGINT) as min_bucket,
          CAST(MAX(FLOOR(combat_time_secs / {bucket_secs})) as BIGINT) as max_bucket
      FROM events
      {tr_filter}
  ),
  time_series AS (
      SELECT
        unnest(generate_series(bounds.min_bucket, bounds.max_bucket, 1)) * {bucket_secs} * 1000 AS bucket_start_ms
      FROM bounds
  ),
  entity_ts AS (
      SELECT CAST(FLOOR(combat_time_secs / {bucket_secs}) * {bucket_secs} * 1000 AS BIGINT) as bucket_start_ms,
             SUM(heal_amount) as total_value
      FROM events
      {filter}
      GROUP BY bucket_start_ms
  )
  SELECT
      time_series.bucket_start_ms,
      COALESCE(entity_ts.total_value, 0) as total_value
  FROM time_series
  LEFT JOIN entity_ts ON time_series.bucket_start_ms = entity_ts.bucket_start_ms
  ORDER BY time_series.bucket_start_ms
          "#)).await?;

        let mut results = Vec::new();
        for batch in &batches {
            let buckets = col_i64(batch, 0)?;
            let values = col_f64(batch, 1)?;
            for i in 0..batch.num_rows() {
                results.push(TimeSeriesPoint {
                    bucket_start_ms: buckets[i],
                    total_value: values[i],
                });
            }
        }
        Ok(results)
    }

    /// Query DTPS (damage taken per second) over time for a target entity.
    pub async fn dtps_over_time(
        &self,
        bucket_ms: i64,
        target_name: Option<&str>,
        time_range: Option<&TimeRange>,
    ) -> Result<Vec<TimeSeriesPoint>, String> {
        let bucket_secs = (bucket_ms as f64 / 1000.0).max(1.0);
        let mut conditions = vec!["combat_time_secs IS NOT NULL".to_string()];

        if let Some(tr) = time_range {
            conditions.push(tr.sql_filter());
        }

        let tr_filter = format!("WHERE {}", conditions.join(" AND "));

        if let Some(n) = target_name {
            conditions.push(format!("target_name = '{}'", sql_escape(n)));
        }
        let filter = format!("WHERE {}", conditions.join(" AND "));

        let batches = self.sql(&format!(r#"
  WITH bounds AS (
      SELECT
          CAST(MIN(FLOOR(combat_time_secs / {bucket_secs})) as BIGINT) as min_bucket,
          CAST(MAX(FLOOR(combat_time_secs / {bucket_secs})) as BIGINT) as max_bucket
      FROM events
      {tr_filter}
  ),
  time_series AS (
      SELECT
        unnest(generate_series(bounds.min_bucket, bounds.max_bucket, 1)) * {bucket_secs} * 1000 AS bucket_start_ms
      FROM bounds
  ),
  entity_ts AS (
      SELECT CAST(FLOOR(combat_time_secs / {bucket_secs}) * {bucket_secs} * 1000 AS BIGINT) as bucket_start_ms,
             SUM(dmg_amount) as total_value
      FROM events
      {filter}
      GROUP BY bucket_start_ms
  )
  SELECT
      time_series.bucket_start_ms,
      COALESCE(entity_ts.total_value, 0) as total_value
  FROM time_series
  LEFT JOIN entity_ts ON time_series.bucket_start_ms = entity_ts.bucket_start_ms
  ORDER BY time_series.bucket_start_ms
          "#)).await?;

        let mut results = Vec::new();
        for batch in &batches {
            let buckets = col_i64(batch, 0)?;
            let values = col_f64(batch, 1)?;
            for i in 0..batch.num_rows() {
                results.push(TimeSeriesPoint {
                    bucket_start_ms: buckets[i],
                    total_value: values[i],
                });
            }
        }
        Ok(results)
    }

    /// Get encounter timeline with phase segments (handles repeated phases).
    pub async fn encounter_timeline(&self) -> Result<EncounterTimeline, String> {
        // Calculate duration from combat_time_secs (only includes actual combat events)
        let duration_secs = scalar_f32(&self.sql(
            "SELECT COALESCE(MAX(combat_time_secs), 0) FROM events WHERE combat_time_secs IS NOT NULL"
        ).await?);

        // Window functions to detect phase transitions and number instances
        // Filter: phase_id must be non-null AND non-empty string
        let batches = self
            .sql(
                r#"
            WITH filtered AS (
                SELECT combat_time_secs, phase_id, phase_name
                FROM events
                WHERE phase_id IS NOT NULL
                  AND phase_id != ''
                  AND combat_time_secs IS NOT NULL
            ),
            transitions AS (
                SELECT combat_time_secs, phase_id, phase_name,
                       CASE WHEN phase_id != LAG(phase_id) OVER (ORDER BY combat_time_secs)
                                 OR LAG(phase_id) OVER (ORDER BY combat_time_secs) IS NULL
                            THEN 1 ELSE 0 END as is_new
                FROM filtered
            ),
            segments AS (
                SELECT *, SUM(is_new) OVER (ORDER BY combat_time_secs) as seg_id FROM transitions
            ),
            bounds AS (
                SELECT phase_id, phase_name, seg_id,
                       MIN(combat_time_secs) as start_secs, MAX(combat_time_secs) as end_secs
                FROM segments GROUP BY phase_id, phase_name, seg_id
            ),
            valid_bounds AS (
                SELECT * FROM bounds WHERE start_secs < end_secs
            )
            SELECT phase_id, phase_name,
                   ROW_NUMBER() OVER (PARTITION BY phase_id ORDER BY seg_id) as instance,
                   start_secs, end_secs
            FROM valid_bounds
            ORDER BY start_secs
        "#,
            )
            .await?;

        let mut phases = Vec::new();
        for batch in &batches {
            let ids = col_strings(batch, 0)?;
            let names = col_strings(batch, 1)?;
            let instances = col_i64(batch, 2)?;
            let starts = col_f32(batch, 3)?;
            let ends = col_f32(batch, 4)?;

            for i in 0..batch.num_rows() {
                phases.push(PhaseSegment {
                    phase_id: ids[i].clone(),
                    phase_name: names[i].clone(),
                    instance: instances[i],
                    start_secs: starts[i],
                    end_secs: ends[i],
                });
            }
        }

        Ok(EncounterTimeline {
            duration_secs,
            phases,
        })
    }

    /// Query effect uptime statistics for the charts panel.
    /// Returns aggregated data per effect (count, duration, uptime%).
    /// Effects are classified as active (triggered by ability) or passive (proc/auto-applied).
    pub async fn query_effect_uptime(
        &self,
        target_name: Option<&str>,
        time_range: Option<&TimeRange>,
        duration_secs: f32,
    ) -> Result<Vec<EffectChartData>, String> {
        // Effect type IDs (the type of log event)
        const APPLY_EFFECT: i64 = 836045448945477;
        const REMOVE_EFFECT: i64 = 836045448945478;
        // Effect IDs (what specifically happened)
        const ABILITY_ACTIVATE: i64 = 836045448945479;
        // Exclude damage/heal "effects" which are action results, not buffs
        const DAMAGE_EFFECT: i64 = 836045448945501;
        const HEAL_EFFECT: i64 = 836045448945500;

        let target_filter = target_name
            .map(|n| format!("AND target_name = '{}'", sql_escape(n)))
            .unwrap_or_default();
        let time_filter = time_range
            .map(|tr| format!("AND {}", tr.sql_filter()))
            .unwrap_or_default();
        let duration = duration_secs.max(0.001);

        // Query pairs ApplyEffect with RemoveEffect events, calculates duration,
        // and detects active vs passive based on whether there's an AbilityActivate
        // event at the exact same timestamp (1:1 match in logs).
        // Group by effect_name only (not effect_id) to consolidate variants.
        // Cap uptime at 100% since overlapping windows aren't merged.
        let batches = self.sql(&format!(r#"
            WITH applies AS (
                SELECT effect_id, effect_name, target_name, combat_time_secs as apply_time, timestamp,
                       ROW_NUMBER() OVER (PARTITION BY effect_id, target_name ORDER BY combat_time_secs) as seq
                FROM events
                WHERE effect_type_id = {APPLY_EFFECT}
                  AND effect_id NOT IN ({DAMAGE_EFFECT}, {HEAL_EFFECT})
                  {target_filter}
                  {time_filter}
            ),
            removes AS (
                SELECT effect_id, target_name, combat_time_secs as remove_time,
                       ROW_NUMBER() OVER (PARTITION BY effect_id, target_name ORDER BY combat_time_secs) as seq
                FROM events
                WHERE effect_type_id = {REMOVE_EFFECT}
                  AND effect_id NOT IN ({DAMAGE_EFFECT}, {HEAL_EFFECT})
                  {target_filter}
                  {time_filter}
            ),
            ability_activations AS (
                SELECT DISTINCT timestamp as activation_ts, ability_id
                FROM events
                WHERE effect_id = {ABILITY_ACTIVATE}
                  {time_filter}
            ),
            paired AS (
                SELECT a.effect_id, a.effect_name, a.apply_time, a.timestamp,
                       COALESCE(r.remove_time, {duration}) as remove_time
                FROM applies a
                LEFT JOIN removes r ON a.effect_id = r.effect_id
                    AND a.target_name = r.target_name
                    AND a.seq = r.seq
            ),
            classified AS (
                SELECT p.effect_id, p.effect_name, p.apply_time,
                       LEAST(p.remove_time, {duration}) - p.apply_time as duration_secs,
                       CASE WHEN aa.activation_ts IS NOT NULL THEN true ELSE false END as is_active,
                       aa.ability_id
                FROM paired p
                LEFT JOIN ability_activations aa ON p.timestamp = aa.activation_ts
                    AND p.effect_id = aa.ability_id
                WHERE p.remove_time > p.apply_time
            ),
            aggregated AS (
                SELECT MIN(effect_id) as effect_id, effect_name, is_active,
                       MIN(ability_id) as ability_id,
                       COUNT(*) as count,
                       SUM(duration_secs) as total_duration
                FROM classified
                GROUP BY effect_name, is_active
            )
            SELECT effect_id, effect_name, ability_id, is_active, count, total_duration,
                   LEAST(total_duration * 100.0 / {duration}, 100.0) as uptime_pct
            FROM aggregated
            ORDER BY total_duration DESC
        "#)).await?;

        let mut results = Vec::new();
        for batch in &batches {
            let effect_ids = col_i64(batch, 0)?;
            let effect_names = col_strings(batch, 1)?;
            // ability_id is nullable (NULL for passive effects)
            let ability_ids: Vec<Option<i64>> = {
                let col = batch.column(2);
                if let Some(a) = col.as_any().downcast_ref::<arrow::array::Int64Array>() {
                    (0..a.len())
                        .map(|i| if a.is_null(i) { None } else { Some(a.value(i)) })
                        .collect()
                } else {
                    vec![None; batch.num_rows()]
                }
            };
            // is_active comes as a boolean, but DataFusion might return it as various types
            let is_actives: Vec<bool> = {
                let col = batch.column(3);
                if let Some(a) = col.as_any().downcast_ref::<arrow::array::BooleanArray>() {
                    (0..a.len()).map(|i| a.value(i)).collect()
                } else {
                    // Fallback: treat as all passive
                    vec![false; batch.num_rows()]
                }
            };
            let counts = col_i64(batch, 4)?;
            let total_durations = col_f32(batch, 5)?;
            let uptime_pcts = col_f32(batch, 6)?;

            for i in 0..batch.num_rows() {
                results.push(EffectChartData {
                    effect_id: effect_ids[i],
                    effect_name: effect_names[i].clone(),
                    ability_id: ability_ids[i],
                    is_active: is_actives[i],
                    count: counts[i],
                    total_duration_secs: total_durations[i],
                    uptime_pct: uptime_pcts[i],
                });
            }
        }
        Ok(results)
    }

    /// Query individual time windows for a specific effect (for chart highlighting).
    pub async fn query_effect_windows(
        &self,
        effect_id: i64,
        target_name: Option<&str>,
        time_range: Option<&TimeRange>,
        duration_secs: f32,
    ) -> Result<Vec<EffectWindow>, String> {
        const APPLY_EFFECT: i64 = 836045448945477;
        const REMOVE_EFFECT: i64 = 836045448945478;

        let target_filter = target_name
            .map(|n| format!("AND target_name = '{}'", sql_escape(n)))
            .unwrap_or_default();
        let time_filter = time_range
            .map(|tr| format!("AND {}", tr.sql_filter()))
            .unwrap_or_default();
        let duration = duration_secs.max(0.001);

        let batches = self
            .sql(&format!(
                r#"
            WITH applies AS (
                SELECT combat_time_secs as apply_time, target_name,
                       ROW_NUMBER() OVER (PARTITION BY target_name ORDER BY combat_time_secs) as seq
                FROM events
                WHERE effect_type_id = {APPLY_EFFECT}
                  AND effect_id = {effect_id}
                  {target_filter}
                  {time_filter}
            ),
            removes AS (
                SELECT combat_time_secs as remove_time, target_name,
                       ROW_NUMBER() OVER (PARTITION BY target_name ORDER BY combat_time_secs) as seq
                FROM events
                WHERE effect_type_id = {REMOVE_EFFECT}
                  AND effect_id = {effect_id}
                  {target_filter}
                  {time_filter}
            )
            SELECT a.apply_time as start_secs,
                   LEAST(COALESCE(r.remove_time, {duration}), {duration}) as end_secs
            FROM applies a
            LEFT JOIN removes r ON a.target_name = r.target_name AND a.seq = r.seq
            WHERE COALESCE(r.remove_time, {duration}) > a.apply_time
            ORDER BY start_secs
        "#
            ))
            .await?;

        let mut results = Vec::new();
        for batch in &batches {
            let starts = col_f32(batch, 0)?;
            let ends = col_f32(batch, 1)?;
            for i in 0..batch.num_rows() {
                results.push(EffectWindow {
                    start_secs: starts[i],
                    end_secs: ends[i],
                });
            }
        }
        Ok(results)
    }

    /// Query combat log rows for the combat log viewer.
    /// Supports pagination via offset/limit for virtual scrolling.
    /// Returns rows ordered by combat_time_secs.
    pub async fn query_combat_log(
        &self,
        offset: u64,
        limit: u64,
        source_filter: Option<&str>,
        target_filter: Option<&str>,
        search_filter: Option<&str>,
        time_range: Option<&TimeRange>,
    ) -> Result<Vec<CombatLogRow>, String> {
        let mut where_clauses = vec!["combat_time_secs IS NOT NULL".to_string()];

        if let Some(source) = source_filter {
            where_clauses.push(format!("source_name = '{}'", sql_escape(source)));
        }
        if let Some(target) = target_filter {
            where_clauses.push(format!("target_name = '{}'", sql_escape(target)));
        }
        if let Some(search) = search_filter {
            let escaped = sql_escape(search);
            where_clauses.push(format!(
                "(source_name LIKE '%{0}%' OR target_name LIKE '%{0}%' OR ability_name LIKE '%{0}%' OR effect_name LIKE '%{0}%')",
                escaped
            ));
        }
        if let Some(tr) = time_range {
            where_clauses.push(tr.sql_filter());
        }

        let where_clause = where_clauses.join(" AND ");

        let batches = self
            .sql(&format!(
                r#"
            SELECT
                line_number,
                combat_time_secs,
                source_name,
                source_entity_type,
                target_name,
                target_entity_type,
                effect_type_name,
                ability_name,
                ability_id,
                effect_name,
                COALESCE(dmg_effective, 0) + COALESCE(heal_effective, 0) as value,
                COALESCE(dmg_absorbed, 0) as absorbed,
                GREATEST(COALESCE(heal_amount, 0) - COALESCE(heal_effective, 0), 0) as overheal,
                COALESCE(threat, 0.0) as threat,
                is_crit,
                COALESCE(dmg_type, '') as damage_type,
                COALESCE(defense_type_id, 0) as defense_type_id
            FROM events
            WHERE {where_clause}
            ORDER BY combat_time_secs
            LIMIT {limit} OFFSET {offset}
        "#
            ))
            .await?;

        let mut results = Vec::new();
        for batch in &batches {
            let line_numbers = col_i64(batch, 0)?;
            let times = col_f32(batch, 1)?;
            let source_names = col_strings(batch, 2)?;
            let source_types = col_strings(batch, 3)?;
            let target_names = col_strings(batch, 4)?;
            let target_types = col_strings(batch, 5)?;
            let effect_types = col_strings(batch, 6)?;
            let ability_names = col_strings(batch, 7)?;
            let ability_ids = col_i64(batch, 8)?;
            let effect_names = col_strings(batch, 9)?;
            let values = col_i32(batch, 10)?;
            let absorbeds = col_i32(batch, 11)?;
            let overheals = col_i32(batch, 12)?;
            let threats = col_f32(batch, 13)?;
            let is_crits = col_bool(batch, 14)?;
            let damage_types = col_strings(batch, 15)?;
            let defense_type_ids = col_i64(batch, 16)?;

            for i in 0..batch.num_rows() {
                results.push(CombatLogRow {
                    row_idx: line_numbers[i] as u64,
                    time_secs: times[i],
                    source_name: source_names[i].clone(),
                    source_type: source_types[i].clone(),
                    target_name: target_names[i].clone(),
                    target_type: target_types[i].clone(),
                    effect_type: effect_types[i].clone(),
                    ability_name: ability_names[i].clone(),
                    ability_id: ability_ids[i],
                    effect_name: effect_names[i].clone(),
                    value: values[i],
                    absorbed: absorbeds[i],
                    overheal: overheals[i],
                    threat: threats[i],
                    is_crit: is_crits[i],
                    damage_type: damage_types[i].clone(),
                    defense_type_id: defense_type_ids[i],
                });
            }
        }
        Ok(results)
    }

    /// Get total count of combat log rows (for pagination).
    pub async fn query_combat_log_count(
        &self,
        source_filter: Option<&str>,
        target_filter: Option<&str>,
        search_filter: Option<&str>,
        time_range: Option<&TimeRange>,
    ) -> Result<u64, String> {
        let mut where_clauses = vec!["combat_time_secs IS NOT NULL".to_string()];

        if let Some(source) = source_filter {
            where_clauses.push(format!("source_name = '{}'", sql_escape(source)));
        }
        if let Some(target) = target_filter {
            where_clauses.push(format!("target_name = '{}'", sql_escape(target)));
        }
        if let Some(search) = search_filter {
            let escaped = sql_escape(search);
            where_clauses.push(format!(
                "(source_name LIKE '%{0}%' OR target_name LIKE '%{0}%' OR ability_name LIKE '%{0}%' OR effect_name LIKE '%{0}%')",
                escaped
            ));
        }
        if let Some(tr) = time_range {
            where_clauses.push(tr.sql_filter());
        }

        let where_clause = where_clauses.join(" AND ");

        let batches = self
            .sql(&format!("SELECT COUNT(*) FROM events WHERE {where_clause}"))
            .await?;

        let count = batches
            .first()
            .and_then(|b| col_i64(b, 0).ok())
            .and_then(|v| v.first().copied())
            .unwrap_or(0) as u64;

        Ok(count)
    }

    /// Get distinct source names for filter dropdown.
    pub async fn query_source_names(&self) -> Result<Vec<String>, String> {
        let batches = self.sql(
            "SELECT DISTINCT source_name FROM events WHERE combat_time_secs IS NOT NULL ORDER BY source_name"
        ).await?;

        let mut results = Vec::new();
        for batch in &batches {
            results.extend(col_strings(batch, 0)?);
        }
        Ok(results)
    }

    /// Get distinct target names for filter dropdown.
    pub async fn query_target_names(&self) -> Result<Vec<String>, String> {
        let batches = self.sql(
            "SELECT DISTINCT target_name FROM events WHERE combat_time_secs IS NOT NULL ORDER BY target_name"
        ).await?;

        let mut results = Vec::new();
        for batch in &batches {
            results.extend(col_strings(batch, 0)?);
        }
        Ok(results)
    }

    /// Query player deaths in the encounter.
    /// Returns a list of player deaths ordered by time.
    pub async fn query_player_deaths(&self) -> Result<Vec<PlayerDeath>, String> {
        // Death events are identified by effect_id::DEATH
        // and target_entity_type = 'Player' or 'Companion'
        let sql = format!(
            r#"
            SELECT
                target_name,
                combat_time_secs
            FROM events
            WHERE effect_id = {}
              AND (target_entity_type = 'Player' OR target_entity_type = 'Companion')
              AND combat_time_secs IS NOT NULL
            ORDER BY combat_time_secs ASC
            "#,
            effect_id::DEATH
        );

        let batches = self.sql(&sql).await?;

        let mut results = Vec::new();
        for batch in &batches {
            let names = col_strings(batch, 0)?;
            let times = col_f32(batch, 1)?;

            for (name, time) in names.into_iter().zip(times) {
                results.push(PlayerDeath {
                    name,
                    death_time_secs: time,
                });
            }
        }
        Ok(results)
    }
}
