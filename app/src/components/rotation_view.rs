//! Rotation visualization component.
//!
//! Displays ability rotation cycles split by an anchor ability,
//! with GCD abilities in a horizontal row and off-GCD weaves stacked above.

use dioxus::prelude::*;

use crate::components::ability_naming::AbilityNamingPills;
use crate::api;
use crate::api::{RotationAnalysis, TimeRange};
use crate::components::ability_icon::AbilityIcon;
use crate::components::mirror_names::{display_id, display_name};
use baras_types::formatting;

#[derive(Props, Clone, PartialEq)]
pub struct RotationViewProps {
    pub encounter_idx: Option<u32>,
    pub time_range: TimeRange,
    pub selected_source: Option<String>,
    /// Shared anchor ability signal (persists across encounters)
    pub selected_anchor: Signal<Option<i64>>,
    /// Optional callback to update the parent's time range (e.g. from context menu)
    #[props(default)]
    pub on_range_change: Option<EventHandler<TimeRange>>,
    /// European number format (swaps `.` and `,`)
    #[props(default)]
    pub european: bool,
}

#[component]
pub fn RotationView(props: RotationViewProps) -> Element {
    let eu = props.european;
    let format_number = move |value: f64| formatting::format_compact_f64(value, eu);
    let format_pct = move |count: i64, total: i64| formatting::format_pct_ratio(count, total, eu);

    let mut available_abilities = use_signal(|| Vec::<(i64, String)>::new());
    let mut selected_anchor = props.selected_anchor;
    let mut rotation = use_signal(|| None::<RotationAnalysis>);
    let mut loading = use_signal(|| false);

    // Context menu state for right-click "Set as range start/end"
    let mut context_menu_pos = use_signal(|| None::<(f64, f64)>);
    let mut context_menu_time = use_signal(|| 0.0f32);

    // Track source in a signal so effects can react to changes
    let mut tracked_source = use_signal(|| props.selected_source.clone());
    if *tracked_source.read() != props.selected_source {
        tracked_source.set(props.selected_source.clone());
    }

    // Track time_range so effects react to phase/time filter changes
    let mut tracked_time_range = use_signal(|| props.time_range.clone());
    if *tracked_time_range.read() != props.time_range {
        tracked_time_range.set(props.time_range.clone());
    }

    // Flag: user has clicked Create (reset on source/anchor change)
    // Initialize as active if we have a held anchor (re-mount with persisted state)
    let mut rotation_active = use_signal(|| selected_anchor.peek().is_some());

    // Track previous source to detect actual player changes vs. re-mounts
    let mut prev_source = use_signal(|| props.selected_source.clone());

    let enc_idx = props.encounter_idx;

    // Load available abilities when source changes, validate held anchor
    use_effect(move || {
        let source = tracked_source.read().clone();
        let prev = prev_source.peek().clone();
        let is_source_change = source != prev;

        if is_source_change {
            prev_source.set(source.clone());
            rotation.set(None);
            rotation_active.set(false);
        }

        let Some(source_name) = source else {
            available_abilities.set(Vec::new());
            selected_anchor.set(None);
            return;
        };

        spawn(async move {
            // Fetch with a dummy anchor to get the abilities list
            let result = api::query_rotation(enc_idx, &source_name, 0, None).await;
            if let Some(analysis) = result {
                // Validate held anchor exists in this player's abilities
                let held = *selected_anchor.peek();
                if let Some(anchor_id) = held {
                    if analysis.abilities.iter().any(|(id, _)| *id == anchor_id) {
                        // Anchor is valid — auto-activate if we had a persisted anchor
                        if !is_source_change {
                            rotation_active.set(true);
                        }
                    } else {
                        selected_anchor.set(None);
                    }
                }
                available_abilities.set(analysis.abilities);
            }
        });
    });

    // Query rotation when Create is clicked (rotation_active) or time_range changes
    use_effect(move || {
        let tr = tracked_time_range();
        let active = rotation_active();

        if !active {
            return;
        }
        let Some(anchor_id) = *selected_anchor.peek() else {
            return;
        };
        let Some(ref source_name) = *tracked_source.peek() else {
            return;
        };
        let source_name = source_name.clone();

        let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
            None
        } else {
            Some(tr)
        };

        loading.set(true);
        spawn(async move {
            let result =
                api::query_rotation(enc_idx, &source_name, anchor_id, tr_opt.as_ref()).await;
            rotation.set(result);
            loading.set(false);
        });
    });

    let abilities: Vec<(i64, String)> = available_abilities
        .read()
        .iter()
        .map(|(id, name)| (*id, display_name(*id, name).into_owned()))
        .collect();
    let source = props.selected_source.clone();

    rsx! {
        div { class: "rotation-view",
            // Controls row
            div { class: "rotation-controls",
                label { "Create Rotation Visualisation:" }
                select {
                    class: "rotation-anchor-select",
                    value: selected_anchor().map(|id| id.to_string()).unwrap_or_default(),
                    onchange: move |evt: Event<FormData>| {
                        let val = evt.value();
                        selected_anchor.set(val.parse::<i64>().ok());
                        rotation.set(None);
                        rotation_active.set(false);
                    },
                    option { value: "", "-- Select Ability --" }
                    for (id, name) in &abilities {
                        option {
                            key: "{id}",
                            value: "{id}",
                            "{name}"
                        }
                    }
                }
                button {
                    class: "btn btn-primary",
                    disabled: selected_anchor().is_none() || source.is_none() || loading(),
                    onclick: move |_| {
                        rotation_active.set(true);
                    },
                    if loading() { "Loading..." } else { "Create" }
                }
                AbilityNamingPills {}
            }

            if source.is_none() {
                div { class: "rotation-placeholder",
                    "Select a player from the sidebar to view their rotation."
                }
            }

            // Rotation cycles
            if let Some(ref analysis) = rotation() {
                if analysis.cycles.is_empty() {
                    div { class: "rotation-placeholder",
                        "No rotation data found for the selected anchor ability."
                    }
                } else {
                    div { class: "rotation-cycles",
                        for (i, cycle) in analysis.cycles.iter().enumerate() {
                            div { class: "rotation-cycle",
                                key: "{i}",
                                // Per-cycle stats
                                div { class: "rotation-cycle-stats",
                                    span { class: "cycle-stat cycle-duration",
                                        "{formatting::format_decimal(cycle.duration_secs, 1, eu)}s"
                                    }
                                    if cycle.total_damage > 0.0 && cycle.duration_secs > 0.0 {
                                        span { class: "cycle-stat dps",
                                            span { class: "cycle-stat-label", "DPS " }
                                            "{format_number(cycle.total_damage / cycle.duration_secs as f64)}"
                                        }
                                    }
                                    if cycle.effective_heal > 0.0 && cycle.duration_secs > 0.0 {
                                        span { class: "cycle-stat hps",
                                            span { class: "cycle-stat-label", "EHPS " }
                                            "{format_number(cycle.effective_heal / cycle.duration_secs as f64)}"
                                        }
                                    }
                                    if cycle.hit_count > 0 {
                                        span { class: "cycle-stat crit",
                                            span { class: "cycle-stat-label", "Crit " }
                                            "{format_pct(cycle.crit_count, cycle.hit_count)}"
                                        }
                                    }
                                }
                                div { class: "rotation-slots",
                                    for (j, slot) in cycle.slots.iter().enumerate() {
                                        {
                                        let gcd_time = slot.gcd_ability.time_secs;
                                        let gcd_name = display_name(slot.gcd_ability.ability_id, &slot.gcd_ability.ability_name).into_owned();
                                        rsx! {
                                        div { class: "gcd-slot", key: "{j}",
                                            // Off-GCD weaves stacked above (reversed: last weave nearest GCD)
                                            for (k, weave) in slot.off_gcd.iter().rev().enumerate() {
                                                {
                                                let weave_time = weave.time_secs;
                                                let weave_name = display_name(weave.ability_id, &weave.ability_name).into_owned();
                                                rsx! {
                                                div { title: "{weave_name}",
                                                    oncontextmenu: move |e: MouseEvent| {
                                                        if props.on_range_change.is_some() {
                                                            e.prevent_default();
                                                            context_menu_pos.set(Some((e.client_coordinates().x, e.client_coordinates().y)));
                                                            context_menu_time.set(weave_time);
                                                        }
                                                    },
                                                    AbilityIcon {
                                                        key: "w{k}",
                                                        ability_id: display_id(weave.ability_id),
                                                        size: 28,
                                                        fallback: weave_name.clone(),
                                                    }
                                                }
                                                }
                                                }
                                            }
                                            // GCD ability on bottom
                                            div { title: "{gcd_name}",
                                                oncontextmenu: move |e: MouseEvent| {
                                                    if props.on_range_change.is_some() {
                                                        e.prevent_default();
                                                        context_menu_pos.set(Some((e.client_coordinates().x, e.client_coordinates().y)));
                                                        context_menu_time.set(gcd_time);
                                                    }
                                                },
                                                AbilityIcon {
                                                    ability_id: display_id(slot.gcd_ability.ability_id),
                                                    size: 40,
                                                    fallback: gcd_name.clone(),
                                                }
                                            }
                                            // GCD gap timing
                                            match slot.gcd_gap {
                                                Some(gap) => rsx! { span { class: "gcd-gap-time", "{formatting::format_decimal(gap, 3, eu)}" } },
                                                None => rsx! { span { class: "gcd-gap-time", visibility: "hidden", "0.000" } },
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

            // Context menu for "Set as range start/end"
            if let Some((x, y)) = *context_menu_pos.read() {
                if props.on_range_change.is_some() {
                    div {
                        style: "position: fixed; top: 0; left: 0; width: 100%; height: 100%; z-index: 999;",
                        onclick: move |_| context_menu_pos.set(None),
                    }
                    div {
                        class: "log-context-menu",
                        style: "position: fixed; left: {x}px; top: {y}px; z-index: 1000;",
                        div {
                            class: "context-menu-item",
                            onclick: move |_| {
                                let t = *context_menu_time.read();
                                if let Some(ref handler) = props.on_range_change {
                                    handler.call(TimeRange::new(t, props.time_range.end));
                                }
                                context_menu_pos.set(None);
                            },
                            "Set as range start"
                        }
                        div {
                            class: "context-menu-item",
                            onclick: move |_| {
                                let t = *context_menu_time.read();
                                if let Some(ref handler) = props.on_range_change {
                                    handler.call(TimeRange::new(props.time_range.start, t));
                                }
                                context_menu_pos.set(None);
                            },
                            "Set as range end"
                        }
                    }
                }
            }
        }
    }
}


