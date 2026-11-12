//! Builds the roster: every character the logs name, merged with the store.

use hashbrown::HashMap;

use baras_types::characters::{CharacterRecord, CharacterRoster, CharacterRow, IconLabel, OperationRef, TierBosses, character_key, operation_key};
use baras_types::history::PullRow;

use super::lockouts::{lockouts, next_reset, now_utc_ms};
use super::store::CharacterStore;
use crate::game_data::{Class, Discipline, Role};
use crate::history::{AreaCatalog, PullIndex};

#[derive(Default)]
struct Seen<'a> {
    name: String,
    server: String,
    /// Local-time millis of the newest thing known about the character
    last_ms: i64,
    rows: Vec<&'a PullRow>,
}

impl Seen<'_> {
    fn note(&mut self, started_ms: i64, last_event_ms: Option<i64>, rows: &[PullRow]) {
        let newest = rows.iter().map(|r| r.start_ms + r.duration_seconds * 1000).max();
        self.last_ms = [Some(started_ms), last_event_ms, newest].into_iter().flatten().fold(self.last_ms, i64::max);
    }
}

/// Classes the character logged, newest first, one entry per class
fn logged_classes(rows: &[&PullRow]) -> Vec<Class> {
    let mut sorted: Vec<&&PullRow> = rows.iter().filter(|r| r.discipline.is_some()).collect();
    sorted.sort_by_key(|r| std::cmp::Reverse(r.start_ms));
    let mut classes: Vec<Class> = Vec::new();
    for r in sorted {
        if let Some(c) = r.discipline.as_deref().and_then(Discipline::from_name).map(|d| d.class())
            && !classes.contains(&c)
        {
            classes.push(c);
        }
    }
    classes
}

fn iso_local(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .filter(|_| ms > 0)
        .map(|t| t.naive_utc().format("%Y-%m-%dT%H:%M:%S").to_string())
        .unwrap_or_default()
}

