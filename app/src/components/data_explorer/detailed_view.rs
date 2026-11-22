//! Two-column layout shared by the Detailed, Rotation, Usage, Charts and Uptime views.

use dioxus::prelude::*;

use super::ability_table::ability_table;
use super::ctx::ExplorerCtx;
use super::entity_sidebar::entity_sidebar;
use super::usage_tab::UsageTab;
use crate::api::TimeRange;
use crate::components::ability_naming::AbilityNamingPills;
use crate::components::charts_panel::ChartsPanel;
use crate::components::dt_summary::DtSummaryTable;
use crate::components::effect_uptime::EffectUptimeView;
use crate::components::entity_picker::EntityPicker;
use crate::components::rotation_view::RotationView;
use crate::types::{DataTab, ViewMode};

pub(super) fn detailed_view(
    ctx: ExplorerCtx,
    shown: ViewMode,
    european: bool,
    on_source_click: Callback<(String, Option<i64>)>,
) -> Element {
    let ExplorerCtx {
        shown_encounter,
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
    } = ctx;
    let eu = european;
    let current_tab = if let ViewMode::Detailed(tab) = shown {
        Some(tab)
    } else {
        None
    };

    rsx! {
        div { class: "explorer-content",
            // Entity breakdown sidebar
            {entity_sidebar(ctx, shown, european, on_source_click)}

            // The panels below are keyed in one-item lists: a single node's key
            // is ignored, and another encounter has to mount a new panel to fade in
            if matches!(shown, ViewMode::Charts) {
                // Charts view in right column
                for nav in [shown_encounter()] {
                div { key: "{nav:?}", class: "charts-main",
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
                                    key: "{shown_encounter():?}",
                                    encounter_idx: shown_encounter(),
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
                }
            } else if matches!(shown, ViewMode::Rotation) {
                // Rotation view in right column
                for nav in [shown_encounter()] {
                div { key: "{nav:?}", class: "ability-section",
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
                        encounter_idx: shown_encounter(),
                        time_range: time_range(),
                        selected_source: selected_source.read().clone(),
                        selected_anchor: selected_rotation_anchor,
                        on_range_change: move |new_range: TimeRange| {
                            time_range.set(new_range);
                        },
                        european: eu,
                    }
                }
                }
            } else if matches!(shown, ViewMode::Uptime) {
                for nav in [shown_encounter()] {
                div { key: "{nav:?}", class: "ability-section",
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
                        encounter_idx: shown_encounter(),
                        time_range: time_range(),
                        duration_secs: timeline.read().as_ref().map_or(0.0, |t| t.duration_secs),
                        selected_source: selected_source.read().clone(),
                        selected_source_id: *selected_source_id.read(),
                        icons: class_icon_lookup,
                        european: eu,
                    }
                }
                }
            } else if matches!(shown, ViewMode::Usage) {
                // Ability Usage view in right column
                for nav in [shown_encounter()] {
                div { key: "{nav:?}", class: "ability-section",
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
                        selected_encounter: shown_encounter,
                        selected_source: selected_source,
                        time_range: time_range,
                        usage_sort_column: usage_sort_column,
                        usage_sort_direction: usage_sort_direction,
                        selected_abilities: usage_selected_abilities,
                        timeline: timeline,
                        european: eu,
                    }
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
                // Always laid out on this tab (dashes until the numbers are in), so
                // nothing below it moves when they arrive
                if current_tab == DataTab::DamageTaken {
                    {
                        let summary = dt_summary.read();
                        let s = summary.as_ref();
                        rsx! {
                            div { class: "damage-taken-summary",
                                DtSummaryTable {
                                    title: "Damage types",
                                    european: eu,
                                    rows: vec![
                                        ("Internal / Elemental", s.map(|s| s.internal_elemental_pct), s.map(|s| s.internal_elemental_total)),
                                        ("Kinetic / Energy", s.map(|s| s.kinetic_energy_pct), s.map(|s| s.kinetic_energy_total)),
                                        ("Force / Tech", s.map(|s| s.force_tech_pct), s.map(|s| s.force_tech_total)),
                                        ("Melee / Ranged", s.map(|s| s.melee_ranged_pct), s.map(|s| s.melee_ranged_total)),
                                    ],
                                }
                                DtSummaryTable {
                                    title: "Mitigation",
                                    european: eu,
                                    rows: vec![
                                        ("Avoided", s.map(|s| s.avoided_pct), None),
                                        ("Shielded", s.map(|s| s.shielded_pct), None),
                                        ("Absorbed (self)", s.map(|s| s.absorbed_self_pct), s.map(|s| s.absorbed_self_total)),
                                        ("Absorbed (given)", s.map(|s| s.absorbed_given_pct), s.map(|s| s.absorbed_given_total)),
                                    ],
                                }
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
                                // Whose rows these are: clicking the selected name again shows everyone
                                span {
                                    class: "breakdown-label showing-source",
                                    title: "Click a name in the sidebar to show only it; click it again to show everyone listed",
                                    if let Some(name) = selected_source.read().as_ref() {
                                        "Showing "
                                        strong { "{name}" }
                                    } else {
                                        "Showing everyone listed"
                                    }
                                }
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
                {ability_table(ctx, current_tab, european)}
            }
            }
        }
    }
}
