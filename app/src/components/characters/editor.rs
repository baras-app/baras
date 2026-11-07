//! Add / edit one character: the fields the logs cannot know, plus manual
//! lockout marks for kills made without logging.

use dioxus::prelude::*;

use baras_types::characters::{
    CHARACTER_NOTE_MAX, CLASSES, CREW_SKILLS, CharacterRecord, CharacterRow, Faction, Lockout, ManualLock,
    OperationRef,
};

use crate::api;
use crate::components::{ToastSeverity, use_toast};

use super::lockouts::{tier_short, until};

const SERVER_OPTIONS: &str = "character-server-options";
const OPERATION_OPTIONS: &str = "character-operation-options";
const TIERS: &[(&str, &str)] = &[("Story", "Story mode"), ("Veteran", "Veteran mode"), ("Master", "Master mode")];

/// What the editor opens on: a saved or discovered row, or nothing (new)
#[derive(Clone, PartialEq)]
pub struct EditTarget {
    pub record: CharacterRecord,
    pub previous_key: Option<String>,
    pub lockouts: Vec<Lockout>,
}

impl From<&CharacterRow> for EditTarget {
    fn from(row: &CharacterRow) -> Self {
        Self {
            record: CharacterRecord {
                name: row.name.clone(),
                server: row.server.clone(),
                // Auto-derived classes stay auto until the user picks
                classes: if row.classes_auto { Vec::new() } else { row.classes.iter().map(|c| c.name.clone()).collect() },
                crew_skills: row.crew_skills.clone(),
                guild: row.guild.clone(),
                faction: Some(row.faction).filter(|f| *f != Faction::Unknown),
                note: row.note.clone(),
                manual_locks: Vec::new(),
            },
            previous_key: Some(row.key.clone()),
            lockouts: row.lockouts.clone(),
        }
    }
}

fn slot(v: &[String], i: usize) -> String {
    v.get(i).cloned().unwrap_or_default()
}

fn set_slot(v: &mut Vec<String>, i: usize, value: String) {
    while v.len() <= i {
        v.push(String::new());
    }
    v[i] = value;
}

