//! Aggro tab: who every enemy and player was targeting, and taunts and
//! threat drops with what they did to the enemy's target.
//!
//! Aggro is read from the enemies' own `TargetSet` lines, not from summed
//! threat: the log books heal threat on the healed player, so per-enemy
//! threat totals do not say who an enemy was on.

use std::collections::{HashMap, HashSet};

use baras_types::{
    AggroRow, AggroSegment, TAUNT_SETTLE_SECS, TauntOutcome, ThreatEvent, ThreatEventKind, ThreatEventTarget,
};

use super::effects::{APPLY_EFFECT, REMOVE_EFFECT};
use super::*;
use crate::game_data::{TAUNT_EFFECTS, effect_id};

/// Lines of one press land this close together
const PRESS_SECS: f32 = 0.5;
/// The log's `Taunt` line follows the taunt's effect by up to this long
const TAUNT_LINE_SECS: f32 = 1.5;

/// One entity's targets over time
struct Lane {
    name: String,
    is_boss: bool,
    /// Last moment the entity counts as present
    end: f32,
    current: Option<(String, f32)>,
    segments: Vec<AggroSegment>,
}

impl Lane {
    fn retarget(&mut self, target: Option<String>, t: f32) {
        if let Some((target, start)) = self.current.take()
            && t > start
        {
            self.segments.push(AggroSegment { start, end: t, target });
        }
        self.current = target.map(|name| (name, t));
    }

    /// Who the entity turned to next after `t`, if it did within `TAUNT_SETTLE_SECS`
    fn next_target(&self, t: f32) -> Option<&str> {
        self.segments
            .iter()
            .find(|s| s.start > t)
            .filter(|s| s.start <= t + TAUNT_SETTLE_SECS)
            .map(|s| s.target.as_str())
    }
}

struct Hit {
    target_id: i64,
    time: f32,
    name: String,
    amount: f32,
    before: String,
    overlapped: String,
}

struct Press {
    time: f32,
    kind: ThreatEventKind,
    source: String,
    ability_id: i64,
    ability_name: String,
    hits: Vec<Hit>,
}

/// The press these lines belong to, started if this is its first line
fn press<'a>(
    presses: &'a mut Vec<Press>,
    time: f32,
    kind: ThreatEventKind,
    source: &str,
    ability: (i64, &str),
) -> &'a mut Press {
    let found = presses
        .iter()
        .rposition(|p| p.kind == kind && p.ability_id == ability.0 && p.source == source)
        .filter(|i| time - presses[*i].time <= PRESS_SECS);
    let i = found.unwrap_or_else(|| {
        presses.push(Press {
            time,
            kind,
            source: source.to_string(),
            ability_id: ability.0,
            ability_name: ability.1.to_string(),
            hits: Vec::new(),
        });
        presses.len() - 1
    });
    &mut presses[i]
}

/// One row per lane with a target at some point
fn rows(lanes: Vec<(i64, Lane)>, players: &HashSet<String>, are_enemies: bool) -> Vec<AggroRow> {
    let mut out: Vec<AggroRow> = lanes
        .into_iter()
        .filter(|(_, lane)| !lane.segments.is_empty())
        .map(|(id, lane)| {
            let instance_id = if are_enemies { id } else { 0 };
            AggroRow::new(lane.name, instance_id, lane.is_boss, lane.segments, |n| players.contains(n))
        })
        .collect();
    AggroRow::sort(&mut out);
    out
}

