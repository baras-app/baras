//! Query module for analyzing encounter data with DataFusion.
//!
//! Provides SQL queries over:
//! - Live Arrow buffers (current encounter)
//! - Historical parquet files (completed encounters)

mod breakdown;
mod column_helpers;
mod combat_log;
mod effects;
pub mod error;
mod overview;
mod time_series;
mod timeline;

pub use error::QueryError;

use std::path::Path;
use std::sync::Arc;

use datafusion::arrow::record_batch::RecordBatch;
use datafusion::config::ConfigOptions;
use datafusion::datasource::MemTable;
use datafusion::prelude::*;

use column_helpers::*;

// Re-export query types from shared types crate
pub use baras_types::{
    AbilityBreakdown, BreakdownMode, CombatLogFilters, CombatLogFindMatch, CombatLogRow, DataTab,
    EffectChartData, EffectWindow, EncounterTimeline, EntityBreakdown, GroupedEntityNames,
    PhaseSegment, PlayerDeath, RaidOverviewRow, TimeRange, TimeSeriesPoint,
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
        EncounterQuery {
            ctx: &self.guard.ctx,
        }
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
                // or missing fields (schema evolution - older parquet files may lack new columns)
                if msg.contains("not found")
                    || msg.contains("does not exist")
                    || msg.contains("No field named")
                {
                    Ok(vec![])
                } else {
                    Err(msg)
                }
            }
        }
    }
}
