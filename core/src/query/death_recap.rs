//! Death recap query — summarizes the final seconds before a player death.

use std::collections::HashMap;

use super::*;
use crate::game_data::{defense_type, effect_id};

/// Lookback from the death event.
const WINDOW_SECS: f32 = 15.0;
/// Included after the death event — killing damage is often logged after death.
const TRAILING_SECS: f32 = 3.0;
/// Maximum entries returned in the event ledger.
const MAX_LEDGER_EVENTS: usize = 80;
/// Maximum HP samples returned for the sparkline.
const MAX_HP_POINTS: usize = 200;

/// A classified event row from the recap window query.
struct RecapRow {
    time_secs: f32,
    source_name: String,
    source_id: i64,
    ability_name: String,
    ability_id: i64,
    effect_id: i64,
    dmg_effective: i32,
    dmg_absorbed: i32,
    is_crit: bool,
    dmg_type: String,
    defense_type_id: i64,
    heal_effective: i32,
    hp: i32,
    max_hp: i32,
    phase_name: String,
}

impl RecapRow {
    fn hp_pct(&self) -> Option<f32> {
        (self.max_hp > 0).then(|| self.hp as f32 * 100.0 / self.max_hp as f32)
    }

    fn is_avoided(&self) -> bool {
        matches!(
            self.defense_type_id,
            defense_type::MISS
                | defense_type::PARRY
                | defense_type::DODGE
                | defense_type::DEFLECT
                | defense_type::RESIST
                | defense_type::IMMUNE
                | defense_type::COVER
        )
    }
}

impl EncounterQuery<'_> {
    /// Build a death recap for one player death.
    ///
    /// Covers the final [`WINDOW_SECS`] before the death plus
    /// [`TRAILING_SECS`] after it (killing damage is often logged after the
    /// death event). Only damage/heal/death events are considered — effect
    /// applications are visible in the combat log view instead.
    pub async fn query_death_recap(
        &self,
        player_name: &str,
        death_time_secs: f32,
    ) -> Result<DeathRecap, String> {
        let lookback_start = (death_time_secs - WINDOW_SECS - 0.5).max(0.0);
        let sql = format!(
            r#"
            SELECT
                combat_time_secs,
                source_name,
                source_id,
                ability_name,
                effect_id,
                dmg_effective,
                dmg_absorbed,
                is_crit,
                dmg_type,
                defense_type_id,
                heal_effective,
                target_hp,
                target_max_hp,
                COALESCE(phase_name, '') AS phase_name,
                ability_id
            FROM events
            WHERE target_name = '{}'
              AND combat_time_secs IS NOT NULL
              AND combat_time_secs >= {}
              AND combat_time_secs <= {}
              AND effect_id IN ({}, {}, {})
            ORDER BY combat_time_secs ASC, line_number ASC
            "#,
            sql_escape(player_name),
            lookback_start,
            death_time_secs + TRAILING_SECS + 0.5,
            effect_id::DAMAGE,
            effect_id::HEAL,
            effect_id::DEATH,
        );

        let batches = self.sql(&sql).await?;
        let mut rows = Vec::new();
        for batch in &batches {
            let times = col_f32(batch, 0)?;
            let sources = col_strings(batch, 1)?;
            let source_ids = col_i64(batch, 2)?;
            let abilities = col_strings(batch, 3)?;
            let effect_ids = col_i64(batch, 4)?;
            let dmg_eff = col_i32(batch, 5)?;
            let dmg_abs = col_i32(batch, 6)?;
            let crits = col_bool(batch, 7)?;
            let dmg_types = col_strings(batch, 8)?;
            let def_types = col_i64(batch, 9)?;
            let heal_eff = col_i32(batch, 10)?;
            let hps = col_i32(batch, 11)?;
            let max_hps = col_i32(batch, 12)?;
            let phases = col_strings(batch, 13)?;
            let ability_ids = col_i64(batch, 14)?;

            for i in 0..batch.num_rows() {
                rows.push(RecapRow {
                    time_secs: times[i],
                    source_name: sources[i].clone(),
                    source_id: source_ids[i],
                    ability_name: abilities[i].clone(),
                    ability_id: ability_ids[i],
                    effect_id: effect_ids[i],
                    dmg_effective: dmg_eff[i],
                    dmg_absorbed: dmg_abs[i],
                    is_crit: crits[i],
                    dmg_type: dmg_types[i].clone(),
                    defense_type_id: def_types[i],
                    heal_effective: heal_eff[i],
                    hp: hps[i],
                    max_hp: max_hps[i],
                    phase_name: phases[i].clone(),
                });
            }
        }

        Ok(build_recap(player_name, death_time_secs, rows))
    }
}