impl EncounterQuery<'_> {
    /// The whole fight's report; `ThreatReport::clipped` narrows it to a range
    pub async fn threat_report(&self) -> Result<ThreatReport, String> {
        // Enemies go by instance everywhere below: same-named ones are numbered
        let enemies = self.enemy_instances().await?;
        let labels: HashMap<i64, String> = enemies.iter().map(|e| (e.id, e.label.clone())).collect();
        let label = |id: i64, name: &str| labels.get(&id).cloned().unwrap_or_else(|| name.to_string());
        let mut fight_end = 0.0f32;
        // Each enemy present until the group's last action on it
        let mut lanes: HashMap<i64, Lane> = HashMap::new();
        for e in &enemies {
            fight_end = fight_end.max(e.last);
            lanes.insert(
                e.id,
                Lane {
                    name: e.label.clone(),
                    is_boss: e.is_boss,
                    end: e.last,
                    current: None,
                    segments: Vec::new(),
                },
            );
        }

        let taunt_ids: Vec<String> = TAUNT_EFFECTS.keys().map(i64::to_string).collect();
        let (target_set, target_cleared) = (effect_id::TARGETSET, effect_id::TARGETCLEARED);
        let (taunt, modify_threat, death) = (effect_id::TAUNT, effect_id::MODIFYTHREAT, effect_id::DEATH);
        let batches = self
            .sql(&format!(
                "SELECT source_id, source_name, source_entity_type, target_id, target_name, target_entity_type,
                        effect_type_id, effect_id, effect_name, ability_id, ability_name,
                        CAST(threat AS REAL), CAST(COALESCE(combat_time_secs, 0.0) AS REAL)
                 FROM events
                 WHERE effect_id IN ({target_set}, {target_cleared})
                    OR (combat_time_secs >= 0
                        AND (effect_id IN ({taunt}, {death})
                             OR (effect_id = {modify_threat} AND threat < 0)
                             OR (effect_id IN ({}) AND effect_type_id IN ({APPLY_EFFECT}, {REMOVE_EFFECT}))))
                 ORDER BY line_number",
                taunt_ids.join(", ")
            ))
            .await?;
        let mut players: HashSet<String> = HashSet::new();
        let mut player_ids: HashSet<i64> = HashSet::new();
        // Enemy instance → player whose taunt effect is on it
        let mut taunted_by: HashMap<i64, String> = HashMap::new();
        let mut presses: Vec<Press> = Vec::new();
        let mut deaths: Vec<(String, f32)> = Vec::new();
        for batch in &batches {
            let (sources, source_names, source_types) =
                (col_i64(batch, 0)?, col_strings(batch, 1)?, col_strings(batch, 2)?);
            let (targets, target_names, target_types) =
                (col_i64(batch, 3)?, col_strings(batch, 4)?, col_strings(batch, 5)?);
            let (kinds, effects, effect_names) = (col_i64(batch, 6)?, col_i64(batch, 7)?, col_strings(batch, 8)?);
            let (abilities, ability_names) = (col_i64(batch, 9)?, col_strings(batch, 10)?);
            let (threats, times) = (col_f32(batch, 11)?, col_f32(batch, 12)?);
            for i in 0..batch.num_rows() {
                // A target picked before the pull (no combat time, or a negative one) holds from 0
                let (t, effect, source) = (times[i].max(0.0), effects[i], source_names[i].as_str());
                fight_end = fight_end.max(t);
                for (kind, name) in [(&source_types[i], source), (&target_types[i], target_names[i].as_str())] {
                    if kind == "Player" && !players.contains(name) {
                        players.insert(name.to_string());
                    }
                }
                if effect == target_set || effect == target_cleared {
                    if source_types[i] == "Player" && player_ids.insert(sources[i]) {
                        lanes.insert(
                            sources[i],
                            Lane {
                                name: source.to_string(),
                                is_boss: false,
                                end: f32::MAX,
                                current: None,
                                segments: Vec::new(),
                            },
                        );
                    }
                    if let Some(lane) = lanes.get_mut(&sources[i]) {
                        lane.retarget((effect == target_set).then(|| label(targets[i], &target_names[i])), t);
                    }
                    continue;
                }
                if effect == death {
                    if target_types[i] == "Player" || lanes.contains_key(&targets[i]) {
                        deaths.push((label(targets[i], &target_names[i]), t));
                    }
                    continue;
                }
                if !matches!(source_types[i].as_str(), "Player" | "Companion") || target_types[i] != "Npc" {
                    continue;
                }
                let target = targets[i];
                if let Some(area) = TAUNT_EFFECTS.get(&effect) {
                    if kinds[i] == REMOVE_EFFECT {
                        if taunted_by.get(&target).is_some_and(|by| by == source) {
                            taunted_by.remove(&target);
                        }
                        continue;
                    }
                    let kind = if *area {
                        ThreatEventKind::AoeTaunt
                    } else {
                        ThreatEventKind::Taunt
                    };
                    let overlapped = taunted_by.insert(target, source.to_string()).filter(|by| by != source);
                    press(&mut presses, t, kind, source, (abilities[i], &ability_names[i]))
                        .hits
                        .push(Hit {
                            target_id: target,
                            time: t,
                            name: label(target, &target_names[i]),
                            amount: 0.0,
                            before: lanes
                                .get(&target)
                                .and_then(|l| l.current.clone())
                                .map_or_else(String::new, |c| c.0),
                            overlapped: overlapped.unwrap_or_default(),
                        });
                } else if effect == taunt {
                    // The threat a taunt set, logged after its effect; a taunt
                    // with no effect line stands on its own
                    let pending = presses
                        .iter_mut()
                        .rev()
                        .take_while(|p| t - p.time <= TAUNT_LINE_SECS)
                        .filter(|p| p.kind != ThreatEventKind::Drop && p.source == source)
                        .flat_map(|p| p.hits.iter_mut())
                        .find(|h| h.target_id == target && h.amount == 0.0);
                    if let Some(hit) = pending {
                        hit.amount = threats[i];
                    } else {
                        press(&mut presses, t, ThreatEventKind::Taunt, source, (0, &effect_names[i]))
                            .hits
                            .push(Hit {
                                target_id: target,
                                time: t,
                                name: label(target, &target_names[i]),
                                amount: threats[i],
                                before: lanes
                                    .get(&target)
                                    .and_then(|l| l.current.clone())
                                    .map_or_else(String::new, |c| c.0),
                                overlapped: String::new(),
                            });
                    }
                } else if effect == modify_threat {
                    press(
                        &mut presses,
                        t,
                        ThreatEventKind::Drop,
                        source,
                        (abilities[i], &ability_names[i]),
                    )
                    .hits
                    .push(Hit {
                        target_id: target,
                        time: t,
                        name: label(target, &target_names[i]),
                        amount: threats[i],
                        before: String::new(),
                        overlapped: String::new(),
                    });
                }
            }
        }
        for lane in lanes.values_mut() {
            lane.end = lane.end.min(fight_end);
            lane.retarget(None, lane.end);
            lane.segments.retain_mut(|s| {
                s.end = s.end.min(lane.end);
                s.end > s.start
            });
        }

        let events = presses
            .into_iter()
            .map(|p| ThreatEvent {
                time_secs: p.time,
                kind: p.kind,
                targets: p
                    .hits
                    .into_iter()
                    .map(|h| {
                        let lane = lanes.get(&h.target_id).filter(|_| p.kind != ThreatEventKind::Drop);
                        let (outcome, other) = match lane.map(|l| l.next_target(h.time)) {
                            None => (None, String::new()),
                            Some(_) if h.before == p.source => (Some(TauntOutcome::AlreadyHad), String::new()),
                            Some(Some(next)) if next == p.source => (Some(TauntOutcome::TookAggro), h.before),
                            Some(Some(next)) => (Some(TauntOutcome::NoSwitch), next.to_string()),
                            Some(None) => (None, String::new()),
                        };
                        ThreatEventTarget {
                            name: h.name,
                            amount: h.amount,
                            outcome,
                            other,
                            overlapped: h.overlapped,
                        }
                    })
                    .collect(),
                source: p.source,
                ability_id: p.ability_id,
                ability_name: p.ability_name,
            })
            .collect();

        let (player_lanes, enemy_lanes): (Vec<_>, Vec<_>) =
            lanes.into_iter().partition(|(id, _)| player_ids.contains(id));
        Ok(ThreatReport {
            enemies: rows(enemy_lanes, &players, true),
            players: rows(player_lanes, &players, false),
            events,
            deaths,
        })
    }
}
