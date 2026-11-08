//! Characters tab: every character your logs have named, across servers,
//! with the details the logs cannot know and the operations each one is
//! locked out of until the next reset.

mod editor;
mod lockouts;
mod matrix;
mod settings;
mod table;

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

use std::collections::HashSet;

use baras_types::characters::{CharacterRecord, CharacterRow, operation_key};

use crate::api;
use crate::components::{ToastSeverity, use_toast};

pub use settings::LockoutResetSetting;

use editor::{CharacterEditor, EditTarget};
use lockouts::until;
use matrix::LockoutMatrix;
use table::{CharacterTable, SortKey, sort_rows};

const ROLES: &[&str] = &["Tank", "Healer", "Dps"];

/// Is the character locked out of this operation at `tier` (empty = any mode)?
fn locked_at(row: &CharacterRow, op_key: &str, tier: &str) -> bool {
    row.lockouts
        .iter()
        .any(|l| operation_key(l.area_id, &l.operation) == op_key && (tier.is_empty() || l.tier == tier))
}

const TIERS: [(&str, &str); 3] = [("Story", "SM"), ("Veteran", "VM"), ("Master", "NiM")];

/// Lockout picks: characters locked out of any of these (operation key,
/// mode) pairs are hidden. An empty mode means any mode.
#[derive(Clone, PartialEq, Default)]
struct FreeFor {
    picks: HashSet<(String, String)>,
}

impl FreeFor {
    fn excludes(&self, row: &CharacterRow) -> bool {
        self.picks.iter().any(|(k, t)| locked_at(row, k, t))
    }

    fn has_op(&self, key: &str) -> bool {
        self.picks.iter().any(|(k, _)| k == key)
    }

    fn toggle(&mut self, key: &str, tier: &str) {
        let pick = (key.to_string(), tier.to_string());
        if !self.picks.remove(&pick) {
            self.picks.insert(pick);
        }
    }
}

