//! Data query commands for the encounter explorer.
//!
//! Provides SQL-based queries over encounter data using DataFusion.

use baras_core::query::{
    AbilityBreakdown, AbilityUsageRow, BreakdownMode, CombatLogFilters, CombatLogFindMatch,
    CombatLogRow, CombatLogSortColumn, DamageTakenSummary, DataTab, EffectChartData, EffectWindow,
    EffectUptimeCell, EncounterTimeline, EntityBreakdown, GroupMetric, GroupTimeSeries, HpPoint, NpcHealthRow, PlayerDeath, RaidOverviewRow,
    RotationAnalysis, SortDirection, TimeRange, TimeSeriesPoint,
};
use tauri::State;

use crate::service::ServiceHandle;

/// Query ability breakdown for an encounter and data tab.
/// Pass encounter_idx for historical, or None for live encounter.
#[tauri::command]
pub async fn query_breakdown(
    handle: State<'_, ServiceHandle>,
    tab: DataTab,
    encounter_idx: Option<u32>,
    entity_name: Option<String>,
    entity_id: Option<i64>,
    time_range: Option<TimeRange>,
    entity_types: Option<Vec<String>>,
    breakdown_mode: Option<BreakdownMode>,
    duration_secs: Option<f32>,
) -> Result<Vec<AbilityBreakdown>, String> {
    handle
        .query_breakdown(
            tab,
            encounter_idx,
            entity_name,
            entity_id,
            time_range,
            entity_types,
            breakdown_mode,
            duration_secs,
        )
        .await
}

/// Query damage/healing breakdown by entity for a data tab.
#[tauri::command]
pub async fn query_entity_breakdown(
    handle: State<'_, ServiceHandle>,
    tab: DataTab,
    encounter_idx: Option<u32>,
    time_range: Option<TimeRange>,
) -> Result<Vec<EntityBreakdown>, String> {
    handle
        .query_entity_breakdown(tab, encounter_idx, time_range)
        .await
}

/// Query raid overview - aggregated stats per player.
#[tauri::command]
pub async fn query_raid_overview(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    time_range: Option<TimeRange>,
    duration_secs: Option<f32>,
) -> Result<Vec<RaidOverviewRow>, String> {
    handle
        .query_raid_overview(encounter_idx, time_range, duration_secs)
        .await
}

/// Query DPS over time with specified bucket size.
#[tauri::command]
pub async fn query_dps_over_time(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    bucket_ms: i64,
    source_name: Option<String>,
    time_range: Option<TimeRange>,
) -> Result<Vec<TimeSeriesPoint>, String> {
    handle
        .query_dps_over_time(encounter_idx, bucket_ms, source_name, time_range)
        .await
}

/// Every player's metric over time, for the Overview's group chart.
#[tauri::command]
pub async fn query_group_over_time(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    bucket_ms: i64,
    metric: GroupMetric,
    time_range: Option<TimeRange>,
) -> Result<GroupTimeSeries, String> {
    handle
        .query_group_over_time(encounter_idx, bucket_ms, metric, time_range)
        .await
}

/// Every effect a source applied, uptime per target.
#[tauri::command]
pub async fn query_effect_uptime_by_target(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    source: String,
    time_range: Option<TimeRange>,
    duration_secs: f32,
) -> Result<Vec<EffectUptimeCell>, String> {
    handle
        .query_effect_uptime_by_target(encounter_idx, source, time_range, duration_secs)
        .await
}

/// List available encounter parquet files.
#[tauri::command]
pub async fn list_encounter_files(handle: State<'_, ServiceHandle>) -> Result<Vec<u32>, String> {
    handle.list_encounter_files().await
}

/// Get encounter timeline with phase segments.
#[tauri::command]
pub async fn query_encounter_timeline(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
) -> Result<EncounterTimeline, String> {
    handle.query_encounter_timeline(encounter_idx).await
}

/// Query HPS over time with specified bucket size.
#[tauri::command]
pub async fn query_hps_over_time(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    bucket_ms: i64,
    source_name: Option<String>,
    time_range: Option<TimeRange>,
) -> Result<Vec<TimeSeriesPoint>, String> {
    handle
        .query_hps_over_time(encounter_idx, bucket_ms, source_name, time_range)
        .await
}

/// Query EHPS (effective healing) over time with specified bucket size.
#[tauri::command]
pub async fn query_ehps_over_time(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    bucket_ms: i64,
    source_name: Option<String>,
    time_range: Option<TimeRange>,
) -> Result<Vec<TimeSeriesPoint>, String> {
    handle
        .query_ehps_over_time(encounter_idx, bucket_ms, source_name, time_range)
        .await
}

/// Query EHT (effective healing taken) over time with specified bucket size.
#[tauri::command]
pub async fn query_eht_over_time(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    bucket_ms: i64,
    target_name: Option<String>,
    time_range: Option<TimeRange>,
) -> Result<Vec<TimeSeriesPoint>, String> {
    handle
        .query_eht_over_time(encounter_idx, bucket_ms, target_name, time_range)
        .await
}

/// Query HP% over time with specified bucket size.
#[tauri::command]
pub async fn query_hp_over_time(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    bucket_ms: i64,
    target_name: Option<String>,
    time_range: Option<TimeRange>,
) -> Result<Vec<HpPoint>, String> {
    handle
        .query_hp_over_time(encounter_idx, bucket_ms, target_name, time_range)
        .await
}

/// Query DTPS over time with specified bucket size.
#[tauri::command]
pub async fn query_dtps_over_time(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    bucket_ms: i64,
    target_name: Option<String>,
    time_range: Option<TimeRange>,
) -> Result<Vec<TimeSeriesPoint>, String> {
    handle
        .query_dtps_over_time(encounter_idx, bucket_ms, target_name, time_range)
        .await
}

