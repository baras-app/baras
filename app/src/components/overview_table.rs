//! Overview stats table for the Data Explorer
//!
//! One table per side: PvE encounters render a single table with a group
//! total; PvP encounters render two (friendly/enemy) with per-side totals
//! under a colored side heading.

use dioxus::prelude::*;

use baras_types::formatting;

use crate::api::RaidOverviewRow;
use crate::components::class_icons::{get_class_icon, get_role_icon};

/// Sortable columns for the overview table
#[derive(Clone, Copy, PartialEq, Default)]
pub enum OverviewSort {
    #[default]
    DPS,
    DamageTotal,
    ThreatTotal,
    TPS,
    DamageTakenTotal,
    DTPS,
    APS,
    HealingTotal,
    HPS,
    HealingPct,
    EHPS,
    ShieldingTotal,
    SPS,
    APM,
    Interrupts,
}

impl OverviewSort {
    pub fn extract(self, row: &RaidOverviewRow) -> f64 {
        match self {
            Self::DamageTotal => row.damage_total,
            Self::DPS => row.dps,
            Self::ThreatTotal => row.threat_total,
            Self::TPS => row.tps,
            Self::DamageTakenTotal => row.damage_taken_total,
            Self::DTPS => row.dtps,
            Self::APS => row.aps,
            Self::HealingTotal => row.healing_total,
            Self::HPS => row.hps,
            Self::HealingPct => row.healing_pct,
            Self::EHPS => row.ehps,
            Self::ShieldingTotal => row.shielding_given_total,
            Self::SPS => row.sps,
            Self::APM => row.apm,
            Self::Interrupts => row.interrupts,
        }
    }
}

/// Column totals for a table footer row
#[derive(Clone, PartialEq, Default)]
pub struct OverviewTotals {
    damage: f64,
    dps: f64,
    threat: f64,
    tps: f64,
    damage_taken: f64,
    dtps: f64,
    aps: f64,
    shielding: f64,
    sps: f64,
    healing: f64,
    hps: f64,
    ehps: f64,
    interrupts: f64,
}

impl OverviewTotals {
    pub fn from_rows<'a>(rows: impl Iterator<Item = &'a RaidOverviewRow>) -> Self {
        rows.fold(Self::default(), |mut t, r| {
            t.damage += r.damage_total;
            t.dps += r.dps;
            t.threat += r.threat_total;
            t.tps += r.tps;
            t.damage_taken += r.damage_taken_total;
            t.dtps += r.dtps;
            t.aps += r.aps;
            t.shielding += r.shielding_given_total;
            t.sps += r.sps;
            t.healing += r.healing_total;
            t.hps += r.hps;
            t.ehps += r.ehps;
            t.interrupts += r.interrupts;
            t
        })
    }
}

#[derive(Props, Clone, PartialEq)]
pub struct OverviewTableProps {
    /// Player rows, already sorted
    pub rows: Vec<RaidOverviewRow>,
    /// Footer totals: one row per (label, totals) entry
    pub total_rows: Vec<(&'static str, OverviewTotals)>,
    /// Colored side heading above the table: (css class, text) — PvP only
    #[props(default)]
    pub side_label: Option<(&'static str, &'static str)>,
    pub sort_col: Signal<OverviewSort>,
    pub sort_asc: Signal<bool>,
    /// European number formatting
    pub european: bool,
}

#[component]
pub fn OverviewTable(props: OverviewTableProps) -> Element {
    let mut sort_col = props.sort_col;
    let mut sort_asc = props.sort_asc;
    let eu = props.european;
    let format_number = move |n: f64| formatting::format_compact_f64(n, eu);
    let format_pct = move |n: f64| formatting::format_pct(n, eu);

    let cur = *sort_col.read();
    let asc = *sort_asc.read();
    let arrow = move |col: OverviewSort| -> &'static str {
        if cur == col {
            if asc { " \u{25B2}" } else { " \u{25BC}" }
        } else {
            ""
        }
    };
    let sort_click = move |col: OverviewSort| {
        move |_: MouseEvent| {
            if *sort_col.read() == col {
                let was_asc = *sort_asc.peek();
                sort_asc.set(!was_asc);
            } else {
                sort_col.set(col);
                sort_asc.set(false);
            }
        }
    };

