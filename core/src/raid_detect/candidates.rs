//! Players available to the raid-frame matcher.
//!
//! Names always come from the log. OCR only picks the player.

use std::collections::HashMap;

use chrono::NaiveDateTime;

use super::normalize::normalize;
use crate::combat_log::{Entity, EntityType};
use crate::context::resolve;

/// A player seen in the combat log, eligible to be matched to a raid-frame row.
#[derive(Debug, Clone)]
pub struct PlayerCandidate {
    /// Log entity id, stable for the session.
    pub entity_id: i64,
    /// Exact spelling from the log — this is what gets displayed.
    pub name: String,
    /// Folded form used for comparison only. See [`normalize`].
    pub normalized: String,
    /// Most recent health reading from the log.
    pub current_hp: i32,
    /// Most recent max health from the log.
    pub max_hp: i32,
    /// When this player was last seen in the log.
    pub last_seen: NaiveDateTime,
}

impl PlayerCandidate {
    /// Health as a whole percentage, matching how SWTOR renders it.
    ///
    /// Returns `None` when max health is unknown, which happens for entities
    /// that have only ever appeared as an effect target.
    pub fn hp_percent(&self) -> Option<u8> {
        if self.max_hp <= 0 {
            return None;
        }
        let pct = (self.current_hp as f64 / self.max_hp as f64) * 100.0;
        Some(pct.round().clamp(0.0, 100.0) as u8)
    }
}

/// Candidate players, keyed by log entity id.
#[derive(Debug, Default)]
pub struct CandidateSet {
    by_entity: HashMap<i64, PlayerCandidate>,
}

impl CandidateSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a player and keep the latest populated health reading.
    pub fn observe(&mut self, entity: &Entity, at: NaiveDateTime) {
        if entity.entity_type != EntityType::Player || entity.log_id == 0 {
            return;
        }
        let name = resolve(entity.name);
        if name.is_empty() {
            return;
        }
        self.observe_raw(entity.log_id, name, entity.health, at);
    }

    /// Record a player without constructing an interned [`Entity`].
    pub fn observe_raw(&mut self, entity_id: i64, name: &str, health: (i32, i32), at: NaiveDateTime) {
        let (current_hp, max_hp) = health;

        match self.by_entity.get_mut(&entity_id) {
            Some(existing) => {
                if at >= existing.last_seen {
                    existing.last_seen = at;
                    if max_hp > 0 {
                        existing.current_hp = current_hp;
                        existing.max_hp = max_hp;
                    }
                }
                // A later line may carry a fuller spelling than the first sighting.
                if name.len() > existing.name.len() {
                    existing.normalized = normalize(name);
                    existing.name = name.to_string();
                }
            }
            None => {
                let (current_hp, max_hp) = if max_hp > 0 { (current_hp, max_hp) } else { (0, 0) };
                self.by_entity.insert(
                    entity_id,
                    PlayerCandidate {
                        entity_id,
                        name: name.to_string(),
                        normalized: normalize(name),
                        current_hp,
                        max_hp,
                        last_seen: at,
                    },
                );
            }
        }
    }

    /// Drop players not seen since `cutoff`.
    pub fn expire_before(&mut self, cutoff: NaiveDateTime) {
        self.by_entity.retain(|_, c| c.last_seen >= cutoff);
    }

    /// Candidates in deterministic order, newest first.
    pub fn candidates(&self) -> Vec<PlayerCandidate> {
        let mut out: Vec<PlayerCandidate> = self.by_entity.values().cloned().collect();
        out.sort_by(|a, b| {
            b.last_seen
                .cmp(&a.last_seen)
                .then_with(|| a.entity_id.cmp(&b.entity_id))
        });
        out
    }

    pub fn len(&self) -> usize {
        self.by_entity.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_entity.is_empty()
    }

    pub fn clear(&mut self) {
        self.by_entity.clear();
    }
}
