//! Entity editing tab
//!
//! CRUD for boss entity (NPC) roster definitions.
//! Entities define which NPCs are bosses, adds, triggers, and kill targets.

use dioxus::prelude::*;

use crate::api;
use crate::types::{BossListItem, EntityListItem};

// ─────────────────────────────────────────────────────────────────────────────
// Entities Tab
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn EntitiesTab(
    boss: BossListItem,
    entities: Vec<EntityListItem>,
    on_change: EventHandler<Vec<EntityListItem>>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let mut expanded_entity = use_signal(|| None::<String>);
    let mut show_new_entity = use_signal(|| false);

    rsx! {
        div { class: "entities-tab",
            // Header
            div { class: "flex items-center justify-between mb-sm",
                span { class: "text-sm text-secondary", "{entities.len()} entities" }
                button {
                    class: "btn btn-success btn-sm",
                    onclick: move |_| show_new_entity.set(true),
                    "+ New Entity"
                }
            }

            // Help text
            div { class: "text-xs text-muted mb-sm",
                "Add NPCs to the entity roster by game ids. Entity roster names can be used as selectors for source/target filter conditions."
            }

            // New entity form
            if show_new_entity() {
                {
                    let entities_for_create = entities.clone();
                    rsx! {
                        NewEntityForm {
                            boss: boss.clone(),
                            on_create: move |new_entity: EntityListItem| {
                                let entities_clone = entities_for_create.clone();
                                spawn(async move {
                                    if let Some(created) = api::create_entity(&new_entity).await {
                                        let mut current = entities_clone;
                                        current.push(created);
                                        on_change.call(current);
                                        on_status.call(("Created".to_string(), false));
                                    } else {
                                        on_status.call(("Failed to create".to_string(), true));
                                    }
                                });
                                show_new_entity.set(false);
                            },
                            on_cancel: move |_| show_new_entity.set(false),
                        }
                    }
                }
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
    entity: EntityListItem,
    expanded: bool,
    all_entities: Vec<EntityListItem>,
    on_toggle: EventHandler<()>,
    on_change: EventHandler<Vec<EntityListItem>>,
    on_status: EventHandler<(String, bool)>,
    on_collapse: EventHandler<()>,
) -> Element {
    let id_count = entity.ids.len();

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
                if entity.triggers_encounter {
                    span { class: "tag tag-warning", "Trigger" }
                }
                if entity.is_kill_target {
                    span { class: "tag tag-success", "Kill Target" }
                }
                // Show HP overlay tag when behavior differs from is_boss default
                if entity.show_on_hp_overlay && !entity.is_boss {
                    span { class: "tag tag-info", "HP Overlay" }
                }
                if !entity.show_on_hp_overlay && entity.is_boss {
                    span { class: "tag tag-muted", "HP Hidden" }
                }
            }

            // Expanded content
            if expanded {
                {
                    let all_entities_for_save = all_entities.clone();
                    let all_entities_for_delete = all_entities.clone();
                    rsx! {
                        div { class: "list-item-body",
                            EntityEditForm {
                                entity: entity.clone(),
                                on_save: move |(updated, original_name): (EntityListItem, String)| {
                                    let all = all_entities_for_save.clone();
                                    on_status.call(("Saving...".to_string(), false));
                                    spawn(async move {
                                        if api::update_entity(&updated, &original_name).await {
                                            // Update local state
                                            let new_list: Vec<_> = all.iter()
                                                .map(|e| if e.name == original_name { updated.clone() } else { e.clone() })
                                                .collect();
                                            on_change.call(new_list);
                                            on_status.call(("Saved".to_string(), false));
                                        } else {
                                            on_status.call(("Failed to save".to_string(), true));
                                        }
                                    });
                                },
                                on_delete: {
                                    let all_entities = all_entities_for_delete.clone();
                                    move |entity_to_delete: EntityListItem| {
                                        let all_entities = all_entities.clone();
                                        spawn(async move {
                                            if api::delete_entity(
                                                &entity_to_delete.name,
                                                &entity_to_delete.boss_id,
                                                &entity_to_delete.file_path
                                            ).await {
                                                let updated: Vec<_> = all_entities.iter()
                                                    .filter(|e| e.name != entity_to_delete.name)
                                                    .cloned()
                                                    .collect();
                                                on_change.call(updated);
                                                on_collapse.call(());
                                                on_status.call(("Deleted".to_string(), false));
                                            } else {
                                                on_status.call(("Failed to delete".to_string(), true));
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
    entity: EntityListItem,
    on_save: EventHandler<(EntityListItem, String)>,
    on_delete: EventHandler<EntityListItem>,
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
                            checked: draft().triggers_encounter,
                            onchange: move |e| {
                                let mut d = draft();
                                d.triggers_encounter = e.checked();
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
                            checked: draft().show_on_hp_overlay,
                            onchange: move |e| {
                                let mut d = draft();
                                d.show_on_hp_overlay = e.checked();
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
// New Entity Form
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn NewEntityForm(
    boss: BossListItem,
    on_create: EventHandler<EntityListItem>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut name = use_signal(|| String::new());
    let mut ids = use_signal(Vec::<i64>::new);
    let mut is_boss = use_signal(|| false);
    let mut triggers_encounter = use_signal(|| false);
    let mut is_kill_target = use_signal(|| false);
    let mut show_on_hp_overlay = use_signal(|| false);

    let handle_create = move |_| {
        let new_entity = EntityListItem {
            name: name(),
            boss_id: boss.id.clone(),
            boss_name: boss.name.clone(),
            file_path: boss.file_path.clone(),
            ids: ids(),
            is_boss: is_boss(),
            triggers_encounter: triggers_encounter(),
            is_kill_target: is_kill_target(),
            show_on_hp_overlay: show_on_hp_overlay(),
        };
        on_create.call(new_entity);
    };

    rsx! {
        div { class: "new-item-form mb-md",
            div { class: "form-row-hz",
                label { "Name" }
                input {
                    class: "input-inline",
                    style: "width: 200px;",
                    placeholder: "e.g., Styrak",
                    value: "{name}",
                    oninput: move |e| name.set(e.value())
                }
            }

            div { class: "form-row-hz", style: "align-items: flex-start;",
                label { style: "padding-top: 6px;", "NPC IDs" }
                NpcIdChipEditor {
                    ids: ids(),
                    on_change: move |new_ids| ids.set(new_ids)
                }
            }

            div { class: "form-section",
                div { class: "flex gap-md flex-wrap",
                    label { class: "flex items-center gap-xs cursor-pointer",
                        input {
                            r#type: "checkbox",
                            checked: is_boss(),
                            onchange: move |e| is_boss.set(e.checked())
                        }
                        "Is Boss"
                    }

                    label { class: "flex items-center gap-xs cursor-pointer",
                        input {
                            r#type: "checkbox",
                            checked: triggers_encounter(),
                            onchange: move |e| triggers_encounter.set(e.checked())
                        }
                        "Triggers Encounter"
                    }

                    label { class: "flex items-center gap-xs cursor-pointer",
                        input {
                            r#type: "checkbox",
                            checked: is_kill_target(),
                            onchange: move |e| is_kill_target.set(e.checked())
                        }
                        "Kill Target"
                    }

                    label { class: "flex items-center gap-xs cursor-pointer",
                        input {
                            r#type: "checkbox",
                            checked: show_on_hp_overlay(),
                            onchange: move |e| show_on_hp_overlay.set(e.checked())
                        }
                        "Show on HP Overlay"
                    }
                }
            }

            div { class: "flex gap-xs mt-sm",
                button {
                    class: if name().is_empty() { "btn btn-sm" } else { "btn btn-success btn-sm" },
                    disabled: name().is_empty(),
                    onclick: handle_create,
                    "Create Entity"
                }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
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
