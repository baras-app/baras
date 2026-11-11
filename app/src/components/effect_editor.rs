//! Effect Editor Panel
//!
//! UI for viewing and editing effect definitions with:
//! - Grouped by file with collapsible headers
//! - Inline expansion for editing
//! - Full CRUD operations

use crate::components::editor_drawer::{DiscardPrompt, layout_style, EditorDrawer, provide_drawer_guard};
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local as spawn;

use super::effect_form::EffectEditForm;
use super::effect_trigger::effect_icon_id;
use super::encounter_editor::InlineNameCreator;
use crate::components::effect_filters::{EffectFilterBar, matches_filters};
use crate::components::ability_icon::ColorIcon;
use crate::components::editor_sort::{apply_rank, use_stable_rank};
use crate::components::editor_common::{Origin, OriginBadge};
use crate::api;
use crate::types::{AlertTrigger, AudioConfig, EffectImportPreview, EffectListItem, EntityFilter, Trigger, UiSessionState};

/// Create a default effect with sensible defaults
fn default_effect(name: String) -> EffectListItem {
    EffectListItem {
        id: String::new(),
        name,
        display_text: None,
        is_user_override: false,
        is_bundled: false,
        enabled: true,
        trigger: Trigger::EffectApplied {
            effects: vec![],
            source: EntityFilter::LocalPlayer,
            target: EntityFilter::Any,
            position: vec![],
        },
        ignore_effect_removed: false,
        refresh_abilities: vec![],
        duration_secs: Some(15.0),
        is_aoe_refresh: false,
        aoe_refresh_immediate: false,
        is_refreshed_on_modify: false,
        single_instance_per_target: false,
        default_charges: None,
        color: Some([80, 200, 80, 255]),
        show_at_secs: 0.0,
        display_targets: vec![],
        icon_ability_id: None,
        show_icon: true,
        display_source: false,
        show_target: false,
        track_uptime: false,
        track_uptime_after_first: false,
        stack_priority: false,
        show_countdown: true,
        show_single_stack: false,
        is_affected_by_alacrity: false,
        cooldown_ready_secs: 0.0,
        disciplines: vec![],
        source_disciplines: vec![],
        target_disciplines: vec![],
        ignore_refreshes: false,
        refresh_scope: Default::default(),
        persist_past_death: false,
        track_outside_combat: true,
        on_apply_trigger_timer: None,
        on_expire_trigger_timer: None,
        is_alert: false,
        alert_text: None,
        alert_on: AlertTrigger::None,
        alert_countdown_secs: None,
        audio: AudioConfig::default(),
        modifiers: vec![],
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Main Panel
// ─────────────────────────────────────────────────────────────────────────────

/// Marker ID for a draft effect that hasn't been saved yet
const DRAFT_EFFECT_ID: &str = "__new_draft__";

#[derive(Props, Clone, PartialEq)]
pub struct EffectEditorProps {
    /// Unified UI session state (includes persisted state for this panel)
    pub state: Signal<UiSessionState>,
}

#[component]
pub fn EffectEditorPanel(mut props: EffectEditorProps) -> Element {
    let guard = provide_drawer_guard();
    // Data state (loaded fresh)
    let mut effects = use_signal(Vec::<EffectListItem>::new);
    let mut loading = use_signal(|| true);
    let mut save_status = use_signal(String::new);
    let mut status_is_error = use_signal(|| false);
    // Draft for new effects - not yet saved to backend
    let mut draft_effect = use_signal(|| None::<EffectListItem>);
    // Import state
    let mut import_preview = use_signal(|| None::<EffectImportPreview>);
    let mut import_toml_content = use_signal(|| None::<String>);

    // Extract persisted state fields
    let search_query = use_signal(|| props.state.read().effects_editor.search_query.clone());
    let mut expanded_effect = use_signal(|| props.state.read().effects_editor.expanded_effect.clone());
    let mut hide_disabled_effects = use_signal(|| props.state.read().effects_editor.hide_disabled_effects);
    let filters = use_signal(|| props.state.read().effects_editor.filters.clone());

    // Sync persisted state back to unified state
    use_effect(move || {
        let mut state = props.state.write();
        state.effects_editor.search_query = search_query.read().clone();
        state.effects_editor.expanded_effect = expanded_effect.read().clone();
        state.effects_editor.hide_disabled_effects = *hide_disabled_effects.read();
        state.effects_editor.filters = filters.read().clone();
    });

    // Load effects on mount
    use_future(move || async move {
        if let Some(e) = api::get_effect_definitions().await {
            effects.set(e);
        }
        loading.set(false);
    });

    // Bring the restored open effect into view once, when the list first
    // loads; picking a row later must not move the list (peek: no subscription)
    use_effect(move || {
        if !loading() {
            if let Some(effect_id) = expanded_effect.peek().clone() {
                // Small delay to ensure DOM is rendered
                spawn(async move {
                    gloo_timers::future::TimeoutFuture::new(100).await;
                    if let Some(window) = web_sys::window() {
                        if let Some(document) = window.document() {
                            let element_id = format!("effect-{}", effect_id);
                            if let Some(element) = document.get_element_by_id(&element_id) {
                                element.scroll_into_view();
                            }
                        }
                    }
                });
            }
        }
    });

    // Name search + hide-disabled; the sidebar counts against this list
    let searched_effects = use_memo(move || {
        let query = search_query().to_lowercase();
        let hide_disabled = hide_disabled_effects();
        effects()
            .into_iter()
            .filter(|e| !(hide_disabled && !e.enabled) && (query.is_empty() || e.name.to_lowercase().contains(&query)))
            .collect::<Vec<_>>()
    });
    let filtered_effects = use_memo(move || {
        let f = filters.read();
        searched_effects().into_iter().filter(|e| matches_filters(&f, e)).collect::<Vec<_>>()
    });
    // Always A–Z (the backend order is arbitrary); a rename keeps its row in place
    let rank = use_stable_rank(effects.read().iter().map(|e| (e.id.as_str(), e.name.as_str())), true);
    let mut shown_effects = filtered_effects();
    apply_rank(&mut shown_effects, &rank.read(), |e| &e.id);

    // Handlers
    let on_save = move |updated_effect: EffectListItem| {
        let mut current = effects();
        if let Some(idx) = current.iter().position(|e| e.id == updated_effect.id) {
            current[idx] = updated_effect.clone();
            effects.set(current);
        }

        spawn(async move {
            match api::update_effect_definition(&updated_effect).await {
                Ok(()) => {
                    save_status.set("Saved".to_string());
                    status_is_error.set(false);
                    // Refetch to get updated is_bundled/is_user_override flags
                    if let Some(e) = api::get_effect_definitions().await {
                        effects.set(e);
                    }
                }
                Err(e) => {
                    save_status.set(e);
                    status_is_error.set(true);
                }
            }
        });
    };

    let mut on_delete = move |effect: EffectListItem| {
        let effect_id = effect.id.clone();
        let is_reset = effect.is_bundled; // Built-in items get "reset", not "delete"

        if !is_reset {
            // Custom item — remove from local list immediately
            let current = effects();
            let filtered: Vec<_> = current.into_iter().filter(|e| e.id != effect_id).collect();
            effects.set(filtered);
        }
        expanded_effect.set(None);

        spawn(async move {
            match api::delete_effect_definition(&effect.id).await {
                Ok(()) => {
                    if is_reset {
                        save_status.set("Reset to built-in".to_string());
                        // Refetch to get the original bundled definition back
                        if let Some(e) = api::get_effect_definitions().await {
                            effects.set(e);
                        }
                    } else {
                        save_status.set("Deleted".to_string());
                    }
                    status_is_error.set(false);
                }
                Err(e) => {
                    save_status.set(e);
                    status_is_error.set(true);
                }
            }
        });
    };

    let on_duplicate = move |effect: EffectListItem| {
        spawn(async move {
            match api::duplicate_effect_definition(&effect.id).await {
                Ok(new_effect) => {
                    let new_id = new_effect.id.clone();
                    let mut current = effects();
                    current.push(new_effect);
                    effects.set(current);
                    expanded_effect.set(Some(new_id));
                    save_status.set("Duplicated".to_string());
                    status_is_error.set(false);
                }
                Err(e) => {
                    save_status.set(e);
                    status_is_error.set(true);
                }
            }
        });
    };

    let on_export_effect = move |effect: EffectListItem| {
        let effect_id = effect.id.clone();
        spawn(async move {
            match api::export_effects_toml(Some(&effect_id)).await {
                Ok(toml) => {
                    let default_name = format!("{}.toml", effect_id);
                    if let Some(path) = api::save_file_dialog(&default_name).await {
                        match api::save_export_file(&path, &toml).await {
                            Ok(()) => {
                                save_status.set("Exported".to_string());
                                status_is_error.set(false);
                            }
                            Err(e) => {
                                save_status.set(e);
                                status_is_error.set(true);
                            }
                        }
                    }
                }
                Err(e) => {
                    save_status.set(e);
                    status_is_error.set(true);
                }
            }
        });
    };

    let on_create = move |name: String| {
        // Create a local draft - don't save to backend yet
        let mut new_effect = default_effect(name);
        new_effect.id = DRAFT_EFFECT_ID.to_string();

        // Set the draft and expand it
        draft_effect.set(Some(new_effect));
        expanded_effect.set(Some(DRAFT_EFFECT_ID.to_string()));
        save_status.set("Fill in effect details and click Save".to_string());
        status_is_error.set(false);
    };

    // Handler for saving a draft (creates new effect on backend)
    let on_save_draft = move |mut effect: EffectListItem| {
        // Generate ID from name (snake_case)
        effect.id = effect
            .name
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect::<String>()
            .split('_')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("_");

        spawn(async move {
            match api::create_effect_definition(&effect).await {
                Ok(created) => {
                    let created_id = created.id.clone();
                    let mut current = effects();
                    current.push(created);
                    effects.set(current);
                    // Clear draft and expand the new effect
                    draft_effect.set(None);
                    expanded_effect.set(Some(created_id));
                    save_status.set("Created".to_string());
                    status_is_error.set(false);
                }
                Err(e) => {
                    save_status.set(e);
                    status_is_error.set(true);
                }
            }
        });
    };

    // Handler for canceling draft creation
    let on_cancel_draft = move |_: ()| {
        draft_effect.set(None);
        expanded_effect.set(None);
        save_status.set(String::new());
    };

    rsx! {
        div { class: "editor-layout", style: layout_style(),
            DiscardPrompt {}
            div { class: "editor-main effect-editor-panel",
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
                        span { class: "effect-count", "{filtered_effects().len()} of {effects().len()} effects" }
                        {
                            let disabled_count = effects().iter().filter(|e| !e.enabled).count();
                            rsx! {
                                if disabled_count > 0 {
                                    label { class: "flex items-center gap-xs text-xs text-muted cursor-pointer",
                                        input {
                                            r#type: "checkbox",
                                            checked: hide_disabled_effects(),
                                            onchange: move |e| hide_disabled_effects.set(e.checked()),
                                        }
                                        "Hide disabled ({disabled_count})"
                                    }
                                }
                            }
                        }
                        button {
                            class: "btn btn-sm",
                            onclick: move |_| {
                                spawn(async move {
                                    match api::export_effects_toml(None).await {
                                        Ok(toml) => {
                                            if let Some(path) = api::save_file_dialog("effects_custom.toml").await {
                                                match api::save_export_file(&path, &toml).await {
                                                    Ok(()) => {
                                                        save_status.set("Exported".to_string());
                                                        status_is_error.set(false);
                                                    }
                                                    Err(e) => {
                                                        save_status.set(e);
                                                        status_is_error.set(true);
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            save_status.set(e);
                                            status_is_error.set(true);
                                        }
                                    }
                                });
                            },
                            "Export All"
                        }
                        button {
                            class: "btn btn-sm",
                            onclick: move |_| {
                                spawn(async move {
                                    let Some(path) = api::open_toml_file_dialog().await else { return };
                                    let content = match api::read_import_file(&path).await {
                                        Ok(c) => c,
                                        Err(e) => {
                                            save_status.set(e);
                                            status_is_error.set(true);
                                            return;
                                        }
                                    };
                                    match api::preview_import_effects(&content).await {
                                        Ok(preview) => {
                                            import_toml_content.set(Some(content));
                                            import_preview.set(Some(preview));
                                        }
                                        Err(e) => {
                                            save_status.set(e);
                                            status_is_error.set(true);
                                        }
                                    }
                                });
                            },
                            "Import"
                        }
                        InlineNameCreator {
                            button_label: "+ New Effect",
                            placeholder: "Effect name...",
                            on_create: on_create,
                        }
                    }
                }

                EffectFilterBar { effects: searched_effects, filters, search: search_query }

                // Effect list (flat)
                if loading() {
                    div { class: "empty-state", "Loading effects..." }
                } else if filtered_effects().is_empty() && draft_effect().is_none() {
                    if effects().is_empty() {
                        div { class: "empty-state-guidance",
                            div { class: "empty-state-icon",
                                i { class: "fa-solid fa-sparkles" }
                            }
                            p { "No effects defined yet" }
                            p { class: "hint", "Click \"+ New Effect\" above to create your first effect" }
                        }
                    } else {
                        div { class: "empty-state", "No effects match the search and filters" }
                    }
                } else {
                    div { class: "effect-list",
                        // Draft effect at the top (if any)
                        if let Some(draft) = draft_effect() {
                            {
                                let is_draft_expanded = expanded_effect() == Some(DRAFT_EFFECT_ID.to_string());
                                rsx! {
                                    EffectRow {
                                        key: "{DRAFT_EFFECT_ID}",
                                        effect: draft,
                                        expanded: is_draft_expanded,
                                        is_draft: true,
                                        on_toggle: move |_| {
                                            guard.switch(expanded_effect, (!is_draft_expanded).then(|| DRAFT_EFFECT_ID.to_string()));
                                        },
                                        on_save: on_save_draft,
                                        on_delete: on_cancel_draft,
                                        on_duplicate: move |_| {},
                                        on_cancel: on_cancel_draft,
                                    }
                                }
                            }
                        }

                        // Existing effects
                        for effect in shown_effects {
                            {
                                let effect_key = effect.id.clone();
                                let is_effect_expanded = expanded_effect() == Some(effect_key.clone());
                                let effect_clone = effect.clone();
                                let effect_for_delete = effect.clone();
                                let effect_for_duplicate = effect.clone();
                                let effect_for_export = effect.clone();

                                rsx! {
                                    EffectRow {
                                        key: "{effect_key}",
                                        effect: effect_clone,
                                        expanded: is_effect_expanded,
                                        is_draft: false,
                                        on_toggle: move |_| {
                                            guard.switch(expanded_effect, (!is_effect_expanded).then(|| effect_key.clone()));
                                        },
                                        on_save: on_save,
                                        on_delete: move |_| on_delete(effect_for_delete.clone()),
                                        on_duplicate: move |_| on_duplicate(effect_for_duplicate.clone()),
                                        on_export: move |_| on_export_effect(effect_for_export.clone()),
                                        on_cancel: move |_| {},
                                    }
                                }
                            }
                        }
                    }
                }

                // Import preview modal
                if let Some(preview) = import_preview() {
                    EffectImportPreviewModal {
                        preview: preview,
                        on_confirm: move |_| {
                            let content = import_toml_content().unwrap_or_default();
                            import_preview.set(None);
                            import_toml_content.set(None);
                            spawn(async move {
                                match api::import_effects_toml(&content).await {
                                    Ok(()) => {
                                        // Refresh the effects list
                                        if let Some(e) = api::get_effect_definitions().await {
                                            effects.set(e);
                                        }
                                        save_status.set("Imported".to_string());
                                        status_is_error.set(false);
                                    }
                                    Err(e) => {
                                        save_status.set(e);
                                        status_is_error.set(true);
                                    }
                                }
                            });
                        },
                        on_cancel: move |_| {
                            import_preview.set(None);
                            import_toml_content.set(None);
                        },
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Import Preview Modal
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn EffectImportPreviewModal(
    preview: EffectImportPreview,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let has_errors = !preview.errors.is_empty();
    let replace_count = preview.effects_to_replace.len();
    let add_count = preview.effects_to_add.len();

    // Build combined rows: replacements first, then additions
    let rows: Vec<(&str, &crate::types::EffectImportDiff)> = preview
        .effects_to_replace
        .iter()
        .map(|e| ("replace", e))
        .chain(preview.effects_to_add.iter().map(|e| ("add", e)))
        .collect();

    rsx! {
        div { class: "modal-overlay", onclick: move |_| on_cancel.call(()),
            div { class: "modal-content", style: "max-width: 600px;",
                onclick: move |e| e.stop_propagation(),

                div { class: "modal-header",
                    h3 { "Import Effect Definitions" }
                }

                div { class: "modal-body", style: "max-height: 400px; overflow-y: auto; padding: 0;",
                    // Errors
                    for error in &preview.errors {
                        div { style: "color: var(--color-error); padding: 4px 12px;",
                            "{error}"
                        }
                    }

                    if !rows.is_empty() {
                        table { class: "import-table", style: "width: 100%; border-collapse: collapse;",
                            thead {
                                tr { style: "text-align: left; border-bottom: 1px solid var(--border-medium);",
                                    th { style: "padding: 4px 8px; width: 24px;" }
                                    th { style: "padding: 4px 8px;", "Name" }
                                    th { style: "padding: 4px 8px;", "ID" }
                                    th { style: "padding: 4px 8px;", "Target" }
                                }
                            }
                            tbody {
                                for (action, effect) in &rows {
                                    tr { style: "border-bottom: 1px solid var(--border-subtle);",
                                        td { style: "padding: 3px 8px; width: 24px; text-align: center;",
                                            if *action == "replace" {
                                                span { class: "text-warning", "~" }
                                            } else {
                                                span { class: "text-success", "+" }
                                            }
                                        }
                                        td { style: "padding: 3px 8px;", "{effect.name}" }
                                        td { style: "padding: 3px 8px;",
                                            code { class: "text-xs", "{effect.id}" }
                                        }
                                        td { style: "padding: 3px 8px;",
                                            {
                                                let targets_label = match effect.display_targets.len() {
                                                    0 => "None".to_string(),
                                                    1 => effect.display_targets[0].label().to_string(),
                                                    n => format!("⊞ {n} targets"),
                                                };
                                                rsx! { span { class: "tag", "{targets_label}" } }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "modal-footer",
                    div { class: "text-muted text-sm", style: "margin-right: auto;",
                        "{replace_count} replace, {add_count} new"
                        if preview.effects_unchanged > 0 {
                            {
                                let unchanged = preview.effects_unchanged;
                                rsx! { ", {unchanged} unchanged" }
                            }
                        }
                    }
                    button {
                        class: "btn",
                        onclick: move |_| on_cancel.call(()),
                        "Cancel"
                    }
                    button {
                        class: "btn btn-success",
                        disabled: has_errors,
                        onclick: move |_| on_confirm.call(()),
                        "Import"
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Effect Row
// ─────────────────────────────────────────────────────────────────────────────

/// Bundled + user override = edited built-in
pub(crate) fn effect_origin(is_draft: bool, is_bundled: bool, is_user_override: bool) -> Origin {
    if is_draft { Origin::New } else { Origin::of(is_bundled && !is_user_override, is_bundled && is_user_override) }
}

#[component]
fn EffectRow(
    effect: EffectListItem,
    expanded: bool,
    #[props(default = false)] is_draft: bool,
    on_toggle: EventHandler<()>,
    on_save: EventHandler<EffectListItem>,
    on_delete: EventHandler<()>,
    on_duplicate: EventHandler<()>,
    #[props(default)] on_export: EventHandler<()>,
    #[props(default)] on_cancel: EventHandler<()>,
) -> Element {
    let mut is_dirty = use_signal(|| false);
    let color = effect.color.unwrap_or([128, 128, 128, 255]);
    let color_hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);

    // Clones for toggle handlers
    let effect_for_enable = effect.clone();
    let effect_for_audio = effect.clone();

    rsx! {
        div {
            id: "effect-{effect.id}",
            class: "list-item",
            class: if expanded { "expanded" },
            class: if !effect.enabled { "item-disabled" },
            div {
                class: "list-item-header",
                onclick: move |_| on_toggle.call(()),

                ColorIcon {
                    icon_id: effect.show_icon.then(|| effect_icon_id(effect.icon_ability_id, &effect.trigger)).flatten(),
                    color: color_hex.clone(),
                }

                // Name | ID grouped left-aligned
                div { class: "row-col-name-id",
                    span { class: "effect-name truncate", "{effect.name}" }
                    if expanded && is_dirty() {
                        span { class: "unsaved-indicator", title: "Unsaved changes" }
                    }
                    if !is_draft {
                        span { class: "text-xs text-mono text-muted", "  {effect.id}" }
                    }
                }

                // Trigger / target / duration
                span { class: "row-col-trigger",
                    if effect.is_alert {
                        span { class: "tag tag-alert", "Alert" }
                    } else {
                        {
                            let targets_label = match effect.display_targets.len() {
                                0 => "None".to_string(),
                                1 => effect.display_targets[0].label().to_string(),
                                n => format!("⊞ {n} targets"),
                            };
                            rsx! { span { class: "tag", "{targets_label}" } }
                        }
                    }
                }

                span { class: "row-col-duration",
                    if !effect.is_alert {
                        if let Some(dur) = effect.duration_secs {
                            span { class: "effect-duration", "{dur:.0}s" }
                        }
                    }
                }

                // Right side - toggle buttons (clickable without expanding)
                div { class: "flex items-center gap-xs shrink-0",
                    // Enabled toggle
                    span {
                        class: "row-toggle",
                        title: if effect.enabled { "Disable effect" } else { "Enable effect" },
                        onclick: move |e| {
                            e.stop_propagation();
                            let mut updated = effect_for_enable.clone();
                            updated.enabled = !updated.enabled;
                            on_save.call(updated);
                        },
                        span {
                            class: if effect.enabled { "text-success" } else { "text-muted" },
                            if effect.enabled { "✓" } else { "○" }
                        }
                    }

                    // Audio toggle
                    span {
                        class: "row-toggle",
                        title: if effect.audio.enabled { "Disable audio" } else { "Enable audio" },
                        onclick: move |e| {
                            e.stop_propagation();
                            let mut updated = effect_for_audio.clone();
                            updated.audio.enabled = !updated.audio.enabled;
                            on_save.call(updated);
                        },
                        span {
                            class: if effect.audio.enabled { "text-primary" } else { "text-muted" },
                            if effect.audio.enabled { "🔊" } else { "🔇" }
                        }
                    }
                    OriginBadge { origin: effect_origin(is_draft, effect.is_bundled, effect.is_user_override), kind: "effect" }
                }
            }

            if expanded {
                EditorDrawer {
                    title: if effect.name.is_empty() { "New effect".to_string() } else { effect.name.clone() },
                    dirty: is_dirty(),
                    on_close: on_toggle,
                EffectEditForm {
                    effect: effect.clone(),
                    is_draft: is_draft,
                    is_bundled: effect.is_bundled,
                    is_user_override: effect.is_user_override,
                    on_save: on_save,
                    on_delete: on_delete,
                    on_duplicate: on_duplicate,
                    on_export: on_export,
                    on_dirty: move |dirty: bool| is_dirty.set(dirty),
                }
                }
            }
        }
    }
}
