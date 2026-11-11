//! Entity editing tab
//!
//! CRUD for boss entity (NPC) roster definitions.
//! Entities define which NPCs are bosses, adds, triggers, and kill targets.
//! Uses EntityDefinition DSL type directly.

use crate::components::editor_sort::{ENCOUNTER_SORT_AZ, SortToggle, apply_rank, use_stable_rank};
use dioxus::prelude::*;

use crate::components::editor_drawer::{EditorDrawer, use_drawer_guard};
use crate::api;
use crate::types::{BossWithPath, EncounterItem, EntityDefinition};

use super::entity_form::EntityEditForm;
use super::tabs::EncounterData;
use super::InlineNameCreator;

// ─────────────────────────────────────────────────────────────────────────────
// Entities Tab
// ─────────────────────────────────────────────────────────────────────────────

/// Create a default entity definition
fn default_entity(name: String) -> EntityDefinition {
    EntityDefinition {
        name,
        ids: vec![],
        is_boss: false,
        triggers_encounter: None, // Uses is_boss default
        is_kill_target: false,
        show_on_hp_overlay: None, // Uses is_boss default
        hp_markers: vec![],
        shields: vec![],
        pushes_at: None,
        single_instance: false,
    }
}

#[component]
pub fn EntitiesTab(
    boss_with_path: BossWithPath,
    expanded_entity: Signal<Option<String>>,
    on_change: EventHandler<Vec<EntityDefinition>>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let guard = use_drawer_guard();
    // Extract entities from BossWithPath
    let entities = boss_with_path.boss.entities.clone();
    // A–Z when toggled; renaming leaves the row in place
    let rank = use_stable_rank(entities.iter().map(|x| (x.name.as_str(), x.name.as_str())), ENCOUNTER_SORT_AZ());
    let mut sorted_entities = entities.clone();
    apply_rank(&mut sorted_entities, &rank.read(), |x| &x.name);

    rsx! {
        div { class: "entities-tab",
            // Header
            div { class: "flex items-center justify-between mb-sm editor-tab-header",
                div { class: "flex items-center gap-sm",
                    span { class: "text-sm text-secondary", "{entities.len()} entities" }
                    SortToggle { az: ENCOUNTER_SORT_AZ.signal() }
                }
                {
                    let bwp = boss_with_path.clone();
                    let entities_for_create = entities.clone();
                    rsx! {
                        InlineNameCreator {
                            button_label: "+ New Entity",
                            placeholder: "Entity name...",
                            on_create: move |name: String| {
                                let entities_clone = entities_for_create.clone();
                                let boss_id = bwp.boss.id.clone();
                                let file_path = bwp.file_path.clone();
                                let entity = default_entity(name);
                                let item = EncounterItem::Entity(entity);
                                spawn(async move {
                                    match api::create_encounter_item(&boss_id, &file_path, &item).await {
                                        Ok(EncounterItem::Entity(created)) => {
                                            let created_name = created.name.clone();
                                            let mut current = entities_clone;
                                            current.push(created);
                                            on_change.call(current);
                                            expanded_entity.set(Some(created_name));
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

            // Help text
            div { class: "text-xs text-muted mb-sm",
                "Add NPCs to the entity roster by game ids. Entity roster names can be used as selectors for source/target filter conditions."
            }

            // Entity list
            if entities.is_empty() {
                div { class: "empty-state text-sm", "No entities defined" }
            } else {
                for entity in sorted_entities.clone() {
                    {
                        let entity_key = entity.name.clone();
                        let is_expanded = expanded_entity() == Some(entity_key.clone());
                        let entities_for_row = entities.clone();

                        rsx! {
                            EntityRow {
                                key: "{entity_key}",
                                entity: entity.clone(),
                                boss_with_path: boss_with_path.clone(),
                                expanded: is_expanded,
                                on_toggle: move |_| {
                                    guard.switch(expanded_entity, if is_expanded { None } else { Some(entity_key.clone()) });
                                },
                                on_change: on_change,
                                on_status: on_status,
                                on_collapse: move |_| expanded_entity.set(None),
                                all_entities: entities_for_row,
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Entity Row
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn EntityRow(
    entity: EntityDefinition,
    boss_with_path: BossWithPath,
    expanded: bool,
    all_entities: Vec<EntityDefinition>,
    on_toggle: EventHandler<()>,
    on_change: EventHandler<Vec<EntityDefinition>>,
    on_status: EventHandler<(String, bool)>,
    on_collapse: EventHandler<()>,
) -> Element {
    let encounter_data = EncounterData::from_boss(&boss_with_path);
    let mut is_dirty = use_signal(|| false);
    let id_count = entity.ids.len();

    // Extract context for API calls
    let boss_id = boss_with_path.boss.id.clone();
    let file_path = boss_with_path.file_path.clone();

    rsx! {
        div { class: "list-item", class: if expanded { "expanded" },
            // Header row
            div {
                class: "list-item-header",
                onclick: move |_| on_toggle.call(()),
                span { class: "font-medium", "{entity.name}" }
                if expanded && is_dirty() {
                    span { class: "unsaved-indicator", title: "Unsaved changes" }
                }
                span { class: "text-xs text-muted text-mono", "{id_count} IDs" }
                if entity.is_boss {
                    span { class: "tag tag-danger", "Boss" }
                }
                // triggers_encounter defaults to is_boss when None
                if entity.triggers_encounter.unwrap_or(entity.is_boss) {
                    span { class: "tag tag-warning", "Trigger" }
                }
                if entity.is_kill_target {
                    span { class: "tag tag-success", "Kill Target" }
                }
                // Show HP overlay tag when behavior differs from is_boss default
                {
                    let shows_hp = entity.show_on_hp_overlay.unwrap_or(entity.is_boss);
                    if shows_hp && !entity.is_boss {
                        rsx! { span { class: "tag tag-info", "HP Overlay" } }
                    } else if !shows_hp && entity.is_boss {
                        rsx! { span { class: "tag tag-muted", "HP Hidden" } }
                    } else {
                        rsx! {}
                    }
                }
                if let Some(pct) = entity.pushes_at {
                    span { class: "tag tag-warning", "Pushes {pct}%" }
                }
            }

            // Expanded content
            if expanded {
                {
                    let all_entities_for_save = all_entities.clone();
                    let all_entities_for_delete = all_entities.clone();
                    let boss_id_save = boss_id.clone();
                    let file_path_save = file_path.clone();
                    let boss_id_delete = boss_id.clone();
                    let file_path_delete = file_path.clone();

                    rsx! {
                        EditorDrawer { title: entity.name.clone(), dirty: is_dirty(), on_close: on_collapse,
                            EntityEditForm {
                                entity: entity.clone(),
                                encounter_data: encounter_data.clone(),
                                on_dirty: move |dirty: bool| is_dirty.set(dirty),
                                on_save: move |(updated, original_name): (EntityDefinition, String)| {
                                    let all = all_entities_for_save.clone();
                                    let boss_id = boss_id_save.clone();
                                    let file_path = file_path_save.clone();
                                    let item = EncounterItem::Entity(updated.clone());
                                    // Entity uses name as ID, so pass original_name for lookup
                                    let orig_id = if original_name != updated.name { Some(original_name.clone()) } else { None };
                                    // Update parent state synchronously so props refresh and dirty indicator clears
                                    let new_list: Vec<_> = all.iter()
                                        .map(|e| if e.name == original_name { updated.clone() } else { e.clone() })
                                        .collect();
                                    on_change.call(new_list);
                                    on_status.call(("Saving...".to_string(), false));
                                    spawn(async move {
                                        match api::update_encounter_item(&boss_id, &file_path, &item, orig_id.as_deref()).await {
                                            Ok(_) => {
                                                on_status.call(("Saved".to_string(), false));
                                            }
                                            Err(_) => on_status.call(("Failed to save".to_string(), true)),
                                        }
                                    });
                                },
                                on_delete: {
                                    let all_entities = all_entities_for_delete.clone();
                                    move |entity_to_delete: EntityDefinition| {
                                        let all_entities = all_entities.clone();
                                        let boss_id = boss_id_delete.clone();
                                        let file_path = file_path_delete.clone();
                                        let entity_name = entity_to_delete.name.clone();
                                        spawn(async move {
                                            match api::delete_encounter_item("entity", &entity_name, &boss_id, &file_path).await {
                                                Ok(_) => {
                                                    let updated: Vec<_> = all_entities.iter()
                                                        .filter(|e| e.name != entity_name)
                                                        .cloned()
                                                        .collect();
                                                    on_change.call(updated);
                                                    on_collapse.call(());
                                                    on_status.call(("Deleted".to_string(), false));
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
