//! Effect uptime and window queries.

use std::collections::HashMap;

use super::*;

// Effect type IDs (the type of log event)
pub(super) const APPLY_EFFECT: i64 = 836045448945477;
pub(super) const REMOVE_EFFECT: i64 = 836045448945478;
// Effect IDs (what specifically happened)
pub(super) const ABILITY_ACTIVATE: i64 = 836045448945479;
// Exclude damage/heal "effects" which are action results, not buffs
pub(super) const DAMAGE_EFFECT: i64 = 836045448945501;
pub(super) const HEAL_EFFECT: i64 = 836045448945500;

/// Build a SQL WHERE clause fragment for source filtering.
/// Returns an AND clause or empty string. `target` is an entity selector
/// (name or `id:<instance>`, see `entity_match_sql`).
pub(super) fn source_filter_clause(source_filter: Option<&str>, target: Option<&str>) -> String {
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

/// One application of an effect, running until it is refreshed, removed or the
/// encounter ends. Times are combat seconds, not yet clamped to a time range.
struct RawWindow {
    effect_id: i64,
    effect_name: String,
    ability_name: String,
    /// Log timestamp of the apply; `None` for one inferred from an orphan remove
    timestamp: Option<i64>,
    start: f64,
    end: f64,
}

struct Apply {
    effect_name: String,
    ability_name: String,
    timestamp: Option<i64>,
    time: f64,
    line: i64,
}

/// The part of a window inside `[range_start, range_end]`, if any.
fn clamp_window(w: &RawWindow, range_start: f64, range_end: f64) -> Option<(f64, f64)> {
    if w.start >= range_end || w.end <= range_start {
        return None;
    }
    let (start, end) = (w.start.max(range_start), w.end.min(range_end));
    (end > start).then_some((start, end))
}

impl EncounterQuery<'_> {
    /// Every effect window matching `filters` (extra `AND …` clauses).
    ///
    /// Pre-combat events (combat_time_secs IS NULL) count from 0.0, so effects
    /// applied before combat run from combat start. Each apply ends at
    /// MIN(next apply, first remove after it, encounter end), per effect and target.
    async fn effect_windows(&self, filters: &str, duration: f64) -> Result<Vec<RawWindow>, String> {
        let batches = self
            .sql(&format!(
                "SELECT effect_type_id, effect_id, effect_name, ability_name, target_name,
                        COALESCE(combat_time_secs, 0.0), timestamp, line_number
                 FROM events
                 WHERE effect_type_id IN ({APPLY_EFFECT}, {REMOVE_EFFECT})
                   AND effect_id NOT IN ({DAMAGE_EFFECT}, {HEAL_EFFECT})
                   {filters}"
            ))
            .await?;

        // Keyed by (effect, target): an effect runs independently on each target
        let mut applies: HashMap<(i64, String), Vec<Apply>> = HashMap::new();
        let mut removes: HashMap<(i64, String), Vec<f64>> = HashMap::new();
        // First remove per (effect, target, effect name)
        let mut first_removes: HashMap<(i64, String, String), f64> = HashMap::new();
        for batch in &batches {
            let types = col_i64(batch, 0)?;
            let effect_ids = col_i64(batch, 1)?;
            let effect_names = col_strings(batch, 2)?;
            let ability_names = col_strings(batch, 3)?;
            let targets = col_strings(batch, 4)?;
            let times = col_f64(batch, 5)?;
            let timestamps = col_timestamp_ms(batch, 6)?;
            let lines = col_i64(batch, 7)?;
            let rows = effect_names.into_iter().zip(ability_names).zip(targets).enumerate();
            for (i, ((effect_name, ability_name), target)) in rows {
                if types[i] == APPLY_EFFECT {
                    applies.entry((effect_ids[i], target)).or_default().push(Apply {
                        effect_name,
                        ability_name,
                        timestamp: Some(timestamps[i]),
                        time: times[i],
                        line: lines[i],
                    });
                } else {
                    removes
                        .entry((effect_ids[i], target.clone()))
                        .or_default()
                        .push(times[i]);
                    first_removes
                        .entry((effect_ids[i], target, effect_name))
                        .and_modify(|t| *t = t.min(times[i]))
                        .or_insert(times[i]);
                }
            }
        }

        // Infer pre-combat applications from "orphan" removes — a remove with no
        // apply before it for the same effect and target. The source filter
        // applies to removes too: EffectRemoved's source is the original applier.
        for ((effect_id, target, effect_name), first_remove) in first_removes {
            let list = applies.entry((effect_id, target)).or_default();
            let first_apply = list
                .iter()
                .filter(|a| a.timestamp.is_some())
                .map(|a| a.time)
                .reduce(f64::min);
            if first_apply.is_none_or(|t| first_remove < t) {
                list.push(Apply {
                    effect_name,
                    ability_name: String::new(),
                    timestamp: None,
                    time: 0.0,
                    line: 0,
                });
            }
        }

        // Identical applies (same effect, names, time and successor) collapse into one
        type ApplyKey = (i64, String, String, u64, Option<i64>, Option<u64>);
        let mut merged: HashMap<ApplyKey, Option<f64>> = HashMap::new();
        for ((effect_id, target), mut list) in applies {
            list.sort_by(|a, b| a.time.total_cmp(&b.time).then(a.line.cmp(&b.line)));
            let removes = removes.get_mut(&(effect_id, target));
            let removes: &[f64] = match removes {
                Some(times) => {
                    times.sort_by(f64::total_cmp);
                    times
                }
                None => &[],
            };
            let next_times: Vec<Option<f64>> = (1..=list.len()).map(|i| list.get(i).map(|a| a.time)).collect();
            for (apply, next) in list.into_iter().zip(next_times) {
                let first_remove = removes.get(removes.partition_point(|&t| t < apply.time)).copied();
                let key = (
                    effect_id,
                    apply.effect_name,
                    apply.ability_name,
                    apply.time.to_bits(),
                    apply.timestamp,
                    next.map(f64::to_bits),
                );
                let slot = merged.entry(key).or_insert(None);
                *slot = match (*slot, first_remove) {
                    (Some(a), Some(b)) => Some(a.min(b)),
                    (a, b) => a.or(b),
                };
            }
        }

        Ok(merged
            .into_iter()
            .map(
                |((effect_id, effect_name, ability_name, time, timestamp, next), first_remove)| {
                    let next = next.map(f64::from_bits).unwrap_or(duration);
                    RawWindow {
                        effect_id,
                        effect_name,
                        ability_name,
                        timestamp,
                        start: f64::from_bits(time),
                        end: next.min(first_remove.unwrap_or(duration)).min(duration),
                    }
                },
            )
            .collect())
    }

    /// Query effect uptime statistics for the charts panel.
    /// Returns aggregated data per effect (count, duration, uptime%).
    ///
    /// Windows are built across the whole encounter, then cut to the time range,
    /// so effects applied before combat or before the range still count.
    /// Overlapping windows are merged before summing, preventing >100% uptime.
    /// An effect is active (vs passive/proc) when an AbilityActivate at the same
    /// timestamp has a matching ability_id or ability_name.
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
        let range_start = time_range.map(|tr| tr.start).unwrap_or(0.0) as f64;
        let range_end = time_range.map(|tr| tr.end).unwrap_or(duration) as f64;
        let range_duration = (range_end - range_start).max(0.001);

        let filters = format!("{target_filter} {src_filter}");
        let activations_sql = format!(
            "SELECT DISTINCT timestamp, ability_id, ability_name FROM events WHERE effect_id = {ABILITY_ACTIVATE}"
        );
        let (windows, activations) = tokio::join!(
            self.effect_windows(&filters, duration as f64),
            self.sql(&activations_sql),
        );
        let mut activated: HashMap<i64, Vec<(i64, String)>> = HashMap::new();
        for batch in &activations? {
            let timestamps = col_timestamp_ms(batch, 0)?;
            let ability_ids = col_i64(batch, 1)?;
            for (i, name) in col_strings(batch, 2)?.into_iter().enumerate() {
                activated.entry(timestamps[i]).or_default().push((ability_ids[i], name));
            }
        }

        // Distinct clamped windows per effect: (start, end, name) -> (active, ability)
        type Spans = HashMap<(u64, u64, String), (bool, Option<i64>)>;
        let mut by_effect: HashMap<i64, Spans> = HashMap::new();
        for w in windows? {
            let Some((start, end)) = clamp_window(&w, range_start, range_end) else {
                continue;
            };
            let ability_id = w
                .timestamp
                .and_then(|ts| activated.get(&ts))
                .into_iter()
                .flatten()
                .filter(|(id, name)| *id == w.effect_id || *name == w.ability_name)
                .map(|(id, _)| *id)
                .min();
            let span = by_effect
                .entry(w.effect_id)
                .or_default()
                .entry((start.to_bits(), end.to_bits(), w.effect_name))
                .or_insert((false, None));
            span.0 |= ability_id.is_some();
            span.1 = [span.1, ability_id].into_iter().flatten().min();
        }

        let mut results = Vec::new();
        for (effect_id, spans) in by_effect {
            let count = spans.len() as i64;
            let mut spans: Vec<(f64, f64, String, bool, Option<i64>)> = spans
                .into_iter()
                .map(|((s, e, name), (active, ability))| (f64::from_bits(s), f64::from_bits(e), name, active, ability))
                .collect();
            spans.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));

            // Overlapping spans form one run; a run is credited to each effect
            // name in it (an id seen under two names is listed once per name).
            let mut runs: HashMap<(String, usize), (f64, f64)> = HashMap::new();
            let mut per_name: HashMap<String, (bool, Option<i64>)> = HashMap::new();
            let (mut run, mut run_end) = (0, f64::NEG_INFINITY);
            for (i, (start, end, name, active, ability)) in spans.into_iter().enumerate() {
                if i > 0 && start > run_end {
                    run += 1;
                }
                run_end = run_end.max(end);
                let info = per_name.entry(name.clone()).or_insert((false, None));
                info.0 |= active;
                info.1 = [info.1, ability].into_iter().flatten().min();
                runs.entry((name, run))
                    .and_modify(|r| *r = (r.0.min(start), r.1.max(end)))
                    .or_insert((start, end));
            }
            let mut totals: HashMap<String, f64> = HashMap::new();
            for ((name, _), (start, end)) in runs {
                *totals.entry(name).or_default() += end - start;
            }
            for (effect_name, total) in totals {
                let (is_active, ability_id) = per_name[&effect_name];
                results.push(EffectChartData {
                    effect_id,
                    effect_name,
                    ability_id,
                    is_active,
                    count,
                    total_duration_secs: total as f32,
                    uptime_pct: (total * 100.0 / range_duration).min(100.0) as f32,
                });
            }
        }
        results.sort_by(|a, b| {
            b.total_duration_secs
                .total_cmp(&a.total_duration_secs)
                .then(a.effect_id.cmp(&b.effect_id))
        });
        Ok(results)
    }

    /// Query individual time windows for a specific effect (for chart highlighting):
    /// its windows cut to the time range, with overlapping ones merged.
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
        let range_start = time_range.map(|tr| tr.start).unwrap_or(0.0) as f64;
        let range_end = time_range.map(|tr| tr.end).unwrap_or(duration) as f64;

        let filters = format!("AND effect_id = {effect_id} {target_filter} {src_filter}");
        let mut spans: Vec<(f64, f64)> = self
            .effect_windows(&filters, duration as f64)
            .await?
            .iter()
            .filter_map(|w| clamp_window(w, range_start, range_end))
            .collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));

        let mut results: Vec<EffectWindow> = Vec::new();
        let mut run_end = f64::NEG_INFINITY;
        for (start, end) in spans {
            match results.last_mut() {
                Some(last) if start <= run_end => last.end_secs = last.end_secs.max(end as f32),
                _ => results.push(EffectWindow {
                    start_secs: start as f32,
                    end_secs: end as f32,
                }),
            }
            run_end = run_end.max(end);
        }
        Ok(results)
    }
}
