//! Raid Debuffs tab: enemies (rows) × the standard raid debuffs (columns),
//! uptime % per cell. Answers "which debuffs did the group keep on the boss,
//! who supplied them, and which did nobody bring".

use dioxus::prelude::*;

use baras_types::{RaidDebuff, formatting};

use crate::api::{self, RaidDebuffCell, TimeRange};
use crate::components::ability_icon::AbilityIcon;
use crate::components::effect_uptime::{UptimeLane, UptimeTimeline};

/// Session-wide, like the other analysis filters
static BOSSES_ONLY: GlobalSignal<bool> = Signal::global(|| true);

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
pub fn RaidDebuffsView(encounter_idx: Option<u32>, time_range: TimeRange, duration_secs: f32, european: bool) -> Element {
    // Timeline selection: a row (enemy), a column (debuff), or a cell (both)
    let mut picked_target = use_signal(|| None::<String>);
    let mut picked_debuff = use_signal(|| None::<RaidDebuff>);
    let targets = use_resource(use_reactive!(|(encounter_idx, time_range)| async move {
        let tr = (time_range.end > 0.0).then_some(time_range);
        api::query_raid_debuffs(encounter_idx, tr.as_ref()).await
    }));
    let loaded = targets.read().clone().flatten();
    let all = loaded.clone().unwrap_or_default();
    let has_boss = all.iter().any(|t| t.is_boss);
    let bosses_only = has_boss && BOSSES_ONLY();
    let rows: Vec<_> = all.iter().filter(|t| !bosses_only || t.is_boss).collect();
    // Applied to at least one enemy in the fight, shown or not
    let brought = |d: RaidDebuff| all.iter().any(|t| t.cells.iter().any(|c| c.debuff == d));
    let groups = ["Damage Taken", "Healing", "Offense"]
        .map(|g| (g, RaidDebuff::ALL.iter().filter(|d| d.group() == g).count()));
    let (picked, debuff) = (picked_target(), picked_debuff());
    // Lanes are enemies for a picked debuff, debuffs for a picked enemy
    let lanes: Vec<UptimeLane> = rows
        .iter()
        .filter(|t| picked.as_ref().is_none_or(|p| *p == t.name))
        .flat_map(|t| t.cells.iter().map(move |c| (*t, c)))
        .filter(|(_, c)| debuff.is_none_or(|d| d == c.debuff) && (picked.is_some() || debuff.is_some()))
        .map(|(t, c)| UptimeLane {
            label: if picked.is_none() { t.name.clone() } else { c.debuff.name().to_string() },
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
    let (range_start, range_end) = if time_range.end > 0.0 {
        (time_range.start, time_range.end)
    } else {
        (0.0, duration_secs)
    };

    rsx! {
        div { class: "uptime-view raid-debuffs-view",
            div { class: "uptime-toolbar",
                span { class: "uptime-title", "Raid debuffs on enemies" }
                if has_boss {
                    div { class: "filter-pills",
                        button {
                            class: if bosses_only { "seg active" } else { "seg" },
                            title: "Hide adds",
                            onclick: move |_| *BOSSES_ONLY.write() = !BOSSES_ONLY(),
                            "Bosses only"
                        }
                    }
                }
            }
            if rows.is_empty() {
                div { class: "uptime-empty",
                    if loaded.is_none() { span { class: "loading-late", "Loading…" } } else { "No enemies attacked in this range" }
                }
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
                                            (true, _) => "uptime-target picked",
                                            (false, true) => "uptime-target",
                                            _ => "uptime-target absent",
                                        },
                                        onclick: move |_| {
                                            let same = picked_target.peek().is_none() && *picked_debuff.peek() == Some(d);
                                            picked_target.set(None);
                                            picked_debuff.set((!same).then_some(d));
                                        },
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
                                            move |_| {
                                                let same = picked_target.peek().as_deref() == Some(name.as_str())
                                                    && picked_debuff.peek().is_none();
                                                picked_target.set((!same).then(|| name.clone()));
                                                picked_debuff.set(None);
                                            }
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
                                                    move |_| {
                                                        picked_target.set(Some(name.clone()));
                                                        picked_debuff.set(Some(d));
                                                    }
                                                },
                                                style: "--pct: {c.uptime_pct}",
                                                title: "{d.name()} on {t.name}: {formatting::format_pct(c.uptime_pct as f64, european)}\n{sources_text(c)}",
                                                "{c.uptime_pct.round()}%"
                                            }
                                        } else {
                                            td {
                                                class: "num uptime-cell none",
                                                title: "{d.name()} was never on {t.name}",
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
            }
        }
    }
}
