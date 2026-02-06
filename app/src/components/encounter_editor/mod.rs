//! Encounter Editor
//!
//! Full CRUD for the BossEncounter DSL: timers, phases, counters, challenges, entities.
//! Uses unified BossWithPath type and EncounterItem enum for streamlined data handling.

mod challenges;
mod conditions;
mod counters;
mod entities;
mod new_forms;
mod notes;
mod phases;
mod tabs;
mod timers;
pub mod triggers;

use dioxus::prelude::*;

// ─────────────────────────────────────────────────────────────────────────────
// Shared: Inline Name Creator
// ─────────────────────────────────────────────────────────────────────────────

/// Reusable inline name input component for creating new items.
/// Handles show/hide state internally. Calls `on_create` with the entered name.
#[component]
pub fn InlineNameCreator(
    button_label: &'static str,
    placeholder: &'static str,
    on_create: EventHandler<String>,
) -> Element {
    let mut show_input = use_signal(|| false);
    let mut name = use_signal(String::new);

    rsx! {
        if show_input() {
            div { class: "flex items-center gap-xs",
                input {
                    class: "input-inline",
                    r#type: "text",
                    placeholder: placeholder,
                    style: "width: 180px;",
                    value: "{name}",
                    autofocus: true,
                    oninput: move |e| name.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter && !name().is_empty() {
                            on_create.call(name());
                            show_input.set(false);
                            name.set(String::new());
                        } else if e.key() == Key::Escape {
                            show_input.set(false);
                            name.set(String::new());
                        }
                    }
                }
                button {
                    class: "btn btn-success btn-sm",
                    disabled: name().is_empty(),
                    onclick: move |_| {
                        if !name().is_empty() {
                            on_create.call(name());
                            show_input.set(false);
                            name.set(String::new());
                        }
                    },
                    "Create"
                }
                button {
                    class: "btn btn-ghost btn-sm",
                    onclick: move |_| {
                        show_input.set(false);
                        name.set(String::new());
                    },
                    "×"
                }
            }
        } else {
            button {
                class: "btn btn-success btn-sm",
                onclick: move |_| show_input.set(true),
                "{button_label}"
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared: NPC ID Chip Editor
// ─────────────────────────────────────────────────────────────────────────────

/// Chip editor for NPC IDs with +Add button
#[component]
pub fn NpcIdChipEditor(ids: Vec<i64>, on_change: EventHandler<Vec<i64>>) -> Element {
    let mut new_input = use_signal(String::new);
    let ids_for_keydown = ids.clone();
    let ids_for_click = ids.clone();

    rsx! {
        div { class: "flex-col gap-xs",
            // ID chips
            if !ids.is_empty() {
                div { class: "flex flex-wrap gap-xs mb-xs",
                    for (idx, id) in ids.iter().enumerate() {
                        {
                            let ids_clone = ids.clone();
                            rsx! {
                                span { class: "chip text-mono",
                                    "{id}"
                                    button {
                                        class: "chip-remove",
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
            }

            // Add new ID
            div { class: "flex gap-xs",
                input {
                    r#type: "text",
                    class: "input-inline text-mono",
                    style: "width: 150px;",
                    placeholder: "NPC ID (Enter)",
                    value: "{new_input}",
                    oninput: move |e| new_input.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter && !new_input().trim().is_empty()
                            && let Ok(id) = new_input().trim().parse::<i64>() {
                                let mut new_ids = ids_for_keydown.clone();
                                if !new_ids.contains(&id) {
                                    new_ids.push(id);
                                    on_change.call(new_ids);
                                }
                                new_input.set(String::new());
                            }
                    }
                }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| {
                        if let Ok(id) = new_input().trim().parse::<i64>() {
                            let mut new_ids = ids_for_click.clone();
                            if !new_ids.contains(&id) {
                                new_ids.push(id);
                                on_change.call(new_ids);
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

use crate::api;
use crate::types::{AreaListItem, BossWithPath, ImportPreview, UiSessionState};

pub use tabs::BossTabs;

// ─────────────────────────────────────────────────────────────────────────────
// Main Panel
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct EncounterEditorProps {
    /// Unified UI session state (includes persisted state for this panel)
    pub state: Signal<UiSessionState>,
}

#[component]
pub fn EncounterEditorPanel(mut props: EncounterEditorProps) -> Element {
    // Area index state (not persisted - loaded fresh each time)
    let mut areas = use_signal(Vec::<AreaListItem>::new);
    let mut loading_areas = use_signal(|| true);

    // Boss state - unified: one signal holds all bosses with their items
    let mut bosses = use_signal(Vec::<BossWithPath>::new);
    let mut loading_bosses = use_signal(|| false);

    // Extract persisted state fields
    let mut selected_area_path = use_signal(|| props.state.read().encounter_builder.selected_area_path.clone());
    let mut selected_area_name = use_signal(|| props.state.read().encounter_builder.selected_area_name.clone());
    let mut expanded_boss = use_signal(|| props.state.read().encounter_builder.expanded_boss.clone());
    let mut area_filter = use_signal(|| props.state.read().encounter_builder.area_filter.clone());
    let active_boss_tab = use_signal(|| props.state.read().encounter_builder.active_boss_tab.clone());
    
    // Expanded items within each tab
    let expanded_timer = use_signal(|| props.state.read().encounter_builder.expanded_timer.clone());
    let expanded_phase = use_signal(|| props.state.read().encounter_builder.expanded_phase.clone());
    let expanded_counter = use_signal(|| props.state.read().encounter_builder.expanded_counter.clone());
    let expanded_challenge = use_signal(|| props.state.read().encounter_builder.expanded_challenge.clone());
    let expanded_entity = use_signal(|| props.state.read().encounter_builder.expanded_entity.clone());
    
    // Derived: selected_area AreaListItem (reconstructed from path/name when areas load)
    let mut selected_area = use_signal(|| None::<AreaListItem>);
    
    // Sync persisted state back to unified state
    use_effect(move || {
        let mut state = props.state.write();
        state.encounter_builder.selected_area_path = selected_area_path.read().clone();
        state.encounter_builder.selected_area_name = selected_area_name.read().clone();
        state.encounter_builder.expanded_boss = expanded_boss.read().clone();
        state.encounter_builder.area_filter = area_filter.read().clone();
        state.encounter_builder.active_boss_tab = active_boss_tab.read().clone();
        state.encounter_builder.expanded_timer = expanded_timer.read().clone();
        state.encounter_builder.expanded_phase = expanded_phase.read().clone();
        state.encounter_builder.expanded_counter = expanded_counter.read().clone();
        state.encounter_builder.expanded_challenge = expanded_challenge.read().clone();
        state.encounter_builder.expanded_entity = expanded_entity.read().clone();
    });
    
    // Non-persisted UI state
    let mut show_new_area = use_signal(|| false);
    let mut show_new_boss = use_signal(|| false);
    let mut status_message = use_signal(|| None::<(String, bool)>);

    // Import preview state
    let mut import_preview = use_signal(|| None::<ImportPreview>);
    let mut import_toml_content = use_signal(String::new);

    // Auto-dismiss toast after 3 seconds
    use_effect(move || {
        if status_message().is_some() {
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(3000).await;
                status_message.set(None);
            });
        }
    });

    // Load area index on mount and restore selected area if persisted
    use_effect(move || {
        spawn(async move {
            if let Some(area_list) = api::get_area_index().await {
                areas.set(area_list.clone());
                
                // Restore selected area from persisted path
                if let Some(ref path) = *selected_area_path.read() {
                    if let Some(area) = area_list.iter().find(|a| &a.file_path == path) {
                        selected_area.set(Some(area.clone()));
                        // Load bosses for the restored area
                        if let Some(boss_list) = api::fetch_area_bosses(&area.file_path).await {
                            bosses.set(boss_list);
                        }
                    }
                }
            }
            loading_areas.set(false);
        });
    });

    // Load bosses when area is selected - single unified call
    let mut load_area_data = move |area: AreaListItem| {
        let file_path = area.file_path.clone();
        let area_name = area.name.clone();
        
        // Update persisted state
        selected_area_path.set(Some(file_path.clone()));
        selected_area_name.set(Some(area_name));
        selected_area.set(Some(area));
        
        loading_bosses.set(true);
        bosses.set(Vec::new());
        expanded_boss.set(None);

        spawn(async move {
            if let Some(b) = api::fetch_area_bosses(&file_path).await {
                bosses.set(b);
            }
            loading_bosses.set(false);
        });
    };

    // Group areas by category (with filtering)
    let grouped_areas = {
        let filter = area_filter().to_lowercase();
        let mut ops = Vec::new();
        let mut fps = Vec::new();
        let mut lairs = Vec::new();
        let mut other = Vec::new();

        for area in areas() {
            if !filter.is_empty() && !area.name.to_lowercase().contains(&filter) {
                continue;
            }
            match area.category.as_str() {
                "operations" => ops.push(area),
                "flashpoints" => fps.push(area),
                "lair_bosses" => lairs.push(area),
                _ => other.push(area),
            }
        }
        (ops, fps, lairs, other)
    };

    rsx! {
        div { class: "editor-layout",
            // ─── Sidebar: Area List ───────────────────────────────────────────
            div { class: "editor-sidebar",
                div { class: "editor-sidebar-header",
                    span { class: "text-sm text-muted", "Areas" }
                    button {
                        class: "btn btn-success btn-sm",
                        onclick: move |_| show_new_area.set(true),
                        "+ New"
                    }
                }

                div { class: "p-sm",
                    input {
                        class: "input input-sm w-full",
                        r#type: "text",
                        placeholder: "Filter...",
                        value: "{area_filter}",
                        oninput: move |e| area_filter.set(e.value())
                    }
                }

                div { class: "editor-sidebar-content",
                    if loading_areas() {
                        div { class: "empty-state text-sm", "Loading..." }
                    } else {
                        if !grouped_areas.0.is_empty() {
                            AreaCategory {
                                name: "Operations",
                                areas: grouped_areas.0.clone(),
                                selected: selected_area(),
                                on_select: move |a| load_area_data(a),
                            }
                        }
                        if !grouped_areas.1.is_empty() {
                            AreaCategory {
                                name: "Flashpoints",
                                areas: grouped_areas.1.clone(),
                                selected: selected_area(),
                                on_select: move |a| load_area_data(a),
                            }
                        }
                        if !grouped_areas.2.is_empty() {
                            AreaCategory {
                                name: "Lair Bosses",
                                areas: grouped_areas.2.clone(),
                                selected: selected_area(),
                                on_select: move |a| load_area_data(a),
                            }
                        }
                        if !grouped_areas.3.is_empty() {
                            AreaCategory {
                                name: "Other",
                                areas: grouped_areas.3.clone(),
                                selected: selected_area(),
                                on_select: move |a| load_area_data(a),
                            }
                        }
                    }
                }
            }

            // ─── Main Content ─────────────────────────────────────────────────
            div { class: "editor-main",
                if selected_area().is_none() {
                    div { class: "empty-state",
                        div { class: "empty-state-icon", "📂" }
                        "Select an area to edit encounters"
                    }
                } else if loading_bosses() {
                    div { class: "empty-state", "Loading..." }
                } else {
                    // Area header
                    div { class: "flex items-center justify-between mb-md",
                        h2 { class: "text-primary", "{selected_area().map(|a| a.name).unwrap_or_default()}" }
                        div { class: "flex gap-xs",
                            button {
                                class: "btn btn-sm",
                                onclick: move |_| {
                                    if let Some(area) = selected_area() {
                                        let area_name = area.name.clone();
                                        let file_path = area.file_path.clone();
                                        spawn(async move {
                                            match api::export_encounter_toml(None, &file_path).await {
                                                Ok(result) => {
                                                    let stem = area_name.to_lowercase().replace(' ', "_");
                                                    let default_name = if result.is_bundled {
                                                        format!("{}_custom.toml", stem)
                                                    } else {
                                                        format!("{}.toml", stem)
                                                    };
                                                    if let Some(save_path) = api::save_file_dialog(&default_name).await {
                                                        match api::save_export_file(&save_path, &result.toml).await {
                                                            Ok(()) => status_message.set(Some(("Area exported".to_string(), false))),
                                                            Err(e) => status_message.set(Some((e, true))),
                                                        }
                                                    }
                                                }
                                                Err(e) => status_message.set(Some((e, true))),
                                            }
                                        });
                                    }
                                },
                                "Export Area"
                            }
                            button {
                                class: "btn btn-sm",
                                onclick: move |_| {
                                    let target_path = selected_area_path.read().clone();
                                    spawn(async move {
                                        if let Some(path) = api::open_toml_file_dialog().await {
                                            match api::read_import_file(&path).await {
                                                Ok(content) => {
                                                    match api::preview_import_encounter(&content, target_path.as_deref()).await {
                                                        Ok(preview) => {
                                                            import_toml_content.set(content);
                                                            import_preview.set(Some(preview));
                                                        }
                                                        Err(e) => status_message.set(Some((e, true))),
                                                    }
                                                }
                                                Err(e) => status_message.set(Some((e, true))),
                                            }
                                        }
                                    });
                                },
                                "Import"
                            }
                            button {
                                class: "btn btn-success btn-sm",
                                onclick: move |_| show_new_boss.set(true),
                                "+ New Boss"
                            }
                        }
                    }

                    // New boss form
                    if show_new_boss() {
                        if let Some(area) = selected_area() {
                            {
                                let file_path = area.file_path.clone();
                                rsx! {
                                    new_forms::NewBossForm {
                                        area: area,
                                        on_create: move |new_boss| {
                                            let fp = file_path.clone();
                                            spawn(async move {
                                                match api::create_boss(&new_boss).await {
                                                    Ok(_) => {
                                                        // Reload area to get fresh BossWithPath
                                                        if let Some(b) = api::fetch_area_bosses(&fp).await {
                                                            bosses.set(b);
                                                        }
                                                        status_message.set(Some(("Boss created".to_string(), false)));
                                                    }
                                                    Err(e) => {
                                                        status_message.set(Some((e, true)));
                                                    }
                                                }
                                            });
                                            show_new_boss.set(false);
                                        },
                                        on_cancel: move |_| show_new_boss.set(false),
                                    }
                                }
                            }
                        }
                    }

                    // Boss list
                    if bosses().is_empty() {
                        div { class: "empty-state", "No bosses in this area" }
                    } else {
                        for bwp in bosses() {
                            {
                                let is_expanded = expanded_boss() == Some(bwp.boss.id.clone());
                                let boss_id = bwp.boss.id.clone();
                                // Extract counts directly from BossWithPath
                                let timer_count = bwp.boss.timers.len();
                                let phase_count = bwp.boss.phases.len();
                                let counter_count = bwp.boss.counters.len();
                                let challenge_count = bwp.boss.challenges.len();
                                let entity_count = bwp.boss.entities.len();

                                rsx! {
                                    div { class: "list-item",
                                        div {
                                            class: "list-item-header",
                                            onclick: move |_| {
                                                expanded_boss.set(if is_expanded { None } else { Some(boss_id.clone()) });
                                            },
                                            span { class: "list-item-expand", if is_expanded { "▼" } else { "▶" } }
                                            span { class: "font-medium text-primary", "{bwp.boss.name}" }
                                            span { class: "text-xs text-mono text-muted", "{bwp.boss.id}" }
                                            if timer_count > 0 {
                                                span { class: "tag", "{timer_count} timers" }
                                            }
                                            if phase_count > 0 {
                                                span { class: "tag", "{phase_count} phases" }
                                            }
                                            if counter_count > 0 {
                                                span { class: "tag", "{counter_count} counters" }
                                            }
                                            if challenge_count > 0 {
                                                span { class: "tag", "{challenge_count} challenges" }
                                            }
                                            if entity_count > 0 {
                                                span { class: "tag", "{entity_count} entities" }
                                            }
                                            {
                                                let export_boss_id = bwp.boss.id.clone();
                                                let export_file_path = bwp.file_path.clone();
                                                rsx! {
                                                    button {
                                                        class: "btn btn-ghost btn-sm",
                                                        style: "margin-left: auto; padding: 2px 8px; font-size: 11px;",
                                                        onclick: move |e| {
                                                            e.stop_propagation();
                                                            let bid = export_boss_id.clone();
                                                            let fp = export_file_path.clone();
                                                            spawn(async move {
                                                                match api::export_encounter_toml(Some(&bid), &fp).await {
                                                                    Ok(result) => {
                                                                        let default_name = if result.is_bundled {
                                                                            format!("{}_custom.toml", bid)
                                                                        } else {
                                                                            format!("{}.toml", bid)
                                                                        };
                                                                        if let Some(save_path) = api::save_file_dialog(&default_name).await {
                                                                            match api::save_export_file(&save_path, &result.toml).await {
                                                                                Ok(()) => status_message.set(Some(("Boss exported".to_string(), false))),
                                                                                Err(e) => status_message.set(Some((e, true))),
                                                                            }
                                                                        }
                                                                    }
                                                                    Err(e) => status_message.set(Some((e, true))),
                                                                }
                                                            });
                                                        },
                                                        "Export"
                                                    }
                                                }
                                            }
                                        }

                                        if is_expanded {
                                            div { class: "list-item-body",
                                                BossTabs {
                                                    boss_with_path: bwp.clone(),
                                                    active_tab: active_boss_tab,
                                                    expanded_timer: expanded_timer,
                                                    expanded_phase: expanded_phase,
                                                    expanded_counter: expanded_counter,
                                                    expanded_challenge: expanded_challenge,
                                                    expanded_entity: expanded_entity,
                                                    on_boss_change: move |updated: BossWithPath| {
                                                        let mut all = bosses();
                                                        if let Some(idx) = all.iter().position(|b| b.boss.id == updated.boss.id) {
                                                            all[idx] = updated;
                                                            bosses.set(all);
                                                        }
                                                    },
                                                    on_status: move |msg| status_message.set(Some(msg)),
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

        // New Area modal
        if show_new_area() {
            new_forms::NewAreaForm {
                on_create: move |new_area| {
                    spawn(async move {
                        match api::create_area(&new_area).await {
                            Ok(_) => {
                                if let Some(a) = api::get_area_index().await {
                                    areas.set(a);
                                }
                                status_message.set(Some(("Area created".to_string(), false)));
                            }
                            Err(e) => {
                                status_message.set(Some((e, true)));
                            }
                        }
                    });
                    show_new_area.set(false);
                },
                on_cancel: move |_| show_new_area.set(false),
            }
        }

        // Import preview modal
        if let Some(preview) = import_preview() {
            ImportPreviewModal {
                preview: preview,
                target_area_name: selected_area_name.read().clone(),
                on_confirm: move |_| {
                    let content = import_toml_content();
                    let target = selected_area_path.read().clone();
                    spawn(async move {
                        match api::import_encounter_toml(&content, target.as_deref()).await {
                            Ok(()) => {
                                // Reload bosses and area index
                                if let Some(ref path) = target {
                                    if let Some(b) = api::fetch_area_bosses(path).await {
                                        bosses.set(b);
                                    }
                                }
                                if let Some(a) = api::get_area_index().await {
                                    areas.set(a);
                                }
                                status_message.set(Some(("Import successful".to_string(), false)));
                            }
                            Err(e) => status_message.set(Some((e, true))),
                        }
                    });
                    import_preview.set(None);
                    import_toml_content.set(String::new());
                },
                on_cancel: move |_| {
                    import_preview.set(None);
                    import_toml_content.set(String::new());
                },
            }
        }

        // Toast notification (fixed bottom-right)
        if let Some((msg, is_error)) = status_message() {
            div {
                class: "toast",
                style: "position: fixed; bottom: 20px; right: 20px; z-index: 1000; \
                        padding: 12px 16px; border-radius: 6px; \
                        background: #2a2a2e; border: 1px solid #3a3a3e; \
                        box-shadow: 0 4px 12px rgba(0,0,0,0.5); \
                        display: flex; align-items: center; gap: 12px;",
                span {
                    style: if is_error { "color: var(--color-error);" } else { "color: var(--color-success);" },
                    if is_error { "✗" } else { "✓" }
                }
                span { "{msg}" }
                button {
                    class: "btn btn-ghost btn-sm",
                    style: "padding: 2px 6px; min-width: auto;",
                    onclick: move |_| status_message.set(None),
                    "×"
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Area Category (collapsible)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn AreaCategory(
    name: &'static str,
    areas: Vec<AreaListItem>,
    selected: Option<AreaListItem>,
    on_select: EventHandler<AreaListItem>,
) -> Element {
    let mut collapsed = use_signal(|| false);

    rsx! {
        div { class: "category-group",
            div {
                class: "category-header",
                onclick: move |_| collapsed.set(!collapsed()),
                span { if collapsed() { "▶" } else { "▼" } }
                span { "{name}" }
                span { class: "sidebar-item-count", "{areas.len()}" }
            }

            if !collapsed() {
                div { class: "category-items",
                    for area in areas {
                        {
                            let is_active = selected.as_ref().map(|s| s.file_path == area.file_path).unwrap_or(false);
                            let area_clone = area.clone();

                            rsx! {
                                div {
                                    class: if is_active { "sidebar-item active" } else { "sidebar-item" },
                                    onclick: move |_| on_select.call(area_clone.clone()),
                                    "{area.name}"
                                    span { class: "sidebar-item-count", "{area.boss_count}" }
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
// Import Preview Modal
// ─────────────────────────────────────────────────────────────────────────────

/// Group items by type, returning (type_label, vec_of_names)
fn group_by_type(items: &[crate::types::ImportItemDiff]) -> Vec<(String, Vec<String>)> {
    let mut map: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for item in items {
        map.entry(item.item_type.clone()).or_default().push(item.name.clone());
    }
    map.into_iter().collect()
}

/// Pluralize an item type label: "timer" -> "timers" etc
fn plural(item_type: &str, count: usize) -> String {
    if count == 1 { item_type.to_string() } else { format!("{}s", item_type) }
}

#[component]
fn ImportPreviewModal(
    preview: ImportPreview,
    target_area_name: Option<String>,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let has_errors = !preview.errors.is_empty();

    rsx! {
        div {
            class: "modal-overlay",
            onclick: move |_| on_cancel.call(()),

            div {
                class: "modal-content",
                style: "max-width: 550px; max-height: 80vh; display: flex; flex-direction: column;",
                onclick: move |e| e.stop_propagation(),

                div { class: "modal-header",
                    h3 { "Import Encounter Definition" }
                }

                // Scrollable body
                div { style: "overflow-y: auto; flex: 1; padding: 0 16px;",

                    // Source/target row
                    div { style: "display: grid; grid-template-columns: auto 1fr; gap: 2px 12px; margin-bottom: 12px;",
                        if let Some(ref name) = preview.source_area_name {
                            span { class: "text-xs text-muted", "Source" }
                            span { class: "text-xs text-primary", "{name}" }
                        }
                        if preview.is_new_area {
                            span { class: "text-xs text-muted", "Target" }
                            span { class: "text-xs", style: "color: var(--color-warning);", "New area" }
                        } else if let Some(ref name) = target_area_name {
                            span { class: "text-xs text-muted", "Target" }
                            span { class: "text-xs text-primary", "{name}" }
                        }
                    }

                    // Validation errors
                    if has_errors {
                        div { class: "mb-sm",
                            for err in &preview.errors {
                                div { class: "text-xs",
                                    style: "color: var(--color-error);",
                                    "{err}"
                                }
                            }
                        }
                    }

                    // Boss previews
                    for boss in &preview.bosses {
                        div { style: "border: 1px solid var(--color-border); border-radius: 4px; \
                                      padding: 8px 10px; margin-bottom: 8px;",
                            // Boss header
                            div { class: "flex items-center gap-xs",
                                style: "margin-bottom: 6px;",
                                span { class: "font-medium text-primary text-sm", "{boss.boss_name}" }
                                if boss.is_new_boss {
                                    span { class: "tag",
                                        style: "background: var(--color-success); color: #000; font-size: 10px; padding: 0 4px;",
                                        "new"
                                    }
                                }
                            }

                            if boss.is_new_boss {
                                // New boss — summarize by type
                                {
                                    let grouped = group_by_type(&boss.items_to_add);
                                    rsx! {
                                        div { class: "text-xs text-muted",
                                            {grouped.iter().map(|(t, names)| format!("{} {}", names.len(), plural(t, names.len()))).collect::<Vec<_>>().join(", ")}
                                        }
                                    }
                                }
                            } else {
                                // Existing boss — show replace/add grouped by type
                                if !boss.items_to_replace.is_empty() {
                                    {
                                        let grouped = group_by_type(&boss.items_to_replace);
                                        rsx! {
                                            for (item_type, names) in grouped {
                                                div { class: "text-xs",
                                                    style: "margin-bottom: 2px;",
                                                    span { style: "color: var(--color-warning);", "Replace " }
                                                    span { class: "text-muted",
                                                        "{names.len()} {plural(&item_type, names.len())}: "
                                                    }
                                                    span { style: "color: var(--color-warning);",
                                                        "{names.join(\", \")}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if !boss.items_to_add.is_empty() {
                                    {
                                        let grouped = group_by_type(&boss.items_to_add);
                                        rsx! {
                                            for (item_type, names) in grouped {
                                                div { class: "text-xs",
                                                    style: "margin-bottom: 2px;",
                                                    span { style: "color: var(--color-success);", "Add " }
                                                    span { class: "text-muted",
                                                        "{names.len()} {plural(&item_type, names.len())}: "
                                                    }
                                                    span { style: "color: var(--color-success);",
                                                        "{names.join(\", \")}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                if boss.items_unchanged > 0 {
                                    div { class: "text-xs text-muted",
                                        style: "margin-top: 2px;",
                                        "{boss.items_unchanged} unchanged"
                                    }
                                }
                            }
                        }
                    }
                }

                // Footer
                div { class: "modal-footer",
                    button {
                        class: "btn btn-ghost",
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
