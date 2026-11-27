//! Replay tab: where every entity stood, faced and how healthy it was over
//! the fight, with its targets, deaths and (for players) output per second.
//!
//! Positions only exist on log lines an entity takes part in, so a track is a
//! sparse series of observations, not a fixed-rate recording.

use std::collections::HashMap;

use baras_types::{ReplayData, ReplayEntity, ReplayEntityKind, ReplayTrack};

use super::*;
use crate::game_data::{effect_id, is_boss};

/// Observations closer together than this collapse into the latest one
const SAMPLE_SECS: f32 = 0.25;

struct Observation {
    x: f32,
    y: f32,
    z: f32,
    facing: f32,
    hp: i32,
    max_hp: i32,
}

struct Builder {
    entity: ReplayEntity,
    order: usize,
    last_bucket: i64,
    died_at: Option<f32>,
}

impl Builder {
    fn new(id: i64, name: &str, kind: ReplayEntityKind, class_id: i64, order: usize) -> Self {
        Self {
            entity: ReplayEntity {
                id,
                name: name.to_string(),
                kind,
                class_id,
                is_boss: kind == ReplayEntityKind::Npc && is_boss(class_id),
                discipline_id: None,
                class_icon: None,
                role_icon: None,
                track: ReplayTrack::default(),
                targets: Vec::new(),
                dead: Vec::new(),
                damage: Vec::new(),
                healing: Vec::new(),
            },
            order,
            last_bucket: i64::MIN,
            died_at: None,
        }
    }

    fn observe(&mut self, t: f32, o: Observation) {
        let track = &mut self.entity.track;
        let bucket = (t / SAMPLE_SECS) as i64;
        let same = |track: &ReplayTrack, i: usize| {
            track.x[i] == o.x
                && track.y[i] == o.y
                && track.z[i] == o.z
                && track.facing[i] == o.facing
                && track.hp[i] == o.hp
                && track.max_hp[i] == o.max_hp
        };
        let n = track.t.len();
        // A run of identical samples keeps its first and last, so standing
        // still reads as standing still up to the moment the entity moves
        let extends_run = n >= 2 && same(track, n - 1) && same(track, n - 2);
        if n > 0 && (bucket == self.last_bucket || extends_run) {
            let i = n - 1;
            track.t[i] = t;
            (track.x[i], track.y[i], track.z[i], track.facing[i]) = (o.x, o.y, o.z, o.facing);
            (track.hp[i], track.max_hp[i]) = (o.hp, o.max_hp);
        } else {
            track.t.push(t);
            track.x.push(o.x);
            track.y.push(o.y);
            track.z.push(o.z);
            track.facing.push(o.facing);
            track.hp.push(o.hp);
            track.max_hp.push(o.max_hp);
        }
        self.last_bucket = bucket;
    }

    fn revive(&mut self, t: f32) {
        if let Some(start) = self.died_at.take() {
            self.entity.dead.push((start, t));
        }
    }
}

fn kind_of(entity_type: &str) -> Option<ReplayEntityKind> {
    match entity_type {
        "Player" => Some(ReplayEntityKind::Player),
        "Companion" => Some(ReplayEntityKind::Companion),
        "Npc" => Some(ReplayEntityKind::Npc),
        _ => None,
    }
}

fn add_at(series: &mut Vec<u32>, t: f32, amount: i32) {
    let i = t as usize;
    if series.len() <= i {
        series.resize(i + 1, 0);
    }
    series[i] += amount.max(0) as u32;
}

