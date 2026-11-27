//! Query module for analyzing encounter data with DataFusion.
//!
//! Provides SQL queries over:
//! - Live Arrow buffers (current encounter)
//! - Historical parquet files (completed encounters)

mod breakdown;
mod column_helpers;
mod combat_log;
mod death_recap;
mod effects;
mod enemies;
pub mod error;
mod gcd;
mod overview;
mod raid_debuffs;
mod recap_defensives;
mod replay;
mod replay_spans;
mod rotation;
mod search_parser;
mod threat;
mod time_series;
mod timeline;
mod uptime;
mod usage;

pub use error::QueryError;

use std::path::Path;
use std::sync::Arc;

use datafusion::arrow::compute::concat_batches;
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::datasource::MemTable;
use datafusion::prelude::*;

use crate::storage::EncounterWriter;
use column_helpers::*;

// Re-export query types from shared types crate
pub use baras_types::{
    AbilityBreakdown, AbilityUsageRow, BreakdownMode, CombatLogFilters, CombatLogFindMatch, CombatLogRow,
    CombatLogSortColumn, DamageTakenSummary, DataTab, DeathRecap, DeathRecapDamageRow, DeathRecapDamageSource,
    DeathRecapDefensive, DeathRecapEffect, DeathRecapEvent, DeathRecapEventKind, DeathRecapHealRow,
    DeathRecapHealSource, DeathRecapHpPoint, DeathRecapKillingBlow, EffectChartData, EffectUptimeCell, EffectWindow,
    EncounterTimeline, EntityBreakdown, GcdSlot, GroupMetric, GroupTimeSeries, GroupedEntityNames, HpPoint,
    NpcHealthRow, PhaseSegment, PlayerDeath, RaidDebuffReport, RaidOverviewRow, ReplayData, RotationAnalysis,
    RotationCycle, RotationEvent, SortDirection, ThreatReport, TimeRange, TimeSeriesPoint,
};

/// Escape single quotes for SQL string literals (O'Brien -> O''Brien)
fn sql_escape(s: &str) -> String {
    s.replace('\'', "''")
}

/// Predicate for an entity selector on one side of an event (`prefix` is
/// "source" or "target"). A selector is a plain entity name, or `id:<instance>`
/// to pin a single NPC spawn among same-named copies.
fn entity_match_sql(prefix: &str, selector: &str) -> String {
    match selector.strip_prefix("id:").and_then(|v| v.trim().parse::<i64>().ok()) {
        Some(id) => format!("{prefix}_id = {id}"),
        None => format!("{prefix}_name = '{}'", sql_escape(selector)),
    }
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
    /// Live in-memory batches, re-registered whenever the buffer has grown
    Live,
}

/// The part of the live event buffer already converted to Arrow.
struct LiveSnapshot {
    buffer_id: u64,
    rows: usize,
    batches: Vec<RecordBatch>,
}

/// Each live registration appends one small batch; past this many they are
/// merged into one so scans do not degrade over a long fight.
const MAX_LIVE_BATCHES: usize = 64;

/// Internal state protected by the lock
struct QueryContextState {
    ctx: SessionContext,
    current_source: RegisteredSource,
    live: Option<LiveSnapshot>,
}

/// Shared query context that manages DataFusion SessionContext lifecycle.
///
/// Key design decisions to minimize memory growth:
/// - Creates a FRESH SessionContext when switching to a different parquet file
///   (this clears all internal DataFusion caches and state)
/// - Reuses context for repeated queries on the same file
/// - Converts only the rows added since the last query for live data
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
    let config = SessionConfig::new()
        .with_target_partitions(1) // Encounters are a few thousand rows; splitting them only adds overhead
        .with_batch_size(4096); // Default is 8192
    SessionContext::new_with_config(config)
}

impl QueryContext {
    pub fn new() -> Self {
        Self {
            state: tokio::sync::RwLock::new(QueryContextState {
                ctx: create_session_context(),
                current_source: RegisteredSource::None,
                live: None,
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
            if let RegisteredSource::Parquet(ref registered_path) = state.current_source
                && registered_path == path
            {
                return Ok(());
            }
        }

        // Slow path: need to register new file
        let mut state = self.state.write().await;

        // Double-check after acquiring write lock
        if let RegisteredSource::Parquet(ref registered_path) = state.current_source
            && registered_path == path
        {
            return Ok(());
        }

        // Create a FRESH SessionContext to clear all internal DataFusion state
        // This prevents memory accumulation from cached query plans, statistics, etc.
        state.ctx = create_session_context();
        state.live = None;

        state
            .ctx
            .register_parquet("events", path.to_string_lossy().as_ref(), ParquetReadOptions::default())
            .await
            .map_err(|e| e.to_string())?;

        state.current_source = RegisteredSource::Parquet(path.to_path_buf());
        Ok(())
    }

    /// Point the context at the live event buffer.
    ///
    /// A poll fires several queries against a buffer that only ever grows, so
    /// this converts just the rows added since the previous call and does
    /// nothing at all when there are none.
    pub async fn register_live(&self, writer: &EncounterWriter) -> Result<(), String> {
        let mut state = self.state.write().await;
        let state = &mut *state;

        // A cleared buffer is a new encounter: nothing converted so far applies.
        if state
            .live
            .as_ref()
            .is_some_and(|live| live.buffer_id != writer.buffer_id() || live.rows > writer.len())
        {
            state.live = None;
        }
        let live = state.live.get_or_insert_with(|| LiveSnapshot {
            buffer_id: writer.buffer_id(),
            rows: 0,
            batches: Vec::new(),
        });

        match writer.batch_from(live.rows) {
            Some(batch) => {
                live.rows = writer.len();
                live.batches.push(batch);
            }
            None if live.batches.is_empty() => return Err("Live buffer is empty".into()),
            None if state.current_source == RegisteredSource::Live => return Ok(()),
            None => {}
        }

        let schema = live.batches[0].schema();
        if live.batches.len() > MAX_LIVE_BATCHES {
            let merged = concat_batches(&schema, &live.batches).map_err(|e| e.to_string())?;
            live.batches = vec![merged];
        }

        let _ = state.ctx.deregister_table("events");
        let mem_table = MemTable::try_new(schema, vec![live.batches.clone()]).map_err(|e| e.to_string())?;
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
        state.live = None;
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
    /// Check whether the registered events table has a column.
    /// Used for schema evolution — older parquet files may lack newer columns.
    async fn has_column(&self, name: &str) -> bool {
        match self.ctx.table("events").await {
            Ok(df) => df.schema().field_with_unqualified_name(name).is_ok(),
            Err(_) => false,
        }
    }

    /// Execute SQL query, returning empty results if table doesn't exist.
    /// This prevents panics when queries are made before parquet data is loaded.
    async fn sql(&self, query: &str) -> Result<Vec<RecordBatch>, String> {
        match self.ctx.sql(query).await {
            Ok(df) => df.collect().await.map_err(|e| e.to_string()),
            Err(e) => {
                let msg = e.to_string();
                // Return empty results for missing table (common during startup or empty encounters)
                // or missing fields (schema evolution - older parquet files may lack new columns)
                if msg.contains("not found") || msg.contains("does not exist") || msg.contains("No field named") {
                    Ok(vec![])
                } else {
                    Err(msg)
                }
            }
        }
    }
}
