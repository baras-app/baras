//! Effect Editor Panel
//!
//! UI for viewing and editing effect definitions with:
//! - Grouped by file with collapsible headers
//! - Inline expansion for editing
//! - Full CRUD operations

use std::collections::HashSet;
use dioxus::prelude::*;

use crate::api;
use crate::types::{EffectCategory, EffectListItem, EffectSelector, EntityFilter};
use super::encounter_editor::triggers::EffectSelectorEditor;

// ─────────────────────────────────────────────────────────────────────────────
// Main Panel
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn EffectEditorPanel() -> Element {
    // State
    let mut effects = use_signal(Vec::<EffectListItem>::new);
    let mut effect_files = use_signal(Vec::<String>::new);
    let mut search_query = use_signal(String::new);
    let mut expanded_effect = use_signal(|| None::<String>);
    let mut expanded_files = use_signal(HashSet::<String>::new);
    let mut loading = use_signal(|| true);
    let mut show_new_effect = use_signal(|| false);
    let mut save_status = use_signal(String::new);
    let mut status_is_error = use_signal(|| false);

    // Load effects on mount
    use_future(move || async move {
        if let Some(e) = api::get_effect_definitions().await {
            effects.set(e);
        }
        if let Some(f) = api::get_effect_files().await {
            effect_files.set(f);
        }
        loading.set(false);
    });

    // Filter effects based on search query
    let filtered_effects = use_memo(move || {
        let query = search_query().to_lowercase();

        if query.is_empty() {
            return effects();
        }

        effects()
            .into_iter()
            .filter(|e| {
                e.name.to_lowercase().contains(&query)
                    || e.id.to_lowercase().contains(&query)
                    || e.category.label().to_lowercase().contains(&query)
            })
            .collect::<Vec<_>>()
    });

    // Group filtered effects by category
    let grouped_effects = use_memo(move || {
        let mut groups: Vec<(EffectCategory, Vec<EffectListItem>)> = Vec::new();

        for effect in filtered_effects() {
            let cat = effect.category;
            if let Some(group) = groups.iter_mut().find(|(k, _)| *k == cat) {
                group.1.push(effect);
            } else {
                groups.push((cat, vec![effect]));
            }
        }

        // Sort groups by category order (HoT, Shield, Buff, etc.)
        let cat_order = |c: &EffectCategory| -> usize {
            EffectCategory::all().iter().position(|x| x == c).unwrap_or(99)
        };
        groups.sort_by(|a, b| cat_order(&a.0).cmp(&cat_order(&b.0)));
        groups
    });

    // Handlers
    let on_save = move |updated_effect: EffectListItem| {
        let mut current = effects();
        if let Some(idx) = current.iter().position(|e| e.id == updated_effect.id) {
            current[idx] = updated_effect.clone();
            effects.set(current);
        }

        spawn(async move {
            if api::update_effect_definition(&updated_effect).await {
                save_status.set("Saved".to_string());
                status_is_error.set(false);
            } else {
                save_status.set("Failed to save".to_string());
                status_is_error.set(true);
            }
        });
    };

    let mut on_delete = move |effect: EffectListItem| {
        let effect_id = effect.id.clone();

        let current = effects();
        let filtered: Vec<_> = current
            .into_iter()
            .filter(|e| e.id != effect_id)
            .collect();
        effects.set(filtered);
        expanded_effect.set(None);

        spawn(async move {
            if api::delete_effect_definition(&effect.id, &effect.file_path).await {
                save_status.set("Deleted".to_string());
                status_is_error.set(false);
            } else {
                save_status.set("Failed to delete".to_string());
                status_is_error.set(true);
            }
        });
    };

    let on_duplicate = move |effect: EffectListItem| {
        spawn(async move {
            if let Some(new_effect) =
                api::duplicate_effect_definition(&effect.id, &effect.file_path).await
            {
                let mut current = effects();
                current.push(new_effect);
                effects.set(current);
                save_status.set("Duplicated".to_string());
                status_is_error.set(false);
            } else {
                save_status.set("Failed to duplicate".to_string());
                status_is_error.set(true);
            }
        });
    };

    let on_create = move |new_effect: EffectListItem| {
        spawn(async move {
            if let Some(created) = api::create_effect_definition(&new_effect).await {
                let mut current = effects();
                current.push(created);
                effects.set(current);
                save_status.set("Created".to_string());
                status_is_error.set(false);
            } else {
                save_status.set("Failed to create".to_string());
                status_is_error.set(true);
            }
        });
        show_new_effect.set(false);
    };

    rsx! {
        div { class: "effect-editor-panel",
            // Header
            div { class: "effect-editor-header",
                h2 { "Effect Definitions" }
                div { class: "header-right",
                    if !save_status().is_empty() {
                        span {
                            class: if status_is_error() { "save-status error" } else { "save-status" },
                            "{save_status()}"
                        }
                    }
                    span { class: "effect-count", "{filtered_effects().len()} effects" }
                    button {
                        class: "btn-new-effect",
                        onclick: move |_| show_new_effect.set(true),
                        "+ New Effect"
                    }
                }
            }

            // Search bar
            div { class: "effect-search-bar",
                input {
                    r#type: "text",
                    placeholder: "Search by name, ID, or category...",
                    value: "{search_query}",
                    class: "effect-search-input",
                    oninput: move |e| search_query.set(e.value())
                }
            }

            // New effect form
            if show_new_effect() {
                NewEffectForm {
                    effect_files: effect_files(),
                    on_create: on_create,
                    on_cancel: move |_| show_new_effect.set(false),
                }
            }

            // Effect list grouped by file
            if loading() {
                div { class: "effect-loading", "Loading effects..." }
            } else if grouped_effects().is_empty() {
                div { class: "effect-empty",
                    if effects().is_empty() {
                        "No effect definitions found"
                    } else {
                        "No effects match your search"
                    }
                }
            } else {
                div { class: "effect-list",
                    for (category, cat_effects) in grouped_effects() {
                        {
                            let cat_key = category.label().to_string();
                            let is_expanded = expanded_files().contains(&cat_key);
                            let cat_key_toggle = cat_key.clone();
                            let cat_label = category.label();
                            let effect_count = cat_effects.len();

                            rsx! {
                                // Category header
                                div {
                                    class: "file-header",
                                    onclick: move |_| {
                                        let mut set = expanded_files();
                                        if set.contains(&cat_key_toggle) {
                                            set.remove(&cat_key_toggle);
                                        } else {
                                            set.insert(cat_key_toggle.clone());
                                        }
                                        expanded_files.set(set);
                                    },
                                    span { class: "file-expand-icon",
                                        if is_expanded { "▼" } else { "▶" }
                                    }
                                    span { class: "file-name", "{cat_label}" }
                                    span { class: "file-effect-count", "({effect_count})" }
                                }

                                // Effects (only if expanded)
                                if is_expanded {
                                    div { class: "file-effects",
                                        for effect in cat_effects {
                                            {
                                                let effect_key = effect.id.clone();
                                                let is_effect_expanded = expanded_effect() == Some(effect_key.clone());
                                                let effect_clone = effect.clone();
                                                let effect_for_delete = effect.clone();
                                                let effect_for_duplicate = effect.clone();

                                                rsx! {
                                                    EffectRow {
                                                        key: "{effect_key}",
                                                        effect: effect_clone,
                                                        expanded: is_effect_expanded,
                                                        on_toggle: move |_| {
                                                            if is_effect_expanded {
                                                                expanded_effect.set(None);
                                                            } else {
                                                                expanded_effect.set(Some(effect_key.clone()));
                                                            }
                                                        },
                                                        on_save: on_save,
                                                        on_delete: move |_| on_delete(effect_for_delete.clone()),
                                                        on_duplicate: move |_| on_duplicate(effect_for_duplicate.clone()),
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

// ─────────────────────────────────────────────────────────────────────────────
// Effect Row
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn EffectRow(
    effect: EffectListItem,
    expanded: bool,
    on_toggle: EventHandler<()>,
    on_save: EventHandler<EffectListItem>,
    on_delete: EventHandler<()>,
    on_duplicate: EventHandler<()>,
) -> Element {
    let color = effect.color.unwrap_or([128, 128, 128, 255]);
    let color_hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);

    rsx! {
        div { class: if expanded { "effect-row expanded" } else { "effect-row" },
            div {
                class: "effect-row-summary",
                onclick: move |_| on_toggle.call(()),

                span { class: "effect-expand-icon",
                    if expanded { "▼" } else { "▶" }
                }

                span {
                    class: "effect-color-dot",
                    style: "background-color: {color_hex}"
                }

                span { class: "effect-name", "{effect.name}" }
                span { class: "effect-id-inline", "{effect.id}" }
                span { class: "effect-category-badge", "{effect.category.label()}" }

                if let Some(dur) = effect.duration_secs {
                    span { class: "effect-duration", "{dur:.0}s" }
                }

                span {
                    class: if effect.enabled { "effect-status enabled" } else { "effect-status disabled" },
                    if effect.enabled { "✓" } else { "✗" }
                }
            }

            if expanded {
                EffectEditForm {
                    effect: effect.clone(),
                    on_save: on_save,
                    on_delete: on_delete,
                    on_duplicate: on_duplicate,
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Effect Edit Form
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn EffectEditForm(
    effect: EffectListItem,
    on_save: EventHandler<EffectListItem>,
    on_delete: EventHandler<()>,
    on_duplicate: EventHandler<()>,
) -> Element {
    let mut draft = use_signal(|| effect.clone());
    let mut confirm_delete = use_signal(|| false);

    let effect_original = effect.clone();
    let has_changes = use_memo(move || draft() != effect_original);

    let color = draft().color.unwrap_or([128, 128, 128, 255]);
    let color_hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);

    rsx! {
        div { class: "effect-edit-form",
            // Effect ID (read-only)
            div { class: "form-row effect-id-row",
                label { "Effect ID" }
                code { class: "effect-id-display", "{effect.id}" }
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

            // Category, Color, Enabled
            div { class: "form-row-inline",
                div { class: "form-field",
                    label { "Category" }
                    select {
                        value: "{draft().category.label()}",
                        onchange: move |e| {
                            let mut d = draft();
                            d.category = match e.value().as_str() {
                                "HoT" => EffectCategory::Hot,
                                "Shield" => EffectCategory::Shield,
                                "Buff" => EffectCategory::Buff,
                                "Debuff" => EffectCategory::Debuff,
                                "Cleansable" => EffectCategory::Cleansable,
                                "Proc" => EffectCategory::Proc,
                                "Mechanic" => EffectCategory::Mechanic,
                                _ => d.category,
                            };
                            draft.set(d);
                        },
                        for cat in EffectCategory::all() {
                            option { value: "{cat.label()}", "{cat.label()}" }
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
                                let mut d = draft();
                                d.color = Some(c);
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

                div { class: "form-field",
                    label { "Raid Frames" }
                    input {
                        r#type: "checkbox",
                        checked: draft().show_on_raid_frames,
                        onchange: move |e| {
                            let mut d = draft();
                            d.show_on_raid_frames = e.checked();
                            draft.set(d);
                        }
                    }
                }

                div { class: "form-field",
                    label { "Effects Overlay" }
                    input {
                        r#type: "checkbox",
                        checked: draft().show_on_effects_overlay,
                        onchange: move |e| {
                            let mut d = draft();
                            d.show_on_effects_overlay = e.checked();
                            draft.set(d);
                        }
                    }
                }
            }

            // Source and Target filters
            div { class: "form-row-inline",
                div { class: "form-field",
                    label { "Source" }
                    EntityFilterSelect {
                        value: draft().source.clone(),
                        options: EntityFilter::source_options(),
                        on_change: move |f| {
                            let mut d = draft();
                            d.source = f;
                            draft.set(d);
                        }
                    }
                }

                div { class: "form-field",
                    label { "Target" }
                    EntityFilterSelect {
                        value: draft().target.clone(),
                        options: EntityFilter::target_options(),
                        on_change: move |f| {
                            let mut d = draft();
                            d.target = f;
                            draft.set(d);
                        }
                    }
                }
            }

            // Duration, Max Stacks, Can Be Refreshed
            div { class: "form-row-inline",
                div { class: "form-field",
                    label { "Duration (sec)" }
                    input {
                        r#type: "number",
                        step: "0.1",
                        min: "0",
                        value: "{draft().duration_secs.unwrap_or(0.0)}",
                        oninput: move |e| {
                            let mut d = draft();
                            d.duration_secs = e.value().parse::<f32>().ok().filter(|&v| v > 0.0);
                            draft.set(d);
                        }
                    }
                }

                div { class: "form-field",
                    label { "Max Stacks" }
                    input {
                        r#type: "number",
                        min: "0",
                        max: "255",
                        value: "{draft().max_stacks}",
                        oninput: move |e| {
                            if let Ok(val) = e.value().parse::<u8>() {
                                let mut d = draft();
                                d.max_stacks = val;
                                draft.set(d);
                            }
                        }
                    }
                }

                div { class: "form-field",
                    label { "Refreshable" }
                    input {
                        r#type: "checkbox",
                        checked: draft().can_be_refreshed,
                        onchange: move |e| {
                            let mut d = draft();
                            d.can_be_refreshed = e.checked();
                            draft.set(d);
                        }
                    }
                }
            }

            // Effects
            div { class: "form-row",
                EffectSelectorEditor {
                    label: "Effects",
                    selectors: draft().effects.clone(),
                    on_change: move |selectors| {
                        let mut d = draft();
                        d.effects = selectors;
                        draft.set(d);
                    }
                }
            }

            // Refresh Abilities
            div { class: "form-row",
                label { "Refresh Abilities" }
                IdListEditor {
                    ids: draft().refresh_abilities.clone(),
                    on_change: move |ids| {
                        let mut d = draft();
                        d.refresh_abilities = ids;
                        draft.set(d);
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

            div { class: "effect-file-info", "File: {effect.file_path}" }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Entity Filter Select
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn EntityFilterSelect(
    value: EntityFilter,
    options: &'static [EntityFilter],
    on_change: EventHandler<EntityFilter>,
) -> Element {
    rsx! {
        select {
            onchange: move |e| {
                let selected = e.value();
                for opt in options {
                    if opt.label() == selected {
                        on_change.call(opt.clone());
                        break;
                    }
                }
            },
            for opt in options.iter() {
                option {
                    value: "{opt.label()}",
                    selected: *opt == value,
                    "{opt.label()}"
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ID List Editor
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn IdListEditor(
    ids: Vec<u64>,
    on_change: EventHandler<Vec<u64>>,
) -> Element {
    let mut new_id_input = use_signal(String::new);

    rsx! {
        div { class: "id-list-editor",
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
                input {
                    r#type: "text",
                    class: "id-input",
                    placeholder: "ID (Enter to add)",
                    value: "{new_id_input}",
                    oninput: move |e| new_id_input.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter
                            && let Ok(id) = new_id_input().parse::<u64>() {
                                let mut new_ids = ids.clone();
                                if !new_ids.contains(&id) {
                                    new_ids.push(id);
                                    on_change.call(new_ids);
                                }
                                new_id_input.set(String::new());
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// New Effect Form
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn NewEffectForm(
    effect_files: Vec<String>,
    on_create: EventHandler<EffectListItem>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut selected_file = use_signal(String::new);
    let mut name = use_signal(String::new);
    let mut category = use_signal(|| EffectCategory::Hot);
    let mut color = use_signal(|| [80u8, 200, 80, 255]);
    let mut source = use_signal(|| EntityFilter::LocalPlayer);
    let mut target = use_signal(|| EntityFilter::GroupMembers);
    let mut duration = use_signal(|| 15.0f32);
    let mut max_stacks = use_signal(|| 1u8);
    let mut effects = use_signal(Vec::<EffectSelector>::new);
    let mut refresh_abilities = use_signal(Vec::<u64>::new);
    let mut show_on_raid_frames = use_signal(|| true);
    let mut show_on_effects_overlay = use_signal(|| false);

    let color_hex = format!("#{:02x}{:02x}{:02x}", color()[0], color()[1], color()[2]);

    rsx! {
        div { class: "new-effect-form",
            div { class: "new-effect-header",
                h3 { "New Effect" }
                button {
                    class: "btn-close",
                    onclick: move |_| on_cancel.call(()),
                    "×"
                }
            }

            // File selector
            div { class: "form-row",
                label { "File" }
                select {
                    class: "file-select",
                    onchange: move |e| selected_file.set(e.value()),
                    option { value: "", "-- Select a file --" }
                    for file in effect_files.iter() {
                        {
                            let file_name = file.rsplit('/').next().unwrap_or(file);
                            rsx! {
                                option { value: "{file}", "{file_name}" }
                            }
                        }
                    }
                }
            }

            if !selected_file().is_empty() {
                div { class: "form-row",
                    label { "Name" }
                    input {
                        r#type: "text",
                        placeholder: "e.g., Static Barrier",
                        value: "{name}",
                        oninput: move |e| name.set(e.value())
                    }
                }

                div { class: "form-row-inline",
                    div { class: "form-field",
                        label { "Category" }
                        select {
                            value: "{category().label()}",
                            onchange: move |e| {
                                category.set(match e.value().as_str() {
                                    "HoT" => EffectCategory::Hot,
                                    "Shield" => EffectCategory::Shield,
                                    "Buff" => EffectCategory::Buff,
                                    "Debuff" => EffectCategory::Debuff,
                                    "Cleansable" => EffectCategory::Cleansable,
                                    "Proc" => EffectCategory::Proc,
                                    "Mechanic" => EffectCategory::Mechanic,
                                    _ => category(),
                                });
                            },
                            for cat in EffectCategory::all() {
                                option { value: "{cat.label()}", "{cat.label()}" }
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

                    div { class: "form-field",
                        label { "Raid Frames" }
                        input {
                            r#type: "checkbox",
                            checked: show_on_raid_frames(),
                            onchange: move |e| show_on_raid_frames.set(e.checked())
                        }
                    }

                    div { class: "form-field",
                        label { "Effects Overlay" }
                        input {
                            r#type: "checkbox",
                            checked: show_on_effects_overlay(),
                            onchange: move |e| show_on_effects_overlay.set(e.checked())
                        }
                    }
                }

                div { class: "form-row-inline",
                    div { class: "form-field",
                        label { "Source" }
                        EntityFilterSelect {
                            value: source(),
                            options: EntityFilter::source_options(),
                            on_change: move |f| source.set(f)
                        }
                    }

                    div { class: "form-field",
                        label { "Target" }
                        EntityFilterSelect {
                            value: target(),
                            options: EntityFilter::target_options(),
                            on_change: move |f| target.set(f)
                        }
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
                        label { "Max Stacks" }
                        input {
                            r#type: "number",
                            min: "0",
                            max: "255",
                            value: "{max_stacks}",
                            oninput: move |e| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    max_stacks.set(val);
                                }
                            }
                        }
                    }
                }

                div { class: "form-row",
                    EffectSelectorEditor {
                        label: "Effects",
                        selectors: effects(),
                        on_change: move |sels| effects.set(sels)
                    }
                }

                div { class: "form-row",
                    label { "Refresh Ability IDs" }
                    IdListEditor {
                        ids: refresh_abilities(),
                        on_change: move |ids| refresh_abilities.set(ids)
                    }
                }

                div { class: "form-actions",
                    button {
                        class: "btn-save",
                        // Require name and at least one effect selector or refresh ability
                        disabled: name().is_empty() || (effects().is_empty() && refresh_abilities().is_empty()),
                        onclick: move |_| {
                            let new_effect = EffectListItem {
                                id: String::new(), // Auto-generated by backend
                                name: name(),
                                file_path: selected_file(),
                                enabled: true,
                                category: category(),
                                effects: effects(),
                                refresh_abilities: refresh_abilities(),
                                source: source(),
                                target: target(),
                                duration_secs: Some(duration()),
                                can_be_refreshed: true,
                                max_stacks: max_stacks(),
                                color: Some(color()),
                                show_on_raid_frames: show_on_raid_frames(),
                                show_on_effects_overlay: show_on_effects_overlay(),
                                persist_past_death: false,
                                track_outside_combat: true,
                                on_apply_trigger_timer: None,
                                on_expire_trigger_timer: None,
                                encounters: vec![],
                                alert_near_expiration: false,
                                alert_threshold_secs: 3.0,
                            };
                            on_create.call(new_effect);
                        },
                        "Create Effect"
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
