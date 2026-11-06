//! Ability and entity breakdown queries.

use std::collections::HashMap;

use super::*;
use crate::game_data::{ATTACK_TYPES, defense_type, effect_id, effect_type_id};

/// Defense types that count as avoidance (misses)
const MISS_IDS: &[i64] = &[
    defense_type::MISS,
    defense_type::DODGE,
    defense_type::PARRY,
    defense_type::DEFLECT,
    defense_type::RESIST,
];

fn miss_ids_csv() -> String {
    MISS_IDS
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

impl EncounterQuery<'_> {
    /// Query ability breakdown for any data tab.
    /// - entity_name: For outgoing tabs (Damage/Healing), filters by source_name.
    ///                For incoming tabs (DamageTaken/HealingTaken), filters by target_name.
    /// - entity_id: When set, filters by the entity's instance id instead of its name
    ///              (one NPC spawn among several with the same name).
    /// - entity_types: Filters by source_entity_type for outgoing, target_entity_type for incoming.
    pub async fn query_breakdown(
        &self,
        tab: DataTab,
        entity_name: Option<&str>,
        entity_id: Option<i64>,
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
        let is_damage_tab = tab == DataTab::Damage || tab == DataTab::DamageTaken;

        // For outgoing (Damage/Healing): filter/group by source, breakdown by target
        // For incoming (DamageTaken/HealingTaken): filter/group by target, breakdown by source
        let (
            entity_col,
            entity_id_col,
            entity_type_col,
            breakdown_name_col,
            breakdown_class_col,
            breakdown_id_col,
        ) = if is_outgoing {
            (
                "source_name",
                "source_id",
                "source_entity_type",
                "target_name",
                "target_class_id",
                "target_id",
            )
        } else {
            (
                "target_name",
                "target_id",
                "target_entity_type",
                "source_name",
                "source_class_id",
                "source_id",
            )
        };

        // Instance id wins over name: it pins the query to one spawn.
        let entity_filter = match (entity_id, entity_name) {
            (Some(id), _) => Some(format!("{entity_id_col} = {id}")),
            (None, Some(n)) => Some(format!("{entity_col} = '{}'", sql_escape(n))),
            (None, None) => None,
        };

        // Build WHERE conditions
        // Damage tabs include miss events (defense_type_id matches avoidance types)
        let mut conditions = if is_damage_tab {
            vec![format!(
                "({value_col} > 0 OR defense_type_id IN ({}))",
                miss_ids_csv()
            )]
        } else {
            vec![format!("{value_col} > 0")]
        };
        if tab == DataTab::Damage {
            conditions.push("source_id != target_id".to_string());
        }
        if let Some(f) = &entity_filter {
            conditions.push(f.clone());
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

        // Per-target activation resolution via TargetSet events (Rust-side binary search).
        // Used when target breakdown is active for outgoing tabs with a specific entity.
        let resolve_act_targets = mode.by_ability
            && (mode.by_target_type || mode.by_target_instance)
            && is_outgoing
            && entity_name.is_some();

        // Build activation count CTE (skip when resolving per-target in Rust)
        let (act_cte, act_join, act_select) = if mode.by_ability && !resolve_act_targets {
            let mut act_conditions = vec![format!(
                "effect_id = {}",
                effect_id::ABILITYACTIVATE
            )];
            if let Some(f) = &entity_filter {
                act_conditions.push(f.clone());
            }
            if let Some(tr) = time_range {
                act_conditions.push(tr.sql_filter());
            }
            if let Some(types) = entity_types {
                let type_list = types
                    .iter()
                    .map(|t| format!("'{}'", sql_escape(t)))
                    .collect::<Vec<_>>()
                    .join(", ");
                act_conditions.push(format!("{} IN ({})", entity_type_col, type_list));
            }
            let act_filter = act_conditions.join(" AND ");
            (
                format!(
                    "WITH activations AS (\
                        SELECT ability_id, COUNT(*) as activation_count \
                        FROM events WHERE {act_filter} \
                        GROUP BY ability_id\
                    ) "
                ),
                " LEFT JOIN activations act ON b.ability_id = act.ability_id".to_string(),
                ", COALESCE(act.activation_count, 0) as activation_count".to_string(),
            )
        } else {
            (String::new(), String::new(), ", 0 as activation_count".to_string())
        };

        // Miss count expression (only for damage tabs)
        let miss_select = if is_damage_tab {
            format!(
                ", SUM(CASE WHEN defense_type_id IN ({}) THEN 1 ELSE 0 END) as miss_count",
                miss_ids_csv()
            )
        } else {
            ", 0 as miss_count".to_string()
        };

        // Crit total and effective healing
        let extra_agg = format!(
            ", SUM(CASE WHEN is_crit THEN CAST({value_col} AS DOUBLE) ELSE 0 END) as crit_total\
             , SUM(CAST(heal_effective AS DOUBLE)) as effective_total"
        );

        // Damage tab columns: damage_type, shield_count, absorbed_total
        let dt_select = if is_damage_tab {
            format!(
                ", MAX(dmg_type) as damage_type\
                 , SUM(CASE WHEN defense_type_id = {} THEN 1 ELSE 0 END) as shield_count\
                 , SUM(CAST(dmg_absorbed AS DOUBLE)) as absorbed_total",
                defense_type::SHIELD
            )
        } else {
            String::new()
        };

        // For damage tabs, hit_count should only count actual hits (not misses)
        let hit_count_expr = if is_damage_tab {
            format!(
                "SUM(CASE WHEN {value_col} > 0 THEN 1 ELSE 0 END) as hit_count"
            )
        } else {
            "COUNT(*) as hit_count".to_string()
        };

        // Build main query - wrap in subquery when using activation CTE
        let main_query = if mode.by_ability {
            format!(
                r#"{act_cte}SELECT b.*{act_select} FROM (
                SELECT {select_str},
                       SUM({value_col}) as total_value,
                       {hit_count_expr},
                       SUM(CASE WHEN is_crit THEN 1 ELSE 0 END) as crit_count,
                       MAX({value_col}) as max_hit,
                       SUM({value_col}) * 100.0 / SUM(SUM({value_col})) OVER () as percent_of_total
                       {miss_select}
                       {extra_agg}
                       {first_hit_col}
                       {dt_select}
                FROM events {filter}
                GROUP BY {group_str}
                ORDER BY total_value DESC
            ) b{act_join}"#
            )
        } else {
            format!(
                r#"SELECT {select_str},
                       SUM({value_col}) as total_value,
                       {hit_count_expr},
                       SUM(CASE WHEN is_crit THEN 1 ELSE 0 END) as crit_count,
                       MAX({value_col}) as max_hit,
                       SUM({value_col}) * 100.0 / SUM(SUM({value_col})) OVER () as percent_of_total
                       {miss_select}
                       {extra_agg}
                       {first_hit_col}
                       {dt_select}
                FROM events {filter}
                GROUP BY {group_str}
                ORDER BY total_value DESC"#
            )
        };

        let batches = self.sql(&main_query).await?;

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
            let miss_counts = col_i64(batch, col_idx)?;
            col_idx += 1;
            let crit_totals = col_f64(batch, col_idx)?;
            col_idx += 1;
            let effective_totals = col_f64(batch, col_idx)?;
            col_idx += 1;

            // Extract first_hit_secs if grouping by target instance
            let first_hit_times = if mode.by_target_instance {
                let v = col_f32(batch, col_idx)?;
                col_idx += 1;
                Some(v)
            } else {
                None
            };

            // Damage tab columns (inside inner subquery, before activation_count)
            let (damage_types, shield_counts, absorbed_totals) = if is_damage_tab {
                let dt = col_strings(batch, col_idx)?;
                col_idx += 1;
                let sc = col_i64(batch, col_idx)?;
                col_idx += 1;
                let at = col_f64(batch, col_idx)?;
                col_idx += 1;
                (dt, sc, at)
            } else {
                let n = batch.num_rows();
                (vec![String::new(); n], vec![0i64; n], vec![0.0f64; n])
            };

            // Activation count (last column — appended by outer JOIN in by_ability path)
            let activation_counts = if mode.by_ability {
                let v = col_i64(batch, col_idx)?;
                let _ = col_idx;
                v
            } else {
                vec![0; batch.num_rows()]
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
                    miss_count: miss_counts[i],
                    activation_count: activation_counts[i],
                    crit_total: crit_totals[i],
                    effective_total: effective_totals[i],
                    is_shield: false,
                    attack_type: ATTACK_TYPES.get(&ids[i]).copied().unwrap_or("").to_string(),
                    damage_type: damage_types[i].clone(),
                    shield_count: shield_counts[i],
                    absorbed_total: absorbed_totals[i],
                    dps: totals[i] / duration,
                    percent_of_total: percents[i],
                });
            }
        }

        // Resolve per-target activation counts from TargetSet timeline
        if resolve_act_targets {
            if let Ok(act_map) = self
                .resolve_activation_targets(
                    entity_name.unwrap(),
                    entity_col,
                    time_range,
                    tab.is_healing(),
                )
                .await
            {
                if mode.by_target_instance {
                    // Instance mode: exact match on (ability_id, target_name, target_id)
                    for result in &mut results {
                        let target = result.target_name.as_deref().unwrap_or("");
                        let tid = result.target_log_id.unwrap_or(0);
                        result.activation_count = act_map
                            .get(&(result.ability_id, target.to_string(), tid))
                            .copied()
                            .unwrap_or(0);
                    }
                } else {
                    // Type mode: aggregate across all instances of the same target name
                    let mut by_type: HashMap<(i64, String), i64> = HashMap::new();
                    for ((aid, name, _tid), count) in &act_map {
                        *by_type.entry((*aid, name.clone())).or_default() += count;
                    }
                    for result in &mut results {
                        let target = result.target_name.as_deref().unwrap_or("");
                        result.activation_count = by_type
                            .get(&(result.ability_id, target.to_string()))
                            .copied()
                            .unwrap_or(0);
                    }
                }
            }
        }

        // Append shield breakdown rows for healing tabs, then recalculate
        // percent_of_total so the denominator includes shield absorption.
        if tab.is_healing() && entity_name.is_some() {
            let is_incoming = tab == DataTab::HealingTaken;
            if let Ok(shields) = self
                .query_shield_breakdown(
                    entity_name.unwrap(),
                    time_range,
                    duration,
                    is_incoming,
                    mode,
                )
                .await
            {
                if !shields.is_empty() {
                    results.extend(shields);
                    // Recalculate percent_of_total across all rows (heals + shields)
                    let grand_total: f64 = results.iter().map(|r| r.total_value).sum();
                    if grand_total > 0.0 {
                        for r in &mut results {
                            r.percent_of_total = r.total_value / grand_total * 100.0;
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    /// Query shield absorption breakdown by ability. Returns rows with is_shield=true.
    /// Outgoing credits shields the entity cast; incoming credits shields that absorbed
    /// damage on the entity, optionally per-source when grouping by counterparty.
    async fn query_shield_breakdown(
        &self,
        entity_name: &str,
        time_range: Option<&TimeRange>,
        duration: f64,
        is_incoming: bool,
        mode: BreakdownMode,
    ) -> Result<Vec<AbilityBreakdown>, String> {
        let time_filter = time_range
            .map(|tr| format!("AND {}", tr.sql_filter()))
            .unwrap_or_default();
        let escaped_name = sql_escape(entity_name);

        let group_by_source = is_incoming && (mode.by_target_type || mode.by_target_instance);

        let (inner_target_filter, shield_source_filter) = if is_incoming {
            (format!("AND target_name = '{escaped_name}'"), String::new())
        } else {
            (
                String::new(),
                format!(
                    "AND CAST(shield['source_id'] AS BIGINT) IN (\
                        SELECT DISTINCT source_id FROM events \
                        WHERE source_name = '{escaped_name}' {time_filter}\
                    )"
                ),
            )
        };

        let (select_src, group_src) = if group_by_source {
            (
                ", CAST(shield['source_id'] AS BIGINT) as src_id",
                ", shield['source_id']",
            )
        } else {
            (", CAST(NULL AS BIGINT) as src_id", "")
        };

        let query = format!(
            r#"
            WITH shield_map AS (
                SELECT effect_id as shield_eid,
                       MIN(ability_id) as ability_id,
                       MIN(ability_name) as ability_name
                FROM events
                WHERE effect_type_id IN ({apply_effect}, {remove_effect})
                GROUP BY effect_id
            ),
            shield_totals AS (
                SELECT
                    CAST(shield['effect_id'] AS BIGINT) as shield_eid
                    {select_src},
                    SUM(CAST(dmg_absorbed AS BIGINT)) as total_absorbed,
                    COUNT(*) as hit_count
                FROM (
                    SELECT dmg_absorbed, UNNEST(active_shields) as shield
                    FROM events
                    WHERE dmg_absorbed > 0 AND cardinality(active_shields) > 0
                      {inner_target_filter}
                      {time_filter}
                )
                WHERE CAST(shield['position'] AS BIGINT) = 1
                  {shield_source_filter}
                GROUP BY shield['effect_id']{group_src}
            )
            SELECT COALESCE(sm.ability_id, st.shield_eid) as ability_id,
                   COALESCE(sm.ability_name, 'Unknown Shield') as ability_name,
                   COALESCE(st.total_absorbed, 0) as total_absorbed,
                   COALESCE(st.hit_count, 0) as hit_count,
                   st.src_id
            FROM shield_totals st
            LEFT JOIN shield_map sm ON st.shield_eid = sm.shield_eid
            WHERE st.total_absorbed > 0
            ORDER BY total_absorbed DESC
            "#,
            apply_effect = effect_type_id::APPLYEFFECT,
            remove_effect = effect_type_id::REMOVEEFFECT,
        );

        // Resolve src_id → (source_name, source_class_id) in Rust — matches the
        // overview.rs pattern for HPS shield attribution.
        let id_lookup: HashMap<i64, (String, i64)> = if group_by_source {
            let lookup_batches = self
                .sql(
                    "SELECT DISTINCT source_id, source_name, source_class_id FROM events \
                     WHERE source_name != ''",
                )
                .await
                .unwrap_or_default();
            let mut map = HashMap::new();
            for batch in &lookup_batches {
                let ids = col_i64(batch, 0)?;
                let names = col_strings(batch, 1)?;
                let class_ids = col_i64(batch, 2)?;
                for i in 0..batch.num_rows() {
                    map.entry(ids[i])
                        .or_insert_with(|| (names[i].clone(), class_ids[i]));
                }
            }
            map
        } else {
            HashMap::new()
        };

        let batches = self.sql(&query).await?;
        let mut results = Vec::new();

        for batch in &batches {
            let ids = col_i64(batch, 0)?;
            let names = col_strings(batch, 1)?;
            let totals = col_f64(batch, 2)?;
            let hits = col_i64(batch, 3)?;
            let src_id_col = batch.column(4);

            for i in 0..batch.num_rows() {
                let h = hits[i] as f64;

                let (tgt_name, tgt_log_id, tgt_class_id) = if group_by_source {
                    use datafusion::arrow::array::{Array, Int64Array};
                    let sid = src_id_col
                        .as_any()
                        .downcast_ref::<Int64Array>()
                        .and_then(|a| if Array::is_null(a, i) { None } else { Some(a.value(i)) });
                    match sid.and_then(|s| id_lookup.get(&s).map(|v| (s, v))) {
                        Some((s, (name, class_id))) => (Some(name.clone()), Some(s), Some(*class_id)),
                        None => (None, None, None),
                    }
                } else {
                    (None, None, None)
                };

                results.push(AbilityBreakdown {
                    ability_name: names[i].clone(),
                    ability_id: ids[i],
                    target_name: tgt_name,
                    target_class_id: tgt_class_id,
                    target_log_id: tgt_log_id,
                    target_first_hit_secs: None,
                    total_value: totals[i],
                    hit_count: hits[i],
                    crit_count: 0,
                    crit_rate: 0.0,
                    max_hit: 0.0,
                    avg_hit: if h > 0.0 { totals[i] / h } else { 0.0 },
                    miss_count: 0,
                    activation_count: 0,
                    crit_total: 0.0,
                    effective_total: totals[i], // shield absorption is fully effective
                    is_shield: true,
                    attack_type: String::new(),
                    damage_type: String::new(),
                    shield_count: 0,
                    absorbed_total: 0.0,
                    dps: totals[i] / duration,
                    percent_of_total: 0.0, // will be relative to heal total, leave 0
                });
            }
        }
        Ok(results)
    }

    /// Resolve per-target activation counts by cross-referencing TargetSet
    /// events with AbilityActivate events via binary search on the target timeline.
    /// For healing: if the current target is an NPC (or no target), attribute to
    /// the casting player (self-heal).
    /// Returns map keyed by (ability_id, target_name, target_id).
    async fn resolve_activation_targets(
        &self,
        entity_name: &str,
        entity_col: &str,
        time_range: Option<&TimeRange>,
        is_healing: bool,
    ) -> Result<HashMap<(i64, String, i64), i64>, String> {
        let time_filter = time_range
            .map(|tr| format!("AND {}", tr.sql_filter()))
            .unwrap_or_default();
        let escaped = sql_escape(entity_name);

        // Build target timeline from TargetSet events (includes target_id for instance mode)
        let target_batches = self
            .sql(&format!(
                "SELECT combat_time_secs, target_name, target_id, target_entity_type \
                 FROM events \
                 WHERE {entity_col} = '{escaped}' \
                   AND effect_id = {} \
                   {time_filter} \
                 ORDER BY combat_time_secs",
                effect_id::TARGETSET,
            ))
            .await?;

        // (time, target_name, target_id, target_entity_type)
        let mut timeline: Vec<(f32, String, i64, String)> = Vec::new();
        for batch in &target_batches {
            let times = col_f32(batch, 0)?;
            let names = col_strings(batch, 1)?;
            let ids = col_i64(batch, 2)?;
            let types = col_strings(batch, 3)?;
            for i in 0..batch.num_rows() {
                timeline.push((times[i], names[i].clone(), ids[i], types[i].clone()));
            }
        }

        // Get entity's own source_id for self-heal attribution
        let self_id = if is_healing {
            let id_batches = self
                .sql(&format!(
                    "SELECT DISTINCT source_id FROM events \
                     WHERE source_name = '{escaped}' LIMIT 1"
                ))
                .await?;
            id_batches
                .first()
                .and_then(|b| col_i64(b, 0).ok())
                .and_then(|v| v.first().copied())
                .unwrap_or(0)
        } else {
            0
        };

        // Get ability activations
        let act_batches = self
            .sql(&format!(
                "SELECT combat_time_secs, ability_id \
                 FROM events \
                 WHERE {entity_col} = '{escaped}' \
                   AND effect_id = {} \
                   {time_filter} \
                 ORDER BY combat_time_secs",
                effect_id::ABILITYACTIVATE,
            ))
            .await?;

        let mut counts: HashMap<(i64, String, i64), i64> = HashMap::new();

        for batch in &act_batches {
            let times = col_f32(batch, 0)?;
            let ability_ids = col_i64(batch, 1)?;

            for i in 0..batch.num_rows() {
                let act_time = times[i];
                let ability_id = ability_ids[i];

                // Binary search: find last TargetSet at or before this activation
                let idx = timeline.partition_point(|t| t.0 <= act_time);
                let (target_name, target_id) = if idx == 0 {
                    if is_healing {
                        (entity_name.to_string(), self_id)
                    } else {
                        continue;
                    }
                } else {
                    let (_, ref name, tid, ref ttype) = timeline[idx - 1];
                    if is_healing && ttype == "Npc" {
                        (entity_name.to_string(), self_id)
                    } else {
                        (name.clone(), tid)
                    }
                };

                *counts
                    .entry((ability_id, target_name, target_id))
                    .or_default() += 1;
            }
        }

        Ok(counts)
    }

    /// Query entity breakdown for any data tab.
    /// - For outgoing tabs (Damage/Healing): groups by source entity.
    /// - For incoming tabs (DamageTaken/HealingTaken): groups by target entity (who received).
    /// - For Charts: every entity that activated an ability or applied an effect, so
    ///   buff-only actors (e.g. an add that only shields the boss) are listed too.
    pub async fn breakdown_by_entity(
        &self,
        tab: DataTab,
        time_range: Option<&TimeRange>,
    ) -> Result<Vec<EntityBreakdown>, String> {
        if tab == DataTab::Charts {
            return self.actor_roster(time_range).await;
        }
        let value_col = tab.value_column();
        let is_outgoing = tab.is_outgoing();
        let is_healing_taken = tab == DataTab::HealingTaken;
        let is_healing_outgoing = tab == DataTab::Healing;

        // For outgoing: group by source (who dealt)
        // For incoming: group by target (who received)
        let (name_col, id_col, type_col) = if is_outgoing {
            ("source_name", "source_id", "source_entity_type")
        } else {
            ("target_name", "target_id", "target_entity_type")
        };

        let mut conditions = vec![format!("{} > 0", value_col)];
        if tab == DataTab::Damage {
            conditions.push("source_id != target_id".to_string());
        }
        if let Some(tr) = time_range {
            conditions.push(tr.sql_filter());
        }
        let filter = format!("WHERE {}", conditions.join(" AND "));

        // HealingTaken adds shielding received (dmg_absorbed on the target) to heal totals,
        // mirroring how HPS in overview.rs credits shielding to the caster.
        let query = if is_healing_taken {
            let time_filter = time_range
                .map(|tr| format!("AND {}", tr.sql_filter()))
                .unwrap_or_default();
            format!(
                r#"
            WITH healing AS (
                SELECT {name_col}, {id_col}, MIN({type_col}) as entity_type,
                       SUM({value_col}) as heal_total,
                       COUNT(DISTINCT ability_id) as abilities_used,
                       MIN(combat_time_secs) as first_hit
                FROM events {filter}
                GROUP BY {name_col}, {id_col}
            ),
            shielding AS (
                SELECT {name_col}, {id_col}, MIN({type_col}) as entity_type,
                       SUM(dmg_absorbed) as shield_total,
                       MIN(combat_time_secs) as first_hit
                FROM events
                WHERE dmg_absorbed > 0 AND cardinality(active_shields) > 0 {time_filter}
                GROUP BY {name_col}, {id_col}
            )
            SELECT COALESCE(h.{name_col}, s.{name_col}) as {name_col},
                   COALESCE(h.{id_col}, s.{id_col}) as {id_col},
                   COALESCE(h.entity_type, s.entity_type) as entity_type,
                   COALESCE(h.heal_total, 0) + COALESCE(s.shield_total, 0) as total_value,
                   COALESCE(h.abilities_used, 0) as abilities_used,
                   LEAST(h.first_hit, s.first_hit) as first_hit_secs
            FROM healing h
            FULL OUTER JOIN shielding s
              ON h.{name_col} = s.{name_col} AND h.{id_col} = s.{id_col}
            ORDER BY total_value DESC
            "#
            )
        } else if is_healing_outgoing {
            // Outgoing healing credits shielding to the shield's caster, not to the
            // source of the absorbed-damage event. Attribute via the pre-computed
            // `active_shields` column (FIFO: position=1 holds the full dmg_absorbed),
            // mirroring query_shield_attribution in overview.rs.
            let time_filter = time_range
                .map(|tr| format!("AND {}", tr.sql_filter()))
                .unwrap_or_default();
            format!(
                r#"
            WITH healing AS (
                SELECT {name_col}, {id_col}, MIN({type_col}) as entity_type,
                       SUM({value_col}) as heal_total,
                       COUNT(DISTINCT ability_id) as abilities_used,
                       MIN(combat_time_secs) as first_hit
                FROM events {filter}
                GROUP BY {name_col}, {id_col}
            ),
            shield_raw AS (
                SELECT CAST(shield['source_id'] AS BIGINT) as sid,
                       CAST(dmg_absorbed AS BIGINT) as absorbed
                FROM (
                    SELECT dmg_absorbed, UNNEST(active_shields) as shield
                    FROM events
                    WHERE dmg_absorbed > 0 AND cardinality(active_shields) > 0 {time_filter}
                )
                WHERE CAST(shield['position'] AS BIGINT) = 1
            ),
            entities AS (
                SELECT source_id, MIN(source_name) as source_name,
                       MIN(source_entity_type) as entity_type
                FROM events GROUP BY source_id
            ),
            shielding AS (
                SELECT sr.sid as {id_col}, e.source_name as {name_col}, e.entity_type,
                       SUM(sr.absorbed) as shield_total
                FROM shield_raw sr
                JOIN entities e ON sr.sid = e.source_id
                GROUP BY sr.sid, e.source_name, e.entity_type
            )
            SELECT COALESCE(h.{name_col}, s.{name_col}) as {name_col},
                   COALESCE(h.{id_col}, s.{id_col}) as {id_col},
                   COALESCE(h.entity_type, s.entity_type) as entity_type,
                   COALESCE(h.heal_total, 0) + COALESCE(s.shield_total, 0) as total_value,
                   COALESCE(h.abilities_used, 0) as abilities_used,
                   h.first_hit as first_hit_secs
            FROM healing h
            FULL OUTER JOIN shielding s
              ON h.{id_col} = s.{id_col}
            ORDER BY total_value DESC
            "#
            )
        } else {
            format!(
                r#"
            SELECT {name_col}, {id_col}, MIN({type_col}) as entity_type,
                   SUM({value_col}) as total_value,
                   COUNT(DISTINCT ability_id) as abilities_used,
                   MIN(combat_time_secs) as first_hit_secs
            FROM events {filter}
            GROUP BY {name_col}, {id_col}
            ORDER BY total_value DESC
            "#
            )
        };

        self.collect_entity_rows(&query).await
    }

    /// Every source entity that acted during the range (ability activation or
    /// effect application), with damage + healing as the sort value.
    async fn actor_roster(
        &self,
        time_range: Option<&TimeRange>,
    ) -> Result<Vec<EntityBreakdown>, String> {
        let time_filter = time_range
            .map(|tr| format!("AND {}", tr.sql_filter()))
            .unwrap_or_default();
        let query = format!(
            r#"
            SELECT source_name, source_id, MIN(source_entity_type) as entity_type,
                   CAST(SUM(dmg_amount + heal_amount) AS DOUBLE) as total_value,
                   COUNT(DISTINCT ability_id) as abilities_used,
                   MIN(combat_time_secs) as first_hit_secs
            FROM events
            WHERE source_id != 0
              AND source_entity_type IN ('Player', 'Companion', 'Npc')
              AND (effect_type_id = {apply_effect} OR effect_id = {ability_activate}) {time_filter}
            GROUP BY source_name, source_id
            ORDER BY total_value DESC
            "#,
            apply_effect = effect_type_id::APPLYEFFECT,
            ability_activate = effect_id::ABILITYACTIVATE,
        );
        self.collect_entity_rows(&query).await
    }

    async fn collect_entity_rows(&self, query: &str) -> Result<Vec<EntityBreakdown>, String> {
        let batches = self.sql(query).await?;

        let mut results = Vec::new();
        for batch in &batches {
            let names = col_strings(batch, 0)?;
            let ids = col_i64(batch, 1)?;
            let entity_types = col_strings(batch, 2)?;
            let totals = col_f64(batch, 3)?;
            let abilities = col_i64(batch, 4)?;
            let first_hits = col_opt_f32(batch, 5)?;

            for i in 0..batch.num_rows() {
                results.push(EntityBreakdown {
                    source_name: names[i].clone(),
                    source_id: ids[i],
                    entity_type: entity_types[i].clone(),
                    total_value: totals[i],
                    abilities_used: abilities[i],
                    first_hit_secs: first_hits[i],
                });
            }
        }
        Ok(results)
    }

    /// Query damage taken summary: damage type breakdown + mitigation stats.
    pub async fn query_damage_taken_summary(
        &self,
        target_name: &str,
        target_id: Option<i64>,
        time_range: Option<&TimeRange>,
        entity_types: Option<&[&str]>,
    ) -> Result<DamageTakenSummary, String> {
        let name = sql_escape(target_name);
        // Instance id pins the summary to one NPC spawn; players fall back to name.
        let target_filter = match target_id {
            Some(id) => format!("target_id = {id}"),
            None => format!("target_name = '{name}'"),
        };
        let time_filter = time_range
            .map(|tr| format!("AND {}", tr.sql_filter()))
            .unwrap_or_default();
        let entity_filter = entity_types
            .filter(|et| !et.is_empty())
            .map(|types| {
                let list = types
                    .iter()
                    .map(|t| format!("'{}'", sql_escape(t)))
                    .collect::<Vec<_>>()
                    .join(",");
                format!("AND source_entity_type IN ({})", list)
            })
            .unwrap_or_default();
        let miss_ids = miss_ids_csv();

        // Main totals query
        let batches = self
            .sql(&format!(
                r#"
            SELECT
                COALESCE(SUM(CAST(dmg_amount AS DOUBLE)), 0) as total_damage,
                COALESCE(SUM(CASE WHEN dmg_type IN ('internal','elemental') THEN CAST(dmg_amount AS DOUBLE) ELSE 0 END), 0) as ie_total,
                COALESCE(SUM(CASE WHEN dmg_type IN ('kinetic','energy') THEN CAST(dmg_amount AS DOUBLE) ELSE 0 END), 0) as ke_total,
                COUNT(*) as total_attempts,
                SUM(CASE WHEN defense_type_id IN ({miss_ids}) THEN 1 ELSE 0 END) as avoided_count,
                SUM(CASE WHEN defense_type_id = {shield} THEN 1 ELSE 0 END) as shielded_count,
                SUM(CASE WHEN dmg_amount > 0 THEN 1 ELSE 0 END) as hit_count
            FROM events
            WHERE {target_filter}
              AND (dmg_amount > 0 OR defense_type_id IN ({miss_ids}))
              {time_filter} {entity_filter}
            "#,
                shield = defense_type::SHIELD,
            ))
            .await?;

        let (total_damage, ie_total, ke_total, total_attempts, avoided_count, shielded_count, hit_count) =
            if let Some(batch) = batches.first()
                && batch.num_rows() > 0
            {
                (
                    col_f64(batch, 0)?[0],
                    col_f64(batch, 1)?[0],
                    col_f64(batch, 2)?[0],
                    col_i64(batch, 3)?[0] as f64,
                    col_i64(batch, 4)?[0] as f64,
                    col_i64(batch, 5)?[0] as f64,
                    col_i64(batch, 6)?[0] as f64,
                )
            } else {
                return Ok(DamageTakenSummary::default());
            };

        if total_damage == 0.0 && total_attempts == 0.0 {
            return Ok(DamageTakenSummary::default());
        }

        // Absorbed self vs given: use UNNEST on active_shields, credit position=1
        let target_id = match target_id {
            Some(id) => id,
            None => self
                .sql(&format!(
                    "SELECT DISTINCT source_id FROM events WHERE source_name = '{name}' LIMIT 1"
                ))
                .await?
                .first()
                .and_then(|b| if b.num_rows() > 0 { Some(col_i64(b, 0).ok()?[0]) } else { None })
                .unwrap_or(0),
        };

        let absorbed_batches = self
            .sql(&format!(
                r#"
            SELECT
                CASE WHEN CAST(shield['source_id'] AS BIGINT) = {target_id} THEN 'self' ELSE 'given' END as source_type,
                SUM(CAST(dmg_absorbed AS DOUBLE)) as absorbed
            FROM (
                SELECT dmg_absorbed, UNNEST(active_shields) as shield
                FROM events
                WHERE {target_filter}
                  AND dmg_absorbed > 0 AND cardinality(active_shields) > 0
                  {time_filter} {entity_filter}
            )
            WHERE CAST(shield['position'] AS BIGINT) = 1
            GROUP BY source_type
            "#,
            ))
            .await;

        let (mut absorbed_self, mut absorbed_given) = (0.0, 0.0);
        if let Ok(ref batches) = absorbed_batches {
            for batch in batches {
                let types = col_strings(batch, 0)?;
                let amounts = col_f64(batch, 1)?;
                for i in 0..batch.num_rows() {
                    match types[i].as_str() {
                        "self" => absorbed_self = amounts[i],
                        _ => absorbed_given = amounts[i],
                    }
                }
            }
        }

        // Attack type breakdown (Force/Tech vs Melee/Ranged) via per-ability totals
        let at_batches = self
            .sql(&format!(
                r#"SELECT ability_id, SUM(CAST(dmg_amount AS DOUBLE)) as total
                FROM events
                WHERE {target_filter} AND dmg_amount > 0
                  {time_filter} {entity_filter}
                GROUP BY ability_id"#,
            ))
            .await?;

        let (mut ft_total, mut mr_total) = (0.0, 0.0);
        for batch in &at_batches {
            let ids = col_i64(batch, 0)?;
            let totals = col_f64(batch, 1)?;
            for i in 0..batch.num_rows() {
                match ATTACK_TYPES.get(&ids[i]).copied() {
                    Some("Force" | "Tech") => ft_total += totals[i],
                    Some("Melee" | "Ranged") => mr_total += totals[i],
                    _ => {}
                }
            }
        }

        let total_absorbed = absorbed_self + absorbed_given;
        Ok(DamageTakenSummary {
            internal_elemental_total: ie_total,
            internal_elemental_pct: if total_damage > 0.0 { ie_total / total_damage * 100.0 } else { 0.0 },
            kinetic_energy_total: ke_total,
            kinetic_energy_pct: if total_damage > 0.0 { ke_total / total_damage * 100.0 } else { 0.0 },
            force_tech_total: ft_total,
            force_tech_pct: if total_damage > 0.0 { ft_total / total_damage * 100.0 } else { 0.0 },
            melee_ranged_total: mr_total,
            melee_ranged_pct: if total_damage > 0.0 { mr_total / total_damage * 100.0 } else { 0.0 },
            avoided_pct: if total_attempts > 0.0 { avoided_count / total_attempts * 100.0 } else { 0.0 },
            shielded_pct: if hit_count > 0.0 { shielded_count / hit_count * 100.0 } else { 0.0 },
            absorbed_self_total: absorbed_self,
            absorbed_self_pct: if total_damage + total_absorbed > 0.0 { absorbed_self / (total_damage + total_absorbed) * 100.0 } else { 0.0 },
            absorbed_given_total: absorbed_given,
            absorbed_given_pct: if total_damage + total_absorbed > 0.0 { absorbed_given / (total_damage + total_absorbed) * 100.0 } else { 0.0 },
        })
    }
}
