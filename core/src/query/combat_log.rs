//! Combat log viewer queries.

use super::*;

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
        let batches = self
            .sql(
                "SELECT DISTINCT source_name FROM events WHERE combat_time_secs IS NOT NULL ORDER BY source_name",
            )
            .await?;

        let mut results = Vec::new();
        for batch in &batches {
            results.extend(col_strings(batch, 0)?);
        }
        Ok(results)
    }

    /// Get distinct target names for filter dropdown.
    pub async fn query_target_names(&self) -> Result<Vec<String>, String> {
        let batches = self
            .sql(
                "SELECT DISTINCT target_name FROM events WHERE combat_time_secs IS NOT NULL ORDER BY target_name",
            )
            .await?;

        let mut results = Vec::new();
        for batch in &batches {
            results.extend(col_strings(batch, 0)?);
        }
        Ok(results)
    }
}
