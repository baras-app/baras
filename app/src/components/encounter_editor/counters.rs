//! Counter editing tab
//!
//! CRUD for boss counter definitions.
//! Uses CounterDefinition DSL type directly.

use crate::components::editor_sort::{ENCOUNTER_SORT_AZ, SortToggle, apply_rank, use_stable_rank};
use dioxus::prelude::*;

use crate::components::editor_drawer::{EditorDrawer, use_drawer_guard};
use crate::components::editor_common::{ENCOUNTER_ORIGIN, FormCard, ItemActions, Origin, OriginBadge, OriginFilter, origin_counts};
use crate::api;
use crate::types::{
    BossWithPath, CounterDefinition, EffectSelector, EffectStackConfig, EncounterItem,
    EntityFilter, StackAggregation, Trigger,
};

use super::tabs::EncounterData;
use super::triggers::{ComposableTriggerEditor, EffectSelectorEditor, EntityFilterDropdown};
use super::InlineNameCreator;

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
            div { class: "flex items-center justify-between mb-sm",
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


                OriginBadge { origin: Origin::of(is_builtin, is_modified), kind: "counter" }

                span { class: "font-medium", "{counter.name}" }
                if expanded && is_dirty() {
                    span { class: "unsaved-indicator", title: "Unsaved changes" }
                }
                span { class: "tag", "{trigger_label}" }
                if counter.decrement_on.is_some() {
                    span { class: "tag tag-info", "↓ Decrement" }
                } else if counter.decrement {
                    span { class: "tag tag-warning", "Decrement" }
                }
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

// ─────────────────────────────────────────────────────────────────────────────
// Counter Edit Form
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn CounterEditForm(
    counter: CounterDefinition,
    #[props(default)] is_builtin: bool,
    #[props(default)] is_modified: bool,
    encounter_data: EncounterData,
    on_save: EventHandler<CounterDefinition>,
    on_delete: EventHandler<CounterDefinition>,
    #[props(default)] on_dirty: EventHandler<bool>,
) -> Element {
    // Clone values needed for closures and display
    let counter_id_display = counter.id.clone();
    let counter_for_delete = counter.clone();
    let counter_for_draft = counter.clone();
    let original = counter.clone();

    let mut draft = use_signal(|| counter_for_draft);
    let mut just_saved = use_signal(|| false);

    // Reset just_saved when user makes new changes after saving
    let original_for_effect = original.clone();
    use_effect(move || {
        if draft() != original_for_effect && just_saved() {
            just_saved.set(false);
        }
    });

    let has_changes = use_memo(move || !just_saved() && draft() != original);

    // Notify parent when dirty state changes
    use_effect(move || {
        on_dirty.call(has_changes());
    });

    let handle_save = move |_: ()| {
        just_saved.set(true);
        let updated = draft();
        on_save.call(updated);
    };

    let handle_delete = move |_: ()| {
        on_delete.call(counter_for_delete.clone());
    };

    rsx! {
        div { class: "counter-edit-form",
            div { class: "edit-grid",
                // ═══ LEFT: Identity Card ═════════════════════════════════════
                FormCard { icon: "fa-tag", title: "Identity",
                    div { class: "form-row-hz",
                        label { "Counter ID" }
                        code { class: "id-display", "{counter_id_display}" }
                    }

                    div { class: "form-row-hz",
                        label { "Name" }
                        input {
                            class: "input-inline w-lg",
                            value: "{draft().name.clone()}",
                            oninput: move |e| {
                                let mut d = draft();
                                d.name = e.value();
                                draft.set(d);
                            }
                        }
                    }

                    div { class: "form-row-hz",
                        label { "Display Text" }
                        input {
                            class: "input-inline w-lg",
                            placeholder: "(defaults to name)",
                            value: "{draft().display_text.clone().unwrap_or_default()}",
                            oninput: move |e| {
                                let mut d = draft();
                                d.display_text = if e.value().is_empty() { None } else { Some(e.value()) };
                                draft.set(d);
                            }
                        }
                    }

                    // ─── Options subsection (hidden for effect stack counters) ─
                    if draft().track_effect_stacks.is_none() {
                        span { class: "text-sm font-bold text-secondary mt-sm", "Options" }

                        div { class: "form-row-hz mt-xs",
                            label { class: "flex items-center",
                                "Initial Value"
                                span {
                                    class: "help-icon",
                                    title: "Starting value for this counter when reset",
                                    "?"
                                }
                            }
                            input {
                                r#type: "number",
                                min: "0",
                                class: "input-inline w-sm",
                                value: "{draft().initial_value}",
                                oninput: move |e| {
                                    if let Ok(val) = e.value().parse::<u32>() {
                                        let mut d = draft();
                                        d.initial_value = val;
                                        draft.set(d);
                                    }
                                }
                            }
                        }

                        div { class: "form-row-hz",
                            label { class: "flex items-center",
                                "Set Value"
                                span {
                                    class: "help-icon",
                                    title: "Set to a specific value on trigger instead of incrementing by 1",
                                    "?"
                                }
                            }
                            div { class: "flex items-center gap-xs",
                                input {
                                    r#type: "checkbox",
                                    checked: draft().set_value.is_some(),
                                    onchange: move |_| {
                                        let mut d = draft();
                                        d.set_value = if d.set_value.is_some() { None } else { Some(1) };
                                        draft.set(d);
                                    }
                                }
                                if draft().set_value.is_some() {
                                    input {
                                        r#type: "number",
                                        min: "0",
                                        class: "input-inline w-sm",
                                        value: "{draft().set_value.unwrap_or(1)}",
                                        oninput: move |e| {
                                            if let Ok(val) = e.value().parse::<u32>() {
                                                let mut d = draft();
                                                d.set_value = Some(val);
                                                draft.set(d);
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        label {
                            class: "flex items-center gap-xs text-sm",
                            input {
                                r#type: "checkbox",
                                checked: draft().decrement,
                                onchange: move |_| {
                                    let mut d = draft();
                                    d.decrement = !d.decrement;
                                    draft.set(d);
                                }
                            }
                            span { class: "flex items-center",
                                "Decrement"
                                span {
                                    class: "help-icon",
                                    title: "Count down instead of up on each trigger",
                                    "?"
                                }
                            }
                        }
                    }
                }

                // ═══ RIGHT: Mode + Config Card ═══════════════════════════════
                FormCard { icon: "fa-bolt", title: "Counter Mode",
                    // Mode selector
                    div { class: "form-row-hz",
                        label { class: "flex items-center",
                            "Mode"
                            span {
                                class: "help-icon",
                                title: "Manual: triggers increment/decrement the counter. Effect Stacks: automatically tracks stack count of a game effect.",
                                "?"
                            }
                        }
                        select {
                            class: "select w-lg",
                            value: if draft().track_effect_stacks.is_some() { "effect_stacks" } else { "manual" },
                            onchange: move |e| {
                                let mut d = draft();
                                match e.value().as_str() {
                                    "effect_stacks" => {
                                        d.track_effect_stacks = Some(EffectStackConfig {
                                            effects: vec![],
                                            target: EntityFilter::Any,
                                            aggregation: StackAggregation::Max,
                                        });
                                    }
                                    _ => {
                                        d.track_effect_stacks = None;
                                    }
                                }
                                draft.set(d);
                            },
                            option { value: "manual", "Default" }
                            option { value: "effect_stacks", "Track Effect Stacks" }
                        }
                    }

                    if let Some(ref config) = draft().track_effect_stacks {
                        // ─── Effect Stack Mode ─────────────────────────────────
                        {
                            rsx! {
                                div { class: "form-row-hz top",
                                    label { class: "flex items-center",
                                        "Effects"
                                        span {
                                            class: "help-icon",
                                            title: "Which game effects to track stacks for (by ID or name)",
                                            "?"
                                        }
                                    }
                                    EffectSelectorEditor {
                                        label: "Effects",
                                        selectors: config.effects.clone(),
                                        on_change: move |sels: Vec<EffectSelector>| {
                                            let mut d = draft();
                                            if let Some(ref mut cfg) = d.track_effect_stacks {
                                                cfg.effects = sels;
                                            }
                                            draft.set(d);
                                        }
                                    }
                                }

                                div { class: "form-row-hz",
                                    label { class: "flex items-center",
                                        "Target"
                                        span {
                                            class: "help-icon",
                                            title: "Which entities' stacks to track (e.g., local player, any player, a specific boss)",
                                            "?"
                                        }
                                    }
                                    EntityFilterDropdown {
                                        label: "Target",
                                        value: config.target.clone(),
                                        options: EntityFilter::target_options(),
                                        on_change: move |f: EntityFilter| {
                                            let mut d = draft();
                                            if let Some(ref mut cfg) = d.track_effect_stacks {
                                                cfg.target = f;
                                            }
                                            draft.set(d);
                                        }
                                    }
                                }

                                div { class: "form-row-hz",
                                    label { class: "flex items-center",
                                        "Aggregation"
                                        span {
                                            class: "help-icon",
                                            title: "How to combine stacks when multiple entities match: Max (highest), Sum (total), Min (lowest)",
                                            "?"
                                        }
                                    }
                                    select {
                                        class: "select w-md",
                                        value: match config.aggregation {
                                            StackAggregation::Max => "max",
                                            StackAggregation::Sum => "sum",
                                            StackAggregation::Min => "min",
                                        },
                                        onchange: move |e| {
                                            let mut d = draft();
                                            if let Some(ref mut cfg) = d.track_effect_stacks {
                                                cfg.aggregation = match e.value().as_str() {
                                                    "sum" => StackAggregation::Sum,
                                                    "min" => StackAggregation::Min,
                                                    _ => StackAggregation::Max,
                                                };
                                            }
                                            draft.set(d);
                                        },
                                        option { value: "max", "Max" }
                                        option { value: "sum", "Sum" }
                                        option { value: "min", "Min" }
                                    }
                                }

                                // Reset trigger still applies in effect stack mode
                                div { class: "form-row-hz top",
                                    label { class: "flex items-center",
                                        "Reset On"
                                        span {
                                            class: "help-icon",
                                            title: "When to reset the counter value (effect stack state is preserved)",
                                            "?"
                                        }
                                    }
                                    ComposableTriggerEditor {
                                        trigger: draft().reset_on,
                                        encounter_data: encounter_data.clone(),
                                        on_change: move |t| {
                                            let mut d = draft();
                                            d.reset_on = t;
                                            draft.set(d);
                                        },
                                        hide_timer_only: true,
                                    }
                                }
                            }
                        }
                    } else {
                        // ─── Manual Triggers Mode ──────────────────────────────
                        // Increment Trigger
                        div { class: "form-row-hz top",
                            label { class: "flex items-center",
                                "Increment On"
                                span {
                                    class: "help-icon",
                                    title: "The game event that increments (or sets) this counter",
                                    "?"
                                }
                            }
                            ComposableTriggerEditor {
                                trigger: draft().increment_on,
                                encounter_data: encounter_data.clone(),
                                on_change: move |t| {
                                    let mut d = draft();
                                    d.increment_on = t;
                                    draft.set(d);
                                },
                                hide_timer_only: true,
                            }
                        }

                        // Decrement Trigger (optional)
                        div { class: "form-row-hz top",
                            label { class: "flex items-center",
                                "Decrement On"
                                span {
                                    class: "help-icon",
                                    title: "Optional separate trigger that decrements the counter",
                                    "?"
                                }
                            }
                            div { class: "flex-col gap-xs",
                                div { class: "flex items-center gap-xs",
                                    input {
                                        r#type: "checkbox",
                                        checked: draft().decrement_on.is_some(),
                                        onchange: move |_| {
                                            let mut d = draft();
                                            d.decrement_on = if d.decrement_on.is_some() {
                                                None
                                            } else {
                                                Some(Trigger::AbilityCast {
                                                    abilities: vec![],
                                                    source: EntityFilter::default(),
                                                    target: EntityFilter::default(),
                                                    position: vec![],
                                                })
                                            };
                                            draft.set(d);
                                        }
                                    }
                                    span { class: "text-xs text-muted", "(enable separate decrement trigger)" }
                                }
                                if let Some(ref decrement_trigger) = draft().decrement_on {
                                    ComposableTriggerEditor {
                                        trigger: decrement_trigger.clone(),
                                        encounter_data: encounter_data.clone(),
                                        on_change: move |t| {
                                            let mut d = draft();
                                            d.decrement_on = Some(t);
                                            draft.set(d);
                                        },
                                        hide_timer_only: true,
                                    }
                                }
                            }
                        }

                        // Reset Trigger
                        div { class: "form-row-hz top",
                            label { class: "flex items-center",
                                "Reset On"
                                span {
                                    class: "help-icon",
                                    title: "The game event that resets this counter to its initial value",
                                    "?"
                                }
                            }
                            ComposableTriggerEditor {
                                trigger: draft().reset_on,
                                encounter_data: encounter_data.clone(),
                                on_change: move |t| {
                                    let mut d = draft();
                                    d.reset_on = t;
                                    draft.set(d);
                                },
                                hide_timer_only: true,
                            }
                        }
                    }
                }
            }

            // ─── Advanced Card ───────────────────────────────────────
            FormCard { icon: "fa-triangle-exclamation", title: "Advanced",
                label { class: "flex items-center gap-xs text-sm",
                    input {
                        r#type: "checkbox",
                        checked: draft().enabled,
                        onchange: move |e| {
                            let mut d = draft();
                            d.enabled = e.checked();
                            draft.set(d);
                        }
                    }
                    "Counter Enabled"
                }
                if !draft().enabled {
                    div { class: "text-xs text-warning mt-xs",
                        i { class: "fa-solid fa-triangle-exclamation" }
                        " Disabling a counter may break phases or timers that depend on it."
                    }
                }
            }

            // ─── Actions ─────────────────────────────────────────────────────
            ItemActions {
                has_changes: has_changes(),
                origin: Origin::of(is_builtin, is_modified),
                on_save: handle_save,
                on_delete: handle_delete,
            }
        }
    }
}
