//! Aggro tab: who each enemy was on (share table and timeline), and taunts
//! and threat drops with what they did.

mod events;
mod swaps;
mod timeline;

use dioxus::prelude::*;
use std::collections::HashMap;

use baras_types::{AggroRow, AggroShare, RaidOverviewRow, ThreatEvent, ThreatEventKind, formatting};

use crate::api::{self, TimeRange};
use crate::components::class_icons::{get_class_icon, get_role_icon};
use crate::components::enemy_views::{BossesOnlyToggle, LoadFailed, loading};
use crate::types::EnemyViewsState;
use events::ThreatEvents;
use swaps::TankSwaps;
use timeline::{AggroLane, AggroMark, AggroTimeline};

/// Chart series preferred per role, so an enemy on a tank reads cool and one
/// loose on a damage dealer reads warm
const TANK: [u8; 2] = [1, 9];
const HEALER: [u8; 2] = [3, 10];
const DAMAGE: [u8; 6] = [2, 8, 4, 5, 12, 11];
/// Chart series there are; no two names share one
const SERIES: u8 = 16;
/// Names past the last series: legend label and series 0 (grey)
const OTHER: (&str, u8) = ("Other", 0);

/// A combat second as m:ss, rounded like every time in the tab
pub(super) fn mmss(t: f32) -> String {
    formatting::format_duration(t.round() as i64)
}

/// Target names by total time on them, most first
fn by_time<'a>(shares: impl Iterator<Item = &'a AggroShare>) -> Vec<&'a str> {
    let mut totals: Vec<(&str, f32)> = Vec::new();
    for share in shares {
        match totals.iter_mut().find(|t| t.0 == share.name) {
            Some(total) => total.1 += share.secs,
            None => totals.push((&share.name, share.secs)),
        }
    }
    totals.sort_by(|a, b| b.1.total_cmp(&a.1));
    totals.into_iter().map(|t| t.0).collect()
}

/// Colour of each name a lane can be on, and the legend entry it falls under
#[derive(Clone, PartialEq, Default)]
pub struct Palette {
    /// (legend label, chart series)
    keys: Vec<(String, u8)>,
    by_name: HashMap<String, usize>,
}

impl Palette {
    /// Gives `name` the first series of `preferred` not taken yet, else any
    /// free one; once all are taken it falls under "Other"
    fn add(&mut self, name: &str, preferred: &[u8]) {
        let taken = |n: &u8| self.keys.iter().any(|k| k.1 == *n);
        let free = preferred.iter().copied().chain(1..=SERIES).find(|n| !taken(n));
        if let Some(n) = free {
            self.by_name.insert(name.to_string(), self.keys.len());
            self.keys.push((name.to_string(), n));
        }
    }

    /// Players an enemy can be on, coloured by role
    fn by_role(holders: &[&str], roster: &Roster) -> Self {
        let mut palette = Self::default();
        for name in holders.iter().copied() {
            palette.add(
                name,
                match roster.role_icon(name) {
                    Some("icon_tank.png") => &TANK,
                    Some("icon_heal.png") => &HEALER,
                    _ => &DAMAGE,
                },
            );
        }
        palette
    }

    /// What players targeted, most-targeted first, so the colours go to the
    /// names that fill the lanes
    fn by_target(players: &[AggroRow]) -> Self {
        let mut palette = Self::default();
        for name in by_time(players.iter().flat_map(|p| &p.shares)) {
            palette.add(name, &[]);
        }
        palette
    }

    /// Legend label and chart series (0 for none) of a name
    pub fn key(&self, name: &str) -> (&str, u8) {
        self.by_name
            .get(name)
            .map_or(OTHER, |i| (self.keys[*i].0.as_str(), self.keys[*i].1))
    }

    /// Legend entries for the names given, each once
    pub fn legend<'a>(&'a self, names: impl Iterator<Item = &'a str>) -> Vec<(&'a str, u8)> {
        let mut out: Vec<(&str, u8)> = Vec::new();
        for key in names.map(|n| self.key(n)) {
            if !out.contains(&key) {
                out.push(key);
            }
        }
        out
    }
}

/// Player name → (role icon, discipline icon), from the overview
#[derive(Clone, PartialEq, Default)]
pub struct Roster(HashMap<String, (Option<String>, Option<String>)>);

impl Roster {
    fn role_icon(&self, name: &str) -> Option<&str> {
        self.0.get(name).and_then(|i| i.0.as_deref())
    }

    pub fn role(&self, name: &str) -> Element {
        let role = self.role_icon(name).and_then(get_role_icon).copied();
        rsx! {
            if let Some(src) = role {
                img { class: "role-icon", src, alt: "" }
            }
        }
    }

