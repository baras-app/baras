//! Effect uptime and window queries.

use datafusion::arrow::array::Array;

use super::*;

// Effect type IDs (the type of log event)
const APPLY_EFFECT: i64 = 836045448945477;
const REMOVE_EFFECT: i64 = 836045448945478;
// Effect IDs (what specifically happened)
const ABILITY_ACTIVATE: i64 = 836045448945479;
// Exclude damage/heal "effects" which are action results, not buffs
const DAMAGE_EFFECT: i64 = 836045448945501;
const HEAL_EFFECT: i64 = 836045448945500;

/// Build a SQL WHERE clause fragment for source filtering.
/// Returns an AND clause or empty string. `target` is an entity selector
/// (name or `id:<instance>`, see `entity_match_sql`).
fn source_filter_clause(source_filter: Option<&str>, target: Option<&str>) -> String {
    match source_filter {
        Some("self") => {
            // Self-applied: the source is the entity being inspected
            if let Some(sel) = target {
                format!("AND {}", entity_match_sql("source", sel))
            } else {
                String::new()
            }
        }
        Some("other_players") => {
            // Other players/companions, excluding self
            if let Some(sel) = target {
                format!(
                    "AND source_entity_type IN ('Player', 'Companion') AND NOT ({})",
                    entity_match_sql("source", sel)
                )
            } else {
                "AND source_entity_type IN ('Player', 'Companion')".to_string()
            }
        }
        Some("npcs") => "AND source_entity_type = 'Npc'".to_string(),
        _ => String::new(), // "all" or None — no filter
    }
}

