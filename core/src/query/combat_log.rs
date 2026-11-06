//! Combat log viewer queries.

use std::collections::HashMap;

use super::*;
use crate::game_data::{effect_id, effect_type_id};
use baras_types::{CombatLogSortColumn, NpcSpawn, SortDirection};

use super::search_parser::build_search_clause;

/// Build event type filter clause based on CombatLogFilters.
fn build_event_filter_clause(filters: &CombatLogFilters) -> Option<String> {
    let mut conditions = Vec::new();

    // If all filters are enabled, no additional filtering needed
    let all_enabled =
        filters.damage && filters.healing && filters.actions && filters.effects && filters.other;
    if all_enabled {
        return None;
    }

    if filters.damage {
        conditions.push(format!("effect_id = {}", effect_id::DAMAGE));
    }
    if filters.healing {
        conditions.push(format!("effect_id = {}", effect_id::HEAL));
    }
    if filters.actions {
        conditions.push(format!(
            "(effect_type_id = {} AND effect_id IN ({}, {}, {}))",
            effect_type_id::EVENT,
            effect_id::ABILITYACTIVATE,
            effect_id::ABILITYDEACTIVATE,
            effect_id::ABILITYINTERRUPT
        ));
    }
    if filters.effects {
        // Buffs/debuffs applied or removed, excluding damage/heal effects
        conditions.push(format!(
            "(effect_type_id IN ({}, {}) AND effect_id NOT IN ({}, {}))",
            effect_type_id::APPLYEFFECT,
            effect_type_id::REMOVEEFFECT,
            effect_id::DAMAGE,
            effect_id::HEAL
        ));
    }
    if filters.other {
        // Other EVENT types not covered by actions (TargetSet, Death, EnterCombat, etc.)
        // Includes ModifyCharges and other misc event types
        conditions.push(format!(
            "((effect_type_id = {} AND effect_id NOT IN ({}, {}, {})) OR effect_type_id = {})",
            effect_type_id::EVENT,
            effect_id::ABILITYACTIVATE,
            effect_id::ABILITYDEACTIVATE,
            effect_id::ABILITYINTERRUPT,
            effect_type_id::MODIFYCHARGES
        ));
    }

    // Build the main OR clause
    if conditions.is_empty() {
        // All filters are off - show nothing
        Some("1=0".to_string())
    } else {
        Some(format!("({})", conditions.join(" OR ")))
    }
}

/// WHERE fragment for a Source/Target dropdown value: the sentinel groups, an
/// `id:<instance>` pin to one NPC spawn, or a plain entity name.
fn entity_filter_sql(prefix: &str, value: &str) -> String {
    match value {
        "__ALL_FRIENDLY__" => format!("{prefix}_entity_type IN ('Player', 'Companion')"),
        "__ALL_NPCS__" => format!("{prefix}_entity_type = 'Npc'"),
        _ => entity_match_sql(prefix, value),
    }
}

