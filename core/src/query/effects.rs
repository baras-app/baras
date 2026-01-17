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

impl EncounterQuery<'_> {
    /// Query effect uptime statistics for the charts panel.
    /// Returns aggregated data per effect (count, duration, uptime%).
    /// Effects are classified as active (triggered by ability) or passive (proc/auto-applied).
    pub async fn query_effect_uptime(
        &self,
        target_name: Option<&str>,
        time_range: Option<&TimeRange>,
        duration_secs: f32,
    ) -> Result<Vec<EffectChartData>, String> {
        let target_filter = target_name
            .map(|n| format!("AND target_name = '{}'", sql_escape(n)))
            .unwrap_or_default();
        let time_filter = time_range
            .map(|tr| format!("AND {}", tr.sql_filter()))
            .unwrap_or_default();
        let duration = duration_secs.max(0.001);

        // Query pairs ApplyEffect with RemoveEffect events, calculates duration,
        // and detects active vs passive based on whether there's an AbilityActivate
        // event at the exact same timestamp (1:1 match in logs).
        // Group by effect_name only (not effect_id) to consolidate variants.
        // Cap uptime at 100% since overlapping windows aren't merged.
        let batches = self.sql(&format!(r#"
            WITH applies AS (
                SELECT effect_id, effect_name, target_name, combat_time_secs as apply_time, timestamp,
                       ROW_NUMBER() OVER (PARTITION BY effect_id, target_name ORDER BY combat_time_secs) as seq
                FROM events
                WHERE effect_type_id = {APPLY_EFFECT}
                  AND effect_id NOT IN ({DAMAGE_EFFECT}, {HEAL_EFFECT})
                  {target_filter}
                  {time_filter}
            ),
            removes AS (
                SELECT effect_id, target_name, combat_time_secs as remove_time,
                       ROW_NUMBER() OVER (PARTITION BY effect_id, target_name ORDER BY combat_time_secs) as seq
                FROM events
                WHERE effect_type_id = {REMOVE_EFFECT}
                  AND effect_id NOT IN ({DAMAGE_EFFECT}, {HEAL_EFFECT})
                  {target_filter}
                  {time_filter}
            ),
            ability_activations AS (
                SELECT DISTINCT timestamp as activation_ts, ability_id
                FROM events
                WHERE effect_id = {ABILITY_ACTIVATE}
                  {time_filter}
            ),
            paired AS (
                SELECT a.effect_id, a.effect_name, a.apply_time, a.timestamp,
                       COALESCE(r.remove_time, {duration}) as remove_time
                FROM applies a
                LEFT JOIN removes r ON a.effect_id = r.effect_id
                    AND a.target_name = r.target_name
                    AND a.seq = r.seq
            ),
            classified AS (
                SELECT p.effect_id, p.effect_name, p.apply_time,
                       LEAST(p.remove_time, {duration}) - p.apply_time as duration_secs,
                       CASE WHEN aa.activation_ts IS NOT NULL THEN true ELSE false END as is_active,
                       aa.ability_id
                FROM paired p
                LEFT JOIN ability_activations aa ON p.timestamp = aa.activation_ts
                    AND p.effect_id = aa.ability_id
                WHERE p.remove_time > p.apply_time
            ),
            aggregated AS (
                SELECT MIN(effect_id) as effect_id, effect_name, is_active,
                       MIN(ability_id) as ability_id,
                       COUNT(*) as count,
                       SUM(duration_secs) as total_duration
                FROM classified
                GROUP BY effect_name, is_active
            )
            SELECT effect_id, effect_name, ability_id, is_active, count, total_duration,
                   LEAST(total_duration * 100.0 / {duration}, 100.0) as uptime_pct
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
    pub async fn query_effect_windows(
        &self,
        effect_id: i64,
        target_name: Option<&str>,
        time_range: Option<&TimeRange>,
        duration_secs: f32,
    ) -> Result<Vec<EffectWindow>, String> {
        let target_filter = target_name
            .map(|n| format!("AND target_name = '{}'", sql_escape(n)))
            .unwrap_or_default();
        let time_filter = time_range
            .map(|tr| format!("AND {}", tr.sql_filter()))
            .unwrap_or_default();
        let duration = duration_secs.max(0.001);

        let batches = self
            .sql(&format!(
                r#"
            WITH applies AS (
                SELECT combat_time_secs as apply_time, target_name,
                       ROW_NUMBER() OVER (PARTITION BY target_name ORDER BY combat_time_secs) as seq
                FROM events
                WHERE effect_type_id = {APPLY_EFFECT}
                  AND effect_id = {effect_id}
                  {target_filter}
                  {time_filter}
            ),
            removes AS (
                SELECT combat_time_secs as remove_time, target_name,
                       ROW_NUMBER() OVER (PARTITION BY target_name ORDER BY combat_time_secs) as seq
                FROM events
                WHERE effect_type_id = {REMOVE_EFFECT}
                  AND effect_id = {effect_id}
                  {target_filter}
                  {time_filter}
            )
            SELECT a.apply_time as start_secs,
                   LEAST(COALESCE(r.remove_time, {duration}), {duration}) as end_secs
            FROM applies a
            LEFT JOIN removes r ON a.target_name = r.target_name AND a.seq = r.seq
            WHERE COALESCE(r.remove_time, {duration}) > a.apply_time
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
