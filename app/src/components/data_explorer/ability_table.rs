//! Detailed ability breakdown table: column set, sortable header and totals row.

use dioxus::prelude::*;

use super::ability_rows::table_body;
use super::ctx::{ExplorerCtx, repaint_with};
use super::grouping::AbilityGroups;
use crate::types::{DataTab, SortColumn, SortDirection};
use baras_types::formatting;

/// Which columns the current tab and breakdown mode show
#[derive(Clone, Copy)]
pub(super) struct TableCols {
    pub show_breakdown_col: bool,
    pub show_ability_col: bool,
    pub breakdown_col_label: &'static str,
    pub rate_label: &'static str,
    pub is_damage_tab: bool,
    pub is_damage_taken: bool,
    pub is_healing_tab: bool,
    pub is_outgoing_tab: bool,
}

pub(super) fn ability_table(ctx: ExplorerCtx, tab: DataTab, european: bool) -> Element {
    repaint_with(ctx.abilities);
    let mode = *ctx.breakdown_mode.read();
    let is_damage_tab = tab == DataTab::Damage || tab == DataTab::DamageTaken;
    let is_healing_tab = tab.is_healing();
    let cols = TableCols {
        show_breakdown_col: mode.by_target_type || mode.by_target_instance,
        show_ability_col: mode.by_ability,
        breakdown_col_label: if tab.is_outgoing() { "Target" } else { "Source" },
        rate_label: tab.rate_label(),
        is_damage_tab,
        is_damage_taken: tab == DataTab::DamageTaken,
        is_healing_tab,
        is_outgoing_tab: tab.is_outgoing(),
    };
    let table_class = if is_damage_tab {
        "ability-table damage-tab"
    } else if is_healing_tab {
        "ability-table healing-tab"
    } else {
        "ability-table"
    };

    rsx! {
        // Scrolls on its own under the fixed controls, header pinned, so a
        // longer or shorter list never moves anything above it
        // Keyed in a one-item list so another encounter or tab mounts a new
        // table, which fades in; sorting and filtering keep this one
        for nav in [format!("{:?}-{tab:?}", (ctx.shown_encounter)())] {
            div { key: "{nav}", class: "ability-table-scroll fade-in",
                table { class: table_class,
                    {table_head(ctx.sort_column, ctx.sort_direction, cols)}
                    {table_body(ctx.grouped_abilities, cols, european)}
                    {table_foot(ctx.grouped_abilities, cols, european)}
                }
            }
        }
    }
}

