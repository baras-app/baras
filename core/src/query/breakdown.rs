//! Ability and entity breakdown queries.

use super::*;

impl EncounterQuery<'_> {
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
}
