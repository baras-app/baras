//! Counter editing tab
//!
//! CRUD for boss counter definitions.
//! Uses CounterDefinition DSL type directly.

use dioxus::prelude::*;

use crate::api;
use crate::components::def_list::{ColSort, Column, HideDisabled, ListCount, ListHead, NoteMark, sort_by_column};
use crate::components::editor_common::{ENCOUNTER_ORIGIN, Origin, OriginBadge, OriginFilter, origin_counts};
use crate::components::editor_drawer::use_drawer_guard;
use crate::types::{BossWithPath, CounterDefinition, EncounterItem, EntityFilter, Trigger};

use super::InlineNameCreator;
use super::counter_form::CounterEditForm;
use super::tabs::EncounterData;

// ─────────────────────────────────────────────────────────────────────────────
// Counters Tab
// ─────────────────────────────────────────────────────────────────────────────

/// Create a default counter definition
fn default_counter(name: String) -> CounterDefinition {
    CounterDefinition {
        id: String::new(), // Backend will generate
        name,
        enabled: true,
        display_text: None,
        note: None,
        increment_on: Trigger::AbilityCast {
            abilities: vec![],
            source: EntityFilter::default(),
            target: EntityFilter::default(),
            position: vec![],
        },
        decrement_on: None,
        reset_on: Trigger::CombatEnd,
        initial_value: 0,
        decrement: false,
        set_value: None,
        track_effect_stacks: None,
    }
}

/// Column names, in the order of the rows' grid
const COLUMNS: [Column; 5] = [
    Column::new("Name", "", "Sort by name"),
    Column::new("Counts on", "col-dim", "What raises the counter"),
    Column::new("Decrements on", "col-dim col-drop-1", "What lowers the counter"),
    Column::new(
        "Resets on",
        "col-dim col-drop-2",
        "What returns the counter to its initial value",
    ),
    Column::BLANK,
];

/// What raises the counter
fn counts_on(c: &CounterDefinition) -> &'static str {
    if c.track_effect_stacks.is_some() {
        "Effect Stacks"
    } else {
        c.increment_on.label()
    }
}

/// What lowers the counter, if anything does
fn decrements_on(c: &CounterDefinition) -> Option<&'static str> {
    match &c.decrement_on {
        Some(t) => Some(t.label()),
        // Deprecated flag: the "counts on" trigger lowers it instead
        None => c.decrement.then_some("Counts down"),
    }
}

fn sort_text(col: usize, c: &CounterDefinition) -> String {
    match col {
        1 => counts_on(c).to_string(),
        2 => decrements_on(c).unwrap_or_default().to_string(),
        3 => c.reset_on.label().to_string(),
        _ => c.name.clone(),
    }
}

/// Column the counters list is sorted by (session-wide)
static COUNTER_SORT: GlobalSignal<ColSort> = Signal::global(|| None);

