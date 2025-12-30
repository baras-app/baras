//! Timer Editor Panel
//!
//! UI for viewing and editing encounter timers with:
//! - Area index sidebar for lazy loading (grouped by category)
//! - Grouped by boss with collapsible headers (collapsed by default)
//! - Inline expansion for editing
//! - Full CRUD operations with composable trigger editing

use std::collections::HashSet;
use dioxus::prelude::*;

use crate::api;
use crate::types::{AreaListItem, BossListItem, TimerListItem, TimerTrigger};

// ─────────────────────────────────────────────────────────────────────────────
// Main Panel
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn TimerEditorPanel() -> Element {
    // Area index state
    let mut areas = use_signal(Vec::<AreaListItem>::new);
    let mut selected_area = use_signal(|| None::<AreaListItem>);
    let mut loading_areas = use_signal(|| true);

    // Timer state (loaded on area selection)
    let mut timers = use_signal(Vec::<TimerListItem>::new);
    let mut bosses = use_signal(Vec::<BossListItem>::new);
    let mut loading_timers = use_signal(|| false);

    // UI state
    let mut area_filter = use_signal(String::new);
    let mut search_query = use_signal(String::new);
    let mut expanded_timer = use_signal(|| None::<String>);
    let mut expanded_bosses = use_signal(HashSet::<String>::new);
    let mut show_new_timer = use_signal(|| false);
    let mut save_status = use_signal(String::new);
    let mut status_is_error = use_signal(|| false);

    // Load area index on mount
    use_future(move || async move {
        web_sys::console::log_1(&"[TIMER_EDITOR] Loading area index...".into());
        match api::get_area_index().await {
            Some(a) => {
                web_sys::console::log_1(&format!("[TIMER_EDITOR] Got {} areas", a.len()).into());
                areas.set(a);
            }
            None => {
                web_sys::console::log_1(&"[TIMER_EDITOR] get_area_index returned None".into());
            }
        }
        loading_areas.set(false);
    });

    // Load timers when area is selected
    let mut load_area_timers = move |area: AreaListItem| {
        let file_path = area.file_path.clone();
        selected_area.set(Some(area));
        loading_timers.set(true);
        timers.set(Vec::new());
        bosses.set(Vec::new());
        expanded_bosses.set(HashSet::new());
        search_query.set(String::new());

        spawn(async move {
            if let Some(t) = api::get_timers_for_area(&file_path).await {
                timers.set(t);
            }
            if let Some(b) = api::get_bosses_for_area(&file_path).await {
                bosses.set(b);
            }
            loading_timers.set(false);
        });
    };

    // Filter timers based on search query
    let filtered_timers = use_memo(move || {
        let query = search_query().to_lowercase();

        if query.is_empty() {
            return timers();
        }

        timers()
            .into_iter()
            .filter(|t| {
                t.name.to_lowercase().contains(&query)
                    || t.boss_name.to_lowercase().contains(&query)
            })
            .collect::<Vec<_>>()
    });

    // Group filtered timers by boss
    let grouped_timers = use_memo(move || {
        let mut groups: Vec<(String, String, Vec<TimerListItem>)> = Vec::new();

        for timer in filtered_timers() {
            let boss_key = format!("{}_{}", timer.category, timer.boss_id);
            if let Some(group) = groups.iter_mut().find(|(k, _, _)| k == &boss_key) {
                group.2.push(timer);
            } else {
                groups.push((boss_key, timer.boss_name.clone(), vec![timer]));
            }
        }

        groups.sort_by(|a, b| a.1.cmp(&b.1));
        groups
    });

    // Group areas by category (with filtering)
    let grouped_areas = use_memo(move || {
        let filter = area_filter().to_lowercase();
        let mut groups: Vec<(String, Vec<AreaListItem>)> = Vec::new();

        for area in areas() {
            // Filter by name if filter is set
            if !filter.is_empty() && !area.name.to_lowercase().contains(&filter) {
                continue;
            }

            if let Some(group) = groups.iter_mut().find(|(cat, _)| cat == &area.category) {
                group.1.push(area);
            } else {
                groups.push((area.category.clone(), vec![area]));
            }
        }

        // Sort categories: operations first, then flashpoints, then others
        groups.sort_by(|a, b| {
            let order = |s: &str| match s {
                "operations" => 0,
                "flashpoints" => 1,
                "lair_bosses" => 2,
                _ => 3,
            };
            order(&a.0).cmp(&order(&b.0))
        });

        groups
    });

    // Handlers
    let on_save = move |updated_timer: TimerListItem| {
        let mut current = timers();
        if let Some(idx) = current.iter().position(|t| {
            t.timer_id == updated_timer.timer_id && t.boss_id == updated_timer.boss_id
        }) {
            current[idx] = updated_timer.clone();
            timers.set(current);
        }

        spawn(async move {
            if api::update_encounter_timer(&updated_timer).await {
                save_status.set("Saved".to_string());
                status_is_error.set(false);
            } else {
                save_status.set("Failed to save".to_string());
                status_is_error.set(true);
            }
        });
    };

    let mut on_delete = move |timer: TimerListItem| {
        let timer_id = timer.timer_id.clone();
        let boss_id = timer.boss_id.clone();

        let current = timers();
        let filtered: Vec<_> = current
            .into_iter()
            .filter(|t| !(t.timer_id == timer_id && t.boss_id == boss_id))
            .collect();
        timers.set(filtered);
        expanded_timer.set(None);

        spawn(async move {
            if api::delete_encounter_timer(&timer.timer_id, &timer.boss_id, &timer.file_path).await {
                save_status.set("Deleted".to_string());
                status_is_error.set(false);
            } else {
                save_status.set("Failed to delete".to_string());
                status_is_error.set(true);
            }
        });
    };

    let on_duplicate = move |timer: TimerListItem| {
        spawn(async move {
            if let Some(new_timer) =
                api::duplicate_encounter_timer(&timer.timer_id, &timer.boss_id, &timer.file_path).await
            {
                let mut current = timers();
                current.push(new_timer);
                timers.set(current);
                save_status.set("Duplicated".to_string());
                status_is_error.set(false);
            } else {
                save_status.set("Failed to duplicate".to_string());
                status_is_error.set(true);
            }
        });
    };

    let on_create = move |new_timer: TimerListItem| {
        spawn(async move {
            if let Some(created) = api::create_encounter_timer(&new_timer).await {
                let mut current = timers();
                current.push(created);
                timers.set(current);
                save_status.set("Created".to_string());
                status_is_error.set(false);
            } else {
                save_status.set("Failed to create".to_string());
                status_is_error.set(true);
            }
        });
        show_new_timer.set(false);
    };

    rsx! {
        div { class: "timer-editor-panel",
            // Header
            div { class: "timer-editor-header",
                h2 { "Encounter Timers" }
                div { class: "header-right",
                    if !save_status().is_empty() {
                        span {
                            class: if status_is_error() { "save-status error" } else { "save-status" },
                            "{save_status()}"
                        }
                    }
                    if selected_area().is_some() {
                        span { class: "timer-count", "{filtered_timers().len()} timers" }
                        button {
                            class: "btn-new-timer",
                            onclick: move |_| show_new_timer.set(true),
                            "+ New Timer"
                        }
                    }
                }
            }

            div { class: "timer-editor-content",
                // Area sidebar
                div { class: "area-sidebar",
                    // Filter input
                    div { class: "area-filter",
                        input {
                            r#type: "text",
                            placeholder: "Filter areas...",
                            value: "{area_filter}",
                            class: "area-filter-input",
                            oninput: move |e| area_filter.set(e.value())
                        }
                    }

                    if loading_areas() {
                        div { class: "area-loading", "Loading areas..." }
                    } else if grouped_areas().is_empty() && !area_filter().is_empty() {
                        div { class: "area-no-match", "No areas match" }
                    } else {
                        for (category, category_areas) in grouped_areas() {
                            div { class: "area-category",
                                div { class: "area-category-header",
                                    "{format_category(&category)}"
                                }
                                for area in category_areas {
                                    {
                                        let is_selected = selected_area()
                                            .as_ref()
                                            .map(|s| s.file_path == area.file_path)
                                            .unwrap_or(false);
                                        let area_clone = area.clone();

                                        rsx! {
                                            div {
                                                class: if is_selected { "area-item selected" } else { "area-item" },
                                                onclick: move |_| load_area_timers(area_clone.clone()),
                                                span { class: "area-name", "{area.name}" }
                                                span { class: "area-counts",
                                                    "{area.boss_count}b / {area.timer_count}t"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Timer content area
                div { class: "timer-content",
                    if selected_area().is_none() {
                        div { class: "timer-empty-state",
                            div { class: "empty-icon", "📋" }
                            div { class: "empty-text", "Select an area to view timers" }
                        }
                    } else if loading_timers() {
                        div { class: "timer-loading", "Loading timers..." }
                    } else {
                        // Search bar
                        div { class: "timer-search-bar",
                            input {
                                r#type: "text",
                                placeholder: "Search timers...",
                                value: "{search_query}",
                                class: "timer-search-input",
                                oninput: move |e| search_query.set(e.value())
                            }
                        }

                        // New timer form
                        if show_new_timer() {
                            NewTimerForm {
                                bosses: bosses(),
                                on_create: on_create,
                                on_cancel: move |_| show_new_timer.set(false),
                            }
                        }

                        // Timer list grouped by boss
                        if grouped_timers().is_empty() {
                            div { class: "timer-empty",
                                if timers().is_empty() {
                                    "No timers in this area"
                                } else {
                                    "No timers match your search"
                                }
                            }
                        } else {
                            div { class: "timer-list",
                                for (boss_key, boss_name, boss_timers) in grouped_timers() {
                                    {
                                        let is_expanded = expanded_bosses().contains(&boss_key);
                                        let boss_key_toggle = boss_key.clone();
                                        let timer_count = boss_timers.len();

                                        rsx! {
                                            // Boss header
                                            div {
                                                class: "boss-header",
                                                onclick: move |_| {
                                                    let mut set = expanded_bosses();
                                                    if set.contains(&boss_key_toggle) {
                                                        set.remove(&boss_key_toggle);
                                                    } else {
                                                        set.insert(boss_key_toggle.clone());
                                                    }
                                                    expanded_bosses.set(set);
                                                },
                                                span { class: "boss-expand-icon",
                                                    if is_expanded { "▼" } else { "▶" }
                                                }
                                                span { class: "boss-name", "{boss_name}" }
                                                span { class: "boss-timer-count", "({timer_count})" }
                                            }

                                            // Timers (only if expanded)
                                            if is_expanded {
                                                div { class: "boss-timers",
                                                    for timer in boss_timers {
                                                        {
                                                            let timer_key = format!("{}_{}", timer.boss_id, timer.timer_id);
                                                            let is_timer_expanded = expanded_timer() == Some(timer_key.clone());
                                                            let timer_clone = timer.clone();
                                                            let timer_for_delete = timer.clone();
                                                            let timer_for_duplicate = timer.clone();

                                                            rsx! {
                                                                TimerRow {
                                                                    key: "{timer_key}",
                                                                    timer: timer_clone,
                                                                    expanded: is_timer_expanded,
                                                                    on_toggle: move |_| {
                                                                        if is_timer_expanded {
                                                                            expanded_timer.set(None);
                                                                        } else {
                                                                            expanded_timer.set(Some(timer_key.clone()));
                                                                        }
                                                                    },
                                                                    on_save: on_save,
                                                                    on_delete: move |_| on_delete(timer_for_delete.clone()),
                                                                    on_duplicate: move |_| on_duplicate(timer_for_duplicate.clone()),
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
                    }
                }
            }
        }
    }
}

/// Format category name for display
fn format_category(cat: &str) -> &str {
    match cat {
        "operations" => "Operations",
        "flashpoints" => "Flashpoints",
        "lair_bosses" => "Lair Bosses",
        _ => cat,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Timer Row
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn TimerRow(
    timer: TimerListItem,
    expanded: bool,
    on_toggle: EventHandler<()>,
    on_save: EventHandler<TimerListItem>,
    on_delete: EventHandler<()>,
    on_duplicate: EventHandler<()>,
) -> Element {
    let color_hex = format!(
        "#{:02x}{:02x}{:02x}",
        timer.color[0], timer.color[1], timer.color[2]
    );

    rsx! {
        div { class: if expanded { "timer-row expanded" } else { "timer-row" },
            div {
                class: "timer-row-summary",
                onclick: move |_| on_toggle.call(()),

                span { class: "timer-expand-icon",
                    if expanded { "▼" } else { "▶" }
                }

                span {
                    class: "timer-color-dot",
                    style: "background-color: {color_hex}"
                }

                span { class: "timer-name", "{timer.name}" }
                span { class: "timer-id-inline", "{timer.timer_id}" }
                span { class: "timer-trigger-badge", "{timer.trigger.label()}" }
                span { class: "timer-duration", "{timer.duration_secs:.1}s" }

                span {
                    class: if timer.enabled { "timer-status enabled" } else { "timer-status disabled" },
                    if timer.enabled { "✓" } else { "✗" }
                }
            }

            if expanded {
                TimerEditForm {
                    timer: timer.clone(),
                    on_save: on_save,
                    on_delete: on_delete,
                    on_duplicate: on_duplicate,
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Timer Edit Form
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn TimerEditForm(
    timer: TimerListItem,
    on_save: EventHandler<TimerListItem>,
    on_delete: EventHandler<()>,
    on_duplicate: EventHandler<()>,
) -> Element {
    let mut draft = use_signal(|| timer.clone());
    let mut confirm_delete = use_signal(|| false);

    let has_changes = use_memo(move || {
        let d = draft();
        d.name != timer.name
            || d.enabled != timer.enabled
            || d.duration_secs != timer.duration_secs
            || d.color != timer.color
            || d.difficulties != timer.difficulties
            || d.trigger != timer.trigger
    });

    let color_hex = format!(
        "#{:02x}{:02x}{:02x}",
        draft().color[0], draft().color[1], draft().color[2]
    );

    rsx! {
        div { class: "timer-edit-form",
            // Timer ID (read-only)
            div { class: "form-row timer-id-row",
                label { "Timer ID" }
                code { class: "timer-id-display", "{timer.timer_id}" }
                span { class: "timer-id-hint", "(use for chains_to)" }
            }

            // Name
            div { class: "form-row",
                label { "Name" }
                input {
                    r#type: "text",
                    value: "{draft().name}",
                    oninput: move |e| {
                        let mut d = draft();
                        d.name = e.value();
                        draft.set(d);
                    }
                }
            }

            // Duration, Color, Enabled
            div { class: "form-row-inline",
                div { class: "form-field",
                    label { "Duration" }
                    input {
                        r#type: "number",
                        step: "0.1",
                        min: "0",
                        value: "{draft().duration_secs}",
                        oninput: move |e| {
                            if let Ok(val) = e.value().parse::<f32>() {
                                let mut d = draft();
                                d.duration_secs = val;
                                draft.set(d);
                            }
                        }
                    }
                }

                div { class: "form-field",
                    label { "Color" }
                    input {
                        r#type: "color",
                        value: "{color_hex}",
                        class: "color-picker",
                        oninput: move |e| {
                            if let Some(color) = parse_hex_color(&e.value()) {
                                let mut d = draft();
                                d.color = color;
                                draft.set(d);
                            }
                        }
                    }
                }

                div { class: "form-field",
                    label { "Enabled" }
                    input {
                        r#type: "checkbox",
                        checked: draft().enabled,
                        onchange: move |e| {
                            let mut d = draft();
                            d.enabled = e.checked();
                            draft.set(d);
                        }
                    }
                }
            }

            // Trigger editor
            div { class: "form-row trigger-section",
                label { "Trigger" }
                ComposableTriggerEditor {
                    trigger: draft().trigger.clone(),
                    on_change: move |new_trigger| {
                        let mut d = draft();
                        d.trigger = new_trigger;
                        draft.set(d);
                    }
                }
            }

            // Difficulties
            div { class: "form-row",
                label { "Difficulties" }
                div { class: "difficulty-toggles",
                    for diff in ["story", "veteran", "master"] {
                        {
                            let diff_str = diff.to_string();
                            let is_active = draft().difficulties.contains(&diff_str);
                            let diff_clone = diff_str.clone();

                            rsx! {
                                button {
                                    class: if is_active { "diff-btn active" } else { "diff-btn" },
                                    onclick: move |_| {
                                        let mut d = draft();
                                        if d.difficulties.contains(&diff_clone) {
                                            d.difficulties.retain(|x| x != &diff_clone);
                                        } else {
                                            d.difficulties.push(diff_clone.clone());
                                        }
                                        draft.set(d);
                                    },
                                    "{diff}"
                                }
                            }
                        }
                    }
                }
            }

            // Actions
            div { class: "form-actions",
                button {
                    class: "btn-save",
                    disabled: !has_changes(),
                    onclick: move |_| on_save.call(draft()),
                    "Save"
                }

                button {
                    class: "btn-duplicate",
                    onclick: move |_| on_duplicate.call(()),
                    "Duplicate"
                }

                if confirm_delete() {
                    span { class: "delete-confirm",
                        "Delete? "
                        button {
                            class: "btn-delete-yes",
                            onclick: move |_| on_delete.call(()),
                            "Yes"
                        }
                        button {
                            class: "btn-delete-no",
                            onclick: move |_| confirm_delete.set(false),
                            "No"
                        }
                    }
                } else {
                    button {
                        class: "btn-delete",
                        onclick: move |_| confirm_delete.set(true),
                        "Delete"
                    }
                }
            }

            div { class: "timer-file-info", "File: {timer.file_path}" }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Composable Trigger Editor
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn ComposableTriggerEditor(
    trigger: TimerTrigger,
    on_change: EventHandler<TimerTrigger>,
) -> Element {
    rsx! {
        div { class: "composable-trigger-editor",
            TriggerNode {
                trigger: trigger,
                on_change: on_change,
                depth: 0,
            }
        }
    }
}

/// Recursive trigger node
#[component]
fn TriggerNode(
    trigger: TimerTrigger,
    on_change: EventHandler<TimerTrigger>,
    depth: u8,
) -> Element {
    let is_composite = matches!(trigger, TimerTrigger::AllOf { .. } | TimerTrigger::AnyOf { .. });

    let trigger_for_and = trigger.clone();
    let trigger_for_or = trigger.clone();

    rsx! {
        div { class: "trigger-node depth-{depth}",
            if is_composite {
                CompositeEditor {
                    trigger: trigger.clone(),
                    on_change: on_change,
                    depth: depth,
                }
            } else {
                SimpleTriggerEditor {
                    trigger: trigger.clone(),
                    on_change: on_change,
                }
            }

            if depth == 0 && !is_composite {
                div { class: "trigger-compose-actions",
                    button {
                        class: "btn-compose",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_change.call(TimerTrigger::AllOf {
                                conditions: vec![trigger_for_and.clone()]
                            });
                        },
                        "+ AND"
                    }
                    button {
                        class: "btn-compose",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_change.call(TimerTrigger::AnyOf {
                                conditions: vec![trigger_for_or.clone()]
                            });
                        },
                        "+ OR"
                    }
                }
            }
        }
    }
}

/// Editor for composite triggers
#[component]
fn CompositeEditor(
    trigger: TimerTrigger,
    on_change: EventHandler<TimerTrigger>,
    depth: u8,
) -> Element {
    let (is_all_of, conditions) = match &trigger {
        TimerTrigger::AllOf { conditions } => (true, conditions.clone()),
        TimerTrigger::AnyOf { conditions } => (false, conditions.clone()),
        _ => return rsx! { span { "Invalid composite" } },
    };

    let label = if is_all_of { "ALL OF (AND)" } else { "ANY OF (OR)" };

    let conditions_for_toggle = conditions.clone();
    let conditions_for_unwrap = conditions.clone();
    let conditions_for_add = conditions.clone();
    let conditions_len = conditions.len();

    rsx! {
        div { class: "composite-trigger",
            div { class: "composite-header",
                span { class: "composite-label", "{label}" }
                button {
                    class: "btn-toggle-type",
                    onclick: move |_| {
                        let new_trigger = if is_all_of {
                            TimerTrigger::AnyOf { conditions: conditions_for_toggle.clone() }
                        } else {
                            TimerTrigger::AllOf { conditions: conditions_for_toggle.clone() }
                        };
                        on_change.call(new_trigger);
                    },
                    if is_all_of { "→ OR" } else { "→ AND" }
                }
                if conditions_len == 1 {
                    button {
                        class: "btn-unwrap",
                        onclick: move |_| {
                            if let Some(first) = conditions_for_unwrap.first() {
                                on_change.call(first.clone());
                            }
                        },
                        "Unwrap"
                    }
                }
            }

            div { class: "composite-conditions",
                for (idx, condition) in conditions.iter().enumerate() {
                    {
                        let conditions_for_update = conditions.clone();
                        let conditions_for_remove = conditions.clone();
                        let condition_clone = condition.clone();

                        rsx! {
                            div { class: "condition-item",
                                TriggerNode {
                                    trigger: condition_clone,
                                    on_change: move |new_cond| {
                                        let mut new_conditions = conditions_for_update.clone();
                                        new_conditions[idx] = new_cond;
                                        let new_trigger = if is_all_of {
                                            TimerTrigger::AllOf { conditions: new_conditions }
                                        } else {
                                            TimerTrigger::AnyOf { conditions: new_conditions }
                                        };
                                        on_change.call(new_trigger);
                                    },
                                    depth: depth + 1,
                                }
                                if conditions_len > 1 {
                                    button {
                                        class: "btn-remove-condition",
                                        onclick: move |_| {
                                            let mut new_conditions = conditions_for_remove.clone();
                                            new_conditions.remove(idx);
                                            let new_trigger = if is_all_of {
                                                TimerTrigger::AllOf { conditions: new_conditions }
                                            } else {
                                                TimerTrigger::AnyOf { conditions: new_conditions }
                                            };
                                            on_change.call(new_trigger);
                                        },
                                        "×"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            button {
                class: "btn-add-condition",
                onclick: move |_| {
                    let mut new_conditions = conditions_for_add.clone();
                    new_conditions.push(TimerTrigger::CombatStart);
                    let new_trigger = if is_all_of {
                        TimerTrigger::AllOf { conditions: new_conditions }
                    } else {
                        TimerTrigger::AnyOf { conditions: new_conditions }
                    };
                    on_change.call(new_trigger);
                },
                "+ Add Condition"
            }
        }
    }
}

/// Editor for simple triggers
#[component]
fn SimpleTriggerEditor(
    trigger: TimerTrigger,
    on_change: EventHandler<TimerTrigger>,
) -> Element {
    let trigger_type = trigger.type_name();

    rsx! {
        div { class: "simple-trigger-editor",
            select {
                class: "trigger-type-select",
                value: "{trigger_type}",
                onchange: move |e| {
                    let new_trigger = match e.value().as_str() {
                        "combat_start" => TimerTrigger::CombatStart,
                        "ability_cast" => TimerTrigger::AbilityCast { ability_ids: vec![] },
                        "effect_applied" => TimerTrigger::EffectApplied { effect_ids: vec![] },
                        "effect_removed" => TimerTrigger::EffectRemoved { effect_ids: vec![] },
                        "damage_taken" => TimerTrigger::DamageTaken { ability_ids: vec![] },
                        "timer_expires" => TimerTrigger::TimerExpires { timer_id: String::new() },
                        "phase_entered" => TimerTrigger::PhaseEntered { phase_id: String::new() },
                        "boss_hp_below" => TimerTrigger::BossHpBelow { hp_percent: 50.0, npc_id: None, boss_name: None },
                        _ => trigger.clone(),
                    };
                    on_change.call(new_trigger);
                },
                option { value: "combat_start", "Combat Start" }
                option { value: "ability_cast", "Ability Cast" }
                option { value: "effect_applied", "Effect Applied" }
                option { value: "effect_removed", "Effect Removed" }
                option { value: "damage_taken", "Damage Taken" }
                option { value: "timer_expires", "Timer Expires" }
                option { value: "phase_entered", "Phase Entered" }
                option { value: "boss_hp_below", "Boss HP Below" }
            }

            {
                match trigger.clone() {
                    TimerTrigger::CombatStart => rsx! {
                        span { class: "trigger-hint", "Fires when combat begins" }
                    },
                    TimerTrigger::AbilityCast { ability_ids } => rsx! {
                        IdListEditor {
                            label: "Ability IDs",
                            ids: ability_ids,
                            on_change: move |ids| on_change.call(TimerTrigger::AbilityCast { ability_ids: ids })
                        }
                    },
                    TimerTrigger::EffectApplied { effect_ids } => rsx! {
                        IdListEditor {
                            label: "Effect IDs",
                            ids: effect_ids,
                            on_change: move |ids| on_change.call(TimerTrigger::EffectApplied { effect_ids: ids })
                        }
                    },
                    TimerTrigger::EffectRemoved { effect_ids } => rsx! {
                        IdListEditor {
                            label: "Effect IDs",
                            ids: effect_ids,
                            on_change: move |ids| on_change.call(TimerTrigger::EffectRemoved { effect_ids: ids })
                        }
                    },
                    TimerTrigger::DamageTaken { ability_ids } => rsx! {
                        IdListEditor {
                            label: "Ability IDs",
                            ids: ability_ids,
                            on_change: move |ids| on_change.call(TimerTrigger::DamageTaken { ability_ids: ids })
                        }
                    },
                    TimerTrigger::TimerExpires { timer_id } => rsx! {
                        div { class: "trigger-field",
                            label { "Timer ID" }
                            input {
                                r#type: "text",
                                value: "{timer_id}",
                                oninput: move |e| on_change.call(TimerTrigger::TimerExpires { timer_id: e.value() })
                            }
                        }
                    },
                    TimerTrigger::PhaseEntered { phase_id } => rsx! {
                        div { class: "trigger-field",
                            label { "Phase ID" }
                            input {
                                r#type: "text",
                                value: "{phase_id}",
                                oninput: move |e| on_change.call(TimerTrigger::PhaseEntered { phase_id: e.value() })
                            }
                        }
                    },
                    TimerTrigger::BossHpBelow { hp_percent, npc_id, boss_name } => rsx! {
                        div { class: "trigger-field",
                            label { "HP %" }
                            input {
                                r#type: "number",
                                step: "0.1",
                                min: "0",
                                max: "100",
                                value: "{hp_percent}",
                                oninput: move |e| {
                                    if let Ok(val) = e.value().parse::<f32>() {
                                        on_change.call(TimerTrigger::BossHpBelow {
                                            hp_percent: val,
                                            npc_id,
                                            boss_name: boss_name.clone(),
                                        });
                                    }
                                }
                            }
                        }
                    },
                    _ => rsx! { span { "Composite - use editor above" } },
                }
            }
        }
    }
}

/// ID list editor for ability/effect IDs
#[component]
fn IdListEditor(
    label: &'static str,
    ids: Vec<u64>,
    on_change: EventHandler<Vec<u64>>,
) -> Element {
    let mut new_id_input = use_signal(String::new);

    let hint = match label {
        "Ability IDs" => "Find in combat log: AbilityActivate {...guid=\"XXXXXXXX\"...}",
        "Effect IDs" => "Find in combat log: ApplyEffect {...effectGuid=\"XXXXXXXX\"...}",
        _ => "",
    };

    let ids_for_keydown = ids.clone();
    let ids_for_click = ids.clone();

    rsx! {
        div { class: "id-list-editor",
            div { class: "id-label-row",
                span { class: "id-label", "{label}:" }
                if !hint.is_empty() {
                    span { class: "id-hint", title: "{hint}", "?" }
                }
            }
            div { class: "id-chips",
                for (idx, id) in ids.iter().enumerate() {
                    {
                        let ids_clone = ids.clone();
                        rsx! {
                            span { class: "id-chip",
                                "{id}"
                                button {
                                    class: "id-chip-remove",
                                    onclick: move |_| {
                                        let mut new_ids = ids_clone.clone();
                                        new_ids.remove(idx);
                                        on_change.call(new_ids);
                                    },
                                    "×"
                                }
                            }
                        }
                    }
                }
            }
            div { class: "id-add-row",
                input {
                    r#type: "text",
                    class: "id-input",
                    placeholder: "ID (Enter to add)",
                    value: "{new_id_input}",
                    oninput: move |e| new_id_input.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter
                            && let Ok(id) = new_id_input().parse::<u64>() {
                                let mut new_ids = ids_for_keydown.clone();
                                if !new_ids.contains(&id) {
                                    new_ids.push(id);
                                    on_change.call(new_ids);
                                }
                                new_id_input.set(String::new());
                            }
                    }
                }
                button {
                    class: "btn-add-id",
                    onclick: move |_| {
                        if let Ok(id) = new_id_input().parse::<u64>() {
                            let mut new_ids = ids_for_click.clone();
                            if !new_ids.contains(&id) {
                                new_ids.push(id);
                                on_change.call(new_ids);
                            }
                            new_id_input.set(String::new());
                        }
                    },
                    "Add"
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// New Timer Form
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn NewTimerForm(
    bosses: Vec<BossListItem>,
    on_create: EventHandler<TimerListItem>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut selected_boss_id = use_signal(String::new);
    let mut name = use_signal(String::new);
    let mut duration = use_signal(|| 30.0f32);
    let mut color = use_signal(|| [255u8, 128, 0, 255]);
    let mut trigger = use_signal(|| TimerTrigger::CombatStart);
    let mut difficulties = use_signal(|| vec!["story".to_string(), "veteran".to_string(), "master".to_string()]);

    let selected_boss = bosses.iter().find(|b| b.id == selected_boss_id()).cloned();
    let color_hex = format!("#{:02x}{:02x}{:02x}", color()[0], color()[1], color()[2]);

    rsx! {
        div { class: "new-timer-form",
            div { class: "new-timer-header",
                h3 { "New Timer" }
                button {
                    class: "btn-close",
                    onclick: move |_| on_cancel.call(()),
                    "×"
                }
            }

            // Boss selector (simple dropdown since we're already in an area)
            div { class: "form-row",
                label { "Boss" }
                select {
                    class: "boss-select",
                    value: "{selected_boss_id}",
                    onchange: move |e| selected_boss_id.set(e.value()),
                    option { value: "", "Select a boss..." }
                    for boss in &bosses {
                        option { value: "{boss.id}", "{boss.name}" }
                    }
                }
            }

            if let Some(boss) = selected_boss {
                div { class: "form-row",
                    label { "Timer Name" }
                    input {
                        r#type: "text",
                        placeholder: "e.g., Rocket Salvo",
                        value: "{name}",
                        oninput: move |e| name.set(e.value())
                    }
                }

                div { class: "form-row-inline",
                    div { class: "form-field",
                        label { "Duration" }
                        input {
                            r#type: "number",
                            step: "0.1",
                            min: "0",
                            value: "{duration}",
                            oninput: move |e| {
                                if let Ok(val) = e.value().parse::<f32>() {
                                    duration.set(val);
                                }
                            }
                        }
                    }
                    div { class: "form-field",
                        label { "Color" }
                        input {
                            r#type: "color",
                            value: "{color_hex}",
                            class: "color-picker",
                            oninput: move |e| {
                                if let Some(c) = parse_hex_color(&e.value()) {
                                    color.set(c);
                                }
                            }
                        }
                    }
                }

                div { class: "form-row trigger-section",
                    label { "Trigger" }
                    ComposableTriggerEditor {
                        trigger: trigger(),
                        on_change: move |t| trigger.set(t)
                    }
                }

                div { class: "form-row",
                    label { "Difficulties" }
                    div { class: "difficulty-toggles",
                        for diff in ["story", "veteran", "master"] {
                            {
                                let diff_str = diff.to_string();
                                let is_active = difficulties().contains(&diff_str);
                                let diff_clone = diff_str.clone();

                                rsx! {
                                    button {
                                        class: if is_active { "diff-btn active" } else { "diff-btn" },
                                        onclick: move |_| {
                                            let mut d = difficulties();
                                            if d.contains(&diff_clone) {
                                                d.retain(|x| x != &diff_clone);
                                            } else {
                                                d.push(diff_clone.clone());
                                            }
                                            difficulties.set(d);
                                        },
                                        "{diff}"
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "form-actions",
                    button {
                        class: "btn-save",
                        disabled: name().is_empty(),
                        onclick: move |_| {
                            let new_timer = TimerListItem {
                                timer_id: String::new(),
                                boss_id: boss.id.clone(),
                                boss_name: boss.name.clone(),
                                area_name: boss.area_name.clone(),
                                category: boss.category.clone(),
                                file_path: boss.file_path.clone(),
                                name: name(),
                                enabled: true,
                                duration_secs: duration(),
                                color: color(),
                                phases: vec![],
                                difficulties: difficulties(),
                                trigger: trigger(),
                                can_be_refreshed: false,
                                repeats: 0,
                                chains_to: None,
                                alert_at_secs: None,
                                show_on_raid_frames: false,
                            };
                            on_create.call(new_timer);
                        },
                        "Create Timer"
                    }
                    button {
                        class: "btn-cancel",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn parse_hex_color(hex: &str) -> Option<[u8; 4]> {
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some([r, g, b, 255])
    } else {
        None
    }
}
