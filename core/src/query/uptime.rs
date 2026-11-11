//! Effect Uptime tab: every effect one source applied, per target.
//!
//! Windows are rebuilt from the source's apply/remove events in log order:
//! an apply opens a window on (effect, target instance) unless one is open
//! (a refresh), a remove closes it, a remove with nothing open means it was
//! applied before combat (counted from 0), and a window still open at the end
//! runs to the end of the fight. Same-named targets (adds) share a row: their
//! windows are merged, so the row reads "on at least one of them".

use std::collections::HashMap;

use super::effects::{APPLY_EFFECT, DAMAGE_EFFECT, HEAL_EFFECT, REMOVE_EFFECT};
use super::*;

/// Per (target name, effect) row being built
struct Acc {
    target_type: String,
    effect_name: String,
    ability_id: i64,
    count: i64,
    windows: Vec<(f32, f32)>,
}

impl EncounterQuery<'_> {
    pub async fn effect_uptime_by_target(
        &self,
        source: &str,
        time_range: Option<&TimeRange>,
        duration_secs: f32,
    ) -> Result<Vec<EffectUptimeCell>, String> {
        let duration = duration_secs.max(0.001);
        let (start, end) = time_range.map_or((0.0, duration), |tr| (tr.start, tr.end));
        let span = (end - start).max(0.001);
        let source_match = entity_match_sql("source", source);

        let batches = self
            .sql(&format!(
                "SELECT target_name, target_entity_type, target_id, effect_id, effect_name,
                        ability_id, effect_type_id, CAST(COALESCE(combat_time_secs, 0.0) AS REAL)
                 FROM events
                 WHERE effect_type_id IN ({APPLY_EFFECT}, {REMOVE_EFFECT})
                   AND effect_id NOT IN ({DAMAGE_EFFECT}, {HEAL_EFFECT})
                   AND {source_match}
                 ORDER BY line_number"
            ))
            .await?;

        let mut open: HashMap<(i64, i64), f32> = HashMap::new();
        let mut seen: std::collections::HashSet<(i64, i64)> = std::collections::HashSet::new();
        // Instance → its row key, to close windows still open at the end
        let mut row_of: HashMap<(i64, i64), (String, i64)> = HashMap::new();
        let mut rows: HashMap<(String, i64), Acc> = HashMap::new();

        for batch in &batches {
            let target_names = col_strings(batch, 0)?;
            let target_types = col_strings(batch, 1)?;
            let target_ids = col_i64(batch, 2)?;
            let effect_ids = col_i64(batch, 3)?;
            let effect_names = col_strings(batch, 4)?;
            let ability_ids = col_i64(batch, 5)?;
            let kinds = col_i64(batch, 6)?;
            let times = col_f32(batch, 7)?;

            for i in 0..batch.num_rows() {
                let inst = (effect_ids[i], target_ids[i]);
                let key = (target_names[i].clone(), effect_ids[i]);
                let acc = rows.entry(key.clone()).or_insert_with(|| Acc {
                    target_type: target_types[i].clone(),
                    effect_name: effect_names[i].clone(),
                    ability_id: 0,
                    count: 0,
                    windows: Vec::new(),
                });
                row_of.insert(inst, key);
                let t = times[i];
                if kinds[i] == APPLY_EFFECT {
                    if acc.ability_id == 0 {
                        acc.ability_id = ability_ids[i];
                    }
                    if t >= start && t <= end {
                        acc.count += 1;
                    }
                    open.entry(inst).or_insert(t);
                } else if let Some(from) = open.remove(&inst) {
                    acc.windows.push((from, t));
                } else if !seen.contains(&inst) {
                    // Removed before any apply: it was up when the fight began
                    acc.windows.push((0.0, t));
                }
                seen.insert(inst);
            }
        }
        for (inst, from) in open {
            if let Some(acc) = row_of.get(&inst).and_then(|k| rows.get_mut(k)) {
                acc.windows.push((from, duration));
            }
        }

        let mut out: Vec<EffectUptimeCell> = rows
            .into_iter()
            .filter_map(|((target_name, effect_id), mut acc)| {
                let windows = merge(&mut acc.windows, start, end);
                let covered: f32 = windows.iter().map(|(a, b)| b - a).sum();
                (covered > 0.0).then(|| EffectUptimeCell {
                    target_name,
                    target_type: acc.target_type,
                    effect_id,
                    effect_name: acc.effect_name,
                    ability_id: acc.ability_id,
                    count: acc.count,
                    uptime_pct: (covered * 100.0 / span).min(100.0),
                    windows,
                })
            })
            .collect();
        out.sort_by(|a, b| b.uptime_pct.total_cmp(&a.uptime_pct));
        Ok(out)
    }
}

/// The windows' union, clipped to [start, end], in order
fn merge(windows: &mut [(f32, f32)], start: f32, end: f32) -> Vec<(f32, f32)> {
    windows.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f32, f32)> = Vec::new();
    for &(a, b) in windows.iter() {
        let (a, b) = (a.max(start), b.min(end));
        if b <= a {
            continue;
        }
        match out.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}