/// Assemble the recap from classified rows (pure aggregation, no I/O).
fn build_recap(player_name: &str, requested_death_time: f32, rows: Vec<RecapRow>) -> DeathRecap {
    // Resolve the actual death event closest to the requested time
    let death = rows
        .iter()
        .filter(|r| r.effect_id == effect_id::DEATH)
        .min_by(|a, b| {
            let da = (a.time_secs - requested_death_time).abs();
            let db = (b.time_secs - requested_death_time).abs();
            da.total_cmp(&db)
        });
    let death_time = death.map(|r| r.time_secs).unwrap_or(requested_death_time);
    let phase_name = death
        .map(|r| r.phase_name.clone())
        .filter(|p| !p.is_empty());

    let window_start = (death_time - WINDOW_SECS).max(0.0);
    let window_end = death_time + TRAILING_SECS;

    // Keyed by source instance id so same-named NPC spawns stay separate
    let mut damage_map: HashMap<i64, DeathRecapDamageSource> = HashMap::new();
    let mut heal_map: HashMap<i64, DeathRecapHealSource> = HashMap::new();
    let mut damage_total = 0i64;
    let mut absorbed_total = 0i64;
    let mut healing_total = 0i64;
    // First hit that zeroed HP is the true killing blow; last damaging hit is
    // the fallback when no zero-HP sample exists
    let mut kb_zeroed: Option<DeathRecapKillingBlow> = None;
    let mut kb_last: Option<DeathRecapKillingBlow> = None;
    let mut last_hp: Option<i32> = None;
    let mut last_hp_pct: Option<f32> = None;
    let mut hp_timeline = Vec::new();
    let mut events = Vec::new();

    for r in &rows {
        if r.time_secs < window_start || r.time_secs > window_end {
            continue;
        }

        let (kind, value, absorbed) = if r.effect_id == effect_id::DAMAGE {
            let src = damage_map
                .entry(r.source_id)
                .or_insert_with(|| DeathRecapDamageSource {
                    source_name: r.source_name.clone(),
                    source_id: r.source_id,
                    hits: 0,
                    crits: 0,
                    total: 0,
                    max_hit: 0,
                    absorbed: 0,
                    abilities: Vec::new(),
                });
            src.hits += 1;
            src.crits += r.is_crit as u32;
            src.total += r.dmg_effective as i64;
            src.max_hit = src.max_hit.max(r.dmg_effective);
            src.absorbed += r.dmg_absorbed as i64;
            let row = find_or_push(
                &mut src.abilities,
                |a| a.ability_name == r.ability_name,
                || DeathRecapDamageRow {
                    ability_name: r.ability_name.clone(),
                    hits: 0,
                    crits: 0,
                    total: 0,
                    max_hit: 0,
                    absorbed: 0,
                    damage_type: String::new(),
                },
            );
            if row.damage_type.is_empty() && !r.dmg_type.is_empty() {
                row.damage_type = r.dmg_type.clone();
            }
            row.hits += 1;
            row.crits += r.is_crit as u32;
            row.total += r.dmg_effective as i64;
            row.max_hit = row.max_hit.max(r.dmg_effective);
            row.absorbed += r.dmg_absorbed as i64;
            damage_total += r.dmg_effective as i64;
            absorbed_total += r.dmg_absorbed as i64;

            if r.dmg_effective > 0 {
                let kb = DeathRecapKillingBlow {
                    ability_name: r.ability_name.clone(),
                    source_name: r.source_name.clone(),
                    amount: r.dmg_effective,
                    overkill: last_hp
                        .map(|hp| (r.dmg_effective - hp).max(0))
                        .unwrap_or(0),
                    is_crit: r.is_crit,
                    damage_type: r.dmg_type.clone(),
                };
                if r.max_hp > 0 && r.hp == 0 {
                    // Recorded HP hitting 0 — the definitive killing blow
                    if kb_zeroed.is_none() {
                        kb_zeroed = Some(kb);
                    }
                } else if r.max_hp == 0
                    && last_hp.is_none_or(|hp| r.dmg_effective >= hp)
                {
                    // Post-hit HP unknown — plausible killer only if the damage
                    // could have exceeded the last known remaining HP. A hit the
                    // player demonstrably survived (post-hit HP > 0) never
                    // qualifies; deaths with no qualifying hit report UNKNOWN
                    // (e.g. /stuck)
                    kb_last = Some(kb);
                }
            }
            (DeathRecapEventKind::Damage, r.dmg_effective, r.dmg_absorbed)
        } else if r.effect_id == effect_id::HEAL {
            let src = heal_map
                .entry(r.source_id)
                .or_insert_with(|| DeathRecapHealSource {
                    source_name: r.source_name.clone(),
                    source_id: r.source_id,
                    casts: 0,
                    effective: 0,
                    abilities: Vec::new(),
                });
            src.casts += 1;
            src.effective += r.heal_effective as i64;
            let row = find_or_push(
                &mut src.abilities,
                |a| a.ability_name == r.ability_name,
                || DeathRecapHealRow {
                    ability_name: r.ability_name.clone(),
                    casts: 0,
                    effective: 0,
                },
            );
            row.casts += 1;
            row.effective += r.heal_effective as i64;
            healing_total += r.heal_effective as i64;
            (DeathRecapEventKind::Heal, r.heal_effective, 0)
        } else if r.effect_id == effect_id::DEATH {
            (DeathRecapEventKind::Death, 0, 0)
        } else {
            continue;
        };

        if let Some(pct) = r.hp_pct() {
            last_hp = Some(r.hp);
            last_hp_pct = Some(pct);
            // Sparkline ends at the death event; trailing samples are all 0
            if r.time_secs <= death_time {
                hp_timeline.push(DeathRecapHpPoint {
                    time_secs: r.time_secs,
                    hp_pct: pct,
                });
            }
        }

        events.push(DeathRecapEvent {
            time_secs: r.time_secs,
            kind,
            source_name: r.source_name.clone(),
            ability_name: r.ability_name.clone(),
            ability_id: if kind == DeathRecapEventKind::Death { 0 } else { r.ability_id },
            value,
            absorbed,
            is_crit: r.is_crit && kind == DeathRecapEventKind::Damage,
            avoided: kind == DeathRecapEventKind::Damage && r.is_avoided(),
            damage_type: if kind == DeathRecapEventKind::Damage { r.dmg_type.clone() } else { String::new() },
            hp_pct: last_hp_pct.unwrap_or(0.0),
        });
    }

    // Final 0% point at the moment of death
    hp_timeline.push(DeathRecapHpPoint {
        time_secs: death_time,
        hp_pct: 0.0,
    });
    downsample(&mut hp_timeline);

    let mut damage_rows: Vec<_> = damage_map.into_values().collect();
    damage_rows.sort_by(|a, b| b.total.cmp(&a.total));
    for src in &mut damage_rows {
        src.abilities.sort_by(|a, b| b.total.cmp(&a.total));
    }
    let mut healing_rows: Vec<_> = heal_map.into_values().collect();
    healing_rows.sort_by(|a, b| b.effective.cmp(&a.effective));
    for src in &mut healing_rows {
        src.abilities.sort_by(|a, b| b.effective.cmp(&a.effective));
    }

    if events.len() > MAX_LEDGER_EVENTS {
        events.drain(..events.len() - MAX_LEDGER_EVENTS);
    }

    DeathRecap {
        player_name: player_name.to_string(),
        death_time_secs: death_time,
        window_start_secs: window_start,
        phase_name,
        killing_blow: kb_zeroed.or(kb_last),
        damage_total,
        absorbed_total,
        healing_total,
        hp_timeline,
        damage_rows,
        healing_rows,
        events,
    }
}

/// Find the first element matching `pred`, pushing a new one if absent.
/// Linear scan — ability lists per source are tiny in a 15s window.
fn find_or_push<T>(items: &mut Vec<T>, pred: impl Fn(&T) -> bool, make: impl FnOnce() -> T) -> &mut T {
    let idx = items.iter().position(pred).unwrap_or_else(|| {
        items.push(make());
        items.len() - 1
    });
    &mut items[idx]
}

/// Cap sparkline points by striding, always keeping first and last.
fn downsample(points: &mut Vec<DeathRecapHpPoint>) {
    if points.len() <= MAX_HP_POINTS {
        return;
    }
    let stride = points.len().div_ceil(MAX_HP_POINTS);
    let last = *points.last().unwrap();
    let mut kept: Vec<_> = points.iter().copied().step_by(stride).collect();
    if kept.last() != Some(&last) {
        kept.push(last);
    }
    *points = kept;
}
