//! Effect Editor Panel
//!
//! UI for viewing and editing effect definitions with:
//! - Grouped by file with collapsible headers
//! - Inline expansion for editing
//! - Full CRUD operations

use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local as spawn;

use super::encounter_editor::InlineNameCreator;
use super::encounter_editor::triggers::{
    AbilitySelectorEditor, EffectSelectorEditor, EntityFilterDropdown,
};
use super::{ToastSeverity, use_toast};
use crate::api;
use crate::types::{
    AbilitySelector, AlertTrigger, AudioConfig, DisplayTarget, EffectImportPreview,
    EffectListItem, EffectSelector, EntityFilter, RefreshAbility, Trigger, UiSessionState,
    effect_alert_label,
};

// ─────────────────────────────────────────────────────────────────────────────
// Trigger Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Get the source and target filters from a trigger
fn get_trigger_filters(trigger: &Trigger) -> (EntityFilter, EntityFilter) {
    match trigger {
        Trigger::EffectApplied { source, target, .. }
        | Trigger::EffectRemoved { source, target, .. }
        | Trigger::AbilityCast { source, target, .. }
        | Trigger::DamageTaken { source, target, .. }
        | Trigger::HealingTaken { source, target, .. } => (source.clone(), target.clone()),
        _ => (EntityFilter::Any, EntityFilter::Any),
    }
}

/// Get the "when" label for effect-based triggers
fn get_effect_when_label(trigger: &Trigger) -> &'static str {
    match trigger {
        Trigger::EffectApplied { .. } => "Effect Applied",
        Trigger::EffectRemoved { .. } => "Effect Removed",
        _ => "Effect Applied",
    }
}

/// Get the effects from an effect-based trigger (returns is_effect_trigger, effects)
fn get_trigger_effects(trigger: &Trigger) -> (bool, Vec<EffectSelector>) {
    match trigger {
        Trigger::EffectApplied { effects, .. } | Trigger::EffectRemoved { effects, .. } => {
            (true, effects.clone())
        }
        _ => (false, vec![]),
    }
}

/// Get abilities from an ability-based trigger
fn get_trigger_abilities(trigger: &Trigger) -> Vec<AbilitySelector> {
    match trigger {
        Trigger::AbilityCast { abilities, .. }
        | Trigger::DamageTaken { abilities, .. }
        | Trigger::HealingTaken { abilities, .. } => abilities.clone(),
        _ => vec![],
    }
}

/// Set the source filter on a trigger
fn set_trigger_source(trigger: Trigger, source: EntityFilter) -> Trigger {
    match trigger {
        Trigger::EffectApplied {
            effects, target, ..
        } => Trigger::EffectApplied {
            effects,
            source,
            target,
        },
        Trigger::EffectRemoved {
            effects, target, ..
        } => Trigger::EffectRemoved {
            effects,
            source,
            target,
        },
        Trigger::AbilityCast {
            abilities, target, ..
        } => Trigger::AbilityCast {
            abilities,
            source,
            target,
        },
        Trigger::DamageTaken {
            abilities, target, ..
        } => Trigger::DamageTaken {
            abilities,
            source,
            target,
        },
        Trigger::HealingTaken {
            abilities, target, ..
        } => Trigger::HealingTaken {
            abilities,
            source,
            target,
        },
        other => other,
    }
}

/// Set the target filter on a trigger
fn set_trigger_target(trigger: Trigger, target: EntityFilter) -> Trigger {
    match trigger {
        Trigger::EffectApplied {
            effects, source, ..
        } => Trigger::EffectApplied {
            effects,
            source,
            target,
        },
        Trigger::EffectRemoved {
            effects, source, ..
        } => Trigger::EffectRemoved {
            effects,
            source,
            target,
        },
        Trigger::AbilityCast {
            abilities, source, ..
        } => Trigger::AbilityCast {
            abilities,
            source,
            target,
        },
        Trigger::DamageTaken {
            abilities, source, ..
        } => Trigger::DamageTaken {
            abilities,
            source,
            target,
        },
        Trigger::HealingTaken {
            abilities, source, ..
        } => Trigger::HealingTaken {
            abilities,
            source,
            target,
        },
        other => other,
    }
}

/// Set the effects on an effect-based trigger
fn set_trigger_effects(trigger: Trigger, effects: Vec<EffectSelector>) -> Trigger {
    match trigger {
        Trigger::EffectApplied { source, target, .. } => Trigger::EffectApplied {
            effects,
            source,
            target,
        },
        Trigger::EffectRemoved { source, target, .. } => Trigger::EffectRemoved {
            effects,
            source,
            target,
        },
        other => other,
    }
}

/// Set the abilities on an ability-based trigger
fn set_trigger_abilities(trigger: Trigger, abilities: Vec<AbilitySelector>) -> Trigger {
    match trigger {
        Trigger::AbilityCast { source, target, .. } => Trigger::AbilityCast {
            abilities,
            source,
            target,
        },
        Trigger::DamageTaken { source, target, .. } => Trigger::DamageTaken {
            abilities,
            source,
            target,
        },
        Trigger::HealingTaken { source, target, .. } => Trigger::HealingTaken {
            abilities,
            source,
            target,
        },
        other => other,
    }
}

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
        },
        ignore_effect_removed: false,
        refresh_abilities: vec![],
        duration_secs: Some(15.0),
        is_aoe_refresh: false,
        is_refreshed_on_modify: false,
        color: Some([80, 200, 80, 255]),
        show_at_secs: 0.0,
        display_target: DisplayTarget::None,
        icon_ability_id: None,
        show_icon: true,
        display_source: false,
        is_affected_by_alacrity: false,
        cooldown_ready_secs: 0.0,
        disciplines: vec![],
        persist_past_death: false,
        track_outside_combat: true,
        on_apply_trigger_timer: None,
        on_expire_trigger_timer: None,
        is_alert: false,
        alert_text: None,
        alert_on: AlertTrigger::None,
        audio: AudioConfig::default(),
    }
}
use crate::utils::parse_hex_color;

/// UI-level trigger type for effect tracking
#[derive(Clone, Copy, PartialEq, Default)]
enum EffectTriggerType {
    /// Track based on game effect applied/removed
    #[default]
    EffectBased,
    /// Track based on ability cast (for procs/cooldowns)
    AbilityCast,
    /// Track based on damage taken from an ability
    DamageTaken,
    /// Track based on healing taken from an ability
    HealingTaken,
}

