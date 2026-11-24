//! Body of the ability breakdown table: target group headers and ability rows.

use dioxus::prelude::*;

use super::ability_table::TableCols;
use super::grouping::{AbilityGroups, GroupStats};
use crate::api::AbilityBreakdown;
use crate::components::ability_icon::AbilityIcon;
use crate::components::mirror_names::display_id;
use baras_types::formatting;

pub(super) fn table_body(grouped_abilities: Memo<AbilityGroups>, cols: TableCols, eu: bool) -> Element {
    let TableCols {
        show_ability_col,
        show_activations,
        is_damage_tab,
        is_damage_taken,
        is_healing_tab,
        is_outgoing_tab,
        ..
    } = cols;
    let format_number = |n: f64| formatting::format_compact_f64(n, eu);
    let format_pct = |n: f64| formatting::format_pct(n, eu);

    // Compute split averages for group stats
    let group_miss_pct = |s: &GroupStats| -> f64 {
        let total = s.hits + s.miss_count;
        if total > 0 {
            s.miss_count as f64 / total as f64 * 100.0
        } else {
            0.0
        }
    };
    let group_avg_hit = |s: &GroupStats| -> f64 { if s.hits > 0 { s.total / s.hits as f64 } else { 0.0 } };
    let group_avg_crit = |s: &GroupStats| -> f64 {
        let crits = (s.crit_pct / 100.0 * s.hits as f64) as i64;
        if crits > 0 { s.crit_total / crits as f64 } else { 0.0 }
    };
    let group_eff_pct = |s: &GroupStats| -> f64 {
        if s.total > 0.0 {
            s.effective_total / s.total * 100.0
        } else {
            0.0
        }
    };

    // Compute split averages for ability rows
    let ability_avg_crit = |a: &AbilityBreakdown| -> f64 {
        if a.crit_count > 0 {
            a.crit_total / a.crit_count as f64
        } else {
            0.0
        }
    };
    let ability_miss_pct = |a: &AbilityBreakdown| -> f64 {
        let total = a.hit_count + a.miss_count;
        if total > 0 {
            a.miss_count as f64 / total as f64 * 100.0
        } else {
            0.0
        }
    };
    let ability_eff_pct = |a: &AbilityBreakdown| -> f64 {
        if a.total_value > 0.0 {
            a.effective_total / a.total_value * 100.0
        } else {
            0.0
        }
    };

    rsx! {
        tbody {
            if grouped_abilities().iter().all(|(_, abilities)| abilities.is_empty()) {
                tr {
                    td { colspan: "{cols.count()}", class: "empty-message", "Nothing recorded for this selection in this range" }
                }
            }
            for (stats, abilities) in grouped_abilities().iter() {
                if let Some(target) = &stats.target {
                    tr { class: "group-header",
                        td { class: "group-target",
                            i { class: "fa-solid fa-caret-down group-icon" }
                            "{target}"
                            if let Some(t) = stats.first_hit {
                                span { class: "target-time",
                                    " @{(t as i32) / 60}:{(t as i32) % 60:02}"
                                }
                            }
                        }
                        if show_activations {
                            td { class: "num group-stat", "{stats.activation_count}" }
                        }
                        td { class: "num group-stat", "{stats.hits}" }
                        td { class: "num group-stat col-val", "{format_number(stats.total)}" }
                        td { class: "num group-stat col-pct col-pct-bar",
                            div { class: "pct-bar-track",
                                span { class: "pct-bar-fill", style: "width: {stats.percent}%;" }
                                span { class: "pct-text", "{format_pct(stats.percent)}" }
                            }
                        }
                        if is_healing_tab {
                            td { class: "num group-stat col-val col-eff", "{format_number(stats.effective_total)}" }
                            td { class: "num group-stat col-pct col-eff", "{format_pct(group_eff_pct(stats))}" }
                        }
                        td { class: "num group-stat col-val", "{format_number(stats.rate)}" }
                        if is_healing_tab {
                            td { class: "num group-stat col-val col-eff",
                                {
                                    let ehps = if stats.total > 0.0 { stats.rate * stats.effective_total / stats.total } else { 0.0 };
                                    format_number(ehps)
                                }
                            }
                        }
                        if is_damage_tab {
                            td { class: "num group-stat col-pct", "{format_pct(group_miss_pct(stats))}" }
                        }
                        if is_damage_taken {
                            td { class: "num group-stat col-pct",
                                {
                                    let total = stats.hits + stats.shield_count;
                                    if total > 0 {
                                        format_pct(stats.shield_count as f64 / total as f64 * 100.0)
                                    } else {
                                        "-".to_string()
                                    }
                                }
                            }
                            td { class: "num group-stat col-val",
                                if stats.absorbed_total > 0.0 { "{format_number(stats.absorbed_total)}" } else { "-" }
                            }
                            td { class: "num group-stat col-dmg-type", "-" }
                            td { class: "num group-stat col-dmg-type", "-" }
                        }
                        if !is_damage_taken {
                            td { class: "num group-stat col-pct", "{format_pct(stats.crit_pct)}" }
                        }
                        td { class: "num group-stat col-avg col-avg-first",
                            {

                                if is_damage_tab {
                                    let total_attempts = stats.hits + stats.miss_count;
                                    if total_attempts > 0 { format_number(stats.total / total_attempts as f64) } else { format_number(0.0) }
                                } else {
                                    format_number(stats.avg)
                                }
                            }
                        }
                        if is_damage_tab {
                            td { class: "num group-stat col-avg", "{format_number(group_avg_hit(stats))}" }
                        }
                        td { class: "num group-stat col-avg", "{format_number(group_avg_crit(stats))}" }
                        td { class: "num group-stat col-avg",
                            if stats.max_hit > 0.0 { "{format_number(stats.max_hit)}" } else { "-" }
                        }
                        if is_outgoing_tab {
                            td { class: "num group-stat col-avg",
                                {
                                    if stats.activation_count > 0 {
                                        format_number(stats.total / stats.activation_count as f64)
                                    } else {
                                        "-".to_string()
                                    }
                                }
                            }
                        }
                        if is_healing_tab {
                            td { class: "num group-stat col-avg col-eff",
                                {
                                    if stats.activation_count > 0 {
                                        format_number(stats.effective_total / stats.activation_count as f64)
                                    } else {
                                        "-".to_string()
                                    }
                                }
                            }
                        }
                    }
                }
                if show_ability_col {
                    for (idx, ability) in abilities.iter().enumerate() {
                        tr { key: "{stats.target.as_deref().unwrap_or(\"\")}-{idx}-{ability.ability_id}", class: if ability.is_shield { if stats.target.is_some() { "ability-row shield-row indented" } else { "ability-row shield-row" } } else if stats.target.is_some() { "ability-row indented" } else { "ability-row" },
                            td { class: "ability-name-cell",
                                span { class: "ability-name-inner",
                                    AbilityIcon { ability_id: display_id(ability.ability_id) }
                                    span { class: "ability-name-text",
                                        if !ability.ability_name.is_empty() {
                                            "{ability.ability_name}"
                                        } else {
                                            "Ability #{ability.ability_id}"
                                        }
                                    }
                                    if ability.is_shield {
                                        span { class: "shield-badge", " (shield)" }
                                    }
                                }
                            }
                            // Activations and Hits as columns #2 and #3
                            if show_activations {
                                td { class: "num",
                                    if ability.activation_count > 0 {
                                        "{ability.activation_count}"
                                    } else {
                                        "-"
                                    }
                                }
                            }
                            td { class: "num", "{ability.hit_count}" }
                            // Total with inline bar
                            td { class: if is_healing_tab && ability.is_shield { "num col-shield" } else { "num col-val" }, "{format_number(ability.total_value)}" }
                            // %
                            td { class: "num col-pct col-pct-bar",
                                div { class: "pct-bar-track",
                                    span { class: "pct-bar-fill", style: "width: {ability.percent_of_total}%;" }
                                    span { class: "pct-text", "{format_pct(ability.percent_of_total)}" }
                                }
                            }
                            if is_healing_tab {
                                td { class: if ability.is_shield { "num col-shield" } else { "num col-val col-eff" }, "{format_number(ability.effective_total)}" }
                                td { class: "num col-pct col-eff", "{format_pct(ability_eff_pct(ability))}" }
                            }
                            td { class: if is_healing_tab && ability.is_shield { "num col-shield" } else { "num col-val" }, "{format_number(ability.dps)}" }
                            if is_healing_tab {
                                td { class: if ability.is_shield { "num col-shield" } else { "num col-val col-eff" },
                                    {
                                        let ehps = if ability.total_value > 0.0 { ability.dps * ability.effective_total / ability.total_value } else { 0.0 };
                                        format_number(ehps)
                                    }
                                }
                            }
                            if is_damage_tab {
                                td { class: "num col-pct",
                                    if ability.is_shield { "-" } else { "{format_pct(ability_miss_pct(ability))}" }
                                }
                            }
                            if is_damage_taken {
                                td { class: "num col-pct",
                                    {
                                        if ability.hit_count > 0 {
                                            format_pct(ability.shield_count as f64 / ability.hit_count as f64 * 100.0)
                                        } else {
                                            "-".to_string()
                                        }
                                    }
                                }
                                td { class: "num col-val",
                                    if ability.absorbed_total > 0.0 { "{format_number(ability.absorbed_total)}" } else { "-" }
                                }
                                td { class: "num col-dmg-type",
                                    {
                                        match ability.attack_type.as_str() {
                                            "" => "-",
                                            other => other,
                                        }
                                    }
                                }
                                td { class: "num col-dmg-type",
                                    {
                                        match ability.damage_type.as_str() {
                                            "" => "-",
                                            other => other,
                                        }
                                    }
                                }
                            }
                            if !is_damage_taken {
                                td { class: "num col-pct",
                                    if ability.is_shield { "-" } else { "{format_pct(ability.crit_rate)}" }
                                }
                            }
                            td { class: "num col-avg col-avg-first",
                                {
                                    let overall = if is_damage_tab {
                                        let total_attempts = ability.hit_count + ability.miss_count;
                                        if total_attempts > 0 { ability.total_value / total_attempts as f64 } else { 0.0 }
                                    } else {
                                        ability.avg_hit
                                    };
                                    format_number(overall)
                                }
                            }
                            if is_damage_tab {
                                td { class: "num col-avg",
                                    if ability.is_shield { "-" } else { "{format_number(ability.avg_hit)}" }
                                }
                            }
                            td { class: "num col-avg",
                                if ability.is_shield || ability.crit_count == 0 { "-" } else { "{format_number(ability_avg_crit(ability))}" }
                            }
                            td { class: "num col-avg",
                                if ability.max_hit > 0.0 { "{format_number(ability.max_hit)}" } else { "-" }
                            }
                            if is_outgoing_tab {
                                td { class: "num col-avg",
                                    {
                                        if ability.activation_count > 0 {
                                            format_number(ability.total_value / ability.activation_count as f64)
                                        } else {
                                            "-".to_string()
                                        }
                                    }
                                }
                            }
                            if is_healing_tab {
                                td { class: "num col-avg col-eff",
                                    {
                                        if ability.activation_count > 0 {
                                            format_number(ability.effective_total / ability.activation_count as f64)
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
}
