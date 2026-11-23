//! Crit shares and GCD press intervals for one player (training dummy view).

use std::collections::HashMap;

use baras_types::{AbilityInterval, ChannelPress, GcdAnalysis, GcdPress};

use super::*;
use crate::game_data::{channel_secs, effect_id, effect_type_id, is_off_gcd};

/// Events logged before combat started carry no combat time (or a negative
/// one); buffs and pre-casts there are not part of the parse
const IN_COMBAT: &str = "combat_time_secs >= 0";
/// Hits this close together are one tick (several targets, or both weapons)
const TICK_MERGE_SECS: f32 = 0.15;
/// A tick can land this long after its channel's base length (travel time)
const TICK_GRACE_SECS: f32 = 0.3;

impl EncounterQuery<'_> {
    /// `(crit rate, crit damage share)` of a player's damage, each 0-100.
    pub async fn query_crit_shares(&self, source_name: &str) -> Result<(f32, f32), String> {
        let sql = format!(
            "SELECT COUNT(*), SUM(CASE WHEN is_crit THEN 1 ELSE 0 END), \
                    SUM(dmg_amount), SUM(CASE WHEN is_crit THEN dmg_amount ELSE 0 END) \
             FROM events WHERE dmg_amount > 0 AND source_id != target_id AND source_name = '{}' \
               AND {IN_COMBAT}",
            sql_escape(source_name)
        );
        let pct = |part: f64, whole: f64| {
            if whole > 0.0 {
                (part / whole * 100.0) as f32
            } else {
                0.0
            }
        };
        for batch in &self.sql(&sql).await? {
            let (hits, crits) = (col_f64(batch, 0)?, col_f64(batch, 1)?);
            let (dmg, crit_dmg) = (col_f64(batch, 2)?, col_f64(batch, 3)?);
            if let Some(i) = (0..hits.len()).next() {
                return Ok((pct(crits[i], hits[i]), pct(crit_dmg[i], dmg[i])));
            }
        }
        Ok((0.0, 0.0))
    }

    /// A player's on-GCD presses and the time between them.
    pub async fn query_gcd_analysis(&self, source_name: &str) -> Result<GcdAnalysis, String> {
        let sql = format!(
            "SELECT combat_time_secs, ability_id, ability_name \
             FROM events WHERE effect_id = {} AND source_name = '{}' AND {IN_COMBAT} \
             ORDER BY combat_time_secs ASC, line_number ASC",
            effect_id::ABILITYACTIVATE,
            sql_escape(source_name)
        );
        let mut presses: Vec<(f32, i64, String)> = Vec::new();
        for batch in &self.sql(&sql).await? {
            let (times, ids, names) = (col_f32(batch, 0)?, col_i64(batch, 1)?, col_strings(batch, 2)?);
            presses.extend(
                times
                    .into_iter()
                    .zip(ids)
                    .zip(names)
                    .map(|((t, id), name)| (t, id, name))
                    .filter(|(_, id, _)| !is_off_gcd(*id)),
            );
        }
        // Damage and heal ticks of the channels pressed, by ability, chronological
        let mut channel_ids: Vec<i64> = presses
            .iter()
            .map(|p| p.1)
            .filter(|id| channel_secs(*id).is_some())
            .collect();
        channel_ids.sort_unstable();
        channel_ids.dedup();
        let mut ticks: HashMap<i64, Vec<f32>> = HashMap::new();
        if !channel_ids.is_empty() {
            let ids: Vec<String> = channel_ids.iter().map(i64::to_string).collect();
            let sql = format!(
                "SELECT combat_time_secs, ability_id FROM events \
                 WHERE effect_type_id = {} AND effect_id IN ({}, {}) AND ability_id IN ({}) \
                   AND source_name = '{}' AND {IN_COMBAT} \
                 ORDER BY combat_time_secs ASC, line_number ASC",
                effect_type_id::APPLYEFFECT,
                effect_id::DAMAGE,
                effect_id::HEAL,
                ids.join(", "),
                sql_escape(source_name)
            );
            for batch in &self.sql(&sql).await? {
                for (t, id) in col_f32(batch, 0)?.into_iter().zip(col_i64(batch, 1)?) {
                    ticks.entry(id).or_default().push(t);
                }
            }
        }
        Ok(analyze(&presses, &ticks))
    }
}

/// Distinct tick instants in `ticks` (ascending) within `[from, until)`
fn count_ticks(ticks: &[f32], from: f32, until: f32) -> u8 {
    let mut count = 0u8;
    let mut last = f32::NEG_INFINITY;
    for &t in &ticks[ticks.partition_point(|t| *t < from)..] {
        if t >= until {
            break;
        }
        if t - last > TICK_MERGE_SECS {
            count = count.saturating_add(1);
        }
        last = t;
    }
    count
}

