//! Raid debuff matrix: uptime of each standard debuff on every enemy.
//!
//! Windows are rebuilt as in `uptime`, per (effect, enemy instance, source),
//! then clipped to the time the group was acting on that instance: a debuff
//! left on a dead add, or one still open at the end, does not count past it.

use std::collections::HashMap;

use baras_types::{RaidDebuff, RaidDebuffCell, RaidDebuffReport, RaidDebuffSource, RaidDebuffTarget, RaidDebuffWorth};

use super::effects::{APPLY_EFFECT, REMOVE_EFFECT};
use super::enemies::FROM_GROUP;
use super::uptime::merge;
use super::*;
use crate::game_data::{ATTACK_TYPES, RAID_DEBUFF_DISCIPLINES, RAID_DEBUFFS};

#[derive(Default)]
struct Row {
    is_boss: bool,
    present: Vec<(f32, f32)>,
    windows: HashMap<RaidDebuff, Vec<(f32, f32)>>,
    /// (debuff, source, ability) → (ability name, applications)
    sources: HashMap<(RaidDebuff, String, i64), (String, u32)>,
    /// debuff → (damage of the kind it raises, the part that landed without it)
    worth: HashMap<RaidDebuff, (f64, f64)>,
}

impl EncounterQuery<'_> {
    pub async fn raid_debuffs(&self, time_range: Option<&TimeRange>) -> Result<RaidDebuffReport, String> {
        let (start, end) = time_range.map_or((0.0, f32::MAX), |tr| (tr.start, tr.end));
        // enemy instance → (its row's label, the part of the range it was fought in)
        let mut present: HashMap<i64, (String, f32, f32)> = HashMap::new();
        let mut rows: HashMap<String, Row> = HashMap::new();
        for e in self.enemy_instances().await? {
            let (from, to) = (e.first.max(start), e.last.min(end));
            if to <= from {
                continue;
            }
            let row = rows.entry(e.label.clone()).or_default();
            row.is_boss = e.is_boss;
            row.present.push((from, to));
            present.insert(e.id, (e.label, from, to));
        }

        let ids: Vec<String> = RAID_DEBUFFS.keys().map(i64::to_string).collect();
        let batches = self
            .sql(&format!(
                "SELECT target_id, effect_id, source_id, source_name, ability_id, ability_name,
                        effect_type_id, CAST(COALESCE(combat_time_secs, 0.0) AS REAL)
                 FROM events
                 WHERE {FROM_GROUP} AND effect_type_id IN ({APPLY_EFFECT}, {REMOVE_EFFECT})
                   AND effect_id IN ({})
                 ORDER BY line_number",
                ids.join(", ")
            ))
            .await?;
        // (effect, enemy instance, source) → when its open window began
        let mut open: HashMap<(i64, i64, i64), f32> = HashMap::new();
        for batch in &batches {
            let (targets, effects, sources) = (col_i64(batch, 0)?, col_i64(batch, 1)?, col_i64(batch, 2)?);
            let (source_names, abilities, ability_names) =
                (col_strings(batch, 3)?, col_i64(batch, 4)?, col_strings(batch, 5)?);
            let (kinds, times) = (col_i64(batch, 6)?, col_f32(batch, 7)?);
            for i in 0..batch.num_rows() {
                let (Some(debuff), Some((name, from, to))) =
                    (RAID_DEBUFFS.get(&effects[i]).copied(), present.get(&targets[i]))
                else {
                    continue;
                };
                let Some(row) = rows.get_mut(name) else { continue };
                let inst = (effects[i], targets[i], sources[i]);
                let t = times[i];
                if kinds[i] == APPLY_EFFECT {
                    if t >= start && t <= end {
                        row.sources
                            .entry((debuff, source_names[i].clone(), abilities[i]))
                            .or_insert_with(|| (ability_names[i].clone(), 0))
                            .1 += 1;
                    }
                    open.entry(inst).or_insert(t);
                } else if let Some(began) = open.remove(&inst) {
                    let (a, b) = (began.max(*from), t.min(*to));
                    if b > a {
                        row.windows.entry(debuff).or_default().push((a, b));
                    }
                }
            }
        }
        for ((effect, target, _), began) in open {
            if let (Some(debuff), Some((name, from, to))) = (RAID_DEBUFFS.get(&effect), present.get(&target))
                && let Some(row) = rows.get_mut(name)
                && *to > began.max(*from)
            {
                row.windows.entry(*debuff).or_default().push((began.max(*from), *to));
            }
        }

        for row in rows.values_mut() {
            for windows in row.windows.values_mut() {
                *windows = merge(windows, start, end);
            }
        }

        // Each hit on an enemy counts toward the debuffs that raise its kind
        let range = time_range
            .map(|tr| format!("AND {}", tr.sql_filter()))
            .unwrap_or_default();
        let batches = self
            .sql(&format!(
                "SELECT target_id, CAST(combat_time_secs AS REAL), CAST(dmg_amount AS DOUBLE), dmg_type_id, ability_id
                 FROM events
                 WHERE {FROM_GROUP} AND dmg_amount > 0 AND combat_time_secs >= 0 {range}"
            ))
            .await?;
        for batch in &batches {
            let (targets, times, amounts) = (col_i64(batch, 0)?, col_f32(batch, 1)?, col_f64(batch, 2)?);
            let (dmg_types, abilities) = (col_i64(batch, 3)?, col_i64(batch, 4)?);
            for i in 0..batch.num_rows() {
                let Some(row) = present.get(&targets[i]).and_then(|p| rows.get_mut(&p.0)) else {
                    continue;
                };
                let attack_type = ATTACK_TYPES.get(&abilities[i]).copied().unwrap_or("");
                for debuff in RaidDebuff::ALL {
                    if !debuff.raises(attack_type, dmg_types[i]) {
                        continue;
                    }
                    let up = row.windows.get(&debuff).is_some_and(|w| {
                        let next = w.partition_point(|(a, _)| *a <= times[i]);
                        next > 0 && times[i] < w[next - 1].1
                    });
                    let worth = row.worth.entry(debuff).or_default();
                    worth.0 += amounts[i];
                    if !up {
                        worth.1 += amounts[i];
                    }
                }
            }
        }

        let mut out: Vec<RaidDebuffTarget> = rows
            .into_iter()
            .map(|(name, mut row)| {
                let present_secs: f32 = merge(&mut row.present, start, end).iter().map(|(a, b)| b - a).sum();
                let cells = RaidDebuff::ALL
                    .into_iter()
                    .filter_map(|debuff| {
                        let windows = row.windows.remove(&debuff)?;
                        let covered: f32 = windows.iter().map(|(a, b)| b - a).sum();
                        let mut sources: Vec<RaidDebuffSource> = row
                            .sources
                            .iter()
                            .filter(|(k, _)| k.0 == debuff)
                            .map(|((_, name, ability_id), (ability_name, count))| RaidDebuffSource {
                                name: name.clone(),
                                ability_id: *ability_id,
                                ability_name: ability_name.clone(),
                                count: *count,
                            })
                            .collect();
                        sources.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
                        Some(RaidDebuffCell {
                            debuff,
                            uptime_pct: (covered * 100.0 / present_secs.max(0.001)).min(100.0),
                            windows,
                            sources,
                        })
                    })
                    .collect();
                let worth = RaidDebuff::ALL
                    .into_iter()
                    .filter_map(|debuff| {
                        let (affected_dmg, unbuffed_dmg) = *row.worth.get(&debuff)?;
                        Some(RaidDebuffWorth {
                            debuff,
                            affected_dmg,
                            unbuffed_dmg,
                        })
                    })
                    .collect();
                RaidDebuffTarget {
                    name,
                    is_boss: row.is_boss,
                    present_secs,
                    cells,
                    worth,
                }
            })
            .collect();
        out.sort_by(|a, b| {
            b.is_boss
                .cmp(&a.is_boss)
                .then(b.present_secs.total_cmp(&a.present_secs))
        });
        let able = RAID_DEBUFF_DISCIPLINES
            .iter()
            .map(|(debuff, guids)| (*debuff, guids.to_vec()))
            .collect();
        Ok(RaidDebuffReport { targets: out, able })
    }
}
