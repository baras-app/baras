//! Two-column layout shared by the Detailed, Rotation, Usage, Charts and Uptime views.

use dioxus::prelude::*;

use super::ability_table::AbilityTable;
use super::ctx::ExplorerCtx;
use super::entity_sidebar::EntitySidebar;
use super::usage_tab::UsageTab;
use crate::api::TimeRange;
use crate::components::ability_naming::AbilityNamingPills;
use crate::components::charts_panel::ChartsPanel;
use crate::components::dt_summary::DtSummaryTable;
use crate::components::effect_uptime::EffectUptimeView;
use crate::components::entity_picker::EntityPicker;
use crate::components::rotation_view::RotationView;
use crate::types::{DataTab, ViewMode};

#[component]
pub(super) fn DetailedView(
    shown: ViewMode,
    european: bool,
    on_source_click: Callback<(String, Option<i64>)>,
) -> Element {
    let ExplorerCtx {
        selected_encounter,
        selected_source,
        selected_source_id,
        mut breakdown_mode,
        show_players_only,
        selected_rotation_anchor,
        usage_sort_column,
        usage_sort_direction,
        usage_selected_abilities,
        entity_collapsed,
        mut time_range,
        timeline,
        dt_summary,
        entity_list,
        class_icon_lookup,
        ..
    } = use_context();
    let eu = european;
    let current_tab = if let ViewMode::Detailed(tab) = shown {
        Some(tab)
    } else {
        None
    };

    rsx! {
        div { class: "explorer-content",
            // Entity breakdown sidebar
            EntitySidebar { shown, european, on_source_click }

            if matches!(shown, ViewMode::Charts) {
                // Charts view in right column
                div { class: "charts-main",
                    if *entity_collapsed.read() {
                        EntityPicker {
                            entities: entity_list,
                            icons: class_icon_lookup,
                            selected: selected_source.read().clone(),
                            players_only: show_players_only,
                            on_pick: on_source_click,
                        }
                    }
                    if let Some(tl) = timeline.read().as_ref() {
                        {
                            let tr = time_range();
                            rsx! {
                                ChartsPanel {
                                    key: "{selected_encounter():?}",
                                    encounter_idx: *selected_encounter.read(),
                                    duration_secs: tl.duration_secs,
                                    time_range: tr,
                                    selected_source: selected_source,
                                    selected_source_id: selected_source_id,
                                    european: eu,
                                    on_time_range_change: move |new_range: TimeRange| {
                                        time_range.set(new_range);
                                    },
                                }
                            }
                        }
                    }
                }
            } else if matches!(shown, ViewMode::Rotation) {
                // Rotation view in right column
                div { class: "ability-section",
                    if *entity_collapsed.read() {
                        EntityPicker {
                            entities: entity_list,
                            icons: class_icon_lookup,
                            selected: selected_source.read().clone(),
                            players_only: show_players_only,
                            on_pick: on_source_click,
                        }
                    }
                    RotationView {
                        encounter_idx: *selected_encounter.read(),
                        time_range: time_range(),
                        selected_source: selected_source.read().clone(),
                        selected_anchor: selected_rotation_anchor,
                        on_range_change: move |new_range: TimeRange| {
                            time_range.set(new_range);
                        },
                        european: eu,
                    }
                }
            } else if matches!(shown, ViewMode::Uptime) {
                div { class: "ability-section",
                    if *entity_collapsed.read() {
                        EntityPicker {
                            entities: entity_list,
                            icons: class_icon_lookup,
                            selected: selected_source.read().clone(),
                            players_only: show_players_only,
                            on_pick: on_source_click,
                        }
                    }
                    EffectUptimeView {
                        encounter_idx: *selected_encounter.read(),
                        time_range: time_range(),
                        duration_secs: timeline.read().as_ref().map_or(0.0, |t| t.duration_secs),
                        selected_source: selected_source.read().clone(),
                        selected_source_id: *selected_source_id.read(),
                        european: eu,
                    }
                }
            } else if matches!(shown, ViewMode::Usage) {
                // Ability Usage view in right column
                div { class: "ability-section usage-section",
                    if *entity_collapsed.read() {
                        EntityPicker {
                            entities: entity_list,
                            icons: class_icon_lookup,
                            selected: selected_source.read().clone(),
                            players_only: show_players_only,
                            on_pick: on_source_click,
                        }
                    }
                    UsageTab {
                        selected_encounter: selected_encounter,
                        selected_source: selected_source,
                        time_range: time_range,
                        usage_sort_column: usage_sort_column,
                        usage_sort_direction: usage_sort_direction,
                        selected_abilities: usage_selected_abilities,
                        timeline: timeline,
                        european: eu,
                    }
                }
            } else if let Some(current_tab) = current_tab {
            // Ability breakdown table
            div { class: "ability-section",
                if *entity_collapsed.read() {
                    EntityPicker {
                        entities: entity_list,
                        icons: class_icon_lookup,
                        selected: selected_source.read().clone(),
                        players_only: show_players_only,
                        on_pick: on_source_click,
                    }
                }
                // DamageTaken summary panel
                if current_tab == DataTab::DamageTaken {
                    if let Some(ref summary) = *dt_summary.read() {
                        div { class: "damage-taken-summary",
                            DtSummaryTable {
                                title: "Damage types",
                                european: eu,
                                rows: vec![
                                    ("Internal / Elemental", summary.internal_elemental_pct, Some(summary.internal_elemental_total)),
                                    ("Kinetic / Energy", summary.kinetic_energy_pct, Some(summary.kinetic_energy_total)),
                                    ("Force / Tech", summary.force_tech_pct, Some(summary.force_tech_total)),
                                    ("Melee / Ranged", summary.melee_ranged_pct, Some(summary.melee_ranged_total)),
                                ],
                            }
                            DtSummaryTable {
                                title: "Mitigation",
                                european: eu,
                                rows: vec![
                                    ("Avoided", summary.avoided_pct, None),
                                    ("Shielded", summary.shielded_pct, None),
                                    ("Absorbed (self)", summary.absorbed_self_pct, Some(summary.absorbed_self_total)),
                                    ("Absorbed (given)", summary.absorbed_given_pct, Some(summary.absorbed_given_total)),
                                ],
                            }
                        }
                    }
                }
                // Header with breakdown controls only
                div { class: "ability-header",
                    // Breakdown mode toggles (nested hierarchy)
                    // Labels change based on tab: outgoing uses "Target", incoming uses "Source"
                    // Instance mode only makes sense for damage tabs (NPCs have multiple spawns)
                    {
                        let tab = current_tab;
                        let is_outgoing = tab.is_outgoing();
                        let type_label = if is_outgoing { "Target type" } else { "Source type" };
                        let instance_label = if is_outgoing { "Target instance" } else { "Source instance" };
                        // Instance mode only for Damage/DamageTaken (NPCs), not Healing (players don't have instances)
                        let show_instance = matches!(tab, DataTab::Damage | DataTab::DamageTaken);
                        rsx! {
                            div { class: "breakdown-controls",
                                span { class: "breakdown-label", "Breakdown by" }
                                div { class: "breakdown-options",
                                    label { class: "breakdown-option primary",
                                        input {
                                            r#type: "checkbox",
                                            checked: breakdown_mode.read().by_ability,
                                            // Can only disable if target type/instance is enabled (need at least one grouping)
                                            disabled: !breakdown_mode.read().by_target_type && !breakdown_mode.read().by_target_instance,
                                            onchange: move |e| {
                                                let mut mode = *breakdown_mode.read();
                                                mode.by_ability = e.checked();
                                                breakdown_mode.set(mode);
                                            }
                                        }
                                        "Ability"
                                    }
                                    div { class: "breakdown-nested",
                                        label { class: "breakdown-option",
                                            input {
                                                r#type: "checkbox",
                                                checked: breakdown_mode.read().by_target_type,
                                                onchange: move |e| {
                                                    let mut mode = *breakdown_mode.read();
                                                    mode.by_target_type = e.checked();
                                                    // If disabling target type, also disable target instance
                                                    if !e.checked() {
                                                        mode.by_target_instance = false;
                                                        // Re-enable ability if nothing else selected
                                                        mode.by_ability = true;
                                                    }
                                                    breakdown_mode.set(mode);
                                                }
                                            }
                                            "{type_label}"
                                        }
                                        if show_instance {
                                            label { class: "breakdown-option nested",
                                                input {
                                                    r#type: "checkbox",
                                                    checked: breakdown_mode.read().by_target_instance,
                                                    disabled: !breakdown_mode.read().by_target_type,
                                                    onchange: move |e| {
                                                        let mut mode = *breakdown_mode.read();
                                                        mode.by_target_instance = e.checked();
                                                        breakdown_mode.set(mode);
                                                    }
                                                }
                                                "{instance_label}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    AbilityNamingPills {}
                }
                // Table with dynamic columns (sortable)
                AbilityTable { tab: current_tab, european }
            }
            }
        }
    }
}