/// Value at quantile `q` (0-1) of an ascending slice
fn quantile(sorted: &[f32], q: f32) -> f32 {
    sorted[((sorted.len() - 1) as f32 * q) as usize]
}

/// Per channel ability: the tick count of a full channel and the median time
/// to the next press after one. Both are read off the parse, since talents
/// and alacrity change a channel's length: a full channel lands the highest
/// tick count seen, ignoring a one-off among many presses (a tick split in
/// two by lag).
fn channel_baselines(presses: &[(i64, f32, u8)]) -> HashMap<i64, (u8, f32)> {
    let mut by_ability: HashMap<i64, Vec<(f32, u8)>> = HashMap::new();
    for (id, interval, ticks) in presses {
        by_ability.entry(*id).or_default().push((*interval, *ticks));
    }
    by_ability
        .into_iter()
        .map(|(id, uses)| {
            let common = (uses.len() / 10).max(1);
            let count = |n: u8| uses.iter().filter(|u| u.1 == n).count();
            let highest = uses.iter().map(|u| u.1).max().unwrap_or(0);
            let full = (1..=highest).rev().find(|n| count(*n) >= common).unwrap_or(highest);
            let mut gaps: Vec<f32> = uses.iter().filter(|u| u.1 >= full).map(|u| u.0).collect();
            gaps.sort_by(f32::total_cmp);
            (id, (full, quantile(&gaps, 0.5)))
        })
        .collect()
}

fn analyze(presses: &[(f32, i64, String)], ticks: &HashMap<i64, Vec<f32>>) -> GcdAnalysis {
    let mut all: Vec<f32> = presses.windows(2).map(|p| p[1].0 - p[0].0).collect();
    all.sort_by(f32::total_cmp);
    let typical = if all.is_empty() { 0.0 } else { quantile(&all, 0.5) };

    // Ticks each channel press landed before the next press
    let channel_presses: Vec<(i64, f32, u8)> = presses
        .windows(2)
        .filter_map(|pair| {
            let ((time, id, _), (next_time, ..)) = (&pair[0], &pair[1]);
            let until = next_time.min(time + channel_secs(*id)? + TICK_GRACE_SECS);
            let landed = ticks.get(id).map_or(0, |t| count_ticks(t, *time, until));
            Some((*id, next_time - time, landed))
        })
        .collect();
    let baselines = channel_baselines(&channel_presses);
    let expected = |id: i64| baselines.get(&id).map_or(typical, |b| b.1.max(typical));

    let mut landed = channel_presses.iter().map(|p| p.2);
    let mut timeline = Vec::with_capacity(presses.len().saturating_sub(1));
    // Per ability: its name and the intervals that followed it
    let mut following: HashMap<i64, (&str, Vec<f32>)> = HashMap::new();
    for pair in presses.windows(2) {
        let ((time, id, name), (next_time, next_id, next)) = (&pair[0], &pair[1]);
        let interval = next_time - time;
        following.entry(*id).or_insert((name, Vec::new())).1.push(interval);
        // `landed` holds one entry per channel press, in press order
        let channel = baselines.get(id).map(|(full_ticks, _)| {
            let ticks = landed.next().unwrap_or(0);
            ChannelPress {
                ticks,
                full_ticks: *full_ticks,
                clipped: ticks < *full_ticks,
            }
        });
        timeline.push(GcdPress {
            time_secs: *time,
            interval_secs: interval,
            ability: name.clone(),
            ability_id: *id,
            next: next.clone(),
            next_id: *next_id,
            expected_secs: expected(*id),
            channel,
        });
    }

    let mut abilities: Vec<AbilityInterval> = following
        .into_iter()
        .map(|(id, (name, mut gaps))| {
            gaps.sort_by(f32::total_cmp);
            AbilityInterval {
                ability_id: id,
                ability_name: name.to_string(),
                uses: gaps.len() as u32,
                median_secs: quantile(&gaps, 0.5),
                longest_secs: gaps[gaps.len() - 1],
                expected_secs: expected(id),
            }
        })
        .collect();
    abilities.sort_by(|a, b| b.uses.cmp(&a.uses).then(a.ability_name.cmp(&b.ability_name)));
    GcdAnalysis {
        gcd_count: presses.len() as u32,
        typical_gcd_secs: typical,
        intervals_secs: all,
        presses: timeline,
        abilities,
    }
}
