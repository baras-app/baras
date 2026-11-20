//! Recovery for logs that start mid-instance.
//!
//! After a crash or disconnect the game resumes logging into a new file
//! without the `AreaEntered` and `DisciplineChanged` lines it writes on
//! entry, because the player never re-entered the area. The previous log's
//! history file remembers the instanced visit it ended in and who was
//! playing; once the new log shows that player and an NPC carrying the
//! visit's id prefix, both lines can be replayed.

use std::path::Path;

use chrono::NaiveDateTime;

use baras_types::history::{InstanceVisit, NPC_COUNTER_SPAN};

use super::history_dir;
use super::store::{LogHistory, history_file_name};
use crate::combat_log::{CombatEvent, Effect, Entity, EntityType};
use crate::context::intern;
use crate::game_data::effect_type_id;
use crate::state::ipc::WorkerPlayerDiscipline;

/// Reconnecting takes minutes; past this the prefix may belong to another instance.
const MAX_RESUME_GAP_SECS: i64 = 2 * 3600;
const ISO: &str = "%Y-%m-%dT%H:%M:%S";

pub struct AreaResume {
    visit: InstanceVisit,
    player: WorkerPlayerDiscipline,
    /// Last activity recorded for the previous log
    last_seen: NaiveDateTime,
    /// The remembered player has appeared in the new log
    player_seen: bool,
}

impl AreaResume {
    /// The instanced visit and local player the log before `log_path` ended
    /// with. None unless its history recorded both: an area without the
    /// player is not worth resuming.
    pub fn for_log(log_path: &Path) -> Option<Self> {
        let current = history_file_name(log_path.file_name()?.to_str()?);
        let dir = history_dir().ok()?;
        let previous = std::fs::read_dir(&dir)
            .ok()?
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.starts_with("combat_") && n.ends_with(".json") && *n < current)
            .max()?;
        let history = LogHistory::load(&dir.join(previous)).ok()?.history;
        let visit = history.visits.last().filter(|v| v.prefix != 0)?.clone();
        let last_seen = history
            .pulls
            .iter()
            .filter_map(|p| p.end_time.as_deref())
            .chain(std::iter::once(visit.entered.as_str()))
            .filter_map(|t| NaiveDateTime::parse_from_str(t, ISO).ok())
            .max()?;
        Some(Self {
            visit,
            player: history.player?,
            last_seen,
            player_seen: false,
        })
    }

    /// The `AreaEntered` and `DisciplineChanged` the game left out, once the
    /// log has shown the remembered player and `event` involves an NPC of
    /// the remembered instance. `Err` when an NPC proves it is another place.
    pub fn entry_events(&mut self, event: &CombatEvent) -> Option<Result<[CombatEvent; 2], ()>> {
        let sides = [&event.source_entity, &event.target_entity];
        self.player_seen |= sides
            .iter()
            .any(|e| e.entity_type == EntityType::Player && e.log_id == self.player.entity_id);
        let npc = sides
            .into_iter()
            .find(|e| e.entity_type == EntityType::Npc && e.log_id != 0)?;
        let gap = (event.timestamp - self.last_seen).num_seconds();
        if npc.log_id / NPC_COUNTER_SPAN != self.visit.prefix || !(0..=MAX_RESUME_GAP_SECS).contains(&gap) {
            return Some(Err(()));
        }
        if !self.player_seen {
            return None;
        }
        let line = |effect| CombatEvent {
            line_number: event.line_number,
            timestamp: event.timestamp,
            source_entity: Entity {
                name: intern(&self.player.name),
                log_id: self.player.entity_id,
                entity_type: EntityType::Player,
                ..Default::default()
            },
            target_entity: Entity::default(),
            action: Default::default(),
            effect,
            details: Default::default(),
        };
        Some(Ok([
            line(Effect {
                type_id: effect_type_id::AREAENTERED,
                effect_name: intern(&self.visit.area_name),
                effect_id: self.visit.area_id,
                difficulty_name: intern(&self.visit.difficulty),
                difficulty_id: self.visit.difficulty_id,
                server: intern(&self.visit.server),
                ..Default::default()
            }),
            line(Effect {
                type_id: effect_type_id::DISCIPLINECHANGED,
                effect_name: intern(&self.player.class_name),
                effect_id: self.player.class_id,
                discipline_name: intern(&self.player.discipline_name),
                discipline_id: self.player.discipline_id,
                ..Default::default()
            }),
        ]))
    }
}
