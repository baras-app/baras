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
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let mut entities = use_signal(Vec::<EntityListItem>::new);
    let mut loading = use_signal(|| true);
    let mut expanded_entity = use_signal(|| None::<String>);
    let mut show_new_entity = use_signal(|| false);

    let file_path = boss.file_path.clone();
    let boss_id = boss.id.clone();

    // Load entities on mount
    use_effect(move || {
        let file_path = file_path.clone();
        let boss_id = boss_id.clone();
        spawn(async move {
            if let Some(e) = api::get_entities_for_area(&file_path).await {
                let boss_entities: Vec<_> = e.into_iter().filter(|e| e.boss_id == boss_id).collect();
                entities.set(boss_entities);
            }
            loading.set(false);
        });
    });

    rsx! {
        div { class: "entities-tab",
            // Header
            div { class: "flex items-center justify-between mb-sm",
                span { class: "text-sm text-secondary",
                    if loading() { "Loading..." } else { "{entities().len()} entities" }
                }
                button {
                    class: "btn btn-success btn-sm",
                    onclick: move |_| show_new_entity.set(true),
                    "+ New Entity"
                }
            }

            // Help text
            div { class: "text-xs text-muted mb-sm",
                "Define NPCs for this encounter. Mark bosses, adds, encounter triggers, and kill targets."
            }

            // New entity form
            if show_new_entity() {
                NewEntityForm {
                    boss: boss.clone(),
                    on_create: move |new_entity: EntityListItem| {
                        spawn(async move {
                            if let Some(created) = api::create_entity(&new_entity).await {
                                let mut current = entities();
                                current.push(created);
                                entities.set(current);
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

            // Entity list
            if loading() {
                div { class: "empty-state text-sm", "Loading entities..." }
            } else if entities().is_empty() {
                div { class: "empty-state text-sm", "No entities defined" }
            } else {
                for entity in entities() {
                    {
                        let entity_key = entity.name.clone();
                        let is_expanded = expanded_entity() == Some(entity_key.clone());
                        let entities_for_row = entities();

                        rsx! {
                            EntityRow {
                                key: "{entity_key}",
                                entity: entity.clone(),
                                expanded: is_expanded,
                                on_toggle: move |_| {
                                    expanded_entity.set(if is_expanded { None } else { Some(entity_key.clone()) });
                                },
                                on_change: move |updated: Vec<EntityListItem>| {
                                    entities.set(updated);
                                },
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

    // Format IDs as comma-separated string for display
    let ids_display = draft().ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(", ");

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
            div { class: "form-row-hz",
                label { "NPC IDs" }
                input {
                    class: "input-inline text-mono",
                    style: "width: 300px;",
                    placeholder: "123456789, 987654321, ...",
                    value: "{ids_display}",
                    oninput: move |e| {
                        let mut d = draft();
                        d.ids = e.value()
                            .split(',')
                            .filter_map(|s| s.trim().parse::<i64>().ok())
                            .collect();
                        draft.set(d);
                    }
                }
            }
            div { class: "text-xs text-muted mb-sm pl-lg",
                "Comma-separated NPC IDs that match this entity across difficulties"
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
                        span { class: "text-xs text-muted", "(primary encounter target)" }
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
                        span { class: "text-xs text-muted", "(damage starts the encounter)" }
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
                        span { class: "text-xs text-muted", "(death ends the encounter)" }
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

    let ids_display = ids().iter().map(|id| id.to_string()).collect::<Vec<_>>().join(", ");

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

            div { class: "form-row-hz",
                label { "NPC IDs" }
                input {
                    class: "input-inline text-mono",
                    style: "width: 300px;",
                    placeholder: "123456789, 987654321, ...",
                    value: "{ids_display}",
                    oninput: move |e| {
                        let parsed: Vec<i64> = e.value()
                            .split(',')
                            .filter_map(|s| s.trim().parse::<i64>().ok())
                            .collect();
                        ids.set(parsed);
                    }
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
