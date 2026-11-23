//! Raid Debuffs tab, per debuff: who supplied it, who else in the group
//! could have, and how much damage landed without it.

use dioxus::prelude::*;

use baras_types::{RaidDebuff, RaidDebuffTarget, RaidOverviewRow, formatting};

use crate::components::ability_icon::AbilityIcon;
use crate::components::class_icons::get_class_icon;

/// One debuff across the enemies shown
#[derive(Clone, PartialEq)]
pub struct CoverageRow {
    debuff: RaidDebuff,
    /// Players who applied it, most applications first
    suppliers: Vec<(String, u32)>,
    /// Discipline GUIDs that apply it
    able: Vec<i64>,
    /// Damage of the kind it raises, and the part that landed without it
    affected_dmg: f64,
    unbuffed_dmg: f64,
}

/// One row per debuff over `targets`, the enemies shown in the matrix
pub fn coverage_rows(targets: &[&RaidDebuffTarget], able: &[(RaidDebuff, Vec<i64>)]) -> Vec<CoverageRow> {
    RaidDebuff::ALL
        .into_iter()
        .map(|debuff| {
            let mut suppliers: Vec<(String, u32)> = Vec::new();
            let cells = targets.iter().flat_map(|t| &t.cells).filter(|c| c.debuff == debuff);
            for source in cells.flat_map(|c| &c.sources) {
                match suppliers.iter_mut().find(|s| s.0 == source.name) {
                    Some(s) => s.1 += source.count,
                    None => suppliers.push((source.name.clone(), source.count)),
                }
            }
            suppliers.sort_by(|a, b| b.1.cmp(&a.1));
            let worth = targets.iter().flat_map(|t| &t.worth).filter(|w| w.debuff == debuff);
            let (affected_dmg, unbuffed_dmg) =
                worth.fold((0.0, 0.0), |sum, w| (sum.0 + w.affected_dmg, sum.1 + w.unbuffed_dmg));
            CoverageRow {
                debuff,
                suppliers,
                able: able
                    .iter()
                    .find(|a| a.0 == debuff)
                    .map(|a| a.1.clone())
                    .unwrap_or_default(),
                affected_dmg,
                unbuffed_dmg,
            }
        })
        .collect()
}

#[component]
pub fn RaidDebuffCoverage(
    rows: Vec<CoverageRow>,
    /// The enemy the rows are narrowed to; None for every enemy shown
    on: Option<String>,
    overview: Signal<Vec<RaidOverviewRow>>,
    european: bool,
) -> Element {
    let overview = overview.read();
    let compact = |n: f64| formatting::format_compact_f64(n, european);
    // A player's discipline icon, when the overview knows their discipline
    let icon = |name: &str| {
        let row = overview.iter().find(|p| p.name == name);
        let src = row
            .and_then(|p| p.class_icon.as_deref())
            .and_then(get_class_icon)
            .copied();
        rsx! {
            if let Some(src) = src {
                img { class: "class-icon", src, alt: "" }
            }
        }
    };

    rsx! {
        div { class: "analysis-panel",
            div { class: "uptime-toolbar",
                span { class: "uptime-title",
                    "Who brought each debuff"
                    if let Some(on) = on {
                        " on {on}"
                    }
                }
            }
            div { class: "analysis-table-box",
                table { class: "analysis-table debuff-coverage",
                    thead {
                        tr {
                            th { class: "debuff-col", "Debuff" }
                            th { "Supplied by" }
                            th { title: "Players whose discipline applies it in other logs, and who did not here",
                                "Could also apply"
                            }
                            th { class: "num debuff-col", title: "Damage of the kind the debuff raises that landed on the enemies covered while it was down",
                                "Damage without it"
                            }
                        }
                    }
                    tbody {
                        for row in rows.iter() {
                            {
                                let d = row.debuff;
                                let others: Vec<&str> = overview
                                    .iter()
                                    .filter(|p| p.discipline_id.is_some_and(|id| row.able.contains(&id)))
                                    .map(|p| p.name.as_str())
                                    .filter(|n| !row.suppliers.iter().any(|s| s.0 == *n))
                                    .collect();
                                rsx! {
                                    tr { key: "{d.name()}", class: if row.suppliers.is_empty() { "debuff-absent" },
                                        td { title: "{d.name()}: {d.description()}",
                                            span { class: "name-with-icon",
                                                AbilityIcon { ability_id: d.icon_effect_id(), size: 16 }
                                                span { class: "name-text", "{d.name()}" }
                                            }
                                        }
                                        td { class: "debuff-players",
                                            for (name, count) in row.suppliers.iter() {
                                                span { {icon(name)} "{name} " span { class: "analysis-note", "×{count}" } }
                                            }
                                            if row.suppliers.is_empty() {
                                                span { class: "analysis-note", "–" }
                                            }
                                        }
                                        td { class: "debuff-players",
                                            for name in others.iter().copied() {
                                                span { {icon(name)} "{name}" }
                                            }
                                            if others.is_empty() && row.suppliers.is_empty() {
                                                span { class: "analysis-note", "–" }
                                            }
                                        }
                                        td { class: "num",
                                            match d.boosts() {
                                                Some(kind) if row.affected_dmg > 0.0 => rsx! {
                                                    span { title: "{compact(row.unbuffed_dmg)} of {compact(row.affected_dmg)} {kind} damage landed while {d.name()} was down",
                                                        "{compact(row.unbuffed_dmg)} "
                                                        span { class: "analysis-note",
                                                            "({formatting::format_pct(row.unbuffed_dmg * 100.0 / row.affected_dmg, european)})"
                                                        }
                                                    }
                                                },
                                                Some(kind) => rsx! {
                                                    span { class: "analysis-note", title: "No {kind} damage on these enemies", "–" }
                                                },
                                                None => rsx! {
                                                    span { class: "analysis-note", title: "The log does not tell which damage {d.name()} raises", "–" }
                                                },
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
