//! Effect Editor Panel
//!
//! UI for viewing and editing effect definitions with:
//! - Grouped by file with collapsible headers
//! - Inline expansion for editing
//! - Full CRUD operations

use dioxus::prelude::*;

use super::encounter_editor::InlineNameCreator;
use super::encounter_editor::triggers::{
    AbilitySelectorEditor, EffectSelectorEditor, EntityFilterDropdown,
};
use crate::api;
use crate::types::{
    AbilitySelector, AudioConfig, DisplayTarget, EffectCategory, EffectListItem, EffectSelector,
    EntityFilter, Trigger,
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
        | Trigger::DamageTaken { source, target, .. } => (source.clone(), target.clone()),
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
        Trigger::AbilityCast { abilities, .. } => abilities.clone(),
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
        other => other,
    }
}

/// Create a default effect with sensible defaults
fn default_effect(name: String) -> EffectListItem {
    EffectListItem {
        id: String::new(),
        name,
        display_text: None,
        file_path: String::new(),
        enabled: true,
        category: EffectCategory::Hot,
        trigger: Trigger::EffectApplied {
            effects: vec![],
            source: EntityFilter::LocalPlayer,
            target: EntityFilter::GroupMembers,
        },
        ignore_effect_removed: false,
        refresh_abilities: vec![],
        duration_secs: Some(15.0),
        is_refreshed_on_modify: false,
        color: Some([80, 200, 80, 255]),
        show_at_secs: 0.0,
        display_target: DisplayTarget::None,
        icon_ability_id: None,
        show_icon: true,
        is_affected_by_alacrity: false,
        cooldown_ready_secs: 0.0,
        persist_past_death: false,
        track_outside_combat: true,
        on_apply_trigger_timer: None,
        on_expire_trigger_timer: None,
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
}

impl EffectTriggerType {
    fn label(&self) -> &'static str {
        match self {
            Self::EffectBased => "Effect Based",
            Self::AbilityCast => "Ability Cast",
        }
    }

    fn all() -> &'static [Self] {
        &[Self::EffectBased, Self::AbilityCast]
    }

    /// Determine trigger type from effect data
    fn from_effect(effect: &EffectListItem) -> Self {
        match &effect.trigger {
            Trigger::AbilityCast { .. } => Self::AbilityCast,
            _ => Self::EffectBased,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Main Panel
// ─────────────────────────────────────────────────────────────────────────────

/// Marker ID for a draft effect that hasn't been saved yet
const DRAFT_EFFECT_ID: &str = "__new_draft__";

#[component]
pub fn EffectEditorPanel() -> Element {
    // State
    let mut effects = use_signal(Vec::<EffectListItem>::new);
    let mut search_query = use_signal(String::new);
    let mut expanded_effect = use_signal(|| None::<String>);
    let mut loading = use_signal(|| true);
    let mut save_status = use_signal(String::new);
    let mut status_is_error = use_signal(|| false);
    // Draft for new effects - not yet saved to backend
    let mut draft_effect = use_signal(|| None::<EffectListItem>);

    // Load effects on mount
    use_future(move || async move {
        if let Some(e) = api::get_effect_definitions().await {
            effects.set(e);
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
                    || e.display_target.label().to_lowercase().contains(&query)
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
        let filtered: Vec<_> = current.into_iter().filter(|e| e.id != effect_id).collect();
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
                let new_id = new_effect.id.clone();
                let mut current = effects();
                current.push(new_effect);
                effects.set(current);
                expanded_effect.set(Some(new_id));
                save_status.set("Duplicated".to_string());
                status_is_error.set(false);
            } else {
                save_status.set("Failed to duplicate".to_string());
                status_is_error.set(true);
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
                    placeholder: "Search by name, ID, or category...",
                    value: "{search_query}",
                    class: "effect-search-input",
                    oninput: move |e| search_query.set(e.value())
                }
            }

            // Effect list (flat)
            if loading() {
                div { class: "effect-loading", "Loading effects..." }
            } else if filtered_effects().is_empty() && draft_effect().is_none() {
                div { class: "effect-empty",
                    if effects().is_empty() {
                        "No effect definitions found"
                    } else {
                        "No effects match your search"
                    }
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
                                    on_cancel: move |_| {},
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
    #[props(default = false)] is_draft: bool,
    on_toggle: EventHandler<()>,
    on_save: EventHandler<EffectListItem>,
    on_delete: EventHandler<()>,
    on_duplicate: EventHandler<()>,
    #[props(default)] on_cancel: EventHandler<()>,
) -> Element {
    let color = effect.color.unwrap_or([128, 128, 128, 255]);
    let color_hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);

    // Clones for toggle handlers
    let effect_for_enable = effect.clone();
    let effect_for_audio = effect.clone();

    rsx! {
        div { class: if expanded { "effect-row expanded" } else { "effect-row" },
            div {
                class: "effect-row-summary",
                onclick: move |_| on_toggle.call(()),

                // Left side - expandable content
                div { class: "flex items-center gap-xs flex-1 min-w-0",
                    span { class: "effect-expand-icon",
                        if expanded { "▼" } else { "▶" }
                    }

                    span {
                        class: "effect-color-dot",
                        style: "background-color: {color_hex}"
                    }

                    span { class: "effect-name", "{effect.name}" }
                    if is_draft {
                        span { class: "effect-new-badge", "(New)" }
                    }
                    if let Some(ref dt) = effect.display_text {
                        if dt != &effect.name {
                            span { class: "effect-display-text", "→ \"{dt}\"" }
                        }
                    }
                    if !is_draft {
                        span { class: "effect-id-inline", "{effect.id}" }
                    }
                    span { class: "effect-target-badge", "{effect.display_target.label()}" }

                    if let Some(dur) = effect.duration_secs {
                        span { class: "effect-duration", "{dur:.0}s" }
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
    #[props(default = false)] is_draft: bool,
    on_save: EventHandler<EffectListItem>,
    on_delete: EventHandler<()>,
    on_duplicate: EventHandler<()>,
) -> Element {
    let mut draft = use_signal(|| effect.clone());
    let mut confirm_delete = use_signal(|| false);
    let mut trigger_type = use_signal(|| EffectTriggerType::from_effect(&effect));
    let mut icon_preview_url = use_signal(|| None::<String>);

    // Load icon preview - use explicit icon_ability_id, or fall back to first trigger effect ID
    let current_draft = draft();
    let preview_id = current_draft.icon_ability_id.or_else(|| {
        // Try to get first effect ID from trigger as fallback
        let (is_effect_trigger, effects) = get_trigger_effects(&current_draft.trigger);
        if is_effect_trigger {
            effects.first().and_then(|sel| match sel {
                EffectSelector::Id(id) => Some(*id),
                EffectSelector::Name(_) => None,
            })
        } else {
            None
        }
    });
    use_effect(move || {
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

    let effect_original = effect.clone();
    // For drafts, always enable save; for existing effects, only when changed
    let has_changes = use_memo(move || is_draft || draft() != effect_original);

    let color = draft().color.unwrap_or([128, 128, 128, 255]);
    let color_hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);

    rsx! {
            div { class: "effect-edit-form",
                div { class: "effect-edit-grid",
                    // ═══ LEFT COLUMN: Main Effect Settings ═══════════════════════════
                    div { class: "effect-edit-left",
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

                        // Category
                        div { class: "form-row-hz",
                            label { "Category" }
                            select {
                                class: "select-inline",
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

                        // Trigger Type and When
                        div { class: "form-row-hz",
                            label { "Trigger" }
                            select {
                                class: "select-inline",
                                value: "{trigger_type().label()}",
                                onchange: move |e| {
                                    let new_type = match e.value().as_str() {
                                        "Effect Based" => EffectTriggerType::EffectBased,
                                        "Ability Cast" => EffectTriggerType::AbilityCast,
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
                            label { "Source" }
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
                            label { "Target" }
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

                        // Duration and Show at
                        div { class: "form-row-hz",
                            label { "Duration" }
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
                            label { "Show at" }
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

                        // Refresh Abilities
                        div { class: "form-row-hz", style: "align-items: flex-start;",
                            AbilitySelectorEditor {
                                label: "Refresh Abilities",
                                selectors: draft().refresh_abilities.clone(),
                                on_change: move |ids| {
                                    let mut d = draft();
                                    d.refresh_abilities = ids;
                                    draft.set(d);
                                }
                            }
                        }

                    }

                    // ═══ RIGHT COLUMN: Checkboxes ════════════════════════════════════
                    div { class: "effect-edit-right",
                        div { class: "flex items-center gap-xs mb-sm",
                            label { class: "text-sm text-secondary", "Color" }
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
                            "Enabled"
                        }

                        // Display Target dropdown
                        div { class: "form-row-hz",
                            label { class: "text-sm text-secondary", "Display Target" }
                            select {
                                class: "select-inline",
                                onchange: move |e| {
                                    let mut d = draft();
                                    d.display_target = match e.value().as_str() {
                                        "None" => DisplayTarget::None,
                                        "Raid Frames" => DisplayTarget::RaidFrames,
                                        "Personal Buffs" => DisplayTarget::PersonalBuffs,
                                        "Personal Debuffs" => DisplayTarget::PersonalDebuffs,
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
                                        selected: *target == draft().display_target,
                                        "{target.label()}"
                                    }
                                }
                            }
                        }

                        // Icon Ability ID with preview
                        div { class: "form-row-hz",
                            label { class: "text-sm text-secondary", "Icon ID" }
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

                        label { class: "flex items-center gap-xs text-sm",
                            input {
                                r#type: "checkbox",
                                checked: draft().show_icon,
                                onchange: move |e| {
                                    let mut d = draft();
                                    d.show_icon = e.checked();
                                    draft.set(d);
                                }
                            }
                            "Show Icon"
                        }

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
                            "Affected by Alacrity"
                        }

                        // Cooldown Ready Secs (only for Cooldowns display target)
                        if draft().display_target == DisplayTarget::Cooldowns {
                            div { class: "form-row-hz",
                                label { class: "text-sm text-secondary", "Ready State" }
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

                        // Hide for Cooldowns - they always ignore effect removed events
                        if draft().display_target != DisplayTarget::Cooldowns {
                            label { class: "flex items-center gap-xs text-sm",
                                input {
                                    r#type: "checkbox",
                                    checked: draft().ignore_effect_removed,
                                    title: "Ignore game EffectRemoved - only expire via duration timer",
                                    onchange: move |e| {
                                        let mut d = draft();
                                        d.ignore_effect_removed = e.checked();
                                        draft.set(d);
                                    }
                                }
                                "Ignore Effect Removed"
                            }
                        }

                        label { class: "flex items-center gap-xs text-sm",
                            input {
                                r#type: "checkbox",
                                checked: draft().is_refreshed_on_modify,
                                onchange: move |e| {
                                    let mut d = draft();
                                    d.is_refreshed_on_modify = e.checked();
                                    draft.set(d);
                                }
                            }
                            "Refresh on Charge Change"
                        }

                        label { class: "flex items-center gap-xs text-sm",
                            input {
                                r#type: "checkbox",
                                checked: draft().persist_past_death,
                                onchange: move |e| {
                                    let mut d = draft();
                                    d.persist_past_death = e.checked();
                                    draft.set(d);
                                }
                            }
                            "Persist Past Death"
                        }

                        label { class: "flex items-center gap-xs text-sm",
                            input {
                                r#type: "checkbox",
                                checked: draft().track_outside_combat,
                                onchange: move |e| {
                                    let mut d = draft();
                                    d.track_outside_combat = e.checked();
                                    draft.set(d);
                                }
                            }
                            "Track Outside Combat"
                        }

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

                        // Audio settings (shown when audio enabled)
                        if draft().audio.enabled {
                             div { class: "form-row-hz",
                            label { "Alert Sound" }
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
                                    option { value: "", "(none)" }
                                    option { value: "Alarm.mp3", "Alarm.mp3" }
                                    option { value: "Alert.mp3", "Alert.mp3" }
                                    // Show custom path if set and not a bundled sound
                                    if let Some(ref path) = draft().audio.file {
                                        if !path.is_empty() && path != "Alarm.mp3" && path != "Alert.mp3" {
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
                                                // Validate extension
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
                            }
    }
                            div { class: "form-row-hz",
                                label { "Offset" }
                                select {
                                    class: "select-inline",
                                    value: "{draft().audio.offset}",
                                    onchange: move |e| {
                                        if let Ok(val) = e.value().parse::<u8>() {
                                            let mut d = draft();
                                            d.audio.offset = val;
                                            draft.set(d);
                                        }
                                    },
                                    option { value: "0", "On expire" }
                                    option { value: "1", "1s before" }
                                    option { value: "2", "2s before" }
                                    option { value: "3", "3s before" }
                                    option { value: "5", "5s before" }
                                }
                            }
                            div { class: "form-row-hz",
                                label { "Countdown" }
                                select {
                                    class: "select-inline",
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
                                }
                            }
                            div { class: "form-row-hz",
                                label { "Voice" }
                                select {
                                    class: "select-inline",
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

                        // Show At

                    }
                }

                // Actions (outside grid, full width)
                div { class: "form-actions",
                    button {
                        class: "btn-save",
                        disabled: !has_changes(),
                        onclick: move |_| on_save.call(draft()),
                        "Save"
                    }

                    if !is_draft {
                        button {
                            class: "btn-duplicate",
                            onclick: move |_| on_duplicate.call(()),
                            "Duplicate"
                        }
                    }

                    if is_draft {
                        // For drafts, show Cancel button (no confirmation needed)
                        button {
                            class: "btn-delete",
                            onclick: move |_| on_delete.call(()),
                            "Cancel"
                        }
                    } else if confirm_delete() {
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
