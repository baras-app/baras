//! Aggro tab: who each enemy was on, taunts and threat drops

use serde::{Deserialize, Serialize};

/// A stretch of time one entity kept the same target
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggroSegment {
    pub start: f32,
    pub end: f32,
    pub target: String,
}

/// Time an entity spent on one target
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggroShare {
    pub name: String,
    pub is_player: bool,
    pub secs: f32,
}

/// Targets of one enemy instance or one player. Enemy names are numbered
/// ("Name #2") when several instances share one, here and wherever an enemy
/// is named in the report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AggroRow {
    pub name: String,
    /// The enemy's instance id; 0 for a player
    pub instance_id: i64,
    pub is_boss: bool,
    /// Time with a target
    pub secs: f32,
    /// Longest first
    pub shares: Vec<AggroShare>,
    /// Chronological
    pub segments: Vec<AggroSegment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreatEventKind {
    Taunt,
    AoeTaunt,
    Drop,
}

/// What a taunt did, read from the enemy's next target change within
/// `TAUNT_SETTLE_SECS`. No change in that time is no outcome: the log does
/// not say why (the enemy died, was mid-cast, or its change was not logged).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TauntOutcome {
    /// The enemy turned to the taunter
    TookAggro,
    /// The enemy was already on the taunter
    AlreadyHad,
    /// The enemy turned to someone else
    NoSwitch,
}

/// How long after a taunt the enemy's next target change still counts as its
/// result. Enemies turn 0.5 s after a taunt on average and up to ~1.6 s
/// (measured over three raid logs); a later change is something else.
pub const TAUNT_SETTLE_SECS: f32 = 3.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreatEventTarget {
    pub name: String,
    /// Threat set by a taunt, or removed by a drop (negative)
    pub amount: f32,
    /// Taunts only
    pub outcome: Option<TauntOutcome>,
    /// Who the enemy was on before a taunt that took aggro, or who it turned
    /// to instead; empty for no target
    pub other: String,
    /// Player whose taunt was still running on this enemy; empty for none
    pub overlapped: String,
}

/// One press of a taunt or threat drop and every enemy it reached
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreatEvent {
    pub time_secs: f32,
    pub kind: ThreatEventKind,
    pub source: String,
    /// 0 when the log does not tie the taunt to an ability
    pub ability_id: i64,
    pub ability_name: String,
    pub targets: Vec<ThreatEventTarget>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ThreatReport {
    /// Bosses first, then by time with a target
    pub enemies: Vec<AggroRow>,
    pub players: Vec<AggroRow>,
    /// Chronological
    pub events: Vec<ThreatEvent>,
    /// (name, combat second) of every player and listed enemy that died
    pub deaths: Vec<(String, f32)>,
}

impl AggroRow {
    /// A row over `segments`, its time summed per target
    pub fn new(
        name: String,
        instance_id: i64,
        is_boss: bool,
        segments: Vec<AggroSegment>,
        is_player: impl Fn(&str) -> bool,
    ) -> Self {
        let mut shares: Vec<AggroShare> = Vec::new();
        for s in &segments {
            let secs = s.end - s.start;
            match shares.iter_mut().find(|x| x.name == s.target) {
                Some(share) => share.secs += secs,
                None => shares.push(AggroShare {
                    name: s.target.clone(),
                    is_player: is_player(&s.target),
                    secs,
                }),
            }
        }
        shares.sort_by(|a, b| b.secs.total_cmp(&a.secs));
        Self {
            name,
            instance_id,
            is_boss,
            secs: shares.iter().map(|s| s.secs).sum(),
            shares,
            segments,
        }
    }

    /// Bosses first, then by time with a target
    pub fn sort(rows: &mut [Self]) {
        rows.sort_by(|a, b| b.is_boss.cmp(&a.is_boss).then(b.secs.total_cmp(&a.secs)));
    }

    /// The row narrowed to `[start, end]`; None when nothing is left
    fn clipped(&self, start: f32, end: f32) -> Option<Self> {
        let segments: Vec<AggroSegment> = self
            .segments
            .iter()
            .filter(|s| s.end > start && s.start < end)
            .map(|s| AggroSegment {
                start: s.start.max(start),
                end: s.end.min(end),
                target: s.target.clone(),
            })
            .collect();
        (!segments.is_empty()).then(|| {
            let is_player = |n: &str| self.shares.iter().any(|s| s.name == n && s.is_player);
            Self::new(self.name.clone(), self.instance_id, self.is_boss, segments, is_player)
        })
    }
}

impl ThreatReport {
    /// The report narrowed to `[start, end]`. Taunt outcomes stay as read
    /// over the whole fight, so a range edge never cuts one off.
    pub fn clipped(&self, start: f32, end: f32) -> Self {
        let rows = |rows: &[AggroRow]| {
            let mut out: Vec<AggroRow> = rows.iter().filter_map(|r| r.clipped(start, end)).collect();
            AggroRow::sort(&mut out);
            out
        };
        let within = |t: f32| t >= start && t <= end;
        Self {
            enemies: rows(&self.enemies),
            players: rows(&self.players),
            events: self.events.iter().filter(|e| within(e.time_secs)).cloned().collect(),
            deaths: self.deaths.iter().filter(|d| within(d.1)).cloned().collect(),
        }
    }
}