/// Query effect uptime statistics for charts panel.
#[tauri::command]
pub async fn query_effect_uptime(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    target_name: Option<String>,
    time_range: Option<TimeRange>,
    duration_secs: f32,
    source_filter: Option<String>,
) -> Result<Vec<EffectChartData>, String> {
    handle
        .query_effect_uptime(encounter_idx, target_name, time_range, duration_secs, source_filter)
        .await
}

/// Query individual time windows for a specific effect.
#[tauri::command]
pub async fn query_effect_windows(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    effect_id: i64,
    target_name: Option<String>,
    time_range: Option<TimeRange>,
    duration_secs: f32,
    source_filter: Option<String>,
) -> Result<Vec<EffectWindow>, String> {
    handle
        .query_effect_windows(
            encounter_idx,
            effect_id,
            target_name,
            time_range,
            duration_secs,
            source_filter,
        )
        .await
}

/// Query combat log rows with pagination for virtual scrolling.
#[tauri::command]
pub async fn query_combat_log(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    offset: u64,
    limit: u64,
    source_filter: Option<String>,
    target_filter: Option<String>,
    search_filter: Option<String>,
    time_range: Option<TimeRange>,
    event_filters: Option<CombatLogFilters>,
    sort_column: Option<CombatLogSortColumn>,
    sort_direction: Option<SortDirection>,
) -> Result<Vec<CombatLogRow>, String> {
    handle
        .query_combat_log(
            encounter_idx,
            offset,
            limit,
            source_filter,
            target_filter,
            search_filter,
            time_range,
            event_filters,
            sort_column.unwrap_or_default(),
            sort_direction.unwrap_or_default(),
        )
        .await
}

/// Get total count of combat log rows for pagination.
#[tauri::command]
pub async fn query_combat_log_count(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    source_filter: Option<String>,
    target_filter: Option<String>,
    search_filter: Option<String>,
    time_range: Option<TimeRange>,
    event_filters: Option<CombatLogFilters>,
) -> Result<u64, String> {
    handle
        .query_combat_log_count(
            encounter_idx,
            source_filter,
            target_filter,
            search_filter,
            time_range,
            event_filters,
        )
        .await
}

/// Find matching rows in combat log (for Find feature).
#[tauri::command]
pub async fn query_combat_log_find(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    find_text: String,
    source_filter: Option<String>,
    target_filter: Option<String>,
    search_filter: Option<String>,
    time_range: Option<TimeRange>,
    event_filters: Option<CombatLogFilters>,
    sort_column: Option<CombatLogSortColumn>,
    sort_direction: Option<SortDirection>,
) -> Result<Vec<CombatLogFindMatch>, String> {
    handle
        .query_combat_log_find(
            encounter_idx,
            find_text,
            source_filter,
            target_filter,
            search_filter,
            time_range,
            event_filters,
            sort_column.unwrap_or_default(),
            sort_direction.unwrap_or_default(),
        )
        .await
}

/// Get distinct source names for combat log filter dropdown, grouped by entity type.
#[tauri::command]
pub async fn query_source_names(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
) -> Result<baras_types::GroupedEntityNames, String> {
    handle.query_source_names(encounter_idx).await
}

/// Get distinct target names for combat log filter dropdown, grouped by entity type.
#[tauri::command]
pub async fn query_target_names(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
) -> Result<baras_types::GroupedEntityNames, String> {
    handle.query_target_names(encounter_idx).await
}

/// Query player deaths in an encounter.
#[tauri::command]
pub async fn query_player_deaths(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
) -> Result<Vec<PlayerDeath>, String> {
    handle.query_player_deaths(encounter_idx).await
}

/// Query a death recap for one player death in an encounter.
#[tauri::command]
pub async fn query_death_recap(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    player_name: String,
    death_time_secs: f32,
) -> Result<baras_types::DeathRecap, String> {
    handle
        .query_death_recap(encounter_idx, player_name, death_time_secs)
        .await
}

/// Query final health state of all NPCs in an encounter.
#[tauri::command]
pub async fn query_npc_health(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    time_range: Option<TimeRange>,
) -> Result<Vec<NpcHealthRow>, String> {
    handle.query_npc_health(encounter_idx, time_range).await
}

/// Query damage taken summary (damage type breakdown + mitigation stats).
#[tauri::command]
pub async fn query_damage_taken_summary(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    entity_name: String,
    entity_id: Option<i64>,
    time_range: Option<TimeRange>,
    entity_types: Option<Vec<String>>,
) -> Result<DamageTakenSummary, String> {
    handle
        .query_damage_taken_summary(encounter_idx, entity_name, entity_id, time_range, entity_types)
        .await
}

/// Query rotation analysis for a player.
#[tauri::command]
pub async fn query_rotation(
    handle: State<'_, ServiceHandle>,
    encounter_idx: Option<u32>,
    source_name: String,
    anchor_ability_id: i64,
    time_range: Option<TimeRange>,
) -> Result<RotationAnalysis, String> {
    handle
        .query_rotation(encounter_idx, source_name, anchor_ability_id, time_range)
        .await
}

/// Query ability usage statistics for a single player.
#[tauri::command]
pub async fn query_ability_usage(
    handle: State<'_, ServiceHandle>,
    source_name: String,
    encounter_idx: Option<u32>,
    time_range: Option<TimeRange>,
) -> Result<Vec<AbilityUsageRow>, String> {
    handle
        .query_ability_usage(source_name, encounter_idx, time_range)
        .await
}