impl EncounterQuery<'_> {
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
        event_filters: Option<&CombatLogFilters>,
        sort_column: CombatLogSortColumn,
        sort_direction: SortDirection,
    ) -> Result<Vec<CombatLogRow>, String> {
        // Always exclude Spend/Restore events (energy/resource changes)
        let mut where_clauses = vec![
            "combat_time_secs IS NOT NULL".to_string(),
            format!(
                "effect_type_id NOT IN ({}, {})",
                effect_type_id::SPEND,
                effect_type_id::RESTORE
            ),
        ];

        if let Some(source) = source_filter {
            where_clauses.push(entity_filter_sql("source", source));
        }
        if let Some(target) = target_filter {
            where_clauses.push(entity_filter_sql("target", target));
        }
        if let Some(search) = search_filter {
            if !search.is_empty() {
                where_clauses.push(build_search_clause(search));
            }
        }
        if let Some(tr) = time_range {
            where_clauses.push(tr.sql_filter());
        }
        if let Some(filters) = event_filters {
            if let Some(filter_clause) = build_event_filter_clause(filters) {
                where_clauses.push(filter_clause);
            }
        }

        let where_clause = where_clauses.join(" AND ");
        let order_col = sort_column.sql_column();
        let order_dir = sort_direction.sql();
        let modify_charges_id = effect_type_id::MODIFYCHARGES;

        // Older parquet files predate the coordinate columns
        let has_coords = self.has_column("source_x").await;
        let coord_select = if has_coords {
            ",\n                source_x, source_y, source_z, source_facing,\n                target_x, target_y, target_z, target_facing"
        } else {
            ""
        };

        let batches = self
            .sql(&format!(
                r#"
            SELECT
                line_number,
                combat_time_secs,
                timestamp,
                source_name,
                source_entity_type,
                target_name,
                target_entity_type,
                effect_type_name,
                ability_name,
                ability_id,
                effect_name,
                CASE WHEN effect_type_id = {modify_charges_id} THEN charges
                     ELSE COALESCE(dmg_effective, 0) + COALESCE(heal_effective, 0)
                END as value,
                COALESCE(dmg_absorbed, 0) as absorbed,
                GREATEST(COALESCE(heal_amount, 0) - COALESCE(heal_effective, 0), 0) as overheal,
                COALESCE(threat, 0.0) as threat,
                is_crit,
                COALESCE(dmg_type, '') as damage_type,
                COALESCE(defense_type_id, 0) as defense_type_id,
                effect_id,
                effect_type_id,
                source_class_id,
                target_class_id,
                source_id,
                target_id,
                source_hp, source_max_hp, target_hp, target_max_hp{coord_select}
            FROM events
            WHERE {where_clause}
            ORDER BY {order_col} {order_dir}, combat_time_secs {order_dir}, line_number {order_dir}
            LIMIT {limit} OFFSET {offset}
        "#
            ))
            .await?;

        let mut results = Vec::new();
        for batch in &batches {
            let num_rows = batch.num_rows();
            let line_numbers = col_i64(batch, 0)?;
            let times = col_f32(batch, 1)?;
            let timestamps = col_timestamp_ms(batch, 2)?;
            let source_names = col_strings(batch, 3)?;
            let source_types = col_strings(batch, 4)?;
            let target_names = col_strings(batch, 5)?;
            let target_types = col_strings(batch, 6)?;
            let effect_types = col_strings(batch, 7)?;
            let ability_names = col_strings(batch, 8)?;
            let ability_ids = col_i64(batch, 9)?;
            let effect_names = col_strings(batch, 10)?;
            let values = col_i32(batch, 11)?;
            let absorbeds = col_i32(batch, 12)?;
            let overheals = col_i32(batch, 13)?;
            let threats = col_f32(batch, 14)?;
            let is_crits = col_bool(batch, 15)?;
            let damage_types = col_strings(batch, 16)?;
            let defense_type_ids = col_i64(batch, 17)?;

            let effect_ids = col_i64(batch, 18)?;
            let effect_type_ids = col_i64(batch, 19)?;
            let source_class_ids = col_i64(batch, 20)?;
            let target_class_ids = col_i64(batch, 21)?;
            let source_ids = col_i64(batch, 22)?;
            let target_ids = col_i64(batch, 23)?;
            let source_hps = col_i32(batch, 24)?;
            let source_max_hps = col_i32(batch, 25)?;
            let target_hps = col_i32(batch, 26)?;
            let target_max_hps = col_i32(batch, 27)?;

            let no_coords = vec![None; num_rows];
            let coord_col = |idx: usize| -> Result<Vec<Option<f32>>, String> {
                if has_coords {
                    col_opt_f32(batch, idx)
                } else {
                    Ok(no_coords.clone())
                }
            };
            let source_xs = coord_col(28)?;
            let source_ys = coord_col(29)?;
            let source_zs = coord_col(30)?;
            let source_facings = coord_col(31)?;
            let target_xs = coord_col(32)?;
            let target_ys = coord_col(33)?;
            let target_zs = coord_col(34)?;
            let target_facings = coord_col(35)?;

            for i in 0..num_rows {
                results.push(CombatLogRow {
                    row_idx: line_numbers[i] as u64,
                    time_secs: times[i],
                    timestamp_ms: timestamps[i],
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
                    effect_id: effect_ids[i],
                    effect_type_id: effect_type_ids[i],
                    source_class_id: source_class_ids[i],
                    target_class_id: target_class_ids[i],
                    source_id: source_ids[i],
                    target_id: target_ids[i],
                    source_hp: source_hps[i],
                    source_max_hp: source_max_hps[i],
                    target_hp: target_hps[i],
                    target_max_hp: target_max_hps[i],
                    source_x: source_xs[i],
                    source_y: source_ys[i],
                    source_z: source_zs[i],
                    source_facing: source_facings[i],
                    target_x: target_xs[i],
                    target_y: target_ys[i],
                    target_z: target_zs[i],
                    target_facing: target_facings[i],
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
        event_filters: Option<&CombatLogFilters>,
    ) -> Result<u64, String> {
        // Always exclude Spend/Restore events (energy/resource changes)
        let mut where_clauses = vec![
            "combat_time_secs IS NOT NULL".to_string(),
            format!(
                "effect_type_id NOT IN ({}, {})",
                effect_type_id::SPEND,
                effect_type_id::RESTORE
            ),
        ];

        if let Some(source) = source_filter {
            where_clauses.push(entity_filter_sql("source", source));
        }
        if let Some(target) = target_filter {
            where_clauses.push(entity_filter_sql("target", target));
        }
        if let Some(search) = search_filter {
            if !search.is_empty() {
                where_clauses.push(build_search_clause(search));
            }
        }
        if let Some(tr) = time_range {
            where_clauses.push(tr.sql_filter());
        }
        if let Some(filters) = event_filters {
            if let Some(filter_clause) = build_event_filter_clause(filters) {
                where_clauses.push(filter_clause);
            }
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

    /// Get distinct source names for filter dropdown, grouped by entity type.
    pub async fn query_source_names(&self) -> Result<GroupedEntityNames, String> {
        self.query_entity_names("source").await
    }

    /// Get distinct target names for filter dropdown, grouped by entity type.
    pub async fn query_target_names(&self) -> Result<GroupedEntityNames, String> {
        self.query_entity_names("target").await
    }

    /// Distinct entity names on one side of the event, plus per-spawn rows for any
    /// NPC name that occurs with more than one instance id (so the dropdown can
    /// offer "this spawn" under the aggregate name).
    async fn query_entity_names(&self, prefix: &str) -> Result<GroupedEntityNames, String> {
        let batches = self
            .sql(&format!(
                "SELECT {prefix}_name, {prefix}_entity_type, {prefix}_id, MIN(combat_time_secs) as first_hit \
                 FROM events WHERE combat_time_secs IS NOT NULL \
                 GROUP BY {prefix}_name, {prefix}_entity_type, {prefix}_id \
                 ORDER BY {prefix}_name, first_hit"
            ))
            .await?;

        let mut friendly = Vec::new();
        let mut npcs = Vec::new();
        let mut spawns: Vec<NpcSpawn> = Vec::new();
        for batch in &batches {
            let names = col_strings(batch, 0)?;
            let types = col_strings(batch, 1)?;
            let ids = col_i64(batch, 2)?;
            let first_hits = col_f32(batch, 3)?;
            for i in 0..batch.num_rows() {
                let name = &names[i];
                match types[i].as_str() {
                    "Player" | "Companion" => {
                        if friendly.last() != Some(name) {
                            friendly.push(name.clone());
                        }
                    }
                    "Npc" => {
                        if npcs.last() != Some(name) {
                            npcs.push(name.clone());
                        }
                        spawns.push(NpcSpawn { name: name.clone(), id: ids[i], first_hit_secs: first_hits[i] });
                    }
                    _ => {} // Skip Empty, SelfReference
                }
            }
        }
        // Only names with several spawns need per-instance entries
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for sp in &spawns {
            *counts.entry(sp.name.as_str()).or_default() += 1;
        }
        let npc_spawns = spawns
            .iter()
            .filter(|sp| counts[sp.name.as_str()] > 1)
            .cloned()
            .collect();
        Ok(GroupedEntityNames { friendly, npcs, npc_spawns })
    }

    /// Find all row positions matching search text (for Find feature).
    ///
    /// The position is the row's index in the filtered result set (for scrolling),
    /// and row_idx (line_number) is used for highlighting when that row is loaded.
    pub async fn query_combat_log_find(
        &self,
        find_text: &str,
        source_filter: Option<&str>,
        target_filter: Option<&str>,
        search_filter: Option<&str>,
        time_range: Option<&TimeRange>,
        event_filters: Option<&CombatLogFilters>,
        sort_column: CombatLogSortColumn,
        sort_direction: SortDirection,
    ) -> Result<Vec<CombatLogFindMatch>, String> {
        if find_text.is_empty() {
            return Ok(vec![]);
        }

        // Build base WHERE clause (same filters as main query)
        // Always exclude Spend/Restore events (energy/resource changes)
        let mut where_clauses = vec![
            "combat_time_secs IS NOT NULL".to_string(),
            format!(
                "effect_type_id NOT IN ({}, {})",
                effect_type_id::SPEND,
                effect_type_id::RESTORE
            ),
        ];

        if let Some(source) = source_filter {
            where_clauses.push(entity_filter_sql("source", source));
        }
        if let Some(target) = target_filter {
            where_clauses.push(entity_filter_sql("target", target));
        }
        if let Some(search) = search_filter {
            if !search.is_empty() {
                where_clauses.push(build_search_clause(search));
            }
        }
        if let Some(tr) = time_range {
            where_clauses.push(tr.sql_filter());
        }
        if let Some(filters) = event_filters {
            if let Some(filter_clause) = build_event_filter_clause(filters) {
                where_clauses.push(filter_clause);
            }
        }

        let base_where = where_clauses.join(" AND ");

        // Find text filter - use COALESCE to handle NULLs
        let find_lower = sql_escape(find_text).to_lowercase();
        let find_filter = format!(
            "(LOWER(COALESCE(src, '')) LIKE '%{0}%' OR LOWER(COALESCE(tgt, '')) LIKE '%{0}%' OR LOWER(COALESCE(abl, '')) LIKE '%{0}%' OR LOWER(COALESCE(eff, '')) LIKE '%{0}%' OR CAST(abl_id AS VARCHAR) LIKE '%{0}%' OR CAST(eff_id AS VARCHAR) LIKE '%{0}%' OR CAST(src_id AS VARCHAR) LIKE '%{0}%' OR CAST(tgt_id AS VARCHAR) LIKE '%{0}%' OR CAST(src_cls_id AS VARCHAR) LIKE '%{0}%' OR CAST(tgt_cls_id AS VARCHAR) LIKE '%{0}%')",
            find_lower
        );

        // CTE: number ALL rows in base result, then filter for find matches
        // This gives us the position in the FULL list for correct scrolling
        let order_col = sort_column.sql_column();
        let order_dir = sort_direction.sql();

        let batches = self
            .sql(&format!(
                r#"
                WITH numbered AS (
                    SELECT
                        line_number,
                        CAST(ROW_NUMBER() OVER (ORDER BY {order_col} {order_dir}, combat_time_secs {order_dir}, line_number {order_dir}) - 1 AS BIGINT) as pos,
                        source_name as src,
                        target_name as tgt,
                        ability_name as abl,
                        effect_name as eff,
                        ability_id as abl_id,
                        effect_id as eff_id,
                        source_id as src_id,
                        target_id as tgt_id,
                        source_class_id as src_cls_id,
                        target_class_id as tgt_cls_id
                    FROM events
                    WHERE {base_where}
                )
                SELECT pos, line_number
                FROM numbered
                WHERE {find_filter}
                ORDER BY pos
                "#
            ))
            .await?;

        let mut results = Vec::new();
        for batch in &batches {
            let positions = col_i64(batch, 0)?;
            let line_numbers = col_i64(batch, 1)?;
            for i in 0..batch.num_rows() {
                results.push(CombatLogFindMatch {
                    pos: positions[i] as u64,
                    row_idx: line_numbers[i] as u64,
                });
            }
        }
        Ok(results)
    }
}
