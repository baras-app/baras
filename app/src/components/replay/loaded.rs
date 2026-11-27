//! The replay data with what the cards need looked up by entity

use std::collections::HashMap;

use baras_types::{MapDefinition, PhaseSegment, ReplayData, ReplayEntity, ReplayEntityKind};

use super::card_view::{CardView, CastView, IconView};

/// Same as the canvas: an NPC stays listed this long after its last log line
const NPC_LINGER_SECS: f32 = 2.0;
/// An interrupted cast stays on the card this long
const INTERRUPT_LINGER_SECS: f32 = 0.6;
/// Buff or debuff icons per card row
const MAX_ICONS: usize = 12;
/// An entity this far past a map's edge (as a share of its size) is still drawn
const EDGE_MARGIN: f32 = 0.25;
/// A map entry is offered when it covers at least 1/N of the players' samples
const MAP_SHARE_DIVISOR: usize = 20;

pub struct Loaded {
    pub data: ReplayData,
    /// Number shown on players who share a discipline with another
    pub numbers: HashMap<i64, u8>,
    /// Entity id → index in `data.entities`
    index_of: HashMap<i64, usize>,
    casts_of: HashMap<i64, Vec<usize>>,
    activations_of: HashMap<i64, Vec<usize>>,
    effects_on: HashMap<i64, Vec<usize>>,
}

/// Index of the last time at or before `t`
fn index_at(times: &[f32], t: f32) -> Option<usize> {
    times.partition_point(|x| *x <= t).checked_sub(1)
}

/// Where an entity was at `t` (its last sample; its first when `t` is earlier)
fn position_at(e: &ReplayEntity, t: f32) -> Option<(f32, f32, f32)> {
    let i = index_at(&e.track.t, t).or_else(|| (!e.track.t.is_empty()).then_some(0))?;
    Some((e.track.x[i], e.track.y[i], e.track.z[i]))
}

/// A player's role, from its role icon: tanks sort first, then healers, then damage
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Tank,
    Healer,
    Damage,
    Unknown,
}

impl Role {
    pub fn of(e: &ReplayEntity) -> Self {
        match e.role_icon.as_deref() {
            Some(icon) if icon.contains("tank") => Self::Tank,
            Some(icon) if icon.contains("heal") => Self::Healer,
            Some(_) => Self::Damage,
            None => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Tank => "TANK",
            Self::Healer => "HEAL",
            Self::Damage => "DPS",
            Self::Unknown => "",
        }
    }

    /// Name the canvas script colours rings by
    pub fn key(self) -> Option<&'static str> {
        match self {
            Self::Tank => Some("tank"),
            Self::Healer => Some("heal"),
            Self::Damage => Some("dps"),
            Self::Unknown => None,
        }
    }
}

/// Average per second from the pull to `t`, over a per-second series
fn rate(series: &[u32], t: f32) -> f32 {
    let now = t as usize;
    let done: u64 = series.iter().take(now).map(|v| *v as u64).sum();
    // The second being played counts for as much of it as has passed
    let current = series.get(now).map_or(0.0, |v| *v as f32 * t.fract());
    (done as f32 + current) / t.max(1.0)
}

impl Loaded {
    pub fn new(data: ReplayData) -> Self {
        let index_of = data.entities.iter().enumerate().map(|(i, e)| (e.id, i)).collect();
        let mut casts_of: HashMap<i64, Vec<usize>> = HashMap::new();
        for (i, cast) in data.casts.iter().enumerate() {
            casts_of.entry(cast.entity_id).or_default().push(i);
        }
        let mut activations_of: HashMap<i64, Vec<usize>> = HashMap::new();
        for (i, activation) in data.activations.iter().enumerate() {
            activations_of.entry(activation.entity_id).or_default().push(i);
        }
        let mut effects_on: HashMap<i64, Vec<usize>> = HashMap::new();
        for (i, effect) in data.effects.iter().enumerate() {
            effects_on.entry(effect.target_id).or_default().push(i);
        }
        // Entities arrive with players in name order, so numbers follow it
        let mut by_discipline: HashMap<i64, Vec<i64>> = HashMap::new();
        for e in &data.entities {
            if let Some(discipline) = e.discipline_id {
                by_discipline.entry(discipline).or_default().push(e.id);
            }
        }
        let numbers = by_discipline
            .into_values()
            .filter(|ids| ids.len() > 1)
            .flat_map(|ids| ids.into_iter().enumerate().map(|(i, id)| (id, i as u8 + 1)))
            .collect();
        Self {
            data,
            numbers,
            index_of,
            casts_of,
            activations_of,
            effects_on,
        }
    }

