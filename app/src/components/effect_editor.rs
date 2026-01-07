//! Effect Editor Panel
//!
//! UI for viewing and editing effect definitions with:
//! - Grouped by file with collapsible headers
//! - Inline expansion for editing
//! - Full CRUD operations

use dioxus::prelude::*;
use std::collections::HashSet;

use super::encounter_editor::InlineNameCreator;
use super::encounter_editor::triggers::{
    AbilitySelectorEditor, EffectSelectorEditor, EntityFilterDropdown,
};
use crate::api;
use crate::types::{
    AbilitySelector, AudioConfig, EffectCategory, EffectListItem, EffectTriggerMode, EntityFilter,
    Trigger,
};

/// Create a default effect with sensible defaults
fn default_effect(name: String) -> EffectListItem {
    EffectListItem {
        id: String::new(),
        name,
        display_text: None,
        file_path: String::new(),
        enabled: true,
        category: EffectCategory::Hot,
        trigger: EffectTriggerMode::EffectApplied,
        start_trigger: None,
        fixed_duration: false,
        effects: vec![],
        refresh_abilities: vec![],
        source: EntityFilter::LocalPlayer,
        target: EntityFilter::GroupMembers,
        duration_secs: Some(15.0),
        can_be_refreshed: true,
        is_refreshed_on_modify: false,
        max_stacks: 1,
        color: Some([80, 200, 80, 255]),
        show_on_raid_frames: true,
        show_on_effects_overlay: false,
        show_at_secs: 0.0,
        persist_past_death: false,
        track_outside_combat: true,
        on_apply_trigger_timer: None,
        on_expire_trigger_timer: None,
        encounters: vec![],
        alert_near_expiration: false,
        alert_threshold_secs: 3.0,
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
        if effect.start_trigger.is_some() {
            Self::AbilityCast
        } else {
            Self::EffectBased
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Main Panel
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn EffectEditorPanel() -> Element {
    // State
    let mut effects = use_signal(Vec::<EffectListItem>::new);
    let mut search_query = use_signal(String::new);
    let mut expanded_effect = use_signal(|| None::<String>);
    let mut expanded_files = use_signal(HashSet::<String>::new);
    let mut loading = use_signal(|| true);
    let mut save_status = use_signal(String::new);
    let mut status_is_error = use_signal(|| false);

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
            EffectCategory::all()
                .iter()
                .position(|x| x == c)
                .unwrap_or(99)
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
        let new_effect = default_effect(name);
        spawn(async move {
            if let Some(created) = api::create_effect_definition(&new_effect).await {
                let created_id = created.id.clone();
                let cat = created.category;
                let mut current = effects();
                current.push(created);
                effects.set(current);
                // Auto-expand the category and the new effect
                let mut cats = expanded_files();
                cats.insert(cat.label().to_string());
                expanded_files.set(cats);
                expanded_effect.set(Some(created_id));
                save_status.set("Created".to_string());
                status_is_error.set(false);
            } else {
                save_status.set("Failed to create".to_string());
                status_is_error.set(true);
            }
        });
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

    // Clones for toggle handlers
    let effect_for_enable = effect.clone();
    let effect_for_audio = effect.clone();
    let effect_for_raid = effect.clone();
    let effect_for_overlay = effect.clone();

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
                    if let Some(ref dt) = effect.display_text {
                        if dt != &effect.name {
                            span { class: "effect-display-text", "→ \"{dt}\"" }
                        }
                    }
                    span { class: "effect-id-inline", "{effect.id}" }
                    span { class: "effect-category-badge", "{effect.category.label()}" }

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

                    // Raid frames toggle
                    span {
                        class: "row-toggle",
                        title: if effect.show_on_raid_frames { "Hide on raid frames" } else { "Show on raid frames" },
                        onclick: move |e| {
                            e.stop_propagation();
                            let mut updated = effect_for_raid.clone();
                            updated.show_on_raid_frames = !updated.show_on_raid_frames;
                            on_save.call(updated);
                        },
                        span {
                            class: if effect.show_on_raid_frames { "text-info" } else { "text-muted" },
                            if effect.show_on_raid_frames { "⊞" } else { "✗" }
                        }
                    }

                    // Effects overlay toggle
                    span {
                        class: "row-toggle",
                        title: if effect.show_on_effects_overlay { "Hide on effects overlay" } else { "Show on effects overlay" },
                        onclick: move |e| {
                            e.stop_propagation();
                            let mut updated = effect_for_overlay.clone();
                            updated.show_on_effects_overlay = !updated.show_on_effects_overlay;
                            on_save.call(updated);
                        },
                        span {
                            class: if effect.show_on_effects_overlay { "text-warning" } else { "text-muted" },
                            if effect.show_on_effects_overlay { "◎" } else { "✗" }
                        }
                    }
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
    let mut trigger_type = use_signal(|| EffectTriggerType::from_effect(&effect));

    let effect_original = effect.clone();
    let has_changes = use_memo(move || draft() != effect_original);

    let color = draft().color.unwrap_or([128, 128, 128, 255]);
    let color_hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);

    rsx! {
            div { class: "effect-edit-form",
                div { class: "effect-edit-grid",
                    // ═══ LEFT COLUMN: Main Effect Settings ═══════════════════════════
                    div { class: "effect-edit-left",
                        // Effect ID (read-only)
                        div { class: "form-row-hz",
                            label { "Effect ID" }
                            code { class: "effect-id-display", "{effect.id}" }
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
                                    match new_type {
                                        EffectTriggerType::EffectBased => {
                                            d.start_trigger = None;
                                        }
                                        EffectTriggerType::AbilityCast => {
                                            d.effects = vec![];
                                            if d.start_trigger.is_none() {
                                                d.start_trigger = Some(Trigger::AbilityCast {
                                                    abilities: vec![],
                                                    source: EntityFilter::LocalPlayer,
                                                });
                                            }
                                        }
                                    }
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
                                    value: "{draft().trigger.label()}",
                                    onchange: move |e| {
                                        let mut d = draft();
                                        d.trigger = match e.value().as_str() {
                                            "Effect Applied" => EffectTriggerMode::EffectApplied,
                                            "Effect Removed" => EffectTriggerMode::EffectRemoved,
                                            _ => d.trigger,
                                        };
                                        draft.set(d);
                                    },
                                    for trigger in EffectTriggerMode::all() {
                                        option { value: "{trigger.label()}", "{trigger.label()}" }
                                    }
                                }
                            }
                        }

                        // Source and Target filters
                        div { class: "form-row-hz",
                            label { "Source" }
                            EntityFilterDropdown {
                                label: "",
                                value: draft().source.clone(),
                                options: EntityFilter::source_options(),
                                on_change: move |f| {
                                    let mut d = draft();
                                    d.source = f;
                                    draft.set(d);
                                }
                            }
                            label { "Target" }
                            EntityFilterDropdown {
                                label: "",
                                value: draft().target.clone(),
                                options: EntityFilter::target_options(),
                                on_change: move |f| {
                                    let mut d = draft();
                                    d.target = f;
                                    draft.set(d);
                                }
                            }
                        }

                        // Duration, Max Stacks
                        div { class: "form-row-hz",
                            label { "Duration" }
                            input {
                                r#type: "number",
                                class: "input-inline",
                                style: "width: 70px;",
                                step: ".1",
                                min: "0",
                                value: "{draft().duration_secs.unwrap_or(0.0) as u32}",
                                oninput: move |e| {
                                    let mut d = draft();
                                    d.duration_secs = e.value().parse::<f32>().ok().filter(|&v| v > 0.0);
                                    draft.set(d);
                                }
                            }
                            span { class: "text-muted", "sec" }
                            label { "Max Stacks" }
                            input {
                                r#type: "number",
                                class: "input-inline",
                                style: "width: 50px;",
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
                            label { "Show at" }
                            input {
                                r#type: "number",
                                class: "input-inline",
                                style: "width: 50px;",
                                min: "0",
                                max: "{draft().duration_secs.unwrap_or(999.0) as u32}",
                                value: "{draft().show_at_secs as u32}",
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
                                    selectors: draft().effects.clone(),
                                    on_change: move |selectors| {
                                        let mut d = draft();
                                        d.effects = selectors;
                                        draft.set(d);
                                    }
                                }
                            } else {
                                TriggerAbilitiesEditor {
                                    abilities: draft().start_trigger.as_ref()
                                        .and_then(|t| match t {
                                            Trigger::AbilityCast { abilities, .. } => Some(abilities.clone()),
                                            _ => None,
                                        })
                                        .unwrap_or_default(),
                                    on_change: move |abilities| {
                                        let mut d = draft();
                                        if let Some(Trigger::AbilityCast { source, .. }) = &d.start_trigger {
                                            d.start_trigger = Some(Trigger::AbilityCast {
                                                abilities,
                                                source: source.clone(),
                                            });
                                        } else {
                                            d.start_trigger = Some(Trigger::AbilityCast {
                                                abilities,
                                                source: EntityFilter::LocalPlayer,
                                            });
                                        }
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

                        label { class: "flex items-center gap-xs text-sm",
                            input {
                                r#type: "checkbox",
                                checked: draft().show_on_raid_frames,
                                onchange: move |e| {
                                    let mut d = draft();
                                    d.show_on_raid_frames = e.checked();
                                    draft.set(d);
                                }
                            }
                            "Show on Raid Frames"
                        }

                        label { class: "flex items-center gap-xs text-sm",
                            input {
                                r#type: "checkbox",
                                checked: draft().show_on_effects_overlay,
                                onchange: move |e| {
                                    let mut d = draft();
                                    d.show_on_effects_overlay = e.checked();
                                    draft.set(d);
                                }
                            }
                            "Show on Effects Overlay"
                        }

                        label { class: "flex items-center gap-xs text-sm",
                            input {
                                r#type: "checkbox",
                                checked: draft().fixed_duration,
                                title: "Ignore game EffectRemoved - only expire via duration timer",
                                onchange: move |e| {
                                    let mut d = draft();
                                    d.fixed_duration = e.checked();
                                    draft.set(d);
                                }
                            }
                            "Fixed Duration (for cooldowns)"
                        }

                        label { class: "flex items-center gap-xs text-sm",
                            input {
                                r#type: "checkbox",
                                checked: draft().can_be_refreshed,
                                onchange: move |e| {
                                    let mut d = draft();
                                    d.can_be_refreshed = e.checked();
                                    draft.set(d);
                                }
                            }
                            "Can be refreshed"
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
                            "Refresh duration when charges are modified"
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