fn table_head(
    mut sort_column: Signal<SortColumn>,
    mut sort_direction: Signal<SortDirection>,
    cols: TableCols,
) -> Element {
    let TableCols {
        show_breakdown_col,
        show_ability_col,
        breakdown_col_label,
        rate_label,
        is_damage_tab,
        is_damage_taken,
        is_healing_tab,
        is_outgoing_tab,
    } = cols;
    let current_sort = *sort_column.read();
    let current_dir = *sort_direction.read();

    // Helper to get sort indicator class
    let sort_class = |col: SortColumn| -> &'static str {
        if current_sort == col {
            match current_dir {
                SortDirection::Asc => "sortable sorted-asc",
                SortDirection::Desc => "sortable sorted-desc",
            }
        } else {
            "sortable"
        }
    };

    // Macro-like helper for sort click (inline to avoid closure issues)
    let sort_click = |col: SortColumn, is_text: bool| {
        move |_| {
            if *sort_column.read() == col {
                let new_dir = match *sort_direction.read() {
                    SortDirection::Asc => SortDirection::Desc,
                    SortDirection::Desc => SortDirection::Asc,
                };
                sort_direction.set(new_dir);
            } else {
                sort_column.set(col);
                sort_direction.set(if is_text {
                    SortDirection::Asc
                } else {
                    SortDirection::Desc
                });
            }
        }
    };

    rsx! {
        thead {
            tr {
                th {
                    class: if show_breakdown_col { sort_class(SortColumn::Target) } else { sort_class(SortColumn::Ability) },
                    onclick: if show_breakdown_col { sort_click(SortColumn::Target, true) } else { sort_click(SortColumn::Ability, true) },
                    if show_breakdown_col {
                        "{breakdown_col_label} / Ability"
                    } else {
                        "Ability"
                    }
                }
                // Activations and Hits as columns #2 and #3
                if show_ability_col {
                    th {
                        class: "num col-wide {sort_class(SortColumn::Activations)}",
                        onclick: sort_click(SortColumn::Activations, false),
                        title: "Times the ability was activated",
                        "Activations"
                    }
                }
                th {
                    class: "num {sort_class(SortColumn::Hits)}",
                    onclick: sort_click(SortColumn::Hits, false),
                    title: "Hits and ticks landed",
                    "Hits"
                }
                th {
                    class: "num col-val {sort_class(SortColumn::Total)}",
                    onclick: sort_click(SortColumn::Total, false),
                    "Total"
                }
                th {
                    class: "num col-pct col-pct-bar {sort_class(SortColumn::Percent)}",
                    onclick: sort_click(SortColumn::Percent, false),
                    title: "Share of the total",
                    "%"
                }
                if is_healing_tab {
                    th {
                        class: "num col-val col-eff col-wide {sort_class(SortColumn::Effective)}",
                        onclick: sort_click(SortColumn::Effective, false),
                        title: "Healing that was not overheal",
                        "Effective"
                    }
                    th {
                        class: "num col-pct col-eff {sort_class(SortColumn::EffectivePct)}",
                        onclick: sort_click(SortColumn::EffectivePct, false),
                        title: "Share of the healing that was effective",
                        "Eff%"
                    }
                }
                th {
                    class: "num col-val {sort_class(SortColumn::Rate)}",
                    onclick: sort_click(SortColumn::Rate, false),
                    "{rate_label}"
                }
                if is_healing_tab {
                    th {
                        class: "num col-val col-eff {sort_class(SortColumn::Effective)}",
                        onclick: sort_click(SortColumn::Effective, false),
                        title: "Effective healing per second",
                        "EHPS"
                    }
                }
                if is_damage_tab {
                    th {
                        class: "num col-pct {sort_class(SortColumn::MissPct)}",
                        onclick: sort_click(SortColumn::MissPct, false),
                        title: if is_damage_taken { "Share of attacks avoided: missed, dodged, parried or resisted" } else { "Share of attacks that did not land" },
                        if is_damage_taken { "Def%" } else { "Miss%" }
                    }
                }
                if is_damage_taken {
                    th {
                        class: "num col-pct {sort_class(SortColumn::ShldPct)}",
                        onclick: sort_click(SortColumn::ShldPct, false),
                        title: "Share of hits that were shielded",
                        "Shld%"
                    }
                    th {
                        class: "num col-val {sort_class(SortColumn::Absorbed)}",
                        onclick: sort_click(SortColumn::Absorbed, false),
                        title: "Damage absorbed",
                        "Abs"
                    }
                    th {
                        class: "num col-dmg-type {sort_class(SortColumn::AttackType)}",
                        onclick: sort_click(SortColumn::AttackType, true),
                        title: "Attack type",
                        "AT"
                    }
                    th {
                        class: "num col-dmg-type {sort_class(SortColumn::DamageType)}",
                        onclick: sort_click(SortColumn::DamageType, true),
                        title: "Damage type",
                        "DT"
                    }
                }
                if !is_damage_taken {
                    th {
                        class: "num col-pct {sort_class(SortColumn::CritPct)}",
                        onclick: sort_click(SortColumn::CritPct, false),
                        title: "Share of hits that were critical",
                        "Crit%"
                    }
                }
                th {
                    class: "num col-avg col-avg-first {sort_class(SortColumn::Avg)}",
                    onclick: sort_click(SortColumn::Avg, false),
                    title: "Average per hit",
                    "Avg"
                }
                if is_damage_tab {
                    th {
                        class: "num col-avg {sort_class(SortColumn::AvgHit)}",
                        onclick: sort_click(SortColumn::AvgHit, false),
                        title: "Average non-critical hit",
                        "Hit"
                    }
                }
                th {
                    class: "num col-avg {sort_class(SortColumn::AvgCrit)}",
                    onclick: sort_click(SortColumn::AvgCrit, false),
                    title: "Average critical hit",
                    "Crit"
                }
                th {
                    class: "num col-avg {sort_class(SortColumn::MaxHit)}",
                    onclick: sort_click(SortColumn::MaxHit, false),
                    title: "Largest single hit",
                    "Max"
                }
                if is_outgoing_tab {
                    th {
                        class: "num col-avg {sort_class(SortColumn::AvgPerActivation)}",
                        onclick: sort_click(SortColumn::AvgPerActivation, false),
                        title: "Average total per activation",
                        "Avg/Act"
                    }
                }
                if is_healing_tab {
                    th {
                        class: "num col-avg col-eff {sort_class(SortColumn::AvgPerActivationEff)}",
                        onclick: sort_click(SortColumn::AvgPerActivationEff, false),
                        title: "Average effective healing per activation",
                        "Avg/Act (eff)"
                    }
                }
            }
        }
    }
}