    pub fn discipline(&self, name: &str) -> Element {
        let class = self
            .0
            .get(name)
            .and_then(|i| i.1.as_deref())
            .and_then(get_class_icon)
            .copied();
        rsx! {
            if let Some(src) = class {
                img { class: "class-icon", src, alt: "" }
            }
        }
    }
}

/// Taunts only: threat drops are too many to pin on a lane
fn is_taunt(e: &&ThreatEvent) -> bool {
    e.kind != ThreatEventKind::Drop
}

/// A taunt's mark, in the chart series of the player who pressed it
fn mark(e: &ThreatEvent, series: u8) -> AggroMark {
    let targets: Vec<&str> = e.targets.iter().map(|t| t.name.as_str()).collect();
    AggroMark {
        time: e.time_secs,
        series,
        text: format!(
            "{} {}: {} → {}",
            mmss(e.time_secs),
            e.source,
            e.ability_name,
            targets.join(", ")
        ),
    }
}

#[component]
pub fn ThreatView(
    encounter_idx: Option<u32>,
    time_range: TimeRange,
    duration_secs: f32,
    overview: Signal<Vec<RaidOverviewRow>>,
    state: Signal<EnemyViewsState>,
    european: bool,
) -> Element {
    // The whole fight, once per encounter (and cached); ranges are cut from it
    let mut fight = use_resource(use_reactive!(|encounter_idx| async move {
        api::query_threat_report(encounter_idx).await
    }));
    let report = use_memo(use_reactive!(|time_range| {
        let loaded = fight.read();
        let report = loaded.as_ref()?.as_ref()?;
        Some(if time_range.end > 0.0 {
            report.clipped(time_range.start, time_range.end)
        } else {
            report.clone()
        })
    }));
    let roster = use_memo(move || {
        Roster(
            overview
                .read()
                .iter()
                .map(|r| (r.name.clone(), (r.role_icon.clone(), r.class_icon.clone())))
                .collect(),
        )
    });
    // Stint hovered in the swap table: (enemy, start, end), outlined on the
    // timeline. Read only there, so hovering never re-renders this view.
    let mut hovered = use_signal(|| None::<(String, f32, f32)>);

    let report = report.read();
    let Some(report) = report.as_ref() else {
        return match fight.read().as_ref() {
            Some(None) => rsx! { LoadFailed { on_retry: move |_| fight.restart() } },
            _ => loading(),
        };
    };
    let roster = roster();
    let controls = state.read();
    let has_boss = report.enemies.iter().any(|r| r.is_boss);
    let bosses_only = has_boss && controls.bosses_only;
    let enemies: Vec<&AggroRow> = report.enemies.iter().filter(|r| !bosses_only || r.is_boss).collect();
    // Players any shown enemy was on are the table's columns
    let holders = by_time(enemies.iter().flat_map(|r| &r.shares).filter(|s| s.is_player));
    // A pick not in this encounter or range shows every enemy
    let enemy = controls
        .aggro_enemy
        .as_deref()
        .filter(|p| enemies.iter().any(|r| r.name == *p));
    let picked = |r: &&&AggroRow| enemy.is_none_or(|e| e == r.name);
    let by_player = controls.aggro_by_player;
    let (range_start, range_end) = if time_range.end > 0.0 {
        (time_range.start, time_range.end)
    } else {
        (0.0, duration_secs)
    };

    let (palette, lanes, title) = if by_player {
        // Each lane is the taunter's own and the colours belong to targets,
        // so the marks keep the plain colour
        let lanes = report
            .players
            .iter()
            .map(|p| AggroLane {
                label: p.name.clone(),
                segments: p.segments.clone(),
                marks: report
                    .events
                    .iter()
                    .filter(is_taunt)
                    .filter(|e| e.source == p.name)
                    .map(|e| mark(e, 0))
                    .collect(),
            })
            .collect();
        (
            Palette::by_target(&report.players),
            lanes,
            "Who each player was targeting".to_string(),
        )
    } else {
        let palette = Palette::by_role(&holders, &roster);
        let lanes: Vec<AggroLane> = enemies
            .iter()
            .filter(picked)
            .map(|r| AggroLane {
                label: r.name.clone(),
                segments: r.segments.clone(),
                marks: report
                    .events
                    .iter()
                    .filter(is_taunt)
                    .filter(|e| e.targets.iter().any(|t| t.name == r.name))
                    .map(|e| mark(e, palette.key(&e.source).1))
                    .collect(),
            })
            .collect();
        let title = enemy.map_or("Who each enemy was on".to_string(), |e| format!("Who {e} was on"));
        (palette, lanes, title)
    };
    let mut set_by_player = move |on: bool| {
        state.with_mut(|s| s.aggro_by_player = on);
    };

    rsx! {
        div { class: "uptime-view threat-view fade-in",
            div { class: "uptime-toolbar",
                span { class: "uptime-title", "Enemy aggro" }
                BossesOnlyToggle { has_boss, state }
            }
            if enemies.is_empty() {
                div { class: "uptime-empty", "No enemies attacked in this range" }
            } else {
                div { class: "overview-table-box uptime-table-box",
                    table { class: "overview-table uptime-table aggro-table",
                        thead {
                            tr {
                                th { class: "name-col", "Enemy" }
                                for name in holders.iter().copied() {
                                    th { class: "uptime-target with-icon", title: "{name}",
                                        {roster.role(name)}
                                        span { "{name}" }
                                    }
                                }
                                th { class: "uptime-target with-icon", title: "Time on another enemy or a companion",
                                    span { "Other" }
                                }
                            }
                        }
                        tbody {
                            for r in enemies.iter() {
                                tr {
                                    key: "{r.name}",
                                    class: if enemy == Some(r.name.as_str()) && !by_player { "uptime-row picked" } else { "uptime-row" },
                                    title: "Click to show only this enemy below; click again for every enemy",
                                    onclick: {
                                        let name = r.name.clone();
                                        move |_| {
                                            state.with_mut(|s| {
                                                let same = s.aggro_enemy.as_deref() == Some(name.as_str());
                                                s.aggro_enemy = (!same).then(|| name.clone());
                                                s.aggro_by_player = false;
                                            });
                                            // Its swap rows are about to go: no mouseleave will clear it
                                            hovered.set(None);
                                        }
                                    },
                                    td {
                                        class: "name-col",
                                        title: "{r.name}\nHad a target for {mmss(r.secs)}",
                                        span { class: "name-with-icon",
                                            span { class: if r.is_boss { "name-text" } else { "name-text npc" }, "{r.name}" }
                                        }
                                    }
                                    for name in holders.iter().copied() {
                                        {share_cell(r, r.shares.iter().find(|s| s.name == name).map_or(0.0, |s| s.secs), name, european)}
                                    }
                                    {share_cell(r, r.shares.iter().filter(|s| !s.is_player).map(|s| s.secs).sum(), "something else", european)}
                                }
                            }
                        }
                    }
                }
            }
            div { class: "uptime-toolbar",
                span { class: "uptime-title", "{title}" }
                div { class: "filter-pills", title: "Timeline lanes",
                    button { class: if by_player { "seg" } else { "seg active" }, onclick: move |_| set_by_player(false), "Enemies" }
                    button { class: if by_player { "seg active" } else { "seg" }, onclick: move |_| set_by_player(true), "Players" }
                }
            }
            AggroTimeline { lanes, palette, roster: roster.clone(), highlight: hovered, range_start, range_end }
            div { class: "threat-lower",
                TankSwaps {
                    // The picked enemy, or every one in the table
                    enemies: enemies.iter().filter(picked).map(|r| (*r).clone()).collect::<Vec<_>>(),
                    events: report.events.clone(),
                    deaths: report.deaths.clone(),
                    hovered,
                    roster: roster.clone(),
                }
                ThreatEvents {
                    // Only what reached the picked enemy, and only its line of an AoE press
                    events: report
                        .events
                        .iter()
                        .filter_map(|e| {
                            let Some(enemy) = enemy else { return Some(e.clone()) };
                            let targets: Vec<_> = e.targets.iter().filter(|t| t.name == enemy).cloned().collect();
                            (!targets.is_empty()).then(|| ThreatEvent { targets, ..e.clone() })
                        })
                        .collect::<Vec<_>>(),
                    on: enemy.map(str::to_string),
                    state,
                    roster: roster.clone(),
                    european,
                }
            }
        }
    }
}

/// Share of an enemy's targeted time spent on `on`
fn share_cell(row: &AggroRow, secs: f32, on: &str, european: bool) -> Element {
    if secs <= 0.0 {
        return rsx! { td { class: "num uptime-cell none" } };
    }
    let pct = secs * 100.0 / row.secs.max(0.001);
    rsx! {
        td {
            class: "num uptime-cell",
            style: "--pct: {pct}",
            title: "{row.name} was on {on} for {mmss(secs)} ({formatting::format_pct(pct as f64, european)})",
            "{pct.round()}%"
        }
    }
}
