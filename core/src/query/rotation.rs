//! Rotation analysis queries.
//!
//! Provides ability sequence extraction and GCD-slot grouping for rotation visualization.

use std::collections::HashMap;

use super::*;
use crate::game_data::{OFF_GCD_ABILITIES, effect_id};

/// A damage or heal event to attribute to a rotation cycle.
struct ValueEvent {
    time_secs: f32,
    ability_id: i64,
    damage: i32,
    effective_heal: i32,
    is_crit: bool,
}

impl EncounterQuery<'_> {
    /// Get distinct abilities used by a player (for anchor ability dropdown).
    pub async fn query_player_abilities(
        &self,
        source_name: &str,
        time_range: Option<&TimeRange>,
    ) -> Result<Vec<(i64, String)>, String> {
        let mut conditions = vec![
            format!("effect_id = {}", effect_id::ABILITYACTIVATE),
            format!("source_name = '{}'", sql_escape(source_name)),
        ];
        if let Some(tr) = time_range {
            conditions.push(tr.sql_filter());
        }

        let sql = format!(
            "SELECT DISTINCT ability_id, ability_name \
             FROM events WHERE {} \
             ORDER BY ability_name",
            conditions.join(" AND ")
        );

        let batches = self.sql(&sql).await?;
        let mut results = Vec::new();
        for batch in &batches {
            let ids = col_i64(batch, 0)?;
            let names = col_strings(batch, 1)?;
            for (id, name) in ids.into_iter().zip(names) {
                results.push((id, name));
            }
        }
        Ok(results)
    }

    /// Build a full rotation analysis: fetch ability activations, group into
    /// GCD slots, split into cycles, and calculate per-cycle stats.
    pub async fn query_rotation(
        &self,
        source_name: &str,
        anchor_ability_id: i64,
        time_range: Option<&TimeRange>,
    ) -> Result<RotationAnalysis, String> {
        // 1. Get distinct abilities for the dropdown
        let abilities = self.query_player_abilities(source_name, time_range).await?;

        let escaped = sql_escape(source_name);
        let time_filter = time_range
            .map(|tr| format!(" AND {}", tr.sql_filter()))
            .unwrap_or_default();

        // 2. Fetch all AbilityActivate events ordered by time
        let sql = format!(
            "SELECT combat_time_secs, ability_id, ability_name \
             FROM events WHERE effect_id = {} AND source_name = '{}'{} \
             ORDER BY combat_time_secs ASC, line_number ASC",
            effect_id::ABILITYACTIVATE, escaped, time_filter
        );

        let batches = self.sql(&sql).await?;
        let mut events = Vec::new();
        for batch in &batches {
            let times = col_f32(batch, 0)?;
            let ids = col_i64(batch, 1)?;
            let names = col_strings(batch, 2)?;
            for i in 0..times.len() {
                events.push(RotationEvent {
                    time_secs: times[i],
                    ability_id: ids[i],
                    ability_name: names[i].clone(),
                });
            }
        }

        // 3. Group into GCD slots and split into cycles
        let slots = group_into_gcd_slots(&events);
        let mut cycles = split_into_cycles(slots, anchor_ability_id);

        // 4. Fetch damage/heal events to compute per-cycle stats
        let sql = format!(
            "SELECT combat_time_secs, ability_id, dmg_amount, heal_effective, is_crit \
             FROM events WHERE source_name = '{}' \
             AND (dmg_amount > 0 OR heal_effective > 0){} \
             ORDER BY combat_time_secs ASC",
            escaped, time_filter
        );

        let batches = self.sql(&sql).await?;
        let mut value_events = Vec::new();
        for batch in &batches {
            let times = col_f32(batch, 0)?;
            let ids = col_i64(batch, 1)?;
            let dmg = col_i64(batch, 2)?;
            let heal = col_i64(batch, 3)?;
            let crits = col_bool(batch, 4)?;
            for i in 0..times.len() {
                value_events.push(ValueEvent {
                    time_secs: times[i],
                    ability_id: ids[i],
                    damage: dmg[i] as i32,
                    effective_heal: heal[i] as i32,
                    is_crit: crits[i],
                });
            }
        }

        // 5. Build cast-time index: for each ability_id, sorted list of (cast_time, cycle_index)
        let cast_index = build_cast_index(&cycles);

        // 6. Compute cycle start times for time-range fallback
        let starts: Vec<f32> = cycles
            .iter()
            .map(|c| {
                c.slots
                    .first()
                    .map(|s| s.gcd_ability.time_secs)
                    .unwrap_or(0.0)
            })
            .collect();

        // 7. Attribute each damage/heal event to a cycle:
        //    - If the ability has a cast in the index, use most recent cast's cycle
        //    - Otherwise (procs, secondary effects), fall back to time-range containment
        let mut cycle_stats: Vec<(f64, f64, i64, i64)> = vec![(0.0, 0.0, 0, 0); cycles.len()];
        for evt in &value_events {
            let cycle_idx = find_cast_cycle(&cast_index, evt.ability_id, evt.time_secs)
                .or_else(|| find_cycle_by_time(&starts, evt.time_secs));
            if let Some(idx) = cycle_idx {
                let stats = &mut cycle_stats[idx];
                stats.0 += evt.damage as f64;
                stats.1 += evt.effective_heal as f64;
                if evt.damage > 0 || evt.effective_heal > 0 {
                    stats.3 += 1; // hit_count
                    if evt.is_crit {
                        stats.2 += 1; // crit_count
                    }
                }
            }
        }

        // 8. Apply stats and durations
        for (i, cycle) in cycles.iter_mut().enumerate() {
            let start = starts[i];
            let end = starts.get(i + 1).copied().unwrap_or_else(|| {
                cycle
                    .slots
                    .last()
                    .map(|s| s.gcd_ability.time_secs + 1.5)
                    .unwrap_or(start)
            });
            cycle.duration_secs = end - start;
            let (dmg, heal, crits, hits) = cycle_stats[i];
            cycle.total_damage = dmg;
            cycle.effective_heal = heal;
            cycle.crit_count = crits;
            cycle.hit_count = hits;
        }

        Ok(RotationAnalysis { cycles, abilities })
    }
}

