//! Overview stats table for the Data Explorer
//!
//! One table per side: PvE encounters render a single table with a group
//! total; PvP encounters render two (friendly/enemy) with per-side totals
//! under a colored side heading.

use dioxus::prelude::*;

use baras_types::{OverviewColumn, UiSessionState, formatting};

use crate::api::{PlayerDeath, RaidOverviewRow};
use crate::components::class_icons::{get_class_icon, get_role_icon};
use crate::components::overview_columns::{ColumnDef, OverviewColumnMenu, column};

#[derive(Props, Clone, PartialEq)]
pub struct OverviewTableProps {
    /// Player rows, already sorted
    pub rows: Vec<RaidOverviewRow>,
    /// Number columns, in display order
    pub columns: Vec<OverviewColumn>,
    /// Label of the footer row summing `rows`; no footer without one
    #[props(default)]
    pub total_label: Option<&'static str>,
    /// Colored side heading above the table: (css class, text) — PvP only
    #[props(default)]
    pub side_label: Option<(&'static str, &'static str)>,
    pub sort_col: Signal<OverviewColumn>,
    pub sort_asc: Signal<bool>,
    /// European number formatting
    pub european: bool,
    /// Set to the row's player while the pointer is over it (group chart highlight)
    #[props(default)]
    pub hovered: Option<Signal<Option<String>>>,
    /// Every death in the encounter, chronological; matched to rows by name + side
    #[props(default)]
    pub deaths: Vec<PlayerDeath>,
    /// Clicked a row's deaths cell: opens that player's first death
    pub on_death_click: EventHandler<PlayerDeath>,
    /// Where column picks are saved; the header gets the columns button with it
    #[props(default)]
    pub layout_state: Option<Signal<UiSessionState>>,
    /// The PvP layout is the one shown and edited
    #[props(default)]
    pub pvp: bool,
}

#[component]
pub fn OverviewTable(props: OverviewTableProps) -> Element {
    let mut sort_col = props.sort_col;
    let mut sort_asc = props.sort_asc;
    let eu = props.european;
    let mut menu_open = use_signal(|| false);
    let cols: Vec<&'static ColumnDef> = props.columns.iter().map(|&id| column(id)).collect();
    let cur = *sort_col.read();
    let asc = *sort_asc.read();
    let arrow = move |col: OverviewColumn| -> &'static str {
        if cur == col {
            if asc { " \u{25B2}" } else { " \u{25BC}" }
        } else {
            ""
        }
    };
    let sort_click = move |col: OverviewColumn| {
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

    let row_deaths = |row: &RaidOverviewRow| -> Vec<PlayerDeath> {
        props
            .deaths
            .iter()
            .filter(|d| d.name == row.name && d.faction == row.faction)
            .cloned()
            .collect()
    };

    rsx! {
        if let Some((class, text)) = props.side_label {
            span { class: "overview-side-label {class}", "{text}" }
        }
        div { class: "overview-table-wrap",
        div { class: "overview-table-box",
            table { class: "overview-table",
                // Fixed layout takes column widths from here, never from the
                // cell contents, so nothing moves with digits or death badges
                colgroup {
                    col { class: "name-col" }
                    col { class: "death-col" }
                    for _ in 0..cols.len() {
                        col { class: "num-col" }
                    }
                }
                thead {
                    tr { class: "sub-header",
                        th { class: "name-col",
                            "Name"
                            if props.layout_state.is_some() {
                                button {
                                    class: "overview-columns-btn",
                                    title: "Choose columns",
                                    onclick: move |_| menu_open.toggle(),
                                    i { class: "fa-solid fa-table-columns" }
                                }
                            }
                        }
                        th { class: "death-col", title: "Deaths" }
                        for c in cols.iter() {
                            th { class: "num sortable", title: "{c.title}", onclick: sort_click(c.id), "{c.label}{arrow(c.id)}" }
                        }
                    }
                }
                tbody {
                    for row in props.rows.iter() {
                        tr {
                            onmouseenter: {
                                let name = row.name.clone();
                                move |_| if let Some(mut h) = props.hovered { h.set(Some(name.clone())) }
                            },
                            onmouseleave: move |_| if let Some(mut h) = props.hovered { h.set(None) },
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
                                    span { class: "name-text", title: "{row.name}", "{row.name}" }
                                }
                            }
                            {death_cell(row_deaths(row), props.on_death_click)}
                            for c in cols.iter() {
                                td { class: "num {c.group.class()}", "{c.format.apply((c.value)(row), eu)}" }
                            }
                        }
                    }
                }
                if let Some(label) = props.total_label {
                    tfoot {
                        tr { class: "totals-row",
                            td { class: "name-col", "{label}" }
                            td { class: "death-col" }
                            for c in cols.iter() {
                                td { class: "num {c.group.class()}",
                                    if c.sums {
                                        "{c.format.apply(props.rows.iter().map(c.value).sum(), eu)}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    if let Some(state) = props.layout_state {
            OverviewColumnMenu { state, shown: props.columns.clone(), pvp: props.pvp, open: menu_open }
        }
        }
    }
}

/// `💀 1:13` for one death, `💀 ×3` for more (times in the tooltip); empty
/// when the player survived. PvP: tinted when the local player landed a kill.
fn death_cell(deaths: Vec<PlayerDeath>, on_click: EventHandler<PlayerDeath>) -> Element {
    let Some(first) = deaths.first().cloned() else {
        return rsx! { td { class: "death-col" } };
    };
    let times: Vec<String> = deaths
        .iter()
        .map(|d| formatting::format_duration(d.death_time_secs as i64))
        .collect();
    let label = if deaths.len() == 1 {
        times[0].clone()
    } else {
        format!("\u{00D7}{}", deaths.len())
    };
    let class = if deaths.iter().any(|d| d.local_kill) {
        "death-col local-kill"
    } else {
        "death-col"
    };
    rsx! {
        td { class: "{class}",
            button {
                class: "death-badge",
                title: "{times.join(\" · \")} — click for death recap",
                onclick: move |_| on_click.call(first.clone()),
                i { class: "fa-solid fa-skull" }
                "{label}"
            }
        }
    }
}
