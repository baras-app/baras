//! Characters tab: every character your logs have named, across servers,
//! with the details the logs cannot know and the operations each one is
//! locked out of until the next reset.

mod editor;
mod filter;
mod lockouts;
mod matrix;
mod settings;
mod table;

use crate::components::multi_select::{Entry, MultiSelect, class_entries, plain_entries};
use crate::components::toggle_switch::ToggleSwitch;
use baras_types::history::keeps;
use dioxus::prelude::*;

use baras_types::characters::{CharacterRecord, CharacterRow};

use crate::api;
use crate::components::{ToastSeverity, use_toast};

pub use settings::LockoutResetSetting;

use editor::{CharacterEditor, EditTarget};
use filter::{LockFilter, lock_filter_block};
use lockouts::until;
use matrix::{LockoutMatrix, TierPicks};
use table::{CharacterTable, SortKey, sort_rows};

/// Toolbar multi-selects; each empty list means "any"
#[derive(Clone, PartialEq, Default)]
struct RosterFilter {
    servers: Vec<String>,
    roles: Vec<String>,
    factions: Vec<String>,
    classes: Vec<String>,
}

impl RosterFilter {
    fn keeps(&self, row: &CharacterRow) -> bool {
        keeps(&self.servers, &row.server)
            && keeps(&self.factions, row.faction.label())
            && keeps_any(&self.roles, row.roles.iter().map(|r| r.name.as_str()))
            && keeps_any(&self.classes, row.classes.iter().map(|c| c.name.as_str()))
    }
}

/// A row with several values (classes, roles) passes when any one is picked
fn keeps_any<'a>(picked: &[String], mut values: impl Iterator<Item = &'a str>) -> bool {
    picked.is_empty() || values.any(|v| keeps(picked, v))
}

fn entries(pairs: &[(&str, &str)]) -> Vec<Entry> {
    pairs
        .iter()
        .map(|(v, l)| (l.to_string(), vec![v.to_string()]))
        .collect()
}

fn matches(row: &CharacterRow, search: &str, filter: &RosterFilter) -> bool {
    let q = search.trim().to_lowercase();
    let text = || {
        [&row.name, &row.guild, &row.note, &row.server]
            .into_iter()
            .any(|f| f.to_lowercase().contains(&q))
    };
    (q.is_empty() || text()) && filter.keeps(row)
}

#[component]
pub fn CharactersPanel() -> Element {
    let mut toast = use_toast();
    let mut roster = use_resource(|| async move { api::get_character_roster().await.unwrap_or_default() });
    let mut search = use_signal(String::new);
    let mut filter: Signal<RosterFilter> = use_signal(RosterFilter::default);
    let mut show_hidden = use_signal(|| false);
    // Which view: the roster table, or the lockout matrix
    let mut matrix = use_signal(|| false);
    // Modes the lockout matrix draws (Story, Veteran, Master)
    let mut matrix_tiers = use_signal(|| [true; 3]);
    // Saved by `TierPicks`; restored once per mount
    use_future(move || async move {
        // All-off can only come from an old first-run default; the picks never allow it
        if let Some(cfg) = api::get_config().await
            && cfg.lockout_tiers.iter().any(|on| *on)
        {
            matrix_tiers.set(cfg.lockout_tiers);
        }
    });
    let locked_for: Signal<LockFilter> = use_signal(LockFilter::default);
    let sort_key = use_signal(|| SortKey::LastSeen);
    let sort_desc = use_signal(|| true);
    let mut editing: Signal<Option<EditTarget>> = use_signal(|| None);

    // A pull just landed: lockouts may have changed
    let mut pulls = use_signal(|| 0u32);
    api::use_tauri_event("history-updated", move |_| {
        let _ = pulls.try_write().map(|mut w| *w += 1);
    });
    use_effect(move || {
        if *pulls.read() > 0 {
            roster.restart();
        }
    });

    let data = roster.read().clone().unwrap_or_default();
    let hidden_count = data.rows.iter().filter(|r| r.hidden).count();
    // Everything the toolbar keeps; the lockout counts are over these
    let pool: Vec<&CharacterRow> = data
        .rows
        .iter()
        .filter(|r| r.hidden == *show_hidden.read())
        .filter(|r| matches(r, &search.read(), &filter.read()))
        .collect();
    let mut rows: Vec<CharacterRow> = pool
        .iter()
        .filter(|r| locked_for.read().keeps(r))
        .map(|r| (*r).clone())
        .collect();
    sort_rows(&mut rows, *sort_key.read(), *sort_desc.read());
    let total = data.rows.len() - hidden_count;
    let f = filter.read().clone();
    let reload = move |_| roster.restart();
    let on_delete = move |row: CharacterRow| {
        spawn(async move {
            match api::delete_character(&row.key).await {
                Ok(true) => {
                    toast.show(
                        format!("Removed {}. Tick “Hidden” to bring it back.", row.name),
                        ToastSeverity::Normal,
                    );
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
                MultiSelect {
                    any_label: "All servers",
                    noun: "servers",
                    entries: plain_entries(&data.servers),
                    picked: f.servers.clone(),
                    on_change: move |v| filter.write().servers = v,
                }
                MultiSelect {
                    any_label: "Any class",
                    noun: "classes",
                    title: "Mirror classes count as one",
                    entries: class_entries(data.rows.iter().flat_map(|r| r.classes.iter().map(|c| c.name.as_str()))),
                    picked: f.classes.clone(),
                    on_change: move |v| filter.write().classes = v,
                }
                MultiSelect {
                    any_label: "Any role",
                    noun: "roles",
                    entries: entries(&[("Tank", "Tank"), ("Healer", "Healer"), ("Dps", "DPS")]),
                    picked: f.roles.clone(),
                    on_change: move |v| filter.write().roles = v,
                }
                MultiSelect {
                    any_label: "Both factions",
                    noun: "factions",
                    entries: entries(&[("Republic", "Republic"), ("Empire", "Empire")]),
                    picked: f.factions.clone(),
                    on_change: move |v| filter.write().factions = v,
                }
                if hidden_count > 0 {
                    ToggleSwitch {
                        label: format!("Hidden ({hidden_count})"),
                        title: "Show the characters you removed instead",
                        checked: *show_hidden.read(),
                        on_change: move |v| show_hidden.set(v),
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
                {lock_filter_block(&data.operations, &pool, locked_for)}
            }
            div { class: "data-tab-selector characters-views",
                button { class: if !*matrix.read() { "data-tab active" } else { "data-tab" }, title: "Every character with its details", onclick: move |_| matrix.set(false),
                    i { class: "fa-solid fa-table-list" } " Roster"
                }
                button { class: if *matrix.read() { "data-tab active" } else { "data-tab" }, title: "Who is locked out of what, boss by boss", onclick: move |_| matrix.set(true),
                    i { class: "fa-solid fa-table-cells" } " Lockouts"
                }
                if *matrix.read() {
                    TierPicks { tiers: matrix_tiers }
                }
            }
            if *matrix.read() {
                LockoutMatrix {
                    rows,
                    operations: data.operations.clone(),
                    tiers: *matrix_tiers.read(),
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