/// Group ability events into GCD slots using the off-GCD ability set.
/// Off-GCD abilities attach to the next on-GCD ability that follows them.
fn group_into_gcd_slots(events: &[RotationEvent]) -> Vec<GcdSlot> {
    let mut slots = Vec::new();
    let mut pending_off_gcd = Vec::new();
    let mut prev_gcd_time: Option<f32> = None;

    for event in events {
        if OFF_GCD_ABILITIES.contains(&event.ability_id) {
            pending_off_gcd.push(event.clone());
        } else {
            let gcd_gap = prev_gcd_time.map(|t| event.time_secs - t);
            prev_gcd_time = Some(event.time_secs);
            slots.push(GcdSlot {
                gcd_ability: event.clone(),
                off_gcd: std::mem::take(&mut pending_off_gcd),
                gcd_gap,
            });
        }
    }

    // Trailing off-GCD abilities with no following GCD — attach to last slot
    if !pending_off_gcd.is_empty()
        && let Some(last) = slots.last_mut() {
            last.off_gcd.extend(pending_off_gcd);
        }

    slots
}

/// Split GCD slots into rotation cycles at each anchor ability occurrence.
fn split_into_cycles(slots: Vec<GcdSlot>, anchor_id: i64) -> Vec<RotationCycle> {
    let mut cycles = Vec::new();
    let mut current_slots = Vec::new();
    let empty_cycle = || RotationCycle {
        slots: Vec::new(),
        duration_secs: 0.0,
        total_damage: 0.0,
        effective_heal: 0.0,
        crit_count: 0,
        hit_count: 0,
    };

    for slot in slots {
        let is_anchor = slot.gcd_ability.ability_id == anchor_id
            || slot.off_gcd.iter().any(|e| e.ability_id == anchor_id);

        if is_anchor && !current_slots.is_empty() {
            let mut cycle = empty_cycle();
            cycle.slots = std::mem::take(&mut current_slots);
            cycles.push(cycle);
        }
        current_slots.push(slot);
    }

    if !current_slots.is_empty() {
        let mut cycle = empty_cycle();
        cycle.slots = current_slots;
        cycles.push(cycle);
    }

    cycles
}

/// Build an index: ability_id -> sorted vec of (cast_time, cycle_index).
/// Used to attribute DOT ticks to the cycle where the ability was cast.
fn build_cast_index(cycles: &[RotationCycle]) -> HashMap<i64, Vec<(f32, usize)>> {
    let mut index: HashMap<i64, Vec<(f32, usize)>> = HashMap::new();
    for (cycle_idx, cycle) in cycles.iter().enumerate() {
        for slot in &cycle.slots {
            index
                .entry(slot.gcd_ability.ability_id)
                .or_default()
                .push((slot.gcd_ability.time_secs, cycle_idx));
            for weave in &slot.off_gcd {
                index
                    .entry(weave.ability_id)
                    .or_default()
                    .push((weave.time_secs, cycle_idx));
            }
        }
    }
    index
}

/// Find the cycle index for a damage/heal event by looking up the most recent
/// cast of the same ability_id at or before the event time.
fn find_cast_cycle(
    cast_index: &HashMap<i64, Vec<(f32, usize)>>,
    ability_id: i64,
    event_time: f32,
) -> Option<usize> {
    let casts = cast_index.get(&ability_id)?;
    // Binary search for the last cast at or before event_time
    let pos = casts.partition_point(|(t, _)| *t <= event_time);
    if pos > 0 {
        Some(casts[pos - 1].1)
    } else {
        // Event before any cast of this ability — attribute to first cycle
        Some(casts.first()?.1)
    }
}

/// Fallback: find which cycle an event falls into by time range alone.
/// Used for procs and secondary effects that have no AbilityActivate cast.
fn find_cycle_by_time(starts: &[f32], event_time: f32) -> Option<usize> {
    if starts.is_empty() {
        return None;
    }
    let pos = starts.partition_point(|&t| t <= event_time);
    Some(if pos > 0 { pos - 1 } else { 0 })
}