fn table_foot(grouped_abilities: Memo<AbilityGroups>, cols: TableCols, eu: bool) -> Element {
    let TableCols {
        show_ability_col,
        is_damage_tab,
        is_damage_taken,
        is_healing_tab,
        is_outgoing_tab,
        ..
    } = cols;
    let format_number = |n: f64| formatting::format_compact_f64(n, eu);
    let format_pct = |n: f64| formatting::format_pct(n, eu);

    rsx! {
        tfoot {
            {
                let groups = grouped_abilities();
                let total_hits: i64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.hit_count)).sum();
                let total_activations: i64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.activation_count)).sum();
                let total_val: f64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.total_value)).sum();
                let total_rate: f64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.dps)).sum();
                let total_eff: f64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.effective_total)).sum();
                let total_miss: i64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.miss_count)).sum();
                let total_crits: i64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.crit_count)).sum();
                let total_shield_count: i64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.shield_count)).sum();
                let total_absorbed: f64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.absorbed_total)).sum();
                let total_max_hit: f64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.max_hit)).fold(0.0_f64, f64::max);
                let crit_pct = if total_hits > 0 { total_crits as f64 / total_hits as f64 * 100.0 } else { 0.0 };
                let avg = if is_damage_tab {
                    let attempts = total_hits + total_miss;
                    if attempts > 0 { total_val / attempts as f64 } else { 0.0 }
                } else if total_hits > 0 {
                    total_val / total_hits as f64
                } else {
                    0.0
                };
                let miss_pct = {
                    let attempts = total_hits + total_miss;
                    if attempts > 0 { total_miss as f64 / attempts as f64 * 100.0 } else { 0.0 }
                };
                let eff_pct = if total_val > 0.0 { total_eff / total_val * 100.0 } else { 0.0 };
                let ehps = if total_val > 0.0 { total_rate * total_eff / total_val } else { 0.0 };
                let shld_pct = if total_hits > 0 {
                    total_shield_count as f64 / total_hits as f64 * 100.0
                } else { 0.0 };

                rsx! {
                    tr { class: "totals-row",
                        td { "Total" }
                        if show_ability_col {
                            td { class: "num", "{total_activations}" }
                        }
                        td { class: "num", "{total_hits}" }
                        td { class: "num col-val", "{format_number(total_val)}" }
                        td { class: "num col-pct col-pct-bar" }
                        if is_healing_tab {
                            td { class: "num col-val col-eff", "{format_number(total_eff)}" }
                            td { class: "num col-pct col-eff", "{format_pct(eff_pct)}" }
                        }
                        td { class: "num col-val", "{format_number(total_rate)}" }
                        if is_healing_tab {
                            td { class: "num col-val col-eff", "{format_number(ehps)}" }
                        }
                        if is_damage_tab {
                            td { class: "num col-pct", "{format_pct(miss_pct)}" }
                        }
                        if is_damage_taken {
                            td { class: "num col-pct", "{format_pct(shld_pct)}" }
                            td { class: "num col-val",
                                if total_absorbed > 0.0 { "{format_number(total_absorbed)}" } else { "-" }
                            }
                            td { class: "num col-dmg-type" }
                            td { class: "num col-dmg-type" }
                        }
                        if !is_damage_taken {
                            td { class: "num col-pct", "{format_pct(crit_pct)}" }
                        }
                        td { class: "num col-avg col-avg-first", "{format_number(avg)}" }
                        if is_damage_tab {
                            td { class: "num col-avg" }
                        }
                        td { class: "num col-avg" }
                        td { class: "num col-avg",
                            if total_max_hit > 0.0 { "{format_number(total_max_hit)}" } else { "-" }
                        }
                        if is_outgoing_tab {
                            td { class: "num col-avg",
                                {
                                    if total_activations > 0 {
                                        format_number(total_val / total_activations as f64)
                                    } else {
                                        "-".to_string()
                                    }
                                }
                            }
                        }
                        if is_healing_tab {
                            td { class: "num col-avg col-eff",
                                {
                                    if total_activations > 0 {
                                        format_number(total_eff / total_activations as f64)
                                    } else {
                                        "-".to_string()
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