#[component]
pub fn CharacterEditor(
    target: EditTarget,
    servers: Vec<String>,
    operations: Vec<OperationRef>,
    on_close: EventHandler<()>,
    /// Something was written; the roster should reload
    on_saved: EventHandler<()>,
) -> Element {
    let mut toast = use_toast();
    let mut rec = use_signal(|| target.record.clone());
    let mut faction = use_signal(|| target.record.faction);
    let key = target.previous_key.clone();
    let is_new = key.is_none();
    let manual: Vec<Lockout> = target.lockouts.iter().filter(|l| l.manual).cloned().collect();
    let mut lock_op = use_signal(String::new);
    let mut lock_tier = use_signal(|| "Master".to_string());

    let save = move |_| {
        let mut record = rec.read().clone();
        record.faction = *faction.read();
        record.classes.retain(|c| !c.is_empty());
        record.crew_skills.retain(|c| !c.is_empty());
        let previous = key.clone();
        spawn(async move {
            match api::save_character(&record, previous.as_deref()).await {
                Ok(_) => {
                    on_saved.call(());
                    on_close.call(());
                }
                Err(e) => toast.show(format!("Could not save character: {e}"), ToastSeverity::Normal),
            }
        });
    };

    let lock_key = target.previous_key.clone();
    let add_lock = {
        let operations = operations.clone();
        move |_| {
            let Some(key) = lock_key.clone() else { return };
            let operation = lock_op.read().trim().to_string();
            if operation.is_empty() {
                return;
            }
            let area_id = operations.iter().find(|o| o.name == operation).map_or(0, |o| o.area_id);
            let lock = ManualLock { operation, area_id, tier: lock_tier.read().clone(), marked_ms: js_sys::Date::now() as i64 };
            spawn(async move {
                match api::add_manual_lock(&key, &lock).await {
                    Ok(true) => {
                        lock_op.set(String::new());
                        on_saved.call(());
                        on_close.call(());
                    }
                    _ => toast.show("Could not save the lockout", ToastSeverity::Normal),
                }
            });
        }
    };

    rsx! {
        div { class: "modal-overlay", onclick: move |_| on_close.call(()),
            div { class: "modal-content character-editor", onclick: move |e| e.stop_propagation(),
                div { class: "modal-header",
                    h3 { if is_new { "Add character" } else { "Edit character" } }
                    button { class: "btn btn-xs btn-ghost", onclick: move |_| on_close.call(()), i { class: "fa-solid fa-xmark" } }
                }
                div { class: "character-form",
                    div { class: "form-row",
                        label { "Name" }
                        input { r#type: "text", value: "{rec.read().name}", autofocus: is_new, oninput: move |e| rec.write().name = e.value() }
                    }
                    div { class: "form-row",
                        label { "Server" }
                        input { r#type: "text", list: SERVER_OPTIONS, placeholder: "Star Forge", value: "{rec.read().server}", oninput: move |e| rec.write().server = e.value() }
                        datalist { id: SERVER_OPTIONS, for s in servers.iter() { option { value: "{s}" } } }
                    }
                    div { class: "form-row",
                        label { "Faction" }
                        select {
                            value: match *faction.read() { None => "", Some(Faction::Republic) => "republic", Some(Faction::Empire) => "empire", Some(Faction::Unknown) => "" },
                            onchange: move |e| faction.set(match e.value().as_str() { "republic" => Some(Faction::Republic), "empire" => Some(Faction::Empire), _ => None }),
                            option { value: "", "Not set" }
                            option { value: "republic", "Republic" }
                            option { value: "empire", "Empire" }
                        }
                    }
                    div { class: "form-row",
                        label { "Guild" }
                        input { r#type: "text", value: "{rec.read().guild}", oninput: move |e| rec.write().guild = e.value() }
                    }
                    div { class: "form-row span-2",
                        label { "Combat styles" span { class: "hint", " · leave empty to follow your logs" } }
                        div { class: "character-slots",
                            for i in 0..2usize {
                                select { value: "{slot(&rec.read().classes, i)}", onchange: move |e| set_slot(&mut rec.write().classes, i, e.value()),
                                    option { value: "", "—" }
                                    for c in CLASSES.iter() { option { value: "{c}", "{c}" } }
                                }
                            }
                        }
                    }
                    div { class: "form-row span-2",
                        label { "Crew skills" }
                        div { class: "character-slots",
                            for i in 0..3usize {
                                select { value: "{slot(&rec.read().crew_skills, i)}", onchange: move |e| set_slot(&mut rec.write().crew_skills, i, e.value()),
                                    option { value: "", "—" }
                                    for s in CREW_SKILLS.iter() { option { value: "{s}", "{s}" } }
                                }
                            }
                        }
                    }
                    div { class: "form-row span-2",
                        label { "Note" }
                        textarea { rows: "3", maxlength: "{CHARACTER_NOTE_MAX}", value: "{rec.read().note}", oninput: move |e| rec.write().note = e.value() }
                    }
                    if !is_new {
                        div { class: "form-row span-2 character-locks",
                            label { "Manual lockouts" span { class: "hint", " · for kills made without logging" } }
                            for l in manual.iter() {
                                {
                                    let (op, tier, key) = (l.operation.clone(), l.tier.clone(), target.previous_key.clone().unwrap_or_default());
                                    let label = format!("{} · {} · resets in {}", l.operation, tier_short(&l.tier), until(l.expires_ms));
                                    rsx! {
                                        div { class: "character-lock-row",
                                            span { "{label}" }
                                            button { class: "btn btn-xs btn-ghost", title: "Clear this mark",
                                                onclick: move |_| {
                                                    let (key, op, tier) = (key.clone(), op.clone(), tier.clone());
                                                    spawn(async move {
                                                        if api::clear_manual_lock(&key, &op, &tier).await.is_ok() {
                                                            on_saved.call(());
                                                            on_close.call(());
                                                        }
                                                    });
                                                },
                                                i { class: "fa-solid fa-xmark" }
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "character-lock-add",
                                input { r#type: "text", list: OPERATION_OPTIONS, placeholder: "Operation", value: "{lock_op}", oninput: move |e| lock_op.set(e.value()) }
                                datalist { id: OPERATION_OPTIONS, for o in operations.iter() { option { value: "{o.name}" } } }
                                select { value: "{lock_tier}", onchange: move |e| lock_tier.set(e.value()),
                                    for (t, label) in TIERS.iter() { option { value: "{t}", "{label}" } }
                                }
                                button { class: "btn btn-xs", onclick: add_lock, i { class: "fa-solid fa-lock" } " Mark locked" }
                            }
                        }
                    }
                }
                div { class: "modal-footer form-actions",
                    button { class: "btn btn-ghost", onclick: move |_| on_close.call(()), "Cancel" }
                    button { class: "btn btn-primary", disabled: rec.read().name.trim().is_empty(), onclick: save, "Save" }
                }
            }
        }
    }
}
