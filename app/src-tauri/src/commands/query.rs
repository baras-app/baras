//! Data query commands for the encounter explorer.
//!
//! Provides SQL-based queries over encounter data using DataFusion.

use baras_core::query::{AbilityBreakdown, EntityBreakdown, TimeSeriesPoint};
use tauri::State;

use crate::service::ServiceHandle;

/// Query damage breakdown by ability for an encounter.
/// Pass encounter_idx for historical, or None for live encounter.
#[tauri::command]
pub async fn query_damage_by_ability(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    source_name: Option<String>,
) -> Result<Vec<AbilityBreakdown>, String> {
    handle.query_damage_by_ability(encounter_idx, source_name).await
}

/// Query damage/healing breakdown by source entity.
#[tauri::command]
pub async fn query_entity_breakdown(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
) -> Result<Vec<EntityBreakdown>, String> {
    handle.query_entity_breakdown(encounter_idx).await
}

/// Query DPS over time with specified bucket size.
#[tauri::command]
pub async fn query_dps_over_time(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    bucket_ms: i64,
    source_name: Option<String>,
) -> Result<Vec<TimeSeriesPoint>, String> {
    handle.query_dps_over_time(encounter_idx, bucket_ms, source_name).await
}

/// List available encounter parquet files.
#[tauri::command]
pub async fn list_encounter_files(
    handle: State<'_, ServiceHandle>,
) -> Result<Vec<u32>, String> {
    handle.list_encounter_files().await
}