impl EffectTriggerType {
    fn label(&self) -> &'static str {
        match self {
            Self::EffectBased => "Effect Based",
            Self::AbilityCast => "Ability Cast",
            Self::DamageTaken => "Damage Taken",
            Self::HealingTaken => "Healing Taken",
        }
    }

    fn all() -> &'static [Self] {
        &[Self::EffectBased, Self::AbilityCast, Self::DamageTaken, Self::HealingTaken]
    }

    /// Determine trigger type from effect data
    fn from_effect(effect: &EffectListItem) -> Self {
        match &effect.trigger {
            Trigger::AbilityCast { .. } => Self::AbilityCast,
            Trigger::DamageTaken { .. } => Self::DamageTaken,
            Trigger::HealingTaken { .. } => Self::HealingTaken,
            _ => Self::EffectBased,
        }
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
    let mut search_query = use_signal(|| props.state.read().effects_editor.search_query.clone());
    let mut expanded_effect = use_signal(|| props.state.read().effects_editor.expanded_effect.clone());
    let mut hide_disabled_effects = use_signal(|| props.state.read().effects_editor.hide_disabled_effects);
    
    // Sync persisted state back to unified state
    use_effect(move || {
        let mut state = props.state.write();
        state.effects_editor.search_query = search_query.read().clone();
        state.effects_editor.expanded_effect = expanded_effect.read().clone();
        state.effects_editor.hide_disabled_effects = *hide_disabled_effects.read();
    });

    // Load effects on mount
    use_future(move || async move {
        if let Some(e) = api::get_effect_definitions().await {
            effects.set(e);
        }
        loading.set(false);
    });
    
    // Scroll to expanded effect when effects finish loading
    use_effect(move || {
        if !loading() {
            if let Some(effect_id) = expanded_effect.read().clone() {
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

    // Filter effects based on search query and hide-disabled toggle
    let filtered_effects = use_memo(move || {
        let query = search_query().to_lowercase();
        let hide_disabled = hide_disabled_effects();

        effects()
            .into_iter()
            .filter(|e| {
                if hide_disabled && !e.enabled {
                    return false;
                }
                if !query.is_empty() {
                    return e.name.to_lowercase().contains(&query)
                        || e.id.to_lowercase().contains(&query)
                        || e.display_target.label().to_lowercase().contains(&query);
                }
                true
            })
            .collect::<Vec<_>>()
    });

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

            // Search bar
            div { class: "effect-search-bar",
                input {
                    r#type: "text",
                    placeholder: "Search by name, ID, or display overlay...",
                    value: "{search_query}",
                    class: "effect-search-input",
                    oninput: move |e| search_query.set(e.value())
                }
            }

            // Effect list (flat)
            if loading() {
                div { class: "effect-loading", "Loading effects..." }
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
                    div { class: "effect-empty", "No effects match your search" }
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
                                        if is_draft_expanded {
                                            expanded_effect.set(None);
                                        } else {
                                            expanded_effect.set(Some(DRAFT_EFFECT_ID.to_string()));
                                        }
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
                    for effect in filtered_effects() {
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
                                        if is_effect_expanded {
                                            expanded_effect.set(None);
                                        } else {
                                            expanded_effect.set(Some(effect_key.clone()));
                                        }
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
                                tr { style: "text-align: left; border-bottom: 1px solid var(--border-color);",
                                    th { style: "padding: 4px 8px; width: 24px;" }
                                    th { style: "padding: 4px 8px;", "Name" }
                                    th { style: "padding: 4px 8px;", "ID" }
                                    th { style: "padding: 4px 8px;", "Target" }
                                }
                            }
                            tbody {
                                for (action, effect) in &rows {
                                    tr { style: "border-bottom: 1px solid var(--border-color-subtle, rgba(255,255,255,0.05));",
                                        td { style: "padding: 3px 8px; width: 24px; text-align: center;",
                                            if *action == "replace" {
                                                span { style: "color: var(--color-warning);", "~" }
                                            } else {
                                                span { style: "color: var(--color-success);", "+" }
                                            }
                                        }
                                        td { style: "padding: 3px 8px;", "{effect.name}" }
                                        td { style: "padding: 3px 8px;",
                                            code { class: "text-xs", "{effect.id}" }
                                        }
                                        td { style: "padding: 3px 8px;",
                                            span { class: "effect-target-badge", "{effect.display_target.label()}" }
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
                        class: "btn-save",
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
            class: match (expanded, effect.enabled) {
                (true, true) => "effect-row expanded",
                (true, false) => "effect-row expanded item-disabled",
                (false, true) => "effect-row",
                (false, false) => "effect-row item-disabled",
            },
            div {
                class: "effect-row-summary",
                onclick: move |_| on_toggle.call(()),

                // Expand arrow
                span { class: "effect-expand-icon",
                    if expanded { "▼" } else { "▶" }
                }

                // Origin badge (B/M/C)
                if is_draft {
                    span { class: "timer-origin", style: "background: var(--success-alpha-30, rgba(46,204,113,0.2)); color: var(--color-success, #2ecc71);", title: "New: not yet saved", "N" }
                } else if effect.is_bundled && effect.is_user_override {
                    span { class: "timer-origin timer-origin-modified", title: "Modified: built-in effect you have edited", "M" }
                } else if effect.is_bundled {
                    span { class: "timer-origin timer-origin-builtin", title: "Built-in: ships with the app", "B" }
                } else {
                    span { class: "timer-origin timer-origin-custom", title: "Custom: created by you", "C" }
                }

                // Color dot
                span {
                    class: "effect-color-dot",
                    style: "background-color: {color_hex}"
                }

                // Name | ID grouped left-aligned
                div { class: "timer-col-name-id",
                    span { class: "effect-name truncate", "{effect.name}" }
                    if expanded && is_dirty() {
                        span { class: "unsaved-indicator", title: "Unsaved changes" }
                    }
                    if let Some(ref dt) = effect.display_text {
                        if dt != &effect.name {
                            span { class: "effect-display-text", "→ \"{dt}\"" }
                        }
                    }
                    if !is_draft {
                        span { class: "text-xs text-mono text-muted", "  {effect.id}" }
                    }
                }

                // Trigger / target / duration
                span { class: "timer-col-trigger",
                    if effect.is_alert {
                        span { class: "tag tag-alert", "Alert" }
                    } else {
                        span { class: "effect-target-badge", "{effect.display_target.label()}" }
                    }
                }

                span { class: "timer-col-duration",
                    if !effect.is_alert {
                        if let Some(dur) = effect.duration_secs {
                            span { class: "effect-duration", "{dur:.0}s" }
                        }
                    }
                }

                // Right side - toggle buttons (clickable without expanding)
                div { class: "flex items-center gap-xs", style: "flex-shrink: 0;",
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
                }
            }

            if expanded {
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

// ─────────────────────────────────────────────────────────────────────────────
// Effect Edit Form
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn EffectEditForm(
    effect: EffectListItem,
    #[props(default = false)] is_draft: bool,
    #[props(default = false)] is_bundled: bool,
    #[props(default = false)] is_user_override: bool,
    on_save: EventHandler<EffectListItem>,
    on_delete: EventHandler<()>,
    on_duplicate: EventHandler<()>,
    #[props(default)] on_export: EventHandler<()>,
    #[props(default)] on_dirty: EventHandler<bool>,
) -> Element {
    let effect_for_draft = effect.clone();
    let effect_for_trigger = effect.clone();
    let effect_original = effect.clone();
    let mut draft = use_signal(|| effect_for_draft);
    let mut confirm_delete = use_signal(|| false);
    let mut trigger_type = use_signal(|| EffectTriggerType::from_effect(&effect_for_trigger));
    let mut icon_preview_url = use_signal(|| None::<String>);

    // Load available sound files once
    let mut sound_files = use_signal(Vec::<String>::new);
    use_future(move || async move {
        sound_files.set(api::list_sound_files().await);
    });

    // Track if form was just saved (resets dirty state)
    let mut just_saved = use_signal(|| false);

    // Load icon preview - use explicit icon_ability_id, or fall back to trigger ID
    use_effect(move || {
        let current_draft = draft(); // Read inside effect for reactivity
        let preview_id = current_draft.icon_ability_id.or_else(|| {
            let (is_effect_trigger, effects) = get_trigger_effects(&current_draft.trigger);
            if is_effect_trigger {
                effects.first().and_then(|sel| match sel {
                    EffectSelector::Id(id) => Some(*id),
                    EffectSelector::Name(_) => None,
                })
            } else {
                let abilities = get_trigger_abilities(&current_draft.trigger);
                abilities.first().and_then(|sel| match sel {
                    AbilitySelector::Id(id) => Some(*id),
                    AbilitySelector::Name(_) => None,
                })
            }
        });

        if let Some(ability_id) = preview_id {
            spawn(async move {
                if let Some(url) = api::get_icon_preview(ability_id).await {
                    icon_preview_url.set(Some(url));
                } else {
                    icon_preview_url.set(None);
                }
            });
        } else {
            icon_preview_url.set(None);
        }
    });

    // Reset just_saved when user makes new changes after saving
    let effect_original_for_effect = effect_original.clone();
    use_effect(move || {
        if draft() != effect_original_for_effect && just_saved() {
            just_saved.set(false);
        }
    });

    // For drafts, always enable save; for existing effects, only when changed
    let has_changes = use_memo(move || is_draft || (!just_saved() && draft() != effect_original));

    // Notify parent when dirty state changes
    use_effect(move || {
        on_dirty.call(has_changes());
    });

    let color = draft().color.unwrap_or([128, 128, 128, 255]);
    let color_hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);

    rsx! {
            div { class: "effect-edit-form",
                div { class: "effect-edit-grid",
                    // ═══ LEFT COLUMN: Identity, Trigger ══════════════════════════════
                    div { class: "effect-edit-left",
                        // ─── Identity Card ───────────────────────────────────────────
                        div { class: "form-card",
                            div { class: "form-card-header",
                                i { class: "fa-solid fa-tag" }
                                span { "Identity" }
                            }
                            div { class: "form-card-content",
                                // Effect ID (read-only) - hidden for drafts
                                if !is_draft {
                                    div { class: "form-row-hz",
                                        label { "Effect ID" }
                                        code { class: "effect-id-display", "{effect.id}" }
                                    }
                                }

                                // Name
                                div { class: "form-row-hz",
                                    label { "Name" }
                                    input {
                                        r#type: "text",
                                        class: "input-inline",
                                        style: "width: 200px;",
                                        value: "{draft().name}",
                                        oninput: move |e| {
                                            let mut d = draft();
                                            d.name = e.value();
                                            draft.set(d);
                                        }
                                    }
                                }

                                if !draft().is_alert {
                                    // Display Text
                                    div { class: "form-row-hz",
                                        label { "Display Text" }
                                        input {
                                            r#type: "text",
                                            class: "input-inline",
                                            style: "width: 200px;",
                                            placeholder: "{draft().name}",
                                            value: "{draft().display_text.clone().unwrap_or_default()}",
                                            oninput: move |e| {
                                                let mut d = draft();
                                                d.display_text = if e.value().is_empty() { None } else { Some(e.value()) };
                                                draft.set(d);
                                            }
                                        }
                                    }

                                    // Display Overlay
                                    div { class: "form-row-hz",
                                        label { class: "flex items-center",
                                            "Display Overlay"
                                            span {
                                                class: "help-icon",
                                                title: "Sets which overlay displays this effect when triggered",
                                                "?"
                                            }
                                        }
                                        select {
                                            class: "select-inline",
                                            value: "{draft().display_target.label()}",
                                            onchange: move |e| {
                                                let mut d = draft();
                                                d.display_target = match e.value().as_str() {
                                                    "None" => DisplayTarget::None,
                                                    "Raid Frames" => DisplayTarget::RaidFrames,
                                                    "Effects A" => DisplayTarget::EffectsA,
                                                    "Effects B" => DisplayTarget::EffectsB,
                                                    "Cooldowns" => DisplayTarget::Cooldowns,
                                                    "DOT Tracker" => DisplayTarget::DotTracker,
                                                    "Effects Overlay" => DisplayTarget::EffectsOverlay,
                                                    _ => d.display_target,
                                                };
                                                draft.set(d);
                                            },
                                            for target in DisplayTarget::all() {
                                                option {
                                                    value: "{target.label()}",
                                                    "{target.label()}"
                                                }
                                            }
                                        }
                                    }
                                }

                                // Disciplines
                                div { class: "form-row-hz",
                                    label { class: "flex items-center",
                                        "Disciplines"
                                        span {
                                            class: "help-icon",
                                            title: "Only activate this effect when your local player is one of the selected disciplines. If empty, the effect applies to all disciplines.",
                                            "?"
                                        }
                                    }
                                    DisciplineSelector {
                                        selected: draft().disciplines.clone(),
                                        on_change: move |new_disciplines: Vec<String>| {
                                            let mut d = draft();
                                            d.disciplines = new_disciplines;
                                            draft.set(d);
                                        }
                                    }
                                }

                                // Color
                                div { class: "form-row-hz",
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

                                if !draft().is_alert {
                                    // Icon ID with preview
                                    div { class: "form-row-hz",
                                        label { class: "flex items-center",
                                            "Icon ID"
                                            span {
                                                class: "help-icon",
                                                title: "Ability ID to use for the icon. Leave blank to auto-detect from trigger.",
                                                "?"
                                            }
                                        }
                                        input {
                                            r#type: "text",
                                            class: "input-inline",
                                            style: "width: 140px;",
                                            placeholder: "(auto)",
                                            value: "{draft().icon_ability_id.map(|id| id.to_string()).unwrap_or_default()}",
                                            oninput: move |e| {
                                                let mut d = draft();
                                                d.icon_ability_id = if e.value().is_empty() {
                                                    None
                                                } else {
                                                    e.value().parse::<u64>().ok()
                                                };
                                                draft.set(d);
                                            }
                                        }
                                        // Icon preview
                                        if let Some(ref url) = icon_preview_url() {
                                            img {
                                                src: "{url}",
                                                class: "icon-preview",
                                                width: "24",
                                                height: "24",
                                                alt: "Icon preview"
                                            }
                                        } else if draft().icon_ability_id.is_some() {
                                            span { class: "text-muted text-xs", "(not found)" }
                                        }
                                    }

                                    // Show Icon
                                    div { class: "form-row-hz",
                                        label { "Show Icon" }
                                        input {
                                            r#type: "checkbox",
                                            checked: draft().show_icon,
                                            onchange: move |e| {
                                                let mut d = draft();
                                                d.show_icon = e.checked();
                                                draft.set(d);
                                            }
                                        }
                                    }

                                    // Display Source - only for personal overlays
                                    if matches!(draft().display_target, DisplayTarget::EffectsA | DisplayTarget::EffectsB | DisplayTarget::Cooldowns) {
                                        div { class: "form-row-hz",
                                            label { class: "flex items-center",
                                                "Display Source"
                                                span {
                                                    class: "help-icon",
                                                    title: "Show who applied this effect on the overlay",
                                                    "?"
                                                }
                                            }
                                            input {
                                                r#type: "checkbox",
                                                checked: draft().display_source,
                                                onchange: move |e| {
                                                    let mut d = draft();
                                                    d.display_source = e.checked();
                                                    draft.set(d);
                                                }
                                            }
                                        }
                                    }
                                }

                                // Entry Enabled
                                div { class: "form-row-hz",
                                    label { "Entry Enabled" }
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
                        }

                        // ─── Trigger Card ────────────────────────────────────────────
                        div { class: "form-card",
                            div { class: "form-card-header",
                                i { class: "fa-solid fa-bolt" }
                                span { "Trigger" }
                            }
                            div { class: "form-card-content",
                                // Trigger Type and When
                                div { class: "form-row-hz",
                                    label { class: "flex items-center",
                                        "Trigger"
                                        span {
                                            class: "help-icon",
                                            title: "How this effect activates: Effect-based tracks game buffs/debuffs, Ability-based tracks when abilities are cast",
                                            "?"
                                        }
                                    }
                                    select {
                                        class: "select-inline",
                                        value: "{trigger_type().label()}",
                                        onchange: move |e| {
                                            let new_type = match e.value().as_str() {
                                                "Effect Based" => EffectTriggerType::EffectBased,
                                                "Ability Cast" => EffectTriggerType::AbilityCast,
                                                "Damage Taken" => EffectTriggerType::DamageTaken,
                                                "Healing Taken" => EffectTriggerType::HealingTaken,
                                                _ => trigger_type(),
                                            };
                                            trigger_type.set(new_type);
                                            let mut d = draft();
                                            // Convert trigger to new type, preserving source/target
                                            let (source, target) = get_trigger_filters(&d.trigger);
                                            d.trigger = match new_type {
                                                EffectTriggerType::EffectBased => Trigger::EffectApplied {
                                                    effects: vec![],
                                                    source,
                                                    target,
                                                },
                                                EffectTriggerType::AbilityCast => Trigger::AbilityCast {
                                                    abilities: vec![],
                                                    source,
                                                    target,
                                                },
                                                EffectTriggerType::DamageTaken => Trigger::DamageTaken {
                                                    abilities: vec![],
                                                    source,
                                                    target,
                                                },
                                                EffectTriggerType::HealingTaken => Trigger::HealingTaken {
                                                    abilities: vec![],
                                                    source,
                                                    target,
                                                },
                                            };
                                            draft.set(d);
                                        },
                                        for tt in EffectTriggerType::all() {
                                            option { value: "{tt.label()}", "{tt.label()}" }
                                        }
                                    }
                                    if trigger_type() == EffectTriggerType::EffectBased {
                                        label { "When" }
                                        select {
                                            class: "select-inline",
                                            value: "{get_effect_when_label(&draft().trigger)}",
                                            onchange: move |e| {
                                                let mut d = draft();
                                                let (_, effects) = get_trigger_effects(&d.trigger);
                                                let (source, target) = get_trigger_filters(&d.trigger);
                                                d.trigger = match e.value().as_str() {
                                                    "Effect Applied" => Trigger::EffectApplied { effects, source, target },
                                                    "Effect Removed" => Trigger::EffectRemoved { effects, source, target },
                                                    _ => d.trigger,
                                                };
                                                draft.set(d);
                                            },
                                            option { value: "Effect Applied", "Effect Applied" }
                                            option { value: "Effect Removed", "Effect Removed" }
                                        }
                                    }
                                }

                                // Source and Target filters
                                div { class: "form-row-hz",
                                    label { class: "flex items-center",
                                        "Source"
                                        span {
                                            class: "help-icon",
                                            title: "Who must cast/apply for this effect to trigger (e.g., Local Player = you, Any = anyone)",
                                            "?"
                                        }
                                    }
                                    EntityFilterDropdown {
                                        label: "",
                                        value: get_trigger_filters(&draft().trigger).0.clone(),
                                        options: EntityFilter::source_options(),
                                        on_change: move |f| {
                                            let mut d = draft();
                                            d.trigger = set_trigger_source(d.trigger.clone(), f);
                                            draft.set(d);
                                        }
                                    }
                                    label { class: "flex items-center",
                                        "Target"
                                        span {
                                            class: "help-icon",
                                            title: "Who must receive this effect for it to trigger (e.g., Any = track on anyone, Local Player = only on you)",
                                            "?"
                                        }
                                    }
                                    EntityFilterDropdown {
                                        label: "",
                                        value: get_trigger_filters(&draft().trigger).1.clone(),
                                        options: EntityFilter::target_options(),
                                        on_change: move |f| {
                                            let mut d = draft();
                                            d.trigger = set_trigger_target(d.trigger.clone(), f);
                                            draft.set(d);
                                        }
                                    }
                                }

                                // Effects or Trigger Abilities (based on trigger type)
                                div { class: "form-row-hz", style: "align-items: flex-start;",
                                    if trigger_type() == EffectTriggerType::EffectBased {
                                        EffectSelectorEditor {
                                            label: "Effects",
                                            selectors: get_trigger_effects(&draft().trigger).1,
                                            on_change: move |selectors| {
                                                let mut d = draft();
                                                d.trigger = set_trigger_effects(d.trigger.clone(), selectors);
                                                draft.set(d);
                                            }
                                        }
                                    } else {
                                        TriggerAbilitiesEditor {
                                            abilities: get_trigger_abilities(&draft().trigger),
                                            on_change: move |abilities| {
                                                let mut d = draft();
                                                d.trigger = set_trigger_abilities(d.trigger.clone(), abilities);
                                                draft.set(d);
                                            }
                                        }
                                    }
                                }

                                if !draft().is_alert {
                                    // Refresh Abilities
                                    div { class: "form-row-hz", style: "align-items: flex-start;",
                                        AbilitySelectorEditor {
                                            label: "Refresh Abilities",
                                            selectors: draft().refresh_abilities.iter().map(|r| r.ability().clone()).collect(),
                                            on_change: move |ids: Vec<AbilitySelector>| {
                                                let mut d = draft();
                                                d.refresh_abilities = ids.into_iter().map(RefreshAbility::Simple).collect();
                                                draft.set(d);
                                            }
                                        }
                                    }

                                    // ─── Behavior Options ──────────────────────────────────
                                    span { class: "text-sm font-bold text-secondary mt-sm", "Behavior" }

                                    label {
                                        class: "flex items-center gap-xs text-sm mt-xs",
                                        input {
                                            r#type: "checkbox",
                                            checked: draft().is_aoe_refresh,
                                            onchange: move |e| {
                                                let mut d = draft();
                                                d.is_aoe_refresh = e.checked();
                                                draft.set(d);
                                            }
                                        }
                                        span { class: "flex items-center",
                                            "AoE Refresh"
                                            span {
                                                class: "help-icon",
                                                title: "Use damage correlation to detect multi-target refreshes (for abilities like Corrosive Grenade)",
                                                "?"
                                            }
                                        }
                                    }

                                    label {
                                        class: "flex items-center gap-xs text-sm",
                                        input {
                                            r#type: "checkbox",
                                            checked: draft().is_refreshed_on_modify,
                                            onchange: move |e| {
                                                let mut d = draft();
                                                d.is_refreshed_on_modify = e.checked();
                                                draft.set(d);
                                            }
                                        }
                                        span { class: "flex items-center",
                                            "Refresh Duration When Charges Modified"
                                            span {
                                                class: "help-icon",
                                                title: "Reset timer when effect stacks change",
                                                "?"
                                            }
                                        }
                                    }

                                    label {
                                        class: "flex items-center gap-xs text-sm",
                                        input {
                                            r#type: "checkbox",
                                            checked: draft().persist_past_death,
                                            onchange: move |e| {
                                                let mut d = draft();
                                                d.persist_past_death = e.checked();
                                                draft.set(d);
                                            }
                                        }
                                        span { class: "flex items-center",
                                            "Persist Past Death"
                                            span {
                                                class: "help-icon",
                                                title: "Keep showing effect after player dies",
                                                "?"
                                            }
                                        }
                                    }

                                    label {
                                        class: "flex items-center gap-xs text-sm",
                                        input {
                                            r#type: "checkbox",
                                            checked: draft().track_outside_combat,
                                            onchange: move |e| {
                                                let mut d = draft();
                                                d.track_outside_combat = e.checked();
                                                draft.set(d);
                                            }
                                        }
                                        span { class: "flex items-center",
                                            "Track Outside Combat"
                                            span {
                                                class: "help-icon",
                                                title: "Continue tracking this effect between combat encounters",
                                                "?"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ═══ RIGHT COLUMN: Timing, Alerts, Audio ═════════════════════════
                    div { class: "effect-edit-right",

                        // ─── Timing Card ─────────────────────────────────────────────
                        div { class: "form-card",
                            div { class: "form-card-header",
                                i { class: "fa-solid fa-clock" }
                                span { "Timing" }
                            }
                            div { class: "form-card-content",
                                // Instant Alert Only
                                div { class: "form-row-hz",
                                    label { class: "flex items-center",
                                        "Instant Alert Only"
                                        span {
                                            class: "help-icon",
                                            title: "Shows a brief alert notification instead of tracking the effect. No duration, no overlay bar/icon — only alert text and audio fire on trigger.",
                                            "?"
                                        }
                                    }
                                    input {
                                        r#type: "checkbox",
                                        checked: draft().is_alert,
                                        onchange: move |e| {
                                            let mut d = draft();
                                            d.is_alert = e.checked();
                                            draft.set(d);
                                        }
                                    }
                                }

                                if !draft().is_alert {
                                // Duration
                                div { class: "form-row-hz",
                                    label { class: "flex items-center",
                                        "Duration"
                                        span {
                                            class: "help-icon",
                                            title: "How long the effect displays (seconds). Set to 0 for effects that track via game events",
                                            "?"
                                        }
                                    }
                                    input {
                                        r#type: "number",
                                        class: "input-inline",
                                        style: "width: 70px;",
                                        step: "any",
                                        min: "0",
                                        value: "{draft().duration_secs.unwrap_or(0.0)}",
                                        oninput: move |e| {
                                            let mut d = draft();
                                            d.duration_secs = e.value().parse::<f32>().ok().filter(|&v| v > 0.0);
                                            draft.set(d);
                                        }
                                    }
                                    span { class: "text-muted", "sec" }
                                }

                                // Show at
                                div { class: "form-row-hz",
                                    label { class: "flex items-center",
                                        "Show at"
                                        span {
                                            class: "help-icon",
                                            title: "Only display the effect when this many seconds remain. 0 = always visible",
                                            "?"
                                        }
                                    }
                                    input {
                                        r#type: "number",
                                        class: "input-inline",
                                        style: "width: 50px;",
                                        step: "any",
                                        min: "0",
                                        max: "{draft().duration_secs.unwrap_or(999.0)}",
                                        value: "{draft().show_at_secs}",
                                        oninput: move |e| {
                                            if let Ok(val) = e.value().parse::<f32>() {
                                                let mut d = draft();
                                                let max_val = d.duration_secs.unwrap_or(f32::MAX);
                                                d.show_at_secs = val.min(max_val).max(0.0);
                                                draft.set(d);
                                            }
                                        }
                                    }
                                    span { class: "text-sm text-secondary", "sec remaining" }
                                }

                                // Duration Affected by Alacrity
                                label { class: "flex items-center gap-xs text-sm",
                                    input {
                                        r#type: "checkbox",
                                        checked: draft().is_affected_by_alacrity,
                                        onchange: move |e| {
                                            let mut d = draft();
                                            d.is_affected_by_alacrity = e.checked();
                                            draft.set(d);
                                        }
                                    }
                                    span { class: "flex items-center",
                                        "Duration Affected by Alacrity"
                                        span {
                                            class: "help-icon",
                                            title: "Adjusts the effect duration based on the player's alacrity stat",
                                            "?"
                                        }
                                    }
                                }

                                // Fixed Duration - hide for Cooldowns (they always ignore effect removed)
                                if draft().display_target != DisplayTarget::Cooldowns {
                                    label { class: "flex items-center gap-xs text-sm",
                                        input {
                                            r#type: "checkbox",
                                            checked: draft().ignore_effect_removed,
                                            onchange: move |e| {
                                                let mut d = draft();
                                                d.ignore_effect_removed = e.checked();
                                                draft.set(d);
                                            }
                                        }
                                        span { class: "flex items-center",
                                            "Fixed Duration"
                                            span {
                                                class: "help-icon",
                                                title: "Use the duration timer instead of tracking when the game removes the effect",
                                                "?"
                                            }
                                        }
                                    }
                                }

                                // Cooldown Ready Secs (only for Cooldowns display target)
                                if draft().display_target == DisplayTarget::Cooldowns {
                                    div { class: "form-row-hz",
                                        label { class: "flex items-center",
                                            "Ready State"
                                            span {
                                                class: "help-icon",
                                                title: "Seconds before expiration to show the cooldown as ready",
                                                "?"
                                            }
                                        }
                                        input {
                                            r#type: "number",
                                            class: "input-inline",
                                            style: "width: 60px;",
                                            step: "0.1",
                                            min: "0",
                                            value: "{draft().cooldown_ready_secs}",
                                            oninput: move |e| {
                                                if let Ok(val) = e.value().parse::<f32>() {
                                                    let mut d = draft();
                                                    d.cooldown_ready_secs = val.max(0.0);
                                                    draft.set(d);
                                                }
                                            }
                                        }
                                        span { class: "text-sm text-muted", "sec" }
                                    }
                                }
                                } // end if !draft().is_alert
                            }
                        }

                        // ─── Alerts Card ─────────────────────────────────────────────
                        div { class: "form-card",
                            div { class: "form-card-header",
                                i { class: "fa-solid fa-bell" }
                                span { "Alerts" }
                            }
                            div { class: "form-card-content",
                                div { class: "form-row-hz",
                                    label { class: "flex items-center",
                                        "Alert Text"
                                        span {
                                            class: "help-icon",
                                            title: "Text shown in the alert notification. Leave blank for no alert",
                                            "?"
                                        }
                                    }
                                    input {
                                        class: "input-inline",
                                        r#type: "text",
                                        style: "width: 220px;",
                                        placeholder: "(none)",
                                        value: "{draft().alert_text.clone().unwrap_or_default()}",
                                        oninput: move |e| {
                                            let mut d = draft();
                                            d.alert_text = if e.value().is_empty() { None } else { Some(e.value()) };
                                            draft.set(d);
                                        }
                                    }
                                }

                                if draft().is_alert {
                                    div { class: "form-row-hz",
                                        label { class: "flex items-center",
                                            "Alert On"
                                            span {
                                                class: "help-icon",
                                                title: "Instant alerts always fire when triggered",
                                                "?"
                                            }
                                        }
                                        span { class: "text-sm text-secondary", "On trigger (instant)" }
                                    }
                                } else {
                                    div { class: "form-row-hz",
                                        label { class: "flex items-center",
                                            "Alert On"
                                            span {
                                                class: "help-icon",
                                                title: "When to show alert text: on effect start, on effect end, or never",
                                                "?"
                                            }
                                        }
                                        select {
                                            class: "select-inline",
                                            value: "{effect_alert_label(&draft().alert_on)}",
                                            onchange: move |e| {
                                                let mut d = draft();
                                                d.alert_on = match e.value().as_str() {
                                                    "Effect Start" => AlertTrigger::OnApply,
                                                    "Effect End" => AlertTrigger::OnExpire,
                                                    _ => AlertTrigger::None,
                                                };
                                                draft.set(d);
                                            },
                                            for trigger in AlertTrigger::all() {
                                                {
                                                    let label = effect_alert_label(trigger);
                                                    rsx! {
                                                        option {
                                                            value: "{label}",
                                                            selected: *trigger == draft().alert_on,
                                                            "{label}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // ─── Audio Card ──────────────────────────────────────────────
                        div { class: "form-card",
                            div { class: "form-card-header",
                                i { class: "fa-solid fa-volume-up" }
                                span { "Audio" }
                            }
                            div { class: "form-card-content",
                                label { class: "flex items-center gap-xs text-sm",
                                    input {
                                        r#type: "checkbox",
                                        checked: draft().audio.enabled,
                                        onchange: move |e| {
                                            let mut d = draft();
                                            d.audio.enabled = e.checked();
                                            draft.set(d);
                                        }
                                    }
                                    "Enable Audio"
                                }

                                if draft().audio.enabled {
                                    div { class: "form-row-hz mt-sm",
                                        label { "Sound" }
                                        div { class: "flex items-center gap-xs",
                                            select {
                                                class: "select-inline",
                                                style: "width: 140px;",
                                                value: "{draft().audio.file.clone().unwrap_or_default()}",
                                                onchange: move |e| {
                                                    let mut d = draft();
                                                    d.audio.file = if e.value().is_empty() { None } else { Some(e.value()) };
                                                    draft.set(d);
                                                },
                                                option { value: "", selected: draft().audio.file.is_none(), "(none)" }
                                                for name in sound_files().iter() {
                                                    {
                                                        let is_selected = draft().audio.file.as_deref() == Some(name.as_str());
                                                        rsx! {
                                                            option { key: "{name}", value: "{name}", selected: is_selected, "{name}" }
                                                        }
                                                    }
                                                }
                                                // Show custom path if set and not in the bundled list
                                                if let Some(ref path) = draft().audio.file {
                                                    if !path.is_empty() && !sound_files().contains(path) {
                                                        option { value: "{path}", selected: true, "{path} (custom)" }
                                                    }
                                                }
                                            }
                                            button {
                                                class: "btn btn-sm",
                                                r#type: "button",
                                                onclick: move |_| {
                                                    spawn(async move {
                                                        if let Some(path) = api::pick_audio_file().await {
                                                            let lower = path.to_lowercase();
                                                            if lower.ends_with(".mp3") || lower.ends_with(".wav") {
                                                                let mut d = draft();
                                                                d.audio.file = Some(path);
                                                                draft.set(d);
                                                            }
                                                        }
                                                    });
                                                },
                                                "Browse"
                                            }
                                            if draft().audio.file.is_some() {
                                                button {
                                                    class: "btn btn-sm",
                                                    r#type: "button",
                                                    title: "Preview sound",
                                                    onclick: move |_| {
                                                        if let Some(ref file) = draft().audio.file {
                                                            let file = file.clone();
                                                            spawn(async move {
                                                                api::preview_sound(&file).await;
                                                            });
                                                        }
                                                    },
                                                    "Play"
                                                }
                                            }
                                        }
                                    }

                                    if !draft().is_alert {
                                        div { class: "form-row-hz",
                                            label { class: "flex items-center",
                                                "Audio Offset"
                                                span {
                                                    class: "help-icon",
                                                    title: "When to play the sound relative to effect expiration",
                                                    "?"
                                                }
                                            }
                                            select {
                                                class: "select-inline",
                                                style: "width: 120px;",
                                                value: "{draft().audio.offset}",
                                                onchange: move |e| {
                                                    if let Ok(val) = e.value().parse::<u8>() {
                                                        let mut d = draft();
                                                        d.audio.offset = val;
                                                        draft.set(d);
                                                    }
                                                },
                                                option { value: "0", "On expiration" }
                                                option { value: "1", "1s before" }
                                                option { value: "2", "2s before" }
                                                option { value: "3", "3s before" }
                                                option { value: "4", "4s before" }
                                                option { value: "5", "5s before" }
                                                option { value: "6", "6s before" }
                                                option { value: "7", "7s before" }
                                                option { value: "8", "8s before" }
                                                option { value: "9", "9s before" }
                                                option { value: "10", "10s before" }
                                            }
                                        }

                                        div { class: "form-row-hz",
                                            label { class: "flex items-center",
                                                "Voice"
                                                span {
                                                    class: "help-icon",
                                                    title: "Voice countdown starting at the specified seconds remaining",
                                                    "?"
                                                }
                                            }
                                            div { class: "flex items-center gap-md",
                                                select {
                                                    class: "select-inline",
                                                    style: "width: 80px;",
                                                    value: "{draft().audio.countdown_start}",
                                                    onchange: move |e| {
                                                        if let Ok(val) = e.value().parse::<u8>() {
                                                            let mut d = draft();
                                                            d.audio.countdown_start = val;
                                                            draft.set(d);
                                                        }
                                                    },
                                                    option { value: "0", "Off" }
                                                    option { value: "3", "3s" }
                                                    option { value: "5", "5s" }
                                                    option { value: "10", "10s" }
                                                }
                                                select {
                                                    class: "select-inline",
                                                    style: "width: 100px;",
                                                    value: "{draft().audio.countdown_voice.clone().unwrap_or_else(|| \"Amy\".to_string())}",
                                                    onchange: move |e| {
                                                        let mut d = draft();
                                                        d.audio.countdown_voice = if e.value() == "Amy" { None } else { Some(e.value()) };
                                                        draft.set(d);
                                                    },
                                                    option { value: "Amy", "Amy" }
                                                    option { value: "Jim", "Jim" }
                                                    option { value: "Yolo", "Yolo" }
                                                    option { value: "Nerevar", "Nerevar" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // Actions (outside grid, full width)
                div { class: "form-actions",
                    button {
                        class: "btn-save",
                        disabled: !has_changes(),
                        onclick: move |_| {
                            just_saved.set(true);
                            on_save.call(draft());
                        },
                        "Save"
                    }

                    if !is_draft {
                        button {
                            class: "btn-duplicate",
                            onclick: move |_| on_duplicate.call(()),
                            "Duplicate"
                        }
                    }

                    if !is_draft && effect.is_user_override {
                        button {
                            class: "btn-duplicate",
                            onclick: move |_| on_export.call(()),
                            "Export"
                        }
                    }

                    if is_draft {
                        // For drafts, show Cancel button (no confirmation needed)
                        button {
                            class: "btn-delete",
                            onclick: move |_| on_delete.call(()),
                            "Cancel"
                        }
                    } else if is_bundled && !is_user_override {
                        // Pure built-in, unmodified — no delete/reset action needed
                    } else if is_bundled && is_user_override {
                        // Modified built-in — offer "Reset to Built-in"
                        if confirm_delete() {
                            span { class: "delete-confirm",
                                "Reset to built-in? "
                                button {
                                    class: "btn-delete-no",
                                    style: "background: var(--color-warning, #f39c12); border-color: var(--color-warning, #f39c12); color: #000; font-weight: 600;",
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
                                style: "background: var(--color-warning, #f39c12); border-color: var(--color-warning, #f39c12); color: #000;",
                                onclick: move |_| confirm_delete.set(true),
                                "Reset to Built-in"
                            }
                        }
                    } else {
                        // Custom item — offer Delete with confirmation
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
                }

            }
        }
}

// ─────────────────────────────────────────────────────────────────────────────
// Discipline Selector (multi-select dropdown)
// ─────────────────────────────────────────────────────────────────────────────

// All discipline display names grouped by class (Imp / Rep mirror pairs)
const ALL_DISCIPLINES: &[(&str, &[&str])] = &[
    ("Sorcerer / Sage", &["Lightning", "Madness", "Corruption", "Telekinetics", "Balance", "Seer"]),
    ("Assassin / Shadow", &["Hatred", "Darkness", "Deception", "Infiltration", "Kinetic Combat", "Serenity"]),
    ("Juggernaut / Guardian", &["Vengeance", "Immortal", "Rage", "Focus", "Vigilance", "Defense"]),
    ("Marauder / Sentinel", &["Annihilation", "Carnage", "Fury", "Combat", "Watchman", "Concentration"]),
    ("Mercenary / Commando", &["Arsenal", "Innovative Ordnance", "Bodyguard", "Gunnery", "Assault Specialist", "Combat Medic"]),
    ("Powertech / Vanguard", &["Shield Tech", "Pyrotech", "Advanced Prototype", "Plasmatech", "Shield Specialist", "Tactics"]),
    ("Operative / Scoundrel", &["Concealment", "Lethality", "Medicine", "Scrapper", "Ruffian", "Sawbones"]),
    ("Sniper / Gunslinger", &["Marksmanship", "Engineering", "Virulence", "Sharpshooter", "Saboteur", "Dirty Fighting"]),
];

#[component]
fn DisciplineSelector(
    selected: Vec<String>,
    on_change: EventHandler<Vec<String>>,
) -> Element {
    let mut dropdown_open = use_signal(|| false);
    let mut dropdown_pos = use_signal(|| (0.0f64, 0.0f64));

    let display = if selected.is_empty() {
        "(all disciplines)".to_string()
    } else if selected.len() == 1 {
        selected[0].clone()
    } else {
        format!("{} disciplines", selected.len())
    };

    rsx! {
        div {
            class: "discipline-selector",
            button {
                class: "select",
                style: "width: 200px; text-align: left;",
                onclick: move |e| {
                    if !dropdown_open() {
                        let click = e.client_coordinates();
                        let offset = e.element_coordinates();
                        let btn_left = click.x - offset.x;
                        let btn_bottom = click.y - offset.y + 30.0;
                        dropdown_pos.set((btn_left, btn_bottom));
                    }
                    dropdown_open.set(!dropdown_open());
                },
                "{display}"
                span { class: "ml-auto", "▾" }
            }

            if dropdown_open() {
                // Invisible backdrop to catch clicks outside the dropdown
                div {
                    style: "position: fixed; inset: 0; z-index: 9999;",
                    onclick: move |_| dropdown_open.set(false),
                }
                div {
                    class: "discipline-dropdown",
                    style: "position: fixed; left: {dropdown_pos().0}px; top: {dropdown_pos().1}px; z-index: 10000; background: #1e1e2e; border: 1px solid var(--border-medium); border-radius: var(--radius-sm); padding: var(--space-xs); min-width: 260px; max-height: 400px; overflow-y: auto; box-shadow: 0 4px 12px rgba(0,0,0,0.5);",

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
                        "(all disciplines)"
                    }

                    // Grouped disciplines
                    for (group_name, disciplines) in ALL_DISCIPLINES.iter() {
                        {
                            let selected_clone = selected.clone();
                            rsx! {
                                div { class: "text-xs text-muted",
                                    style: "padding: 6px 4px 2px 4px; font-weight: 600; border-top: 1px solid var(--border-subtle); margin-top: 2px;",
                                    "{group_name}"
                                }
                                for disc in disciplines.iter() {
                                    {
                                        let disc_name = disc.to_string();
                                        let is_selected = selected_clone.contains(&disc_name);
                                        let selected_for_change = selected_clone.clone();

                                        rsx! {
                                            label { class: "flex items-center gap-xs text-sm p-xs cursor-pointer",
                                                input {
                                                    r#type: "checkbox",
                                                    checked: is_selected,
                                                    onchange: move |_| {
                                                        let mut new_selected = selected_for_change.clone();
                                                        if is_selected {
                                                            new_selected.retain(|d| d != &disc_name);
                                                        } else {
                                                            new_selected.push(disc_name.clone());
                                                        }
                                                        on_change.call(new_selected);
                                                    }
                                                }
                                                "{disc}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

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

// ─────────────────────────────────────────────────────────────────────────────
// Trigger Abilities Editor (for AbilityCast triggers)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn TriggerAbilitiesEditor(
    abilities: Vec<AbilitySelector>,
    on_change: EventHandler<Vec<AbilitySelector>>,
) -> Element {
    let mut new_input = use_signal(String::new);

    let abilities_for_keydown = abilities.clone();
    let abilities_for_click = abilities.clone();

    rsx! {
        div { class: "flex-col gap-xs items-start",
            span { class: "text-sm text-secondary text-left", "Trigger Abilities:" }

            // Ability chips
            div { class: "flex flex-wrap gap-xs",
                for (idx, sel) in abilities.iter().enumerate() {
                    {
                        let abilities_clone = abilities.clone();
                        let display = sel.display();
                        rsx! {
                            span { class: "chip",
                                "{display}"
                                button {
                                    class: "chip-remove",
                                    onclick: move |_| {
                                        let mut new_abs = abilities_clone.clone();
                                        new_abs.remove(idx);
                                        on_change.call(new_abs);
                                    },
                                    "×"
                                }
                            }
                        }
                    }
                }
            }

            // Add new ability
            div { class: "flex gap-xs",
                input {
                    r#type: "text",
                    class: "input-inline",
                    style: "width: 180px;",
                    placeholder: "Ability ID or Name (Enter)",
                    value: "{new_input}",
                    oninput: move |e| new_input.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter && !new_input().trim().is_empty() {
                            let selector = AbilitySelector::from_input(&new_input());
                            let mut new_abs = abilities_for_keydown.clone();
                            if !new_abs.iter().any(|s| s.display() == selector.display()) {
                                new_abs.push(selector);
                                on_change.call(new_abs);
                            }
                            new_input.set(String::new());
                        }
                    }
                }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| {
                        if !new_input().trim().is_empty() {
                            let selector = AbilitySelector::from_input(&new_input());
                            let mut new_abs = abilities_for_click.clone();
                            if !new_abs.iter().any(|s| s.display() == selector.display()) {
                                new_abs.push(selector);
                                on_change.call(new_abs);
                            }
                            new_input.set(String::new());
                        }
                    },
                    "Add"
                }
            }
        }
    }
}
