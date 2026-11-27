//! Replay tab: cast bars and effect windows, keyed by entity instance.
//!
//! Cast lengths come from datamined base times (no alacrity); the log decides
//! when a cast actually stopped.

use std::collections::HashMap;
use std::collections::hash_map::Entry;

use baras_types::{ReplayCast, ReplayEffect};

use super::effects::{APPLY_EFFECT, DAMAGE_EFFECT, HEAL_EFFECT, REMOVE_EFFECT};
use super::*;
use crate::game_data::{cast_secs, channel_secs, effect_id, is_off_gcd};

/// Effect windows shorter than this are not worth an icon
const MIN_EFFECT_SECS: f32 = 0.1;
/// A cancel with more than this share of the cast left counts as an interruption
const CANCEL_SLACK: f32 = 0.2;

fn is_group(entity_type: &str) -> bool {
    matches!(entity_type, "Player" | "Companion")
}

impl EncounterQuery<'_> {
    pub(super) async fn replay_spans(&self, duration: f32) -> Result<(Vec<ReplayCast>, Vec<ReplayEffect>), String> {
        let (activate, interrupt, cancel) = (
            effect_id::ABILITYACTIVATE,
            effect_id::ABILITYINTERRUPT,
            effect_id::ABILITYCANCEL,
        );
        let batches = self
            .sql(&format!(
                "SELECT combat_time_secs, effect_type_id, effect_id, effect_name, ability_id, ability_name,
                        source_id, source_entity_type, target_id, target_entity_type
                 FROM events
                 WHERE effect_id IN ({activate}, {interrupt}, {cancel})
                    OR (effect_type_id IN ({APPLY_EFFECT}, {REMOVE_EFFECT})
                        AND effect_id NOT IN ({DAMAGE_EFFECT}, {HEAL_EFFECT}))
                 ORDER BY line_number"
            ))
            .await?;

        let mut casts: Vec<ReplayCast> = Vec::new();
        // Entity → its cast still running
        let mut casting: HashMap<i64, usize> = HashMap::new();
        let mut effects: Vec<ReplayEffect> = Vec::new();
        // (effect, target, source) → its open window
        let mut open: HashMap<(i64, i64, i64), usize> = HashMap::new();
        let mut started = false;
        for batch in &batches {
            let times = col_opt_f32(batch, 0)?;
            let (kinds, ids, effect_names) = (col_i64(batch, 1)?, col_i64(batch, 2)?, col_strings(batch, 3)?);
            let (abilities, ability_names) = (col_i64(batch, 4)?, col_strings(batch, 5)?);
            let (sources, source_types) = (col_i64(batch, 6)?, col_strings(batch, 7)?);
            let (targets, target_types) = (col_i64(batch, 8)?, col_strings(batch, 9)?);
            for i in 0..batch.num_rows() {
                let t = match times[i] {
                    Some(t) => {
                        started |= t > 0.0;
                        t.max(0.0)
                    }
                    None if !started => 0.0,
                    None => continue,
                };
                let (id, source, target) = (ids[i], sources[i], targets[i]);

                // ─── Casts ───────────────────────────────────────────────────
                if id == activate {
                    let cast = cast_secs(abilities[i]).filter(|s| *s > 0.0);
                    let channel = channel_secs(abilities[i]).filter(|s| *s > 0.0);
                    let timed = cast.or(channel);
                    // A player's next on-GCD ability ends their cast; an NPC's
                    // instant abilities fire alongside its casts
                    let ends_previous = if is_group(&source_types[i]) {
                        !is_off_gcd(abilities[i])
                    } else {
                        timed.is_some()
                    };
                    if ends_previous
                        && let Some(c) = casting.remove(&source)
                        && casts[c].end > t
                    {
                        casts[c].end = t;
                    }
                    if let Some(secs) = timed {
                        casting.insert(source, casts.len());
                        casts.push(ReplayCast {
                            entity_id: source,
                            ability_id: abilities[i],
                            name: ability_names[i].clone(),
                            start: t,
                            end: (t + secs).min(duration),
                            nominal_end: t + secs,
                            is_channel: cast.is_none(),
                            interrupted: false,
                        });
                    }
                    continue;
                }
                if id == interrupt || id == cancel {
                    // An interrupt names the interrupter as source and the caster as target
                    let caster = if id == interrupt && target != 0 { target } else { source };
                    if let Some(c) = casting.remove(&caster)
                        && casts[c].end > t
                    {
                        // A channel sped up by alacrity also logs a cancel just before its base end
                        let remaining = casts[c].nominal_end - t;
                        casts[c].interrupted =
                            id == interrupt || remaining > CANCEL_SLACK * (casts[c].nominal_end - casts[c].start);
                        casts[c].end = t;
                    }
                    continue;
                }

                // ─── Effects ─────────────────────────────────────────────────
                if target == 0 {
                    continue;
                }
                let key = (id, target, source);
                if kinds[i] == APPLY_EFFECT {
                    // A re-apply refreshes the window already open
                    if let Entry::Vacant(slot) = open.entry(key) {
                        slot.insert(effects.len());
                        let name = if effect_names[i].is_empty() {
                            &ability_names[i]
                        } else {
                            &effect_names[i]
                        };
                        effects.push(ReplayEffect {
                            target_id: target,
                            source_id: source,
                            effect_id: id,
                            ability_id: abilities[i],
                            name: name.clone(),
                            start: t,
                            end: duration,
                            // A blank source is a despawned NPC
                            is_debuff: is_group(&source_types[i]) != is_group(&target_types[i]),
                        });
                    }
                } else if source == 0 {
                    // The applier is gone: the effect ends whoever applied it
                    open.retain(|(effect, on, _), w| {
                        let ended = *effect == id && *on == target;
                        if ended {
                            effects[*w].end = t;
                        }
                        !ended
                    });
                } else if let Some(w) = open.remove(&key) {
                    effects[w].end = t;
                } else if t > 0.0 && !effects.iter().any(|e| (e.effect_id, e.target_id, e.source_id) == key) {
                    // Removed without ever being applied: it was up before the pull
                    let name = if effect_names[i].is_empty() {
                        &ability_names[i]
                    } else {
                        &effect_names[i]
                    };
                    effects.push(ReplayEffect {
                        target_id: target,
                        source_id: source,
                        effect_id: id,
                        ability_id: abilities[i],
                        name: name.clone(),
                        start: 0.0,
                        end: t,
                        is_debuff: is_group(&source_types[i]) != is_group(&target_types[i]),
                    });
                }
            }
        }
        casts.retain(|c| c.end > c.start);
        // Stances and passives (self-applied, up from pull to end) say nothing on a card
        effects.retain(|e| {
            e.end - e.start >= MIN_EFFECT_SECS && !(e.source_id == e.target_id && e.start == 0.0 && e.end >= duration)
        });
        Ok((casts, effects))
    }
}
