//! Characters tab: every character your logs have named, across servers,
//! with the details the logs cannot know and the operations each one is
//! locked out of until the next reset.

mod editor;
mod lockouts;
mod settings;
mod table;

use dioxus::prelude::*;

use std::collections::HashSet;

use baras_types::characters::{CharacterRecord, CharacterRow, operation_key};

use crate::api;
use crate::components::{ToastSeverity, use_toast};

pub use settings::LockoutResetSetting;

use editor::{CharacterEditor, EditTarget};
use lockouts::until;
use table::{CharacterTable, SortKey, sort_rows};

const ROLES: &[&str] = &["Tank", "Healer", "Dps"];

/// Which lockouts the "free for" badges look at: operations (by
/// `operation_key`) and the mode they must be free at (empty = any)
#[derive(Clone, PartialEq, Default)]
struct FreeFor {
    operations: HashSet<String>,
    tier: String,
}

impl FreeFor {
    fn locks(&self, row: &CharacterRow, op_key: &str) -> bool {
        row.lockouts
            .iter()
            .any(|l| operation_key(l.area_id, &l.operation) == op_key && (self.tier.is_empty() || l.tier == self.tier))
    }

    fn excludes(&self, row: &CharacterRow) -> bool {
        self.operations.iter().any(|op| self.locks(row, op))
    }
}

fn matches(row: &CharacterRow, search: &str, server: &str, role: &str, faction: &str, locked_only: bool, free_for: &FreeFor) -> bool {
    let q = search.trim().to_lowercase();
    let text = || [&row.name, &row.guild, &row.note, &row.server].into_iter().any(|f| f.to_lowercase().contains(&q));
    (q.is_empty() || text())
        && (server.is_empty() || row.server == server)
        && (role.is_empty() || row.roles.iter().any(|r| r.name == role))
        && (faction.is_empty() || row.faction.label() == faction)
        && (!locked_only || !row.lockouts.is_empty())
        && !free_for.excludes(row)
}

