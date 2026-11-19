//! Ability usage statistics queries.
//!
//! Provides per-player ability activation frequency and inter-cast timing analysis.

use std::collections::BTreeMap;

use super::*;
use crate::game_data::effect_id;

impl EncounterQuery<'_> {
    /// Query ability usage statistics for a single player.
    ///
    /// Returns one row per distinct ability, with cast count, first/last cast timestamps,
    /// and inter-cast timing statistics (avg, median, min, max). Also includes the raw
    /// cast timestamps for timeline visualization.
    pub async fn query_ability_usage(
        &self,
        source_name: &str,
        time_range: Option<&TimeRange>,
    ) -> Result<Vec<AbilityUsageRow>, String> {
        let mut conditions = vec![
            format!("effect_id = {}", effect_id::ABILITYACTIVATE),
            format!("source_name = '{}'", sql_escape(source_name)),
        ];
        if let Some(tr) = time_range {
            conditions.push(tr.sql_filter());
        }

        let sql = format!(
            "SELECT ability_id, ability_name, combat_time_secs \
             FROM events WHERE {} \
             ORDER BY ability_id, combat_time_secs",
            conditions.join(" AND ")
        );

        let batches = self.sql(&sql).await?;

        // Collect all rows grouped by ability_id, preserving time order.
        // BTreeMap gives us deterministic iteration order by ability_id.
        let mut ability_casts: BTreeMap<i64, (String, Vec<f32>)> = BTreeMap::new();

        for batch in &batches {
            let ids = col_i64(batch, 0)?;
            let names = col_strings(batch, 1)?;
            let times = col_f32(batch, 2)?;

            for ((id, name), time) in ids.into_iter().zip(names).zip(times) {
                ability_casts
                    .entry(id)
                    .or_insert_with(|| (name, Vec::new()))
                    .1
                    .push(time);
            }
        }

        // Build result rows with inter-cast timing statistics.
        let mut results = Vec::with_capacity(ability_casts.len());

        for (ability_id, (ability_name, timestamps)) in ability_casts {
            let cast_count = timestamps.len() as i64;
            let first_cast_secs = timestamps.first().copied().unwrap_or(0.0);
            let last_cast_secs = timestamps.last().copied().unwrap_or(0.0);

            let (avg_time_between, median_time_between, min_time_between, max_time_between) = if cast_count >= 2 {
                compute_inter_cast_stats(&timestamps)
            } else {
                (0.0, 0.0, 0.0, 0.0)
            };

            results.push(AbilityUsageRow {
                ability_name,
                ability_id,
                cast_count,
                first_cast_secs,
                last_cast_secs,
                avg_time_between,
                median_time_between,
                min_time_between,
                max_time_between,
                timestamps,
            });
        }

        Ok(results)
    }
}

/// Compute inter-cast timing statistics from an ordered list of timestamps.
///
/// Assumes `timestamps` has at least 2 elements and is sorted ascending.
/// Returns (avg, median, min, max) of the deltas between consecutive casts.
fn compute_inter_cast_stats(timestamps: &[f32]) -> (f32, f32, f32, f32) {
    let mut deltas: Vec<f32> = timestamps.windows(2).map(|w| w[1] - w[0]).collect();

    if deltas.is_empty() {
        return (0.0, 0.0, 0.0, 0.0);
    }

    let sum: f32 = deltas.iter().sum();
    let avg = sum / deltas.len() as f32;

    // Sort for median, min, max
    deltas.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let min = deltas.first().copied().unwrap_or(0.0);
    let max = deltas.last().copied().unwrap_or(0.0);

    let median = if deltas.len().is_multiple_of(2) {
        let mid = deltas.len() / 2;
        (deltas[mid - 1] + deltas[mid]) / 2.0
    } else {
        deltas[deltas.len() / 2]
    };

    (avg, median, min, max)
}