/// `last_events` maps log filename → last event time (local millis) from the
/// directory index, for logs the history has no pulls for.
pub fn roster(
    index: &PullIndex,
    store: &CharacterStore,
    catalog: &AreaCatalog,
    last_events: &std::collections::HashMap<String, i64>,
) -> CharacterRoster {
    let now_ms = now_utc_ms();
    let mut seen: HashMap<String, Seen> = HashMap::new();
    let mut unplaced = Vec::new();
    for log in index.logs().filter(|l| !l.character.is_empty() && l.character != "Unknown") {
        if log.server.is_empty() {
            unplaced.push(log);
            continue;
        }
        let s = seen.entry(character_key(log.character, log.server)).or_insert_with(|| Seen {
            name: log.character.to_string(),
            server: log.server.to_string(),
            ..Default::default()
        });
        s.note(log.started_ms, last_events.get(log.filename).copied(), log.rows);
        s.rows.extend(log.rows);
    }
    // A log that never named its server joins the one character of that
    // name we know about; with several (or none) it stands alone
    for log in unplaced {
        let candidates: Vec<String> = seen
            .values()
            .map(|s| (&s.name, &s.server))
            .chain(store.characters.values().map(|r| (&r.name, &r.server)))
            .filter(|(n, _)| n.as_str() == log.character)
            .map(|(n, s)| character_key(n, s))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let key = match candidates.as_slice() {
            [one] => one.clone(),
            _ => character_key(log.character, ""),
        };
        let (name, server) = key.rsplit_once('@').unwrap_or((&key, ""));
        let (name, server) = (name.to_string(), server.to_string());
        let s = seen.entry(key).or_insert_with(|| Seen { name, server, ..Default::default() });
        s.note(log.started_ms, last_events.get(log.filename).copied(), log.rows);
        s.rows.extend(log.rows);
    }

    let empty = CharacterRecord::default();
    let mut keys: Vec<String> = seen.keys().chain(store.characters.keys()).cloned().collect();
    keys.sort();
    keys.dedup();
    let rows: Vec<CharacterRow> = keys
        .into_iter()
        .map(|key| {
            let record = store.characters.get(&key);
            let s = seen.get(&key);
            let r = record.unwrap_or(&empty);
            let (name, server) = match (record, s) {
                (Some(r), _) => (r.name.clone(), r.server.clone()),
                (None, Some(s)) => (s.name.clone(), s.server.clone()),
                (None, None) => unreachable!("key came from one of the two maps"),
            };
            let pulls = s.map(|s| s.rows.as_slice()).unwrap_or(&[]);
            let classes_auto = r.classes.is_empty();
            let classes: Vec<Class> = if classes_auto {
                logged_classes(pulls)
            } else {
                r.classes.iter().filter_map(|c| Class::from_name(c)).collect()
            };
            let mut roles: Vec<Role> = classes.iter().flat_map(Class::roles).collect();
            roles.sort_by_key(|r| r.to_u8());
            roles.dedup();
            CharacterRow {
                // Combat styles cross factions, so only the user can say
                faction: r.faction.unwrap_or_default(),
                classes: classes
                    .iter()
                    .map(|c| IconLabel { name: c.name().to_string(), icon: c.icon_name().to_string() })
                    .collect(),
                classes_auto,
                roles: roles
                    .iter()
                    .map(|r| IconLabel { name: format!("{r:?}"), icon: r.icon_name().to_string() })
                    .collect(),
                crew_skills: r.crew_skills.clone(),
                guild: r.guild.clone(),
                note: r.note.clone(),
                last_seen: iso_local(s.map_or(0, |s| s.last_ms)),
                lockouts: lockouts(pulls, &r.manual_locks, catalog, store.reset_hour_utc, now_ms),
                saved: record.is_some(),
                hidden: store.hidden.contains(&key),
                key,
                name,
                server,
            }
        })
        .collect();

    let mut servers: Vec<String> = rows.iter().map(|r| r.server.clone()).filter(|s| !s.is_empty()).collect();
    servers.sort();
    servers.dedup();
    // Pull counts per (area, name); each area keeps its most-used name
    let mut names: HashMap<(i64, &str), usize> = HashMap::new();
    for r in index.logs().flat_map(|l| l.rows.iter()).filter(|r| r.is_operation) {
        *names.entry((r.area_id, r.operation.as_str())).or_default() += 1;
    }
    let mut best: HashMap<String, (&str, i64, usize)> = HashMap::new();
    for ((area_id, name), n) in names {
        let e = best.entry(operation_key(area_id, name)).or_insert((name, area_id, 0));
        if n > e.2 {
            *e = (name, area_id, n);
        }
    }
    let mut operations: Vec<OperationRef> = best
        .into_values()
        .map(|(name, area_id, _)| OperationRef {
            name: name.to_string(),
            area_id,
            tiers: catalog
                .area(area_id)
                .map(|a| {
                    a.tiers()
                        .into_iter()
                        .map(|t| TierBosses {
                            // Config keys are lowercase; rows and lockouts use the capitalized word
                            tier: capitalize(t),
                            bosses: a.bosses_at(t, 8).chain(a.bosses_at(t, 16)).map(|b| b.name.clone()).fold(Vec::new(), |mut v, n| {
                                if !v.contains(&n) {
                                    v.push(n);
                                }
                                v
                            }),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .collect();
    operations.sort_by(|a, b| a.name.cmp(&b.name));
    CharacterRoster {
        rows,
        servers,
        operations,
        reset_hour_utc: store.reset_hour_utc,
        next_weekly_reset_ms: next_reset(now_ms, true, store.reset_hour_utc),
        next_daily_reset_ms: next_reset(now_ms, false, store.reset_hour_utc),
    }
}

/// First letter uppercased; char-safe for empty or non-ASCII config keys
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map_or_else(String::new, |c| c.to_uppercase().chain(chars).collect())
}