#[component]
pub fn CharactersPanel() -> Element {
    let mut toast = use_toast();
    let mut roster = use_resource(|| async move { api::get_character_roster().await.unwrap_or_default() });
    let mut search = use_signal(String::new);
    let mut server = use_signal(String::new);
    let mut role = use_signal(String::new);
    let mut faction = use_signal(String::new);
    let mut locked_only = use_signal(|| false);
    let mut show_hidden = use_signal(|| false);
    let mut free_for: Signal<FreeFor> = use_signal(FreeFor::default);
    let sort_key = use_signal(|| SortKey::LastSeen);
    let sort_desc = use_signal(|| true);
    let mut editing: Signal<Option<EditTarget>> = use_signal(|| None);

    let data = roster.read().clone().unwrap_or_default();
    let hidden_count = data.rows.iter().filter(|r| r.hidden).count();
    let mut rows: Vec<CharacterRow> = data
        .rows
        .iter()
        .filter(|r| r.hidden == *show_hidden.read())
        .filter(|r| matches(r, &search.read(), &server.read(), &role.read(), &faction.read(), *locked_only.read(), &free_for.read()))
        .cloned()
        .collect();
    sort_rows(&mut rows, *sort_key.read(), *sort_desc.read());
    let total = data.rows.len() - hidden_count;
    // One badge per operation seen in the logs, with how many characters it
    // locks out at the chosen difficulty
    let op_badges: Vec<(String, String, usize)> = {
        let ff = free_for.read();
        data.operations
            .iter()
            .map(|o| {
                let key = operation_key(o.area_id, &o.name);
                let n = data.rows.iter().filter(|r| !r.hidden && ff.locks(r, &key)).count();
                (key, o.name.clone(), n)
            })
            .collect()
    };
    let active_ops = free_for.read().operations.len();

    let reload = move |_| roster.restart();
    let on_delete = move |row: CharacterRow| {
        spawn(async move {
            match api::delete_character(&row.key).await {
                Ok(true) => {
                    toast.show(format!("Removed {}. Tick “Hidden” to bring it back.", row.name), ToastSeverity::Normal);
                    roster.restart();
                }
                _ => toast.show("Could not remove the character", ToastSeverity::Normal),
            }
        });
    };
    let on_restore = move |row: CharacterRow| {
        spawn(async move {
            if api::restore_character(&row.key).await.is_ok() {
                roster.restart();
            }
        });
    };

    rsx! {
        div { class: "history-panel characters-panel",
            div { class: "history-toolbar",
                input {
                    class: "boss-search-input characters-search",
                    r#type: "text",
                    placeholder: "Search name, guild, note…",
                    value: "{search}",
                    oninput: move |e| search.set(e.value()),
                }
                select { value: "{server}", onchange: move |e| server.set(e.value()),
                    option { value: "", "All servers" }
                    for s in data.servers.iter() { option { value: "{s}", "{s}" } }
                }
                select { value: "{role}", onchange: move |e| role.set(e.value()),
                    option { value: "", "Any role" }
                    for r in ROLES.iter() { option { value: "{r}", if *r == "Dps" { "DPS" } else { "{r}" } } }
                }
                select { value: "{faction}", onchange: move |e| faction.set(e.value()),
                    option { value: "", "Both factions" }
                    option { value: "Republic", "Republic" }
                    option { value: "Empire", "Empire" }
                }
                label { class: "history-toggle",
                    input { r#type: "checkbox", checked: *locked_only.read(), onchange: move |e| locked_only.set(e.checked()) }
                    " Locked only"
                }
                if hidden_count > 0 {
                    label { class: "history-toggle",
                        input { r#type: "checkbox", checked: *show_hidden.read(), onchange: move |e| show_hidden.set(e.checked()) }
                        " Hidden ({hidden_count})"
                    }
                }
                button {
                    class: "btn btn-xs",
                    onclick: move |_| editing.set(Some(EditTarget { record: CharacterRecord::default(), previous_key: None, lockouts: Vec::new() })),
                    i { class: "fa-solid fa-plus" }
                    " Add character"
                }
            }
            div { class: "characters-summary",
                div { class: "history-cards",
                    div { class: "history-card",
                        div { class: "k", "Characters" }
                        div { class: "v", "{total}" }
                    }
                    div { class: "history-card", title: "Veteran and Master lockouts clear on Tuesday at the reset hour",
                        div { class: "k", "Weekly reset" }
                        div { class: "v small", {until(data.next_weekly_reset_ms)} }
                    }
                    div { class: "history-card", title: "Story lockouts clear every day at the reset hour",
                        div { class: "k", "Daily reset" }
                        div { class: "v small", {until(data.next_daily_reset_ms)} }
                    }
                }
                if !op_badges.is_empty() {
                    div { class: "op-filter",
                        div { class: "op-filter-head",
                            i { class: "fa-solid fa-lock-open" }
                            span { class: "op-filter-label", title: "Pick operations to hide every character locked out of them at this mode", "Free for" }
                            select { class: "op-filter-select", value: "{free_for.read().tier}", onchange: move |e| free_for.write().tier = e.value(),
                                option { value: "", "Any mode" }
                                option { value: "Story", "Story mode" }
                                option { value: "Veteran", "Veteran mode" }
                                option { value: "Master", "Master mode" }
                            }
                            if active_ops > 0 {
                                button { class: "btn btn-xs btn-ghost", title: "Show every character again", onclick: move |_| free_for.write().operations.clear(),
                                    i { class: "fa-solid fa-xmark" } " Clear"
                                }
                            }
                        }
                        div { class: "op-filter-grid",
                            for (key, op, count) in op_badges.iter() {
                                {
                                    let (key, active) = (key.clone(), free_for.read().operations.contains(key));
                                    let title = if *count == 0 { format!("{op}: nobody locked out") } else { format!("{op}: {count} locked out · click to hide them") };
                                    rsx! {
                                        button {
                                            class: if active { "op-filter-badge active" } else if *count == 0 { "op-filter-badge free" } else { "op-filter-badge locked" },
                                            title: "{title}",
                                            onclick: move |_| {
                                                let mut ff = free_for.write();
                                                if !ff.operations.remove(&key) {
                                                    ff.operations.insert(key.clone());
                                                }
                                            },
                                            span { class: "op-filter-name", "{op}" }
                                            span { class: "op-filter-count", "{count}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            CharacterTable {
                rows,
                sort_key,
                sort_desc,
                on_edit: move |row: CharacterRow| editing.set(Some(EditTarget::from(&row))),
                on_delete,
                on_restore,
            }
            if let Some(target) = editing.read().clone() {
                CharacterEditor {
                    target,
                    servers: data.servers.clone(),
                    operations: data.operations.clone(),
                    on_close: move |_| editing.set(None),
                    on_saved: reload,
                }
            }
        }
    }
}
