//! Timer editing tab
//!
//! Full CRUD for boss timers with all fields exposed.
//! Uses BossTimerDefinition DSL type directly.

use dioxus::prelude::*;

use crate::api;
use crate::components::def_list::{ColSort, HideDisabled, ListCount, ListHead, sort_by_column};
use crate::components::editor_common::{ENCOUNTER_ORIGIN, Origin, OriginFilter, origin_counts};
use crate::components::editor_drawer::use_drawer_guard;
use crate::types::{
    AlertTrigger, AudioConfig, BossTimerDefinition, BossWithPath, EncounterItem, TimerDisplayTarget, Trigger,
};

use super::InlineNameCreator;
use super::tabs::EncounterData;
use super::timer_row::{self, TimerRow};

// ─────────────────────────────────────────────────────────────────────────────
// Timers Tab
// ─────────────────────────────────────────────────────────────────────────────

/// Create a default timer definition with sensible defaults
fn default_timer(name: String) -> BossTimerDefinition {
    BossTimerDefinition {
        id: String::new(), // Backend generates from name
        name,
        display_text: None,
        trigger: Trigger::CombatStart,
        duration_secs: 30.0,
        is_alert: false,
        alert_on: AlertTrigger::default(),
        alert_text: None,
        alert_countdown_secs: None,
        color: [255, 128, 0, 255], // Orange
        icon_ability_id: None,
        conditions: vec![],
        phases: vec![],
        counter_condition: None,
        difficulties: vec!["story".to_string(), "veteran".to_string(), "master".to_string()],
        group_size: None,
        enabled: true,
        can_be_refreshed: false,
        repeats: 0,
        chains_to: None,
        cancel_trigger: None,
        alert_at_secs: None,
        show_on_raid_frames: false,
        show_at_secs: 0.0,
        display_targets: vec![TimerDisplayTarget::TimersA],
        audio: AudioConfig::default(),
        roles: vec!["Tank".into(), "Healer".into(), "Dps".into()],
        gcd_secs: None,
        queue_on_expire: false,
        queue_priority: 0,
        queue_remove_trigger: None,
        queue_blocking_timers: Vec::new(),
        queue_blocking_condition: None,
        queue_next_audio: None,
        queue_countdown_bar: false,
        queue_hide_from_next: false,
    }
}

/// Column the timers list is sorted by (session-wide)
static TIMER_SORT: GlobalSignal<ColSort> = Signal::global(|| None);

