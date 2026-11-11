//! Counter editing tab
//!
//! CRUD for boss counter definitions.
//! Uses CounterDefinition DSL type directly.

use crate::components::editor_sort::{ENCOUNTER_SORT_AZ, SortToggle, apply_rank, use_stable_rank};
use dioxus::prelude::*;

use crate::components::editor_drawer::{EditorDrawer, use_drawer_guard};
use crate::components::editor_common::{ENCOUNTER_ORIGIN, Origin, OriginBadge, OriginFilter, origin_counts};
use crate::api;
use crate::types::{
    BossWithPath, CounterDefinition, EncounterItem, EntityFilter, Trigger,
};

use super::tabs::EncounterData;
use super::InlineNameCreator;
use super::counter_form::CounterEditForm;

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
    // A–Z when toggled; renaming leaves the row in place
    let rank = use_stable_rank(counters.iter().map(|x| (x.id.as_str(), x.name.as_str())), ENCOUNTER_SORT_AZ());
    let builtin_counter_ids = boss_with_path.builtin_counter_ids.clone();
    let modified_counter_ids = boss_with_path.modified_counter_ids.clone();

    let disabled_count = counters.iter().filter(|c| !c.enabled).count();

    // Filter counters based on toggle
    let mut visible_counters: Vec<CounterDefinition> = if hide_disabled_counters() {
        counters.iter().filter(|c| c.enabled).cloned().collect()
    } else {
        counters.clone()
    };
    apply_rank(&mut visible_counters, &rank.read(), |x| &x.id);
    let origin_of = |id: &str| Origin::of(builtin_counter_ids.iter().any(|b| b == id), modified_counter_ids.iter().any(|m| m == id));
    let origin_counts = origin_counts(counters.iter().map(|x| origin_of(&x.id)));
    let origin_filter = ENCOUNTER_ORIGIN();
    visible_counters.retain(|x| origin_filter.is_none_or(|o| origin_of(&x.id) == o));

    rsx! {
        div { class: "counters-tab",
            // Header
            div { class: "flex items-center justify-between mb-sm editor-tab-header",
                div { class: "flex items-center gap-sm",
                    span { class: "text-sm text-secondary", "{counters.len()} counters" }
                    if disabled_count > 0 {
                        label { class: "flex items-center gap-xs text-xs text-muted cursor-pointer",
                            input {
                                r#type: "checkbox",
                                checked: hide_disabled_counters(),
                                onchange: move |e| hide_disabled_counters.set(e.checked()),
                            }
                            "Hide disabled ({disabled_count})"
                        }
                    }
                    SortToggle { az: ENCOUNTER_SORT_AZ.signal() }
                    OriginFilter { selected: origin_filter, counts: origin_counts, kind: "counters", on_pick: move |o| *ENCOUNTER_ORIGIN.write() = o }
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
    let trigger_label = if counter.track_effect_stacks.is_some() {
        "Effect Stacks"
    } else {
        counter.increment_on.label()
    };

    rsx! {
        div { class: "list-item", class: if expanded { "expanded" }, class: if !counter.enabled { "item-disabled" },
            // Header row
            div {
                class: "list-item-header",
                onclick: move |_| on_toggle.call(()),

                span { class: "font-medium", "{counter.name}" }
                if expanded && is_dirty() {
                    span { class: "unsaved-indicator", title: "Unsaved changes" }
                }
                span { class: "text-xs text-mono text-muted", "{counter.id}" }
                span { class: "tag", "{trigger_label}" }
                if counter.decrement_on.is_some() {
                    span { class: "tag tag-info", "↓ Decrement" }
                } else if counter.decrement {
                    span { class: "tag tag-warning", "Decrement" }
                }
                span { class: "ml-auto shrink-0", OriginBadge { origin: Origin::of(is_builtin, is_modified), kind: "counter" } }
            }

            // Expanded content
            if expanded {
                {
                    let all_counters_for_save = all_counters.clone();
                    let bwp_for_save = boss_with_path.clone();
                    let bwp_for_delete = boss_with_path.clone();
                    let is_reset = is_builtin || is_modified;
                    rsx! {
                        EditorDrawer { title: counter.name.clone(), dirty: is_dirty(), on_close: on_collapse,
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
                            }
                        }
                    }
                }
            }
        }
    }
}