    /// Whether the entity is in the fight at `t` (players always are)
    pub fn is_present(&self, e: &ReplayEntity, t: f32) -> bool {
        if e.kind != ReplayEntityKind::Npc {
            return true;
        }
        let times = &e.track.t;
        let Some(i) = index_at(times, t) else { return false };
        let gone = i == times.len() - 1
            && t > times[i] + NPC_LINGER_SECS
            && times[i] < self.data.duration_secs - NPC_LINGER_SECS;
        !gone && !e.dead.iter().any(|(from, to)| t >= *from && t < *to)
    }

    /// Whether a map entry covers a position in a phase (same rule as the canvas)
    fn covers(map: &MapDefinition, (x, y, z): (f32, f32, f32), phase: Option<&PhaseSegment>) -> bool {
        let b = &map.bounds;
        (b.min_x.min(b.max_x)..=b.min_x.max(b.max_x)).contains(&x)
            && (b.min_y.min(b.max_y)..=b.min_y.max(b.max_y)).contains(&y)
            && map.min_z.is_none_or(|min| z >= min)
            && map.max_z.is_none_or(|max| z <= max)
            && (map.phases.is_empty() || phase.is_some_and(|p| map.phases.contains(&p.phase_id)))
    }

    /// Whether a position is on a map's floor and on or just past its image:
    /// what the canvas still draws (same rule and margin as `mapNear` there)
    fn is_near(map: &MapDefinition, at: (f32, f32, f32), phase: Option<&PhaseSegment>) -> bool {
        let b = &map.bounds;
        let (dx, dy) = (
            (b.max_x - b.min_x).abs() * EDGE_MARGIN,
            (b.max_y - b.min_y).abs() * EDGE_MARGIN,
        );
        let grown = MapDefinition {
            bounds: baras_types::MapRect {
                min_x: b.min_x.min(b.max_x) - dx,
                max_x: b.min_x.max(b.max_x) + dx,
                min_y: b.min_y.min(b.max_y) - dy,
                max_y: b.min_y.max(b.max_y) + dy,
            },
            ..map.clone()
        };
        Self::covers(&grown, at, phase)
    }

    /// The entries this fight needs. An area lists every room of the instance
    /// and pages that span all of it: each place the players stood counts for
    /// the smallest entry covering it, and entries left with too little are
    /// dropped. The whole-instance page stays only if a real share of the
    /// fight happened outside every room page.
    pub fn maps_in_use(&self, maps: &[MapDefinition]) -> Vec<MapDefinition> {
        let mut counts = vec![0usize; maps.len()];
        let mut total = 0usize;
        // Phase limits are checked during playback, not here
        let unphased: Vec<MapDefinition> = maps
            .iter()
            .map(|m| MapDefinition {
                phases: Vec::new(),
                ..m.clone()
            })
            .collect();
        for e in self.data.entities.iter().filter(|e| e.kind == ReplayEntityKind::Player) {
            let t = &e.track;
            for i in 0..t.t.len() {
                total += 1;
                let at = (t.x[i], t.y[i], t.z[i]);
                if let Some(m) = unphased.iter().position(|m| Self::covers(m, at, None)) {
                    counts[m] += 1;
                }
            }
        }
        maps.iter()
            .zip(counts)
            .filter(|(_, n)| *n * MAP_SHARE_DIVISOR >= total.max(1))
            .map(|(m, _)| m.clone())
            .collect()
    }

