//! Raid Debuffs tab: enemies (rows) × the standard raid debuffs (columns),
//! uptime % per cell. Answers "which debuffs did the group keep on the boss,
//! who supplied them, and which did nobody bring".

use dioxus::prelude::*;

use baras_types::{RaidDebuff, RaidDebuffTarget, RaidOverviewRow, formatting};

use crate::api::{self, RaidDebuffCell, TimeRange};
use crate::components::ability_icon::AbilityIcon;
use crate::components::effect_uptime::{UptimeLane, UptimeTimeline};
use crate::components::enemy_views::{BossesOnlyToggle, LoadFailed, loading};
use crate::components::raid_debuff_coverage::{RaidDebuffCoverage, coverage_rows};
use crate::types::EnemyViewsState;

/// A cell's tooltip line on the damage of the kind `debuff` raises that
/// landed on `target` without it; empty when there was none
fn missed_text(target: &RaidDebuffTarget, debuff: RaidDebuff, european: bool) -> String {
    let compact = |n: f64| formatting::format_compact_f64(n, european);
    let (Some(w), Some(kind)) = (target.worth.iter().find(|w| w.debuff == debuff), debuff.boosts()) else {
        return String::new();
    };
    if w.unbuffed_dmg <= 0.0 {
        return String::new();
    }
    format!(
        "\n{} of {} {kind} damage landed without it",
        compact(w.unbuffed_dmg),
        compact(w.affected_dmg)
    )
}

/// "Name (Ability ×3)" per source, for a cell's tooltip
fn sources_text(cell: &RaidDebuffCell) -> String {
    let parts: Vec<String> = cell
        .sources
        .iter()
        .map(|s| format!("{} ({} ×{})", s.name, s.ability_name, s.count))
        .collect();
    parts.join("\n")
}