impl EncounterQuery<'_> {
    /// Query effect uptime statistics for the charts panel.
    /// Returns aggregated data per effect (count, duration, uptime%).
    /// Effects are classified as active (triggered by ability) or passive (proc/auto-applied).
    ///
    /// The algorithm:
    /// 1. Build ALL effect windows from every apply/remove pair across the encounter.
    ///    Pre-combat events (combat_time_secs IS NULL) are included via COALESCE to 0.0,
    ///    so effects applied before combat count from combat start.
    /// 2. Each apply window ends at MIN(next_apply, first_remove_after, encounter_end).
    /// 3. Filter to windows that overlap the target time range, clamping edges to the range.
    ///    This generalises for full-combat, user-selected time ranges, and pre-combat effects.
    /// 4. Merge overlapping intervals per effect before summing, preventing >100% uptime.
    /// 5. Classify active vs passive by checking if there's an AbilityActivate event
    ///    at the same timestamp with matching ability_id OR matching ability_name = effect_name.
    pub async fn query_effect_uptime(
        &self,
        target_name: Option<&str>,
        time_range: Option<&TimeRange>,
        duration_secs: f32,
        source_filter: Option<&str>,
    ) -> Result<Vec<EffectChartData>, String> {
        let target_filter = target_name
            .map(|n| format!("AND {}", entity_match_sql("target", n)))
            .unwrap_or_default();
        let src_filter = source_filter_clause(source_filter, target_name);
        let duration = duration_secs.max(0.001);
        let range_start = time_range.map(|tr| tr.start).unwrap_or(0.0);
        let range_end = time_range.map(|tr| tr.end).unwrap_or(duration);
        let range_duration = (range_end - range_start).max(0.001);

        // Infer pre-combat applications from "orphan" removes — REMOVE events
        // with no preceding APPLY for the same (effect_id, target_name). We
        // synthesize an apply at time 0.0 so the existing pipeline pairs it
        // with the first remove. The source filter applies to removes too,
        // since EffectRemoved's source_name reflects the original applier.
        let synthetic_cte = r#"
            synthetic_applies AS (
                SELECT fr.effect_id, fr.effect_name,
                       '' as ability_name,
                       fr.target_name,
                       CAST(0.0 AS REAL) as apply_time,
                       CAST(NULL AS TIMESTAMP) as timestamp,
                       CAST(0 AS BIGINT) as line_number
                FROM (
                    SELECT effect_id, effect_name, target_name,
                           MIN(remove_time) as first_remove_time
                    FROM removes
                    GROUP BY effect_id, effect_name, target_name
                ) fr
                LEFT JOIN (
                    SELECT effect_id, target_name, MIN(apply_time) as min_apply
                    FROM real_applies
                    GROUP BY effect_id, target_name
                ) a
                  ON fr.effect_id = a.effect_id AND fr.target_name = a.target_name
                WHERE a.min_apply IS NULL OR fr.first_remove_time < a.min_apply
            ),"#;
        let synthetic_union = "UNION ALL
                SELECT effect_id, effect_name, ability_name, target_name,
                       apply_time, timestamp, line_number
                FROM synthetic_applies";

        // Strategy: Build ALL effect windows from every apply/remove pair (including
        // pre-combat events via COALESCE), then filter to windows overlapping the
        // target range and clamp edges. This handles effects applied before combat
        // or before a selected time range.
        let batches = self.sql(&format!(r#"
            WITH real_applies AS (
                SELECT effect_id, effect_name, ability_name, target_name,
                       COALESCE(combat_time_secs, 0.0) as apply_time, timestamp,
                       line_number
                FROM events
                WHERE effect_type_id = {APPLY_EFFECT}
                  AND effect_id NOT IN ({DAMAGE_EFFECT}, {HEAL_EFFECT})
                  {target_filter}
                  {src_filter}
            ),
            removes AS (
                SELECT effect_id, effect_name, target_name,
                       COALESCE(combat_time_secs, 0.0) as remove_time
                FROM events
                WHERE effect_type_id = {REMOVE_EFFECT}
                  AND effect_id NOT IN ({DAMAGE_EFFECT}, {HEAL_EFFECT})
                  {target_filter}
                  {src_filter}
            ),{synthetic_cte}
            applies AS (
                SELECT effect_id, effect_name, ability_name, target_name, apply_time, timestamp,
                       LEAD(apply_time) OVER (
                           PARTITION BY effect_id, target_name
                           ORDER BY apply_time, line_number
                       ) as next_apply_time
                FROM (
                    SELECT effect_id, effect_name, ability_name, target_name,
                           apply_time, timestamp, line_number
                    FROM real_applies
                    {synthetic_union}
                ) all_apply_events
            ),
            apply_with_remove AS (
                SELECT a.effect_id, a.effect_name, a.ability_name, a.apply_time, a.timestamp,
                       a.next_apply_time,
                       MIN(r.remove_time) as first_remove_time
                FROM applies a
                LEFT JOIN removes r
                    ON a.effect_id = r.effect_id
                    AND a.target_name = r.target_name
                    AND r.remove_time >= a.apply_time
                GROUP BY a.effect_id, a.effect_name, a.ability_name,
                         a.apply_time, a.timestamp, a.next_apply_time
            ),
            windows AS (
                SELECT effect_id, effect_name, ability_name, apply_time, timestamp,
                       LEAST(
                           COALESCE(next_apply_time, {duration}),
                           COALESCE(first_remove_time, {duration}),
                           {duration}
                       ) as end_time
                FROM apply_with_remove
            ),
            valid_windows AS (
                SELECT effect_id, effect_name, ability_name, clamped_start as apply_time,
                       clamped_end as end_time, timestamp
                FROM (
                    SELECT effect_id, effect_name, ability_name,
                           GREATEST(apply_time, {range_start}) as clamped_start,
                           LEAST(end_time, {range_end}) as clamped_end,
                           timestamp
                    FROM windows
                    WHERE apply_time < {range_end} AND end_time > {range_start}
                ) clamped
                WHERE clamped_end > clamped_start
            ),
            ability_activations AS (
                SELECT DISTINCT timestamp as activation_ts, ability_id, ability_name as act_name
                FROM events
                WHERE effect_id = {ABILITY_ACTIVATE}
            ),
            classified AS (
                SELECT w.effect_id, w.effect_name, w.apply_time, w.end_time,
                       w.end_time - w.apply_time as duration_secs,
                       CASE
                           WHEN aa.activation_ts IS NOT NULL THEN true
                           ELSE false
                       END as is_active,
                       aa.ability_id
                FROM valid_windows w
                LEFT JOIN ability_activations aa
                    ON w.timestamp = aa.activation_ts
                    AND (w.effect_id = aa.ability_id OR w.ability_name = aa.act_name)
            ),
            deduped AS (
                SELECT effect_id, effect_name,
                       BOOL_OR(is_active) as is_active,
                       MIN(ability_id) as ability_id,
                       apply_time as start_t,
                       end_time as end_t
                FROM classified
                GROUP BY effect_id, effect_name, apply_time, end_time
            ),
            ordered AS (
                SELECT effect_id, effect_name, is_active, ability_id,
                       start_t, end_t,
                       ROW_NUMBER() OVER (PARTITION BY effect_id ORDER BY start_t, end_t) as rn
                FROM deduped
            ),
            merge_pass AS (
                SELECT effect_id, effect_name, is_active, ability_id,
                       start_t, end_t, rn,
                       SUM(CASE WHEN start_t > prev_end THEN 1 ELSE 0 END) OVER (
                           PARTITION BY effect_id ORDER BY rn
                       ) as grp
                FROM (
                    SELECT *,
                           MAX(end_t) OVER (
                               PARTITION BY effect_id ORDER BY rn
                               ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING
                           ) as prev_end
                    FROM ordered
                ) sub
            ),
            merged AS (
                SELECT effect_id, effect_name,
                       BOOL_OR(is_active) as is_active,
                       MIN(ability_id) as ability_id,
                       MIN(start_t) as merged_start,
                       MAX(end_t) as merged_end
                FROM merge_pass
                GROUP BY effect_id, effect_name, grp
            ),
            counts AS (
                SELECT effect_id, COUNT(*) as count
                FROM deduped
                GROUP BY effect_id
            ),
            aggregated AS (
                SELECT m.effect_id, m.effect_name,
                       BOOL_OR(m.is_active) as is_active,
                       MIN(m.ability_id) as ability_id,
                       MIN(c.count) as count,
                       SUM(m.merged_end - m.merged_start) as total_duration
                FROM merged m
                JOIN counts c ON m.effect_id = c.effect_id
                GROUP BY m.effect_id, m.effect_name
            )
            SELECT effect_id, effect_name, ability_id, is_active, count, total_duration,
                   LEAST(total_duration * 100.0 / {range_duration}, 100.0) as uptime_pct
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
    /// Builds all windows from every apply/remove pair (including pre-combat via COALESCE),
    /// filters to those overlapping the target range, clamps edges, then merges overlapping
    /// intervals.
    pub async fn query_effect_windows(
        &self,
        effect_id: i64,
        target_name: Option<&str>,
        time_range: Option<&TimeRange>,
        duration_secs: f32,
        source_filter: Option<&str>,
    ) -> Result<Vec<EffectWindow>, String> {
        let target_filter = target_name
            .map(|n| format!("AND {}", entity_match_sql("target", n)))
            .unwrap_or_default();
        let src_filter = source_filter_clause(source_filter, target_name);
        let duration = duration_secs.max(0.001);
        let range_start = time_range.map(|tr| tr.start).unwrap_or(0.0);
        let range_end = time_range.map(|tr| tr.end).unwrap_or(duration);

        // Synthesize a pre-combat apply at time 0.0 for any target_name whose
        // first remove precedes any apply (or where no apply exists). The
        // source filter applies to removes too, since EffectRemoved's source
        // reflects the original applier.
        let synthetic_cte = r#"
            synthetic_applies AS (
                SELECT CAST(0.0 AS REAL) as apply_time, fr.target_name,
                       CAST(0 AS BIGINT) as line_number
                FROM (
                    SELECT target_name, MIN(remove_time) as first_remove_time
                    FROM removes
                    GROUP BY target_name
                ) fr
                LEFT JOIN (
                    SELECT target_name, MIN(apply_time) as min_apply
                    FROM real_applies
                    GROUP BY target_name
                ) a
                  ON fr.target_name = a.target_name
                WHERE a.min_apply IS NULL OR fr.first_remove_time < a.min_apply
            ),"#;
        let synthetic_union = "UNION ALL
                SELECT apply_time, target_name, line_number FROM synthetic_applies";

        let batches = self
            .sql(&format!(
                r#"
            WITH real_applies AS (
                SELECT COALESCE(combat_time_secs, 0.0) as apply_time, target_name,
                       line_number
                FROM events
                WHERE effect_type_id = {APPLY_EFFECT}
                  AND effect_id = {effect_id}
                  {target_filter}
                  {src_filter}
            ),
            removes AS (
                SELECT COALESCE(combat_time_secs, 0.0) as remove_time, target_name
                FROM events
                WHERE effect_type_id = {REMOVE_EFFECT}
                  AND effect_id = {effect_id}
                  {target_filter}
                  {src_filter}
            ),{synthetic_cte}
            applies AS (
                SELECT apply_time, target_name,
                       LEAD(apply_time) OVER (
                           PARTITION BY target_name
                           ORDER BY apply_time, line_number
                       ) as next_apply_time
                FROM (
                    SELECT apply_time, target_name, line_number
                    FROM real_applies
                    {synthetic_union}
                ) all_apply_events
            ),
            apply_with_remove AS (
                SELECT a.apply_time, a.next_apply_time,
                       MIN(r.remove_time) as first_remove_time
                FROM applies a
                LEFT JOIN removes r
                    ON a.target_name = r.target_name
                    AND r.remove_time >= a.apply_time
                GROUP BY a.apply_time, a.next_apply_time
            ),
            windows AS (
                SELECT GREATEST(apply_time, {range_start}) as start_secs,
                       LEAST(
                           COALESCE(next_apply_time, {duration}),
                           COALESCE(first_remove_time, {duration}),
                           {duration},
                           {range_end}
                       ) as end_secs
                FROM apply_with_remove
                WHERE apply_time < {range_end}
                  AND LEAST(
                      COALESCE(next_apply_time, {duration}),
                      COALESCE(first_remove_time, {duration}),
                      {duration}
                  ) > {range_start}
            ),
            ordered AS (
                SELECT start_secs, end_secs,
                       ROW_NUMBER() OVER (ORDER BY start_secs, end_secs) as rn
                FROM windows
                WHERE end_secs > start_secs
            ),
            merge_pass AS (
                SELECT start_secs, end_secs, rn,
                       SUM(CASE WHEN start_secs > prev_end THEN 1 ELSE 0 END) OVER (
                           ORDER BY rn
                       ) as grp
                FROM (
                    SELECT *,
                           MAX(end_secs) OVER (
                               ORDER BY rn
                               ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING
                           ) as prev_end
                    FROM ordered
                ) sub
            ),
            merged AS (
                SELECT MIN(start_secs) as start_secs, MAX(end_secs) as end_secs
                FROM merge_pass
                GROUP BY grp
            )
            SELECT start_secs, end_secs
            FROM merged
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
}