impl EncounterQuery<'_> {
    pub async fn replay(&self) -> Result<ReplayData, String> {
        let batches = self
            .sql(
                "SELECT combat_time_secs, effect_id, dmg_amount, heal_effective,
                        source_id, source_name, source_entity_type, source_class_id,
                        source_x, source_y, source_z, source_facing, source_hp, source_max_hp,
                        target_id, target_name, target_entity_type, target_class_id,
                        target_x, target_y, target_z, target_facing, target_hp, target_max_hp
                 FROM events
                 ORDER BY line_number",
            )
            .await?;

        let mut builders: HashMap<i64, Builder> = HashMap::new();
        let mut duration = 0.0f32;
        let mut started = false;
        for batch in &batches {
            let times = col_opt_f32(batch, 0)?;
            let effects = col_i64(batch, 1)?;
            let (damage, healing) = (col_i32(batch, 2)?, col_i32(batch, 3)?);
            // Source columns start at 4, target columns at 14
            let mut sides = Vec::with_capacity(2);
            for base in [4, 14] {
                sides.push((
                    col_i64(batch, base)?,
                    col_strings(batch, base + 1)?,
                    col_strings(batch, base + 2)?,
                    col_i64(batch, base + 3)?,
                    [
                        col_opt_f32(batch, base + 4)?,
                        col_opt_f32(batch, base + 5)?,
                        col_opt_f32(batch, base + 6)?,
                        col_opt_f32(batch, base + 7)?,
                    ],
                    col_i32(batch, base + 8)?,
                    col_i32(batch, base + 9)?,
                ));
            }
            for i in 0..batch.num_rows() {
                // Lines before the pull count from 0; lines with no combat
                // time after it (post-combat) are not part of the fight
                let t = match times[i] {
                    Some(t) => {
                        started |= t > 0.0;
                        t.max(0.0)
                    }
                    None if !started => 0.0,
                    None => continue,
                };
                duration = duration.max(t);
                let mut ids = [0i64; 2];
                for (side, (entity_ids, names, types, classes, pos, hp, max_hp)) in sides.iter().enumerate() {
                    let Some(kind) = kind_of(&types[i]) else { continue };
                    let id = entity_ids[i];
                    ids[side] = id;
                    // A self-targeted line carries the same entity twice
                    if side == 1 && id == ids[0] {
                        continue;
                    }
                    let order = builders.len();
                    let b = builders
                        .entry(id)
                        .or_insert_with(|| Builder::new(id, &names[i], kind, classes[i], order));
                    if let (Some(x), Some(y), Some(z), Some(facing)) = (pos[0][i], pos[1][i], pos[2][i], pos[3][i]) {
                        b.observe(
                            t,
                            Observation {
                                x,
                                y,
                                z,
                                facing,
                                hp: hp[i],
                                max_hp: max_hp[i],
                            },
                        );
                    }
                }
                let [source, target] = ids;
                let effect = effects[i];
                if effect == effect_id::TARGETSET || effect == effect_id::TARGETCLEARED {
                    if let Some(b) = builders.get_mut(&source) {
                        let new = (effect == effect_id::TARGETSET && target != 0).then_some(target);
                        if b.entity.targets.last().map(|(_, id)| *id) != Some(new) {
                            b.entity.targets.push((t, new));
                        }
                    }
                } else if effect == effect_id::DEATH {
                    if let Some(b) = builders.get_mut(&target)
                        && b.died_at.is_none()
                    {
                        b.died_at = Some(t);
                    }
                } else if effect == effect_id::REVIVED {
                    for id in ids {
                        if let Some(b) = builders.get_mut(&id) {
                            b.revive(t);
                        }
                    }
                } else if let Some(b) = builders.get_mut(&source)
                    && b.entity.kind == ReplayEntityKind::Player
                {
                    if effect == effect_id::DAMAGE && source != target {
                        add_at(&mut b.entity.damage, t, damage[i]);
                    } else if effect == effect_id::HEAL {
                        add_at(&mut b.entity.healing, t, healing[i]);
                    }
                }
            }
        }

        let mut builders: Vec<Builder> = builders
            .into_values()
            // An NPC never seen with a position cannot be placed
            .filter(|b| b.entity.kind == ReplayEntityKind::Player || !b.entity.track.t.is_empty())
            .collect();
        // Players by name, then the rest in order of appearance
        builders.sort_by(|a, b| {
            let rank = |x: &Builder| x.entity.kind != ReplayEntityKind::Player;
            rank(a).cmp(&rank(b)).then_with(|| {
                if rank(a) {
                    a.order.cmp(&b.order)
                } else {
                    a.entity.name.cmp(&b.entity.name)
                }
            })
        });
        let entities: Vec<ReplayEntity> = builders
            .into_iter()
            .map(|mut b| {
                if let Some(start) = b.died_at.take() {
                    b.entity.dead.push((start, duration));
                }
                b.entity
            })
            .collect();

        let (mut casts, effects) = self.replay_spans(duration).await?;
        // Dying ends a cast
        let deaths: HashMap<i64, &[(f32, f32)]> = entities.iter().map(|e| (e.id, e.dead.as_slice())).collect();
        for cast in &mut casts {
            if let Some(dead) = deaths.get(&cast.entity_id)
                && let Some((died, _)) = dead.iter().find(|(died, _)| *died > cast.start && *died < cast.end)
            {
                cast.end = *died;
            }
        }

        Ok(ReplayData {
            duration_secs: duration,
            entities,
            casts,
            effects,
        })
    }
}