#[component]
pub fn RaidDebuffsView(
    encounter_idx: Option<u32>,
    time_range: TimeRange,
    duration_secs: f32,
    /// Player disciplines, to tell who could have applied a debuff
    overview: Signal<Vec<RaidOverviewRow>>,
    state: Signal<EnemyViewsState>,
    european: bool,
) -> Element {
    let mut targets = use_resource(use_reactive!(|(encounter_idx, time_range)| async move {
        let tr = (time_range.end > 0.0).then_some(time_range);
        api::query_raid_debuffs(encounter_idx, tr.as_ref()).await
    }));
    let loaded = targets.read();
    let report = match loaded.as_ref() {
        Some(Some(report)) => report,
        Some(None) => return rsx! { LoadFailed { on_retry: move |_| targets.restart() } },
        None => return loading(),
    };
    let all = &report.targets;
    let controls = state.read();
    let has_boss = all.iter().any(|t| t.is_boss);
    let bosses_only = has_boss && controls.bosses_only;
    let rows: Vec<_> = all.iter().filter(|t| !bosses_only || t.is_boss).collect();
    // Applied to at least one enemy in the fight, shown or not
    let brought = |d: RaidDebuff| all.iter().any(|t| t.cells.iter().any(|c| c.debuff == d));
    let groups =
        ["Damage Taken", "Healing", "Offense"].map(|g| (g, RaidDebuff::ALL.iter().filter(|d| d.group() == g).count()));
    // Timeline selection: a row (enemy), a column (debuff), or a cell (both).
    // An enemy not shown here (another encounter, or an add under "Bosses only") is no pick.
    let picked = controls
        .debuff_enemy
        .clone()
        .filter(|p| rows.iter().any(|t| t.name == *p));
    let debuff = controls.debuff;
    // Sets the selection, or clears it when it is already the one set
    let mut pick = move |enemy: Option<String>, d: Option<RaidDebuff>, toggle: bool| {
        state.with_mut(|s| {
            let same = toggle && s.debuff_enemy == enemy && s.debuff == d;
            (s.debuff_enemy, s.debuff) = if same { (None, None) } else { (enemy, d) };
        });
    };
    // Lanes are enemies for a picked debuff, debuffs for a picked enemy
    let lanes: Vec<UptimeLane> = rows
        .iter()
        .filter(|t| picked.as_ref().is_none_or(|p| *p == t.name))
        .flat_map(|t| t.cells.iter().map(move |c| (*t, c)))
        .filter(|(_, c)| debuff.is_none_or(|d| d == c.debuff) && (picked.is_some() || debuff.is_some()))
        .map(|(t, c)| UptimeLane {
            label: if picked.is_none() {
                t.name.clone()
            } else {
                c.debuff.name().to_string()
            },
            muted: picked.is_none() && !t.is_boss,
            icon: picked.is_some().then(|| (c.debuff.icon_effect_id(), None)),
            uptime_pct: c.uptime_pct,
            windows: c.windows.clone(),
        })
        .collect();
    let title = match (&picked, debuff) {
        (Some(t), Some(d)) => Some((Some(d), format!("{} on {t}", d.name()))),
        (Some(t), None) => Some((None, format!("Raid debuffs on {t}"))),
        (None, Some(d)) => Some((Some(d), format!("{} on every enemy", d.name()))),
        _ => None,
    };
    // A picked enemy (alone or with a debuff) narrows the coverage to it
    let covered: Vec<&RaidDebuffTarget> = rows
        .iter()
        .copied()
        .filter(|t| picked.as_ref().is_none_or(|p| *p == t.name))
        .collect();
    let coverage = coverage_rows(&covered, &report.able);
    let (range_start, range_end) = if time_range.end > 0.0 {
        (time_range.start, time_range.end)
    } else {
        (0.0, duration_secs)
    };

    rsx! {
        div { class: "uptime-view raid-debuffs-view fade-in",
            div { class: "uptime-toolbar",
                span { class: "uptime-title", "Raid debuffs on enemies" }
                BossesOnlyToggle { has_boss, state }
            }
            if rows.is_empty() {
                div { class: "uptime-empty", "No enemies attacked in this range" }
            } else {
                div { class: "overview-table-box uptime-table-box raid-debuffs-box",
                    table { class: "overview-table uptime-table raid-debuffs-table",
                        thead {
                            tr { class: "raid-debuff-groups",
                                th { class: "name-col" }
                                for (group, span) in groups {
                                    th { colspan: "{span}", "{group}" }
                                }
                            }
                            tr {
                                th { class: "name-col", "Enemy" }
                                for d in RaidDebuff::ALL {
                                    th {
                                        class: match (debuff == Some(d) && picked.is_none(), brought(d)) {
                                            (true, _) => "uptime-target with-icon picked",
                                            (false, true) => "uptime-target with-icon",
                                            _ => "uptime-target with-icon absent",
                                        },
                                        onclick: move |_| pick(None, Some(d), true),
                                        title: if brought(d) { "{d.name()}: {d.description()}" } else { "{d.name()}: {d.description()}\nNobody applied it in this fight" },
                                        AbilityIcon { ability_id: d.icon_effect_id(), size: 18 }
                                        span { "{d.name()}" }
                                    }
                                }
                            }
                        }
                        tbody {
                            for t in rows {
                                tr {
                                    key: "{t.name}",
                                    class: if picked.as_deref() == Some(t.name.as_str()) && debuff.is_none() { "uptime-row picked" } else { "uptime-row" },
                                    td {
                                        class: "name-col",
                                        title: "{t.name}\nAttacked for {formatting::format_duration(t.present_secs as i64)}",
                                        onclick: {
                                            let name = t.name.clone();
                                            move |_| pick(Some(name.clone()), None, true)
                                        },
                                        // The flex wrapper lets a long name truncate inside the fixed column
                                        span { class: "name-with-icon",
                                            span { class: if t.is_boss { "name-text" } else { "name-text npc" }, "{t.name}" }
                                        }
                                    }
                                    for d in RaidDebuff::ALL {
                                        if let Some(c) = t.cells.iter().find(|c| c.debuff == d) {
                                            td {
                                                class: if picked.as_deref() == Some(t.name.as_str()) && debuff == Some(d) { "num uptime-cell picked" } else { "num uptime-cell" },
                                                onclick: {
                                                    let name = t.name.clone();
                                                    move |_| pick(Some(name.clone()), Some(d), false)
                                                },
                                                style: "--pct: {c.uptime_pct}",
                                                title: "{d.name()} on {t.name}: {formatting::format_pct(c.uptime_pct as f64, european)}\n{sources_text(c)}{missed_text(t, d, european)}",
                                                "{c.uptime_pct.round()}%"
                                            }
                                        } else {
                                            td {
                                                class: "num uptime-cell none",
                                                title: "{d.name()} was never on {t.name}{missed_text(t, d, european)}",
                                                "–"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some((icon, title)) = title {
                    UptimeTimeline {
                        title: rsx! {
                            span { class: "uptime-who",
                                if let Some(d) = icon {
                                    AbilityIcon { ability_id: d.icon_effect_id(), size: 16 }
                                }
                                strong { "{title}" }
                            }
                        },
                        lanes,
                        range_start,
                        range_end,
                    }
                } else {
                    div { class: "usage-timeline-container",
                        div { class: "usage-timeline-hint",
                            i { class: "fa-solid fa-bars-progress" }
                            " Click an enemy, a debuff or a cell above to see when debuffs were up"
                        }
                    }
                }
                RaidDebuffCoverage { rows: coverage, on: picked.clone(), overview, european }
            }
        }
    }
}