    pub fn card(
        &self,
        e: &ReplayEntity,
        t: f32,
        // The highlighted entity: its own card shows rates, every card marks its effects
        highlight: Option<i64>,
        maps: &[MapDefinition],
        shown_map: Option<usize>,
        phases: &[PhaseSegment],
    ) -> CardView {
        let died_at = e
            .dead
            .iter()
            .find(|(from, to)| t >= *from && t < *to)
            .map(|(from, _)| *from);
        let mut view = CardView {
            dead: died_at.is_some(),
            died_at,
            ..CardView::default()
        };
        if let Some(i) = index_at(&e.track.t, t).or_else(|| (!e.track.t.is_empty()).then_some(0)) {
            (view.hp, view.max_hp) = (e.track.hp[i], e.track.max_hp[i]);
        }
        if view.dead {
            view.hp = 0;
        }
        view.target = e
            .targets
            .get(e.targets.partition_point(|(at, _)| *at <= t).wrapping_sub(1))
            .and_then(|(_, id)| *id)
            .and_then(|id| self.index_of.get(&id).copied());
        if let Some(target) = view.target.map(|i| &self.data.entities[i])
            && let (Some(a), Some(b)) = (position_at(e, t), position_at(target, t))
        {
            let d = ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2) + (a.2 - b.2).powi(2)).sqrt();
            view.target_distance = Some(d.round() as u32);
        }

        if let Some(casts) = self.casts_of.get(&e.id) {
            let last = casts.partition_point(|c| self.data.casts[*c].start <= t).checked_sub(1);
            if let Some(cast) = last.map(|i| &self.data.casts[casts[i]])
                && (t < cast.end || (cast.interrupted && t < cast.end + INTERRUPT_LINGER_SECS))
            {
                // The log already says when an uninterrupted cast finished
                let full = if cast.interrupted { cast.nominal_end } else { cast.end } - cast.start;
                let done = (t.min(cast.end) - cast.start) / full.max(0.01);
                view.cast = Some(CastView {
                    name: cast.name.clone(),
                    ability_id: cast.ability_id,
                    pct: (done.clamp(0.0, 1.0) * 100.0) as u8,
                    is_channel: cast.is_channel,
                    interrupted: cast.interrupted && t >= cast.end,
                });
            }
        }

        if view.cast.is_none()
            && let Some(uses) = self.activations_of.get(&e.id)
            && let Some(last) = uses
                .partition_point(|a| self.data.activations[*a].t <= t)
                .checked_sub(1)
        {
            let used = &self.data.activations[uses[last]];
            view.last_ability = Some((used.ability_id, used.name.clone()));
        }

        for effect in self
            .effects_on
            .get(&e.id)
            .into_iter()
            .flatten()
            .map(|i| &self.data.effects[*i])
        {
            if t < effect.start || t >= effect.end {
                continue;
            }
            let (row, more) = if effect.is_debuff {
                (&mut view.debuffs, &mut view.more_debuffs)
            } else {
                (&mut view.buffs, &mut view.more_buffs)
            };
            let from_highlight = highlight == Some(effect.source_id);
            let stacks = effect
                .stacks
                .get(effect.stacks.partition_point(|(at, _)| *at <= t).wrapping_sub(1))
                .map_or(1, |(_, n)| *n);
            // One icon per effect, whoever applied it
            if let Some(icon) = row.iter_mut().find(|icon| icon.effect_id == effect.effect_id) {
                icon.from_highlight |= from_highlight;
                icon.stacks = icon.stacks.max(stacks);
                continue;
            }
            if row.len() == MAX_ICONS {
                *more += 1;
                continue;
            }
            row.push(IconView {
                effect_id: effect.effect_id,
                ability_id: effect.ability_id,
                name: effect.name.clone(),
                from_highlight,
                stacks,
            });
        }

        if highlight == Some(e.id) {
            view.rates = Some((rate(&e.damage, t), rate(&e.healing, t)));
        }
        if !maps.is_empty()
            && let Some(i) = index_at(&e.track.t, t).or_else(|| (!e.track.t.is_empty()).then_some(0))
        {
            let at = (e.track.x[i], e.track.y[i], e.track.z[i]);
            let phase = phases.iter().find(|p| t >= p.start_secs && t < p.end_secs);
            view.off_map = shown_map
                .and_then(|s| maps.get(s))
                .is_some_and(|m| !Self::is_near(m, at, phase));
            view.map = maps.iter().position(|m| Self::covers(m, at, phase));
        }
        view
    }

    /// Damage and healing per second of all players together
    pub fn raid_rates(&self, t: f32) -> (f32, f32) {
        self.data.entities.iter().fold((0.0, 0.0), |(d, h), e| {
            (d + rate(&e.damage, t), h + rate(&e.healing, t))
        })
    }
}
