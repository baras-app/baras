//! Entity editing tab
//!
//! CRUD for boss entity (NPC) roster definitions.
//! Entities define which NPCs are bosses, adds, triggers, and kill targets.
//! Uses EntityDefinition DSL type directly.

use dioxus::prelude::*;

use crate::api;
use crate::types::{BossWithPath, EncounterItem, EntityDefinition};

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
    }
}

#[component]
pub fn EntitiesTab(
    boss_with_path: BossWithPath,
    on_change: EventHandler<Vec<EntityDefinition>>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let mut expanded_entity = use_signal(|| None::<String>);

    // Extract entities from BossWithPath
    let entities = boss_with_path.boss.entities.clone();

    rsx! {
        div { class: "entities-tab",
            // Header
            div { class: "flex items-center justify-between mb-sm",
                span { class: "text-sm text-secondary", "{entities.len()} entities" }
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
                for entity in entities.clone() {
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
                                    expanded_entity.set(if is_expanded { None } else { Some(entity_key.clone()) });
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
    let id_count = entity.ids.len();

    // Extract context for API calls
    let boss_id = boss_with_path.boss.id.clone();
    let file_path = boss_with_path.file_path.clone();

    rsx! {
        div { class: "list-item",
            // Header row
            div {
                class: "list-item-header",
                onclick: move |_| on_toggle.call(()),
                span { class: "list-item-expand", if expanded { "▼" } else { "▶" } }
                span { class: "font-medium", "{entity.name}" }
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
                        div { class: "list-item-body",
                            EntityEditForm {
                                entity: entity.clone(),
                                on_save: move |(updated, original_name): (EntityDefinition, String)| {
                                    let all = all_entities_for_save.clone();
                                    let boss_id = boss_id_save.clone();
                                    let file_path = file_path_save.clone();
                                    let item = EncounterItem::Entity(updated.clone());
                                    // Entity uses name as ID, so pass original_name for lookup
                                    let orig_id = if original_name != updated.name { Some(original_name.clone()) } else { None };
                                    on_status.call(("Saving...".to_string(), false));
                                    spawn(async move {
                                        match api::update_encounter_item(&boss_id, &file_path, &item, orig_id.as_deref()).await {
                                            Ok(_) => {
                                                // Update local state
                                                let new_list: Vec<_> = all.iter()
                                                    .map(|e| if e.name == original_name { updated.clone() } else { e.clone() })
                                                    .collect();
                                                on_change.call(new_list);
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

// ─────────────────────────────────────────────────────────────────────────────
// Entity Edit Form
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn EntityEditForm(
    entity: EntityDefinition,
    on_save: EventHandler<(EntityDefinition, String)>,
    on_delete: EventHandler<EntityDefinition>,
) -> Element {
    let original_name = entity.name.clone();
    let mut draft = use_signal(|| entity.clone());
    let original = entity.clone();

    let has_changes = use_memo(move || draft() != original);

    let handle_save = {
        let orig_name = original_name.clone();
        move |_| {
            let updated = draft();
            on_save.call((updated, orig_name.clone()));
        }
    };

    let handle_delete = move |_| {
        on_delete.call(entity.clone());
    };

    rsx! {
        div { class: "entity-edit-form",
            // ─── Name ──────────────────────────────────────────────────────────
            div { class: "form-row-hz",
                label { "Name" }
                input {
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

            // ─── NPC IDs ───────────────────────────────────────────────────────
            div { class: "form-row-hz", style: "align-items: flex-start;",
                label { style: "padding-top: 6px;", "NPC IDs" }
                NpcIdChipEditor {
                    ids: draft().ids.clone(),
                    on_change: move |new_ids| {
                        let mut d = draft();
                        d.ids = new_ids;
                        draft.set(d);
                    }
                }
            }

            // ─── Flags ─────────────────────────────────────────────────────────
            div { class: "form-section",
                div { class: "font-bold text-sm mb-xs", "Flags" }

                div { class: "flex flex-col gap-xs",
                    label { class: "flex items-center gap-xs cursor-pointer",
                        input {
                            r#type: "checkbox",
                            checked: draft().is_boss,
                            onchange: move |e| {
                                let mut d = draft();
                                d.is_boss = e.checked();
                                draft.set(d);
                            }
                        }
                        span { "Is Boss" }
                    }

                    label { class: "flex items-center gap-xs cursor-pointer",
                        input {
                            r#type: "checkbox",
                            checked: draft().triggers_encounter.unwrap_or(draft().is_boss),
                            onchange: move |e| {
                                let mut d = draft();
                                d.triggers_encounter = Some(e.checked());
                                draft.set(d);
                            }
                        }
                        span { "Triggers Encounter" }
                        span { class: "text-xs text-muted", "(appearance of this target loads timers)" }
                    }

                    label { class: "flex items-center gap-xs cursor-pointer",
                        input {
                            r#type: "checkbox",
                            checked: draft().is_kill_target,
                            onchange: move |e| {
                                let mut d = draft();
                                d.is_kill_target = e.checked();
                                draft.set(d);
                            }
                        }
                        span { "Is Kill Target" }
                        span { class: "text-xs text-muted", "(death of all kill targets ends encounter)" }
                    }

                    label { class: "flex items-center gap-xs cursor-pointer",
                        input {
                            r#type: "checkbox",
                            checked: draft().show_on_hp_overlay.unwrap_or(draft().is_boss),
                            onchange: move |e| {
                                let mut d = draft();
                                d.show_on_hp_overlay = Some(e.checked());
                                draft.set(d);
                            }
                        }
                        span { "Show on HP Overlay" }
                        span { class: "text-xs text-muted", "(display this entity on Boss HP bar)" }
                    }
                }
            }

            // ─── Actions ─────────────────────────────────────────────────────
            div { class: "form-actions",
                button {
                    class: if has_changes() { "btn btn-success btn-sm" } else { "btn btn-sm" },
                    disabled: !has_changes(),
                    onclick: handle_save,
                    "Save"
                }
                button {
                    class: "btn btn-danger btn-sm",
                    onclick: handle_delete,
                    "Delete"
                }
            }
        }
    }
}


// ─────────────────────────────────────────────────────────────────────────────
// NPC ID Chip Editor
// ─────────────────────────────────────────────────────────────────────────────

/// Chip editor for NPC IDs with +Add button
#[component]
fn NpcIdChipEditor(
    ids: Vec<i64>,
    on_change: EventHandler<Vec<i64>>,
) -> Element {
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
                        if e.key() == Key::Enter && !new_input().trim().is_empty() {
                            if let Ok(id) = new_input().trim().parse::<i64>() {
                                let mut new_ids = ids_for_keydown.clone();
                                if !new_ids.contains(&id) {
                                    new_ids.push(id);
                                    on_change.call(new_ids);
                                }
                                new_input.set(String::new());
                            }
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