fn matches(row: &CharacterRow, search: &str, server: &str, role: &str, faction: &str, free_for: &FreeFor) -> bool {
    let q = search.trim().to_lowercase();
    let text = || [&row.name, &row.guild, &row.note, &row.server].into_iter().any(|f| f.to_lowercase().contains(&q));
    (q.is_empty() || text())
        && (server.is_empty() || row.server == server)
        && (role.is_empty() || row.roles.iter().any(|r| r.name == role))
        && (faction.is_empty() || row.faction.label() == faction)
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
    let mut show_hidden = use_signal(|| false);
    // Which view: the roster table, or the lockout matrix
    let mut matrix = use_signal(|| false);
    let mut free_for: Signal<FreeFor> = use_signal(FreeFor::default);
    let sort_key = use_signal(|| SortKey::LastSeen);
    let sort_desc = use_signal(|| true);
    let mut editing: Signal<Option<EditTarget>> = use_signal(|| None);

    // A pull just landed: lockouts may have changed
    use_future(move || async move {
        let closure = Closure::new(move |_: JsValue| roster.restart());
        api::tauri_listen("history-updated", &closure).await;
        closure.forget();
    });

    let data = roster.read().clone().unwrap_or_default();
    let hidden_count = data.rows.iter().filter(|r| r.hidden).count();
    let mut rows: Vec<CharacterRow> = data
        .rows
        .iter()
        .filter(|r| r.hidden == *show_hidden.read())
        .filter(|r| matches(r, &search.read(), &server.read(), &role.read(), &faction.read(), &free_for.read()))
        .cloned()
        .collect();
    sort_rows(&mut rows, *sort_key.read(), *sort_desc.read());
    let total = data.rows.len() - hidden_count;
    // One badge per operation seen in the logs, with how many characters it
    // locks out at the chosen difficulty
    // Per operation: its key, name, and how many characters each of its
    // modes locks out (every mode when the area has no definition)
    let op_badges: Vec<(String, String, Vec<(&'static str, &'static str, usize)>)> = data
        .operations
        .iter()
        .map(|o| {
            let key = operation_key(o.area_id, &o.name);
            let modes = TIERS
                .iter()
                .filter(|(t, _)| o.tiers.is_empty() || o.tiers.iter().any(|tb| tb.tier == *t))
                .map(|(t, short)| (*t, *short, data.rows.iter().filter(|r| !r.hidden && locked_at(r, &key, t)).count()))
                .collect();
            (key, o.name.clone(), modes)
        })
        .collect();
    let active_picks = free_for.read().picks.len();

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
                    class: "search-input characters-search",
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
                if hidden_count > 0 {
                    label { class: "history-toggle",
                        input { r#type: "checkbox", checked: *show_hidden.read(), onchange: move |e| show_hidden.set(e.checked()) }
                        " Hidden ({hidden_count})"
                    }
                }
                button {
                    class: "btn btn-xs characters-add",
                    onclick: move |_| editing.set(Some(EditTarget { record: CharacterRecord::default(), previous_key: None, lockouts: Vec::new() })),
                    i { class: "fa-solid fa-plus" }
                    " Add character"
                }
            }
            div { class: "characters-summary",
                div { class: "history-cards characters-stats",
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
                            i { class: "fa-solid fa-lock" }
                            span { class: "op-filter-label", "Lockouts" }
                            span { class: "hint", "Click a circle to hide the characters locked out there" }
                            if active_picks > 0 {
                                button { class: "btn btn-xs btn-ghost", title: "Show every character again", onclick: move |_| free_for.write().picks.clear(),
                                    i { class: "fa-solid fa-xmark" } " Clear"
                                }
                            }
                        }
                        div { class: "op-filter-grid",
                            for (key, op, modes) in op_badges.iter() {
                                {
                                    let ff = free_for.read();
                                    let active = ff.has_op(key);
                                    let any_locked = modes.iter().any(|(.., n)| *n > 0);
                                    let key_tile = key.clone();
                                    rsx! {
                                        div {
                                            class: if active { "op-filter-badge active" } else if any_locked { "op-filter-badge locked" } else { "op-filter-badge free" },
                                            title: "{op} · click to hide characters locked out at any mode",
                                            onclick: move |_| free_for.write().toggle(&key_tile, ""),
                                            span { class: "op-filter-name", "{op}" }
                                            span { class: "op-filter-modes",
                                                for (t, short, n) in modes.iter() {
                                                    {
                                                        let (key, tier) = (key.clone(), *t);
                                                        let lit = ff.picks.contains(&(key.clone(), tier.to_string())) || ff.picks.contains(&(key.clone(), String::new()));
                                                        rsx! {
                                                            button {
                                                                class: format!("op-mode tier-{}{}{}", t.to_lowercase(), if *n == 0 { " none" } else { "" }, if lit { " lit" } else { "" }),
                                                                title: "{short}: {n} locked out · click to hide them",
                                                                onclick: move |e| {
                                                                    e.stop_propagation();
                                                                    free_for.write().toggle(&key, tier);
                                                                },
                                                                "{n}"
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
            div { class: "data-tab-selector characters-views",
                button { class: if !*matrix.read() { "data-tab active" } else { "data-tab" }, title: "Every character with its details", onclick: move |_| matrix.set(false),
                    i { class: "fa-solid fa-table-list" } " Roster"
                }
                button { class: if *matrix.read() { "data-tab active" } else { "data-tab" }, title: "Who is locked out of what, boss by boss", onclick: move |_| matrix.set(true),
                    i { class: "fa-solid fa-table-cells" } " Lockouts"
                }
            }
            if *matrix.read() {
                LockoutMatrix {
                    rows,
                    operations: data.operations.clone(),
                    on_edit: move |row: CharacterRow| editing.set(Some(EditTarget::from(&row))),
                }
            } else {
                CharacterTable {
                    rows,
                    sort_key,
                    sort_desc,
                    on_edit: move |row: CharacterRow| editing.set(Some(EditTarget::from(&row))),
                    on_delete,
                    on_restore,
                }
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
