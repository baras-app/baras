//! Crit shares and GCD press intervals for one player (training dummy view).

use std::collections::HashMap;

use baras_types::{AbilityInterval, GcdAnalysis, GcdPress};

use super::*;
use crate::game_data::{OFF_GCD_ABILITIES, effect_id};

/// Events logged before combat started carry no combat time (or a negative
/// one); buffs and pre-casts there are not part of the parse
const IN_COMBAT: &str = "combat_time_secs >= 0";

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
        let pct = |part: f64, whole: f64| if whole > 0.0 { (part / whole * 100.0) as f32 } else { 0.0 };
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
                    .filter(|(_, id, _)| !OFF_GCD_ABILITIES.contains(id)),
            );
        }
        Ok(analyze(&presses))
    }
}

/// Value at quantile `q` (0-1) of an ascending slice
fn quantile(sorted: &[f32], q: f32) -> f32 {
    sorted[((sorted.len() - 1) as f32 * q) as usize]
}

fn analyze(presses: &[(f32, i64, String)]) -> GcdAnalysis {
    let mut timeline = Vec::with_capacity(presses.len().saturating_sub(1));
    // Per ability: its name and the intervals that followed it
    let mut following: HashMap<i64, (&str, Vec<f32>)> = HashMap::new();
    for pair in presses.windows(2) {
        let ((time, id, name), (next_time, next_id, next)) = (&pair[0], &pair[1]);
        let interval = next_time - time;
        following.entry(*id).or_insert((name, Vec::new())).1.push(interval);
        timeline.push(GcdPress {
            time_secs: *time,
            interval_secs: interval,
            ability: name.clone(),
            ability_id: *id,
            next: next.clone(),
            next_id: *next_id,
        });
    }

    let mut all: Vec<f32> = timeline.iter().map(|p| p.interval_secs).collect();
    all.sort_by(f32::total_cmp);
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
            }
        })
        .collect();
    abilities.sort_by(|a, b| b.uses.cmp(&a.uses).then(a.ability_name.cmp(&b.ability_name)));
    GcdAnalysis {
        gcd_count: presses.len() as u32,
        typical_gcd_secs: if all.is_empty() { 0.0 } else { quantile(&all, 0.5) },
        intervals_secs: all,
        presses: timeline,
        abilities,
    }
}
