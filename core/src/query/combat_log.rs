//! Combat log viewer queries.

use super::*;
use crate::game_data::{effect_id, effect_type_id};

/// Build search clause supporting case-insensitive search and OR logic.
/// Search terms separated by " OR " are combined with OR logic.
fn build_search_clause(search: &str) -> String {
    let terms: Vec<&str> = search.split(" OR ").map(|s| s.trim()).collect();

    let term_clauses: Vec<String> = terms
        .iter()
        .filter(|t| !t.is_empty())
        .map(|term| {
            let escaped = sql_escape(term).to_lowercase();
            format!(
                "(LOWER(source_name) LIKE '%{0}%' OR LOWER(target_name) LIKE '%{0}%' OR LOWER(ability_name) LIKE '%{0}%' OR LOWER(effect_name) LIKE '%{0}%')",
                escaped
            )
        })
        .collect();

    if term_clauses.is_empty() {
        return "1=1".to_string();
    }

    format!("({})", term_clauses.join(" OR "))
}

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
        conditions.push(format!(
            "(effect_type_id = {} AND effect_id NOT IN ({}, {}, {}))",
            effect_type_id::EVENT,
            effect_id::ABILITYACTIVATE,
            effect_id::ABILITYDEACTIVATE,
            effect_id::ABILITYINTERRUPT
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
            match source {
                "__ALL_FRIENDLY__" => {
                    where_clauses.push("source_entity_type IN ('Player', 'Companion')".to_string());
                }
                "__ALL_NPCS__" => {
                    where_clauses.push("source_entity_type = 'Npc'".to_string());
                }
                _ => {
                    where_clauses.push(format!("source_name = '{}'", sql_escape(source)));
                }
            }
        }
        if let Some(target) = target_filter {
            match target {
                "__ALL_FRIENDLY__" => {
                    where_clauses.push("target_entity_type IN ('Player', 'Companion')".to_string());
                }
                "__ALL_NPCS__" => {
                    where_clauses.push("target_entity_type = 'Npc'".to_string());
                }
                _ => {
                    where_clauses.push(format!("target_name = '{}'", sql_escape(target)));
                }
            }
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
                COALESCE(defense_type_id, 0) as defense_type_id,
                effect_id,
                effect_type_id,
                source_class_id,
                target_class_id
            FROM events
            WHERE {where_clause}
            ORDER BY combat_time_secs
            LIMIT {limit} OFFSET {offset}
        "#
            ))
            .await?;

        let mut results = Vec::new();
        for batch in &batches {
            let num_rows = batch.num_rows();
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

            let effect_ids = col_i64(batch, 17)?;
            let effect_type_ids = col_i64(batch, 18)?;
            let source_class_ids = col_i64(batch, 19)?;
            let target_class_ids = col_i64(batch, 20)?;

            for i in 0..num_rows {
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
                    effect_id: effect_ids[i],
                    effect_type_id: effect_type_ids[i],
                    source_class_id: source_class_ids[i],
                    target_class_id: target_class_ids[i],
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
            match source {
                "__ALL_FRIENDLY__" => {
                    where_clauses.push("source_entity_type IN ('Player', 'Companion')".to_string());
                }
                "__ALL_NPCS__" => {
                    where_clauses.push("source_entity_type = 'Npc'".to_string());
                }
                _ => {
                    where_clauses.push(format!("source_name = '{}'", sql_escape(source)));
                }
            }
        }
        if let Some(target) = target_filter {
            match target {
                "__ALL_FRIENDLY__" => {
                    where_clauses.push("target_entity_type IN ('Player', 'Companion')".to_string());
                }
                "__ALL_NPCS__" => {
                    where_clauses.push("target_entity_type = 'Npc'".to_string());
                }
                _ => {
                    where_clauses.push(format!("target_name = '{}'", sql_escape(target)));
                }
            }
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
        let batches = self
            .sql(
                "SELECT DISTINCT source_name, source_entity_type FROM events WHERE combat_time_secs IS NOT NULL ORDER BY source_name",
            )
            .await?;

        let mut friendly = Vec::new();
        let mut npcs = Vec::new();
        for batch in &batches {
            let names = col_strings(batch, 0)?;
            let types = col_strings(batch, 1)?;
            for (name, entity_type) in names.into_iter().zip(types.into_iter()) {
                match entity_type.as_str() {
                    "Player" | "Companion" => friendly.push(name),
                    "Npc" => npcs.push(name),
                    _ => {} // Skip Empty, SelfReference
                }
            }
        }
        Ok(GroupedEntityNames { friendly, npcs })
    }

    /// Get distinct target names for filter dropdown, grouped by entity type.
    pub async fn query_target_names(&self) -> Result<GroupedEntityNames, String> {
        let batches = self
            .sql(
                "SELECT DISTINCT target_name, target_entity_type FROM events WHERE combat_time_secs IS NOT NULL ORDER BY target_name",
            )
            .await?;

        let mut friendly = Vec::new();
        let mut npcs = Vec::new();
        for batch in &batches {
            let names = col_strings(batch, 0)?;
            let types = col_strings(batch, 1)?;
            for (name, entity_type) in names.into_iter().zip(types.into_iter()) {
                match entity_type.as_str() {
                    "Player" | "Companion" => friendly.push(name),
                    "Npc" => npcs.push(name),
                    _ => {} // Skip Empty, SelfReference
                }
            }
        }
        Ok(GroupedEntityNames { friendly, npcs })
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
        time_range: Option<&TimeRange>,
        event_filters: Option<&CombatLogFilters>,
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
            match source {
                "__ALL_FRIENDLY__" => {
                    where_clauses.push("source_entity_type IN ('Player', 'Companion')".to_string());
                }
                "__ALL_NPCS__" => {
                    where_clauses.push("source_entity_type = 'Npc'".to_string());
                }
                _ => {
                    where_clauses.push(format!("source_name = '{}'", sql_escape(source)));
                }
            }
        }
        if let Some(target) = target_filter {
            match target {
                "__ALL_FRIENDLY__" => {
                    where_clauses.push("target_entity_type IN ('Player', 'Companion')".to_string());
                }
                "__ALL_NPCS__" => {
                    where_clauses.push("target_entity_type = 'Npc'".to_string());
                }
                _ => {
                    where_clauses.push(format!("target_name = '{}'", sql_escape(target)));
                }
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
            "(LOWER(COALESCE(src, '')) LIKE '%{0}%' OR LOWER(COALESCE(tgt, '')) LIKE '%{0}%' OR LOWER(COALESCE(abl, '')) LIKE '%{0}%' OR LOWER(COALESCE(eff, '')) LIKE '%{0}%')",
            find_lower
        );

        // CTE: number ALL rows in base result, then filter for find matches
        // This gives us the position in the FULL list for correct scrolling
        let batches = self
            .sql(&format!(
                r#"
                WITH numbered AS (
                    SELECT
                        line_number,
                        CAST(ROW_NUMBER() OVER (ORDER BY combat_time_secs) - 1 AS BIGINT) as pos,
                        source_name as src,
                        target_name as tgt,
                        ability_name as abl,
                        effect_name as eff
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