#[component]
pub fn TimersTab(
    boss_with_path: BossWithPath,
    encounter_data: EncounterData,
    expanded_timer: Signal<Option<String>>,
    hide_disabled_timers: Signal<bool>,
    on_change: EventHandler<Vec<BossTimerDefinition>>,
    on_refetch: EventHandler<()>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let guard = use_drawer_guard();

    let timers = boss_with_path.boss.timers.clone();
    let builtin_timer_ids = boss_with_path.builtin_timer_ids.clone();
    let modified_timer_ids = boss_with_path.modified_timer_ids.clone();
    let disabled_count = timers.iter().filter(|t| !t.enabled).count();

    let origin_of = |id: &str| {
        Origin::of(
            builtin_timer_ids.iter().any(|b| b == id),
            modified_timer_ids.iter().any(|m| m == id),
        )
    };
    let origin_counts = origin_counts(timers.iter().map(|x| origin_of(&x.id)));
    let origin_filter = ENCOUNTER_ORIGIN();
    let mut visible_timers: Vec<BossTimerDefinition> = timers
        .iter()
        .filter(|t| !(hide_disabled_timers() && !t.enabled))
        .filter(|t| origin_filter.is_none_or(|o| origin_of(&t.id) == o))
        .cloned()
        .collect();
    sort_by_column(&mut visible_timers, TIMER_SORT(), timer_row::sort_text);

    let all_hidden = timers.iter().all(|t| t.roles.is_empty());
    let (bwp_bulk, timers_for_bulk) = (boss_with_path.clone(), timers.clone());
    let (bwp_create, timers_for_create) = (boss_with_path.clone(), timers.clone());

    rsx! {
        div { class: "def-tab",
            div { class: "flex items-center justify-between mb-sm editor-tab-header",
                div { class: "flex items-center gap-sm",
                    ListCount { shown: visible_timers.len(), total: timers.len(), noun: "timers" }
                    OriginFilter { selected: origin_filter, counts: origin_counts, kind: "timers", on_pick: move |o| *ENCOUNTER_ORIGIN.write() = o }
                    HideDisabled { hidden: hide_disabled_timers, count: disabled_count, noun: "timers" }
                }
                div { class: "flex items-center gap-sm",
                    if !timers.is_empty() {
                        button {
                            class: "btn btn-sm",
                            title: if all_hidden { "Show every timer of this boss for all roles" } else { "Hide every timer of this boss for all roles" },
                            onclick: move |_| {
                                let new_roles: Vec<String> = if all_hidden {
                                    vec!["Tank".into(), "Healer".into(), "Dps".into()]
                                } else {
                                    vec![]
                                };
                                let mut updated = timers_for_bulk.clone();
                                for t in &mut updated {
                                    t.roles = new_roles.clone();
                                }
                                on_change.call(updated);
                                let boss_id = bwp_bulk.boss.id.clone();
                                let file_path = bwp_bulk.file_path.clone();
                                spawn(async move {
                                    if let Err(e) = api::set_all_timer_roles(&boss_id, &file_path, &new_roles).await {
                                        on_status.call((format!("Failed to save: {e}"), true));
                                    }
                                    on_refetch.call(());
                                });
                            },
                            i { class: if all_hidden { "fa-solid fa-eye" } else { "fa-solid fa-eye-slash" } }
                            if all_hidden { " Show all" } else { " Hide all" }
                        }
                    }
                    InlineNameCreator {
                        button_label: "+ New Timer",
                        placeholder: "Timer name...",
                        on_create: move |name: String| {
                            let timers_clone = timers_for_create.clone();
                            let boss_id = bwp_create.boss.id.clone();
                            let file_path = bwp_create.file_path.clone();
                            let item = EncounterItem::Timer(default_timer(name));
                            spawn(async move {
                                match api::create_encounter_item(&boss_id, &file_path, &item).await {
                                    Ok(EncounterItem::Timer(created)) => {
                                        let created_id = created.id.clone();
                                        let mut current = timers_clone;
                                        current.push(created);
                                        on_change.call(current);
                                        expanded_timer.set(Some(created_id));
                                        on_status.call(("Created".to_string(), false));
                                    }
                                    Ok(_) => on_status.call(("Unexpected response type".to_string(), true)),
                                    Err(e) => on_status.call((e, true)),
                                }
                            });
                        }
                    }
                }
            }

            if visible_timers.is_empty() {
                div { class: "empty-state text-sm",
                    if timers.is_empty() { "No timers defined" } else { "No timers match the filters" }
                }
            } else {
                // The column names sit outside the scrolling rows
                div { class: "def-table",
                    ListHead { cols: "timer-cols", columns: &timer_row::COLUMNS, sort: TIMER_SORT.signal() }
                    div { class: "def-list",
                        for timer in visible_timers {
                            {
                                let timer_key = timer.id.clone();
                                let is_expanded = expanded_timer() == Some(timer_key.clone());
                                rsx! {
                                    TimerRow {
                                        key: "{timer_key}",
                                        is_builtin: builtin_timer_ids.contains(&timer.id),
                                        is_modified: modified_timer_ids.contains(&timer.id),
                                        timer,
                                        all_timers: timers.clone(),
                                        boss_with_path: boss_with_path.clone(),
                                        encounter_data: encounter_data.clone(),
                                        expanded: is_expanded,
                                        on_toggle: move |_| {
                                            guard.switch(expanded_timer, if is_expanded { None } else { Some(timer_key.clone()) });
                                        },
                                        on_change,
                                        on_refetch,
                                        on_status,
                                        on_collapse: move |_| expanded_timer.set(None),
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

// ─────────────────────────────────────────────────────────────────────────────
// Phase Selector (multi-select dropdown)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn PhaseSelector(
    selected: Vec<String>,
    /// (id, label) per phase
    available: Vec<(String, String)>,
    on_change: EventHandler<Vec<String>>,
) -> Element {
    let mut dropdown_open = use_signal(|| false);
    let mut dropdown_pos = use_signal(|| (0.0f64, 0.0f64));

    // Display text
    let display = if selected.is_empty() {
        "(all phases)".to_string()
    } else if let [only] = selected.as_slice() {
        available
            .iter()
            .find(|(id, _)| id == only)
            .map_or_else(|| only.clone(), |(_, label)| label.clone())
    } else {
        format!("{} phases", selected.len())
    };

    rsx! {
        div {
            class: "phase-selector",
            // Dropdown trigger
            button {
                class: "select-trigger min-w-md",
                onclick: move |e| {
                    if !dropdown_open() {
                        // Use element_coordinates to find offset within button,
                        // then subtract from client_coordinates to get button origin
                        let click = e.client_coordinates();
                        let offset = e.element_coordinates();
                        let btn_left = click.x - offset.x;
                        let btn_bottom = click.y - offset.y + 30.0;
                        dropdown_pos.set((btn_left, btn_bottom));
                    }
                    dropdown_open.set(!dropdown_open());
                },
                "{display}"
            }

            // Dropdown menu (fixed position to escape overflow clipping)
            if dropdown_open() {
                div {
                    class: "floating-menu",
                    style: "left: {dropdown_pos().0}px; top: {dropdown_pos().1}px;",

                    if available.is_empty() {
                        span { class: "text-muted text-sm", "No phases defined" }
                    } else {
                        // "All" option (clears selection)
                        label { class: "flex items-center gap-xs text-sm p-xs cursor-pointer",
                            input {
                                r#type: "checkbox",
                                checked: selected.is_empty(),
                                onchange: move |_| {
                                    on_change.call(vec![]);
                                    dropdown_open.set(false);
                                }
                            }
                            "(all phases)"
                        }

                        // Individual phases
                        for (phase_id, phase) in available.iter() {
                            {
                                let phase_id = phase_id.clone();
                                let is_selected = selected.contains(&phase_id);
                                let selected_clone = selected.clone();

                                rsx! {
                                    label { class: "flex items-center gap-xs text-sm p-xs cursor-pointer",
                                        input {
                                            r#type: "checkbox",
                                            checked: is_selected,
                                            onchange: move |_| {
                                                let mut new_selected = selected_clone.clone();
                                                if is_selected {
                                                    new_selected.retain(|p| p != &phase_id);
                                                } else {
                                                    new_selected.push(phase_id.clone());
                                                }
                                                on_change.call(new_selected);
                                            }
                                        }
                                        "{phase}"
                                    }
                                }
                            }
                        }
                    }

                    // Close button
                    button {
                        class: "btn btn-sm w-full mt-xs",
                        onclick: move |_| dropdown_open.set(false),
                        "Done"
                    }
                }
            }
        }
    }
}
