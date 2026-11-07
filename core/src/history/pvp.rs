//! PvP rows for the index: warzone matches as recorded, arena rounds grouped
//! into their best-of-3 matches.

use baras_types::PvpFaction;
use baras_types::history::{PvpMatchRow, PvpPlayer, PvpRound};

use super::record::PullRecord;
use super::store::LogHistory;
use crate::encounter::{PhaseType, PvpOutcome};
use crate::game_data::{PvpAreaKind, game_version_at, pvp_area_kind, server_name};

/// Rounds of one match are never this far apart (queue pop to next round is
/// well under two minutes); a longer gap is a new match on the same map.
const ROUND_GAP_MS: i64 = 10 * 60 * 1000;

pub(super) fn pvp_rows(log: &LogHistory) -> Vec<PvpMatchRow> {
    let mut rounds: Vec<&PullRecord> = log.pulls.iter().filter(|p| p.kind == PhaseType::PvP).collect();
    rounds.sort_by_key(|p| p.start_ms);

    let mut out = Vec::new();
    let mut group: Vec<&PullRecord> = Vec::new();
    for p in rounds {
        let arena = pvp_area_kind(p.area_id) == Some(PvpAreaKind::Arena);
        let continues = arena
            && group.last().is_some_and(|prev| {
                prev.area_id == p.area_id
                    && p.start_ms - (prev.start_ms + prev.duration_secs * 1000) < ROUND_GAP_MS
                    && !prev.pvp_match_end
                    && group.len() < 3
                    && !decided(&group)
            });
        if !continues && !group.is_empty() {
            out.push(match_from(log, &group));
            group.clear();
        }
        group.push(p);
    }
    if !group.is_empty() {
        out.push(match_from(log, &group));
    }
    out
}

fn round_won(p: &PullRecord) -> Option<bool> {
    match p.pvp_outcome {
        Some(PvpOutcome::Win) => Some(true),
        Some(PvpOutcome::Loss) => Some(false),
        _ => None,
    }
}

/// Either side already has its two rounds
fn decided(group: &[&PullRecord]) -> bool {
    let wins = group.iter().filter(|p| round_won(p) == Some(true)).count();
    let losses = group.iter().filter(|p| round_won(p) == Some(false)).count();
    wins >= 2 || losses >= 2
}

fn match_from(log: &LogHistory, group: &[&PullRecord]) -> PvpMatchRow {
    let first = group[0];
    let character = log.character_name.clone().unwrap_or_else(|| "Unknown".into());
    // My discipline from the latest round that has me
    let me = group.iter().rev().find_map(|p| p.roster.iter().find(|m| m.name == character));
    let arena = pvp_area_kind(first.area_id) == Some(PvpAreaKind::Arena);

    // Everyone else, deduplicated by name; a later round's discipline wins
    let mut players: Vec<PvpPlayer> = Vec::new();
    for m in group.iter().flat_map(|p| p.roster.iter()).filter(|m| m.name != character) {
        let Some(faction) = m.pvp_faction else { continue };
        let friendly = faction == PvpFaction::Friendly;
        match players.iter_mut().find(|x| x.name == m.name) {
            Some(x) => {
                x.friendly = friendly;
                if m.discipline_name.is_some() {
                    x.discipline = m.discipline_name.clone();
                }
            }
            None => players.push(PvpPlayer { name: m.name.clone(), discipline: m.discipline_name.clone(), friendly }),
        }
    }
    players.sort_by(|a, b| b.friendly.cmp(&a.friendly).then_with(|| a.name.cmp(&b.name)));

    let rounds: Vec<PvpRound> = group
        .iter()
        .map(|p| PvpRound { encounter_id: p.encounter_id, start_ms: p.start_ms, won: round_won(p), duration_seconds: p.duration_secs })
        .collect();
    let wins = rounds.iter().filter(|r| r.won == Some(true)).count();
    let losses = rounds.iter().filter(|r| r.won == Some(false)).count();
    let won = if !arena {
        None
    } else if wins >= 2 {
        Some(true)
    } else if losses >= 2 {
        Some(false)
    } else {
        None
    };

    PvpMatchRow {
        filename: log.filename.clone(),
        path: None,
        start_ms: first.start_ms,
        encounter_id: first.encounter_id,
        character,
        timestamp: first.start_time.clone(),
        server: server_name(&first.server).to_string(),
        game_version: game_version_at(first.start_time.get(..10).unwrap_or("")).to_string(),
        area_id: first.area_id,
        map: first.area_name.clone(),
        arena,
        won,
        duration_seconds: rounds.iter().map(|r| r.duration_seconds).sum(),
        rounds,
        discipline: me.and_then(|m| m.discipline_name.clone()),
        role: me.and_then(|m| m.discipline.map(|d| format!("{:?}", d.role()))),
        role_icon: me.and_then(|m| m.discipline.map(|d| d.role().icon_name().to_string())),
        discipline_icon: me.and_then(|m| m.discipline.map(|d| d.icon_name().to_string())),
        players,
    }
}