    rsx! {
        if let Some((class, text)) = props.side_label {
            span { class: "overview-side-label {class}", "{text}" }
        }
        div { class: "overview-table-box",
            table { class: "overview-table",
                thead {
                    tr {
                        th { class: "name-col", "Name" }
                        th { class: "section-header", colspan: "2", "Damage Dealt" }
                        th { class: "section-header", colspan: "2", "Threat" }
                        th { class: "section-header", colspan: "3", "Damage Taken" }
                        th { class: "section-header", colspan: "4", "Healing" }
                        th { class: "section-header", colspan: "2", "Shielding" }
                        th { class: "section-header", colspan: "2", "Activity" }
                    }
                    tr { class: "sub-header",
                        th {}
                        th { class: "num sortable", onclick: sort_click(OverviewSort::DamageTotal), "Total{arrow(OverviewSort::DamageTotal)}" }
                        th { class: "num sortable", onclick: sort_click(OverviewSort::DPS), "DPS{arrow(OverviewSort::DPS)}" }
                        th { class: "num sortable", onclick: sort_click(OverviewSort::ThreatTotal), "Total{arrow(OverviewSort::ThreatTotal)}" }
                        th { class: "num sortable", onclick: sort_click(OverviewSort::TPS), "TPS{arrow(OverviewSort::TPS)}" }
                        th { class: "num sortable", onclick: sort_click(OverviewSort::DamageTakenTotal), "Total{arrow(OverviewSort::DamageTakenTotal)}" }
                        th { class: "num sortable", onclick: sort_click(OverviewSort::DTPS), "DTPS{arrow(OverviewSort::DTPS)}" }
                        th { class: "num sortable", onclick: sort_click(OverviewSort::APS), "APS{arrow(OverviewSort::APS)}" }
                        th { class: "num sortable", onclick: sort_click(OverviewSort::HealingTotal), "Total{arrow(OverviewSort::HealingTotal)}" }
                        th { class: "num sortable", onclick: sort_click(OverviewSort::HPS), "HPS{arrow(OverviewSort::HPS)}" }
                        th { class: "num sortable", onclick: sort_click(OverviewSort::HealingPct), "%{arrow(OverviewSort::HealingPct)}" }
                        th { class: "num sortable", onclick: sort_click(OverviewSort::EHPS), "EHPS{arrow(OverviewSort::EHPS)}" }
                        th { class: "num sortable", onclick: sort_click(OverviewSort::ShieldingTotal), "Total{arrow(OverviewSort::ShieldingTotal)}" }
                        th { class: "num sortable", onclick: sort_click(OverviewSort::SPS), "SPS{arrow(OverviewSort::SPS)}" }
                        th { class: "num sortable", onclick: sort_click(OverviewSort::APM), "APM{arrow(OverviewSort::APM)}" }
                        th { class: "num sortable", title: "Interrupt casts", onclick: sort_click(OverviewSort::Interrupts), "Int{arrow(OverviewSort::Interrupts)}" }
                    }
                }
                tbody {
                    for row in props.rows.iter() {
                        tr {
                            td { class: "name-col",
                                span { class: "name-with-icon",
                                    if let Some(role_name) = &row.role_icon {
                                        if let Some(role_asset) = get_role_icon(role_name) {
                                            img {
                                                class: "role-icon",
                                                src: *role_asset,
                                                alt: ""
                                            }
                                        }
                                    }
                                    if let Some(icon_name) = &row.class_icon {
                                        if let Some(icon_asset) = get_class_icon(icon_name) {
                                            img {
                                                class: "class-icon",
                                                src: *icon_asset,
                                                title: "{row.discipline_name.as_deref().unwrap_or(\"\")}",
                                                alt: ""
                                            }
                                        }
                                    }
                                    "{row.name}"
                                }
                            }
                            td { class: "num dmg", "{format_number(row.damage_total)}" }
                            td { class: "num dmg", "{format_number(row.dps)}" }
                            td { class: "num threat", "{format_number(row.threat_total)}" }
                            td { class: "num threat", "{format_number(row.tps)}" }
                            td { class: "num taken", "{format_number(row.damage_taken_total)}" }
                            td { class: "num taken", "{format_number(row.dtps)}" }
                            td { class: "num taken", "{format_number(row.aps)}" }
                            td { class: "num heal", "{format_number(row.healing_total)}" }
                            td { class: "num heal", "{format_number(row.hps)}" }
                            td { class: "num heal", "{format_pct(row.healing_pct)}" }
                            td { class: "num heal", "{format_number(row.ehps)}" }
                            td { class: "num shield", "{format_number(row.shielding_given_total)}" }
                            td { class: "num shield", "{format_number(row.sps)}" }
                            td { class: "num apm", "{formatting::format_decimal_f64(row.apm, 1, eu)}" }
                            td { class: "num apm", "{formatting::format_decimal_f64(row.interrupts, 0, eu)}" }
                        }
                    }
                }
                tfoot {
                    for (label, t) in props.total_rows.iter() {
                        tr { class: "totals-row",
                            td { class: "name-col", "{label}" }
                            td { class: "num dmg", "{format_number(t.damage)}" }
                            td { class: "num dmg", "{format_number(t.dps)}" }
                            td { class: "num threat", "{format_number(t.threat)}" }
                            td { class: "num threat", "{format_number(t.tps)}" }
                            td { class: "num taken", "{format_number(t.damage_taken)}" }
                            td { class: "num taken", "{format_number(t.dtps)}" }
                            td { class: "num taken", "{format_number(t.aps)}" }
                            td { class: "num heal", "{format_number(t.healing)}" }
                            td { class: "num heal", "{format_number(t.hps)}" }
                            td { class: "num heal", "" }
                            td { class: "num heal", "{format_number(t.ehps)}" }
                            td { class: "num shield", "{format_number(t.shielding)}" }
                            td { class: "num shield", "{format_number(t.sps)}" }
                            td { class: "num apm" }
                            td { class: "num apm", "{formatting::format_decimal_f64(t.interrupts, 0, eu)}" }
                        }
                    }
                }
            }
        }
    }
}
