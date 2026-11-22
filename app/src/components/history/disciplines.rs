//! Per-discipline breakdown of one boss's pulls: how often each discipline
//! was played and its best numbers. Folded from the rows the Overview tab
//! already holds.

use dioxus::prelude::*;

use baras_types::formatting;
use baras_types::history::{PullFilter, PullRow};

use crate::components::class_icons::{get_class_icon, get_role_icon};
use crate::components::multi_select::toggle_value;

#[derive(Clone, PartialEq)]
pub(super) struct DisciplineStats {
    discipline: String,
    discipline_icon: Option<String>,
    role_icon: Option<String>,
    pulls: u32,
    kills: u32,
    top_dps: Option<i64>,
    top_hps: Option<i64>,
    top_ehps: Option<i64>,
    fastest_kill: Option<i64>,
}

/// Most played first; pulls without a detected discipline are left out
pub(super) fn summarize(rows: &[PullRow]) -> Vec<DisciplineStats> {
    let mut out: Vec<DisciplineStats> = Vec::new();
    for r in rows {
        let Some(discipline) = &r.discipline else { continue };
        let idx = out.iter().position(|d| d.discipline == *discipline).unwrap_or_else(|| {
            out.push(DisciplineStats {
                discipline: discipline.clone(),
                discipline_icon: r.discipline_icon.clone(),
                role_icon: r.role_icon.clone(),
                pulls: 0,
                kills: 0,
                top_dps: None,
                top_hps: None,
                top_ehps: None,
                fastest_kill: None,
            });
            out.len() - 1
        });
        let d = &mut out[idx];
        d.pulls += 1;
        d.top_dps = d.top_dps.max(r.dps);
        d.top_hps = d.top_hps.max(r.hps);
        d.top_ehps = d.top_ehps.max(r.ehps);
        if r.success {
            d.kills += 1;
            d.fastest_kill = Some(d.fastest_kill.map_or(r.duration_seconds, |f| f.min(r.duration_seconds)));
        }
    }
    out.sort_by_key(|d| std::cmp::Reverse(d.pulls));
    out
}

#[component]
pub(super) fn DisciplineTable(stats: Vec<DisciplineStats>, mut filter: Signal<PullFilter>, european: bool) -> Element {
    let total: u32 = stats.iter().map(|d| d.pulls).sum();
    let fmt = move |n: Option<i64>| {
        n.map(|v| formatting::format_compact(v, european))
            .unwrap_or_else(|| "-".into())
    };
    rsx! {
        // Folded by default: the pull list below is what the page is for
        details { class: "collapsible discipline-breakdown discipline-card",
            summary { class: "collapsible-summary",
                "Disciplines "
                span { class: "section-count", "{stats.len()}" }
            }
            div { class: "discipline-breakdown-table",
            table { class: "pull-history-table",
                thead {
                    tr {
                        th { "Discipline" }
                        th { title: "Pulls on this discipline and their share of all pulls", "Pulls" }
                        th { "Kills" }
                        th { title: "Highest DPS on a pull", "Top DPS" }
                        th { title: "Highest HPS on a pull", "Top HPS" }
                        th { title: "Highest effective HPS on a pull", "Top EHPS" }
                        th { "Fastest Kill" }
                    }
                }
                tbody {
                    for d in stats.iter() {
                        tr { key: "{d.discipline}", class: "run-row",
                            title: "Show only this discipline; click again to clear",
                            onclick: {
                                let name = d.discipline.clone();
                                move |_| {
                                    // Own statement: the read must end before the write
                                    let picked = toggle_value(&filter.read().disciplines, &name, true);
                                    filter.write().disciplines = picked;
                                }
                            },
                            td {
                                span { class: "pull-discipline",
                                    if let Some(icon) = d.role_icon.as_deref().and_then(get_role_icon) {
                                        img { class: "role-icon", src: *icon, alt: "" }
                                    }
                                    if let Some(icon) = d.discipline_icon.as_deref().and_then(get_class_icon) {
                                        img { class: "discipline-icon", src: *icon, alt: "" }
                                    }
                                    "{d.discipline}"
                                }
                            }
                            td { "{d.pulls} " span { class: "sub", "{d.pulls * 100 / total.max(1)} %" } }
                            td { "{d.kills}" }
                            td { {fmt(d.top_dps)} }
                            td { {fmt(d.top_hps)} }
                            td { {fmt(d.top_ehps)} }
                            td { {d.fastest_kill.map(formatting::format_duration).unwrap_or_else(|| "-".into())} }
                        }
                    }
                }
            }
            }
        }
    }
}