#[component]
pub fn CountersTab(
    boss_with_path: BossWithPath,
    encounter_data: EncounterData,
    expanded_counter: Signal<Option<String>>,
    hide_disabled_counters: Signal<bool>,
    on_change: EventHandler<Vec<CounterDefinition>>,
    on_refetch: EventHandler<()>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let guard = use_drawer_guard();
    // Extract counters and badge IDs from BossWithPath
    let counters = boss_with_path.boss.counters.clone();
    let builtin_counter_ids = boss_with_path.builtin_counter_ids.clone();
    let modified_counter_ids = boss_with_path.modified_counter_ids.clone();

    let disabled_count = counters.iter().filter(|c| !c.enabled).count();

    // Filter counters based on toggle
    let mut visible_counters: Vec<CounterDefinition> = if hide_disabled_counters() {
        counters.iter().filter(|c| c.enabled).cloned().collect()
    } else {
        counters.clone()
    };
    let origin_of = |id: &str| {
        Origin::of(
            builtin_counter_ids.iter().any(|b| b == id),
            modified_counter_ids.iter().any(|m| m == id),
        )
    };
    let origin_counts = origin_counts(counters.iter().map(|x| origin_of(&x.id)));
    let origin_filter = ENCOUNTER_ORIGIN();
    visible_counters.retain(|x| origin_filter.is_none_or(|o| origin_of(&x.id) == o));
    sort_by_column(&mut visible_counters, COUNTER_SORT(), sort_text);

    rsx! {
        div { class: "def-tab",
            // Header
            div { class: "flex items-center justify-between mb-sm editor-tab-header",
                div { class: "flex items-center gap-sm",
                    ListCount { shown: visible_counters.len(), total: counters.len(), noun: "counters" }
                    OriginFilter { selected: origin_filter, counts: origin_counts, kind: "counters", on_pick: move |o| *ENCOUNTER_ORIGIN.write() = o }
                    HideDisabled { hidden: hide_disabled_counters, count: disabled_count, noun: "counters" }
                }
                {
                    let bwp = boss_with_path.clone();
                    let counters_for_create = counters.clone();
                    rsx! {
                        InlineNameCreator {
                            button_label: "+ New Counter",
                            placeholder: "Counter name...",
                            on_create: move |name: String| {
                                let counters_clone = counters_for_create.clone();
                                let boss_id = bwp.boss.id.clone();
                                let file_path = bwp.file_path.clone();
                                let counter = default_counter(name);
                                let item = EncounterItem::Counter(counter);
                                spawn(async move {
                                    match api::create_encounter_item(&boss_id, &file_path, &item).await {
                                        Ok(EncounterItem::Counter(created)) => {
                                            let created_id = created.id.clone();
                                            let mut current = counters_clone;
                                            current.push(created);
                                            on_change.call(current);
                                            expanded_counter.set(Some(created_id));
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
            }

            // Counter list
            if visible_counters.is_empty() {
                if counters.is_empty() {
                    div { class: "empty-state text-sm", "No counters defined" }
                } else {
                    div { class: "empty-state text-sm", "No counters match the filters" }
                }
            } else {
                // The column names sit outside the scrolling rows
                div { class: "def-table",
                    ListHead { cols: "counter-cols", columns: &COLUMNS, sort: COUNTER_SORT.signal() }
                    div { class: "def-list",
                        for counter in visible_counters {
                            {
                                let counter_key = counter.id.clone();
                                let is_expanded = expanded_counter() == Some(counter_key.clone());
                                let counters_for_row = counters.clone();
                                let counter_is_builtin = builtin_counter_ids.contains(&counter.id);
                                let counter_is_modified = modified_counter_ids.contains(&counter.id);

                                rsx! {
                                    CounterRow {
                                        key: "{counter_key}",
                                        counter: counter.clone(),
                                        is_builtin: counter_is_builtin,
                                        is_modified: counter_is_modified,
                                        boss_with_path: boss_with_path.clone(),
                                        expanded: is_expanded,
                                        encounter_data: encounter_data.clone(),
                                        on_toggle: move |_| {
                                            guard.switch(expanded_counter, if is_expanded { None } else { Some(counter_key.clone()) });
                                        },
                                        on_change: on_change,
                                        on_refetch: on_refetch,
                                        on_status: on_status,
                                        on_collapse: move |_| expanded_counter.set(None),
                                        all_counters: counters_for_row,
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
// Counter Row
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn CounterRow(
    counter: CounterDefinition,
    is_builtin: bool,
    is_modified: bool,
    boss_with_path: BossWithPath,
    expanded: bool,
    all_counters: Vec<CounterDefinition>,
    encounter_data: EncounterData,
    on_toggle: EventHandler<()>,
    on_change: EventHandler<Vec<CounterDefinition>>,
    on_refetch: EventHandler<()>,
    on_status: EventHandler<(String, bool)>,
    on_collapse: EventHandler<()>,
) -> Element {
    let mut is_dirty = use_signal(|| false);

    rsx! {
        div { class: "list-item", class: if expanded { "expanded" }, class: if !counter.enabled { "item-disabled" },
            // Header row
            div {
                class: "list-item-header def-row counter-cols",
                onclick: move |_| on_toggle.call(()),

                span { class: "col-name", title: "{counter.id}",
                    span { class: "def-name truncate", "{counter.name}" }
                    NoteMark { note: counter.note.clone() }
                    if expanded && is_dirty() {
                        span { class: "unsaved-indicator", title: "Unsaved changes" }
                    }
                }
                span { class: "col-dim truncate", "{counts_on(&counter)}" }
                span { class: "col-dim col-drop-1 truncate",
                    if let Some(down) = decrements_on(&counter) {
                        "{down}"
                    } else {
                        span { class: "text-muted", "—" }
                    }
                }
                span { class: "col-dim col-drop-2 truncate", "{counter.reset_on.label()}" }
                OriginBadge { origin: Origin::of(is_builtin, is_modified), kind: "counter" }
            }

            // Expanded content
            if expanded {
                {
                    let all_counters_for_save = all_counters.clone();
                    let bwp_for_save = boss_with_path.clone();
                    let bwp_for_delete = boss_with_path.clone();
                    let is_reset = is_builtin || is_modified;
                    rsx! {
                        CounterEditForm {
                            counter: counter.clone(),
                            is_builtin: is_builtin,
                            is_modified: is_modified,
                            encounter_data: encounter_data,
                            on_dirty: move |dirty: bool| is_dirty.set(dirty),
                            on_save: move |updated: CounterDefinition| {
                                // Update parent state synchronously so props refresh and dirty indicator clears
                                let mut current = all_counters_for_save.clone();
                                if let Some(idx) = current.iter().position(|c| c.id == updated.id) {
                                    current[idx] = updated.clone();
                                    on_change.call(current);
                                }
                                on_status.call(("Saving...".to_string(), false));
                                let boss_id = bwp_for_save.boss.id.clone();
                                let file_path = bwp_for_save.file_path.clone();
                                let item = EncounterItem::Counter(updated);
                                spawn(async move {
                                    match api::update_encounter_item(&boss_id, &file_path, &item, None).await {
                                        Ok(_) => {
                                            on_status.call(("Saved".to_string(), false));
                                            on_refetch.call(());
                                        }
                                        Err(_) => on_status.call(("Failed to save".to_string(), true)),
                                    }
                                });
                            },
                            on_delete: {
                                let all_counters = all_counters.clone();
                                move |counter_to_delete: CounterDefinition| {
                                    let all_counters = all_counters.clone();
                                    let boss_id = bwp_for_delete.boss.id.clone();
                                    let file_path = bwp_for_delete.file_path.clone();
                                    spawn(async move {
                                        match api::delete_encounter_item("counter", &counter_to_delete.id, &boss_id, &file_path).await {
                                            Ok(_) => {
                                                if is_reset {
                                                    on_status.call(("Reset to built-in".to_string(), false));
                                                    on_collapse.call(());
                                                    on_refetch.call(());
                                                } else {
                                                    let updated: Vec<_> = all_counters.iter()
                                                        .filter(|c| c.id != counter_to_delete.id)
                                                        .cloned()
                                                        .collect();
                                                    on_change.call(updated);
                                                    on_collapse.call(());
                                                    on_status.call(("Deleted".to_string(), false));
                                                }
                                            }
                                            Err(err) => {
                                                on_status.call((err, true));
                                            }
                                        }
                                    });
                                }
                            },
                            on_close: on_collapse,
                        }
                    }
                }
            }
        }
    }
}
