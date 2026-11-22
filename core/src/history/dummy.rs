//! Training dummy rows for the index: kills only, the logging character's
//! own numbers.

use baras_types::history::DummyRow;

use super::store::LogHistory;
use crate::encounter::PhaseType;
use crate::game_data::{game_version_at, server_name};

/// Files below this store version marked every unabandoned parse a kill and
/// recorded neither crit numbers nor the dummy's HP; they wait for a re-scan.
const MIN_VERSION: u32 = 8;

pub(super) fn dummy_rows(log: &LogHistory) -> Vec<DummyRow> {
    if log.version < MIN_VERSION {
        return Vec::new();
    }
    let character = log.character_name.clone().unwrap_or_else(|| "Unknown".into());
    log.pulls
        .iter()
        .filter(|p| p.kind == PhaseType::DummyParse && p.success && !p.partial)
        .filter_map(|p| {
            let me = p.roster.iter().find(|m| m.name == character)?;
            Some(DummyRow {
                filename: log.filename.clone(),
                path: None,
                start_ms: p.start_ms,
                encounter_id: p.encounter_id,
                character: character.clone(),
                timestamp: p.start_time.clone(),
                server: server_name(&p.server).to_string(),
                game_version: game_version_at(p.start_time.get(..10).unwrap_or("")).to_string(),
                dummy_hp: p.dummy_hp.unwrap_or(0),
                duration_seconds: p.duration_secs,
                discipline: me.discipline_name.clone(),
                role: me.discipline.map(|d| format!("{:?}", d.role())),
                role_icon: me.discipline.map(|d| d.role().icon_name().to_string()),
                discipline_icon: me.discipline.map(|d| d.icon_name().to_string()),
                dps: me.dps,
                apm: me.apm,
                crit_pct: me.crit_pct,
                crit_damage_pct: me.crit_damage_pct,
                dps_pct: None,
            })
        })
        .collect()
}
