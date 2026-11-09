//! Encounter Editor
//!
//! Full CRUD for the BossEncounter DSL: timers, phases, counters, challenges, entities.
//! Uses unified BossWithPath type and EncounterItem enum for streamlined data handling.

mod area_sidebar;
mod boss_header;
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

use crate::components::editor_drawer::{DiscardPrompt, layout_style, provide_drawer_guard};
use crate::components::sidebar_header::SidebarHeader;
use area_sidebar::{AreaCategory, BossRow};
use boss_header::EditorHeader;
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
                    class: "input-inline w-lg",
                    r#type: "text",
                    placeholder: placeholder,
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
                    class: "input-inline text-mono w-md",
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
    let guard = provide_drawer_guard();
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
    let mut sidebar_collapsed = use_signal(|| props.state.read().encounter_builder.sidebar_collapsed);
    let active_boss_tab = use_signal(|| props.state.read().encounter_builder.active_boss_tab.clone());
    
    // Expanded items within each tab
    let expanded_timer = use_signal(|| props.state.read().encounter_builder.expanded_timer.clone());
    let expanded_phase = use_signal(|| props.state.read().encounter_builder.expanded_phase.clone());
    let expanded_counter = use_signal(|| props.state.read().encounter_builder.expanded_counter.clone());
    let expanded_challenge = use_signal(|| props.state.read().encounter_builder.expanded_challenge.clone());
    let expanded_entity = use_signal(|| props.state.read().encounter_builder.expanded_entity.clone());
    let hide_disabled_timers = use_signal(|| props.state.read().encounter_builder.hide_disabled_timers);
    let hide_disabled_phases = use_signal(|| props.state.read().encounter_builder.hide_disabled_phases);
    let hide_disabled_counters = use_signal(|| props.state.read().encounter_builder.hide_disabled_counters);
    
    // Derived: selected_area AreaListItem (reconstructed from path/name when areas load)
    let mut selected_area = use_signal(|| None::<AreaListItem>);
    
    // Sync persisted state back to unified state
    use_effect(move || {
        let mut state = props.state.write();
        state.encounter_builder.selected_area_path = selected_area_path.read().clone();
        state.encounter_builder.selected_area_name = selected_area_name.read().clone();
        state.encounter_builder.expanded_boss = expanded_boss.read().clone();
        state.encounter_builder.area_filter = area_filter.read().clone();
        state.encounter_builder.sidebar_collapsed = *sidebar_collapsed.read();
        state.encounter_builder.active_boss_tab = active_boss_tab.read().clone();
        state.encounter_builder.expanded_timer = expanded_timer.read().clone();
        state.encounter_builder.expanded_phase = expanded_phase.read().clone();
        state.encounter_builder.expanded_counter = expanded_counter.read().clone();
        state.encounter_builder.expanded_challenge = expanded_challenge.read().clone();
        state.encounter_builder.expanded_entity = expanded_entity.read().clone();
        state.encounter_builder.hide_disabled_timers = *hide_disabled_timers.read();
        state.encounter_builder.hide_disabled_phases = *hide_disabled_phases.read();
        state.encounter_builder.hide_disabled_counters = *hide_disabled_counters.read();
    });
    
    // Non-persisted UI state
    let mut show_new_area = use_signal(|| false);
    let mut show_new_boss = use_signal(|| false);
    let mut bosses_open = use_signal(|| true);
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
    let load_area_data = move |area: AreaListItem| {
        let file_path = area.file_path.clone();
        let area_name = area.name.clone();
        
        // Update persisted state
        selected_area_path.set(Some(file_path.clone()));
        selected_area_name.set(Some(area_name));
        selected_area.set(Some(area));
        
        loading_bosses.set(true);
        bosses.set(Vec::new());
        expanded_boss.set(None);
        bosses_open.set(true);

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
        let mut groups: [(&'static str, Vec<AreaListItem>); 4] =
            [("Operations", vec![]), ("Flashpoints", vec![]), ("Lair Bosses", vec![]), ("Other", vec![])];
        for area in areas() {
            if !filter.is_empty() && !area.name.to_lowercase().contains(&filter) {
                continue;
            }
            let i = match area.category.as_str() {
                "operations" => 0,
                "flashpoints" => 1,
                "lair_bosses" => 2,
                _ => 3,
            };
            groups[i].1.push(area);
        }
        groups
    };
    let boss_rows: Vec<BossRow> = bosses
        .read()
        .iter()
        .map(|b| (b.boss.id.clone(), b.boss.name.clone(), b.boss.enabled, b.boss.is_final_boss))
        .collect();
    let open_boss: Option<BossWithPath> = expanded_boss
        .read()
        .as_ref()
        .and_then(|id| bosses.read().iter().find(|b| &b.boss.id == id).cloned());

    // Area actions (header menu)
    let export_area = move |_| {
        if let Some(area) = selected_area() {
            spawn(async move {
                match api::export_encounter_toml(None, &area.file_path).await {
                    Ok(result) => {
                        let stem = area.name.to_lowercase().replace(' ', "_");
                        let default_name = if result.is_bundled { format!("{stem}_custom.toml") } else { format!("{stem}.toml") };
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
    };
    let import_into_area = move |_| {
        let target_path = selected_area_path.read().clone();
        spawn(async move {
            if let Some(path) = api::open_toml_file_dialog().await {
                match api::read_import_file(&path).await {
                    Ok(content) => match api::preview_import_encounter(&content, target_path.as_deref()).await {
                        Ok(preview) => {
                            import_toml_content.set(content);
                            import_preview.set(Some(preview));
                        }
                        Err(e) => status_message.set(Some((e, true))),
                    },
                    Err(e) => status_message.set(Some((e, true))),
                }
            }
        });
    };
    let refetch_bosses = move |_| {
        if let Some(fp) = selected_area_path.read().clone() {
            spawn(async move {
                if let Some(b) = api::fetch_area_bosses(&fp).await {
                    bosses.set(b);
                }
            });
        }
    };

    rsx! {
        div { class: "editor-layout", style: layout_style(),
            DiscardPrompt {}
            // ─── Sidebar: Areas, and the open area's bosses ───────────────────
            aside { class: "explorer-sidebar", class: if sidebar_collapsed() { "collapsed" },
                SidebarHeader {
                    icon: "fa-solid fa-map-location-dot",
                    title: "Areas",
                    collapsed: sidebar_collapsed(),
                    on_toggle: move |_| sidebar_collapsed.toggle(),
                    action: rsx! {
                        button {
                            class: "btn btn-xs sidebar-header-action",
                            title: "Create a new area",
                            onclick: move |_| show_new_area.set(true),
                            i { class: "fa-solid fa-plus" }
                            " New"
                        }
                    },
                }
                if !sidebar_collapsed() {
                    div { class: "sidebar-search",
                        input {
                            class: "search-input",
                            r#type: "text",
                            placeholder: "Filter areas...",
                            value: "{area_filter}",
                            oninput: move |e| area_filter.set(e.value())
                        }
                    }
    
                    div { class: "sidebar-encounter-list",
                        if loading_areas() {
                            div { class: "sidebar-empty", "Loading..." }
                        } else {
                            for (name, group) in grouped_areas {
                                if !group.is_empty() {
                                    AreaCategory {
                                        name,
                                        areas: group,
                                        selected: selected_area(),
                                        bosses: boss_rows.clone(),
                                        loading: loading_bosses(),
                                        bosses_open: bosses_open(),
                                        selected_boss: expanded_boss(),
                                        on_select: move |a: AreaListItem| {
                                            // The open area again folds or unfolds its bosses
                                            if selected_area_path.read().as_ref() == Some(&a.file_path) {
                                                bosses_open.toggle();
                                            } else {
                                                let mut load = load_area_data;
                                                guard.confirm(move |_| load(a.clone()));
                                            }
                                        },
                                        on_select_boss: move |id: String| guard.switch(expanded_boss, Some(id)),
                                        on_new_boss: move |_| show_new_boss.set(true),
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ─── Main: the open boss ──────────────────────────────────────────
            div { class: "editor-main",
                if let Some(area) = selected_area() {
                    EditorHeader {
                        area_name: area.name.clone(),
                        boss: open_boss.clone(),
                        on_refetch: refetch_bosses,
                        on_status: move |msg| status_message.set(Some(msg)),
                        on_export_area: export_area,
                        on_import: import_into_area,
                    }
                    if show_new_boss() {
                        {
                            let file_path = area.file_path.clone();
                            rsx! {
                                new_forms::NewBossForm {
                                    area: area.clone(),
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
                                                Err(e) => status_message.set(Some((e, true))),
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
                if selected_area().is_none() {
                    div { class: "empty-state",
                        div { class: "empty-state-icon", "📂" }
                        "Select an area to edit encounters"
                    }
                } else if loading_bosses() {
                    div { class: "empty-state", "Loading..." }
                } else if let Some(bwp) = open_boss {
                    BossTabs {
                        key: "{bwp.boss.id}",
                        boss_with_path: bwp.clone(),
                        active_tab: active_boss_tab,
                        expanded_timer: expanded_timer,
                        expanded_phase: expanded_phase,
                        expanded_counter: expanded_counter,
                        expanded_challenge: expanded_challenge,
                        expanded_entity: expanded_entity,
                        hide_disabled_timers: hide_disabled_timers,
                        hide_disabled_phases: hide_disabled_phases,
                        hide_disabled_counters: hide_disabled_counters,
                        on_boss_change: move |updated: BossWithPath| {
                            let mut all = bosses();
                            if let Some(idx) = all.iter().position(|b| b.boss.id == updated.boss.id) {
                                all[idx] = updated;
                                bosses.set(all);
                            }
                        },
                        on_refetch: refetch_bosses,
                        on_status: move |msg| status_message.set(Some(msg)),
                    }
                } else if bosses.read().is_empty() {
                    div { class: "empty-state", "No bosses in this area yet" }
                } else {
                    div { class: "empty-state", "Select a boss in the sidebar" }
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
                        background: var(--bg-input); border: 1px solid var(--border-strong); \
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
                            span { class: "text-xs text-warning", "New area" }
                        } else if let Some(ref name) = target_area_name {
                            span { class: "text-xs text-muted", "Target" }
                            span { class: "text-xs text-primary", "{name}" }
                        }
                    }

                    // Validation errors
                    if has_errors {
                        div { class: "mb-sm",
                            for err in &preview.errors {
                                div { class: "text-xs text-error",
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
                                                    span { class: "text-warning", "Replace " }
                                                    span { class: "text-muted",
                                                        "{names.len()} {plural(&item_type, names.len())}: "
                                                    }
                                                    span { class: "text-warning",
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
                                                    span { class: "text-success", "Add " }
                                                    span { class: "text-muted",
                                                        "{names.len()} {plural(&item_type, names.len())}: "
                                                    }
                                                    span { class: "text-success",
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

// ─────────────────────────────────────────────────────────────────────────────
// Shared: Difficulties Editor
// ─────────────────────────────────────────────────────────────────────────────

/// Toggle-button row for selecting difficulty tiers (story / veteran / master).
///
/// Empty selection means "all difficulties". Matches the same semantic as
/// timer difficulties.
#[component]
pub fn DifficultiesEditor(
    difficulties: Vec<String>,
    on_change: EventHandler<Vec<String>>,
) -> Element {
    rsx! {
        div { class: "flex gap-xs",
            for diff in ["story", "veteran", "master"] {
                {
                    let diff_str = diff.to_string();
                    let is_active = difficulties.iter().any(|d| d == diff);
                    let diffs_clone = difficulties.clone();
                    rsx! {
                        button {
                            class: if is_active { "toggle-btn active" } else { "toggle-btn" },
                            onclick: move |_| {
                                let mut updated = diffs_clone.clone();
                                if is_active {
                                    updated.retain(|x| x != &diff_str);
                                } else {
                                    updated.push(diff_str.clone());
                                }
                                on_change.call(updated);
                            },
                            "{diff}"
                        }
                    }
                }
            }
        }
    }
}
