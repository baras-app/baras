//! The roster table: one row per character, sortable by the header cells.

use dioxus::prelude::*;

use baras_types::characters::{CharacterRow, Faction};
use baras_types::history::iso_to_ms;

use crate::components::class_icons::{get_class_icon, get_role_icon};

use super::lockouts::{LockoutChips, ago};

/// Faction emblems (Sith cog, Jedi wings) drawn inline on a colored disc
#[component]
pub fn FactionIcon(faction: Faction) -> Element {
    match faction {
        Faction::Empire => rsx! {
            svg { class: "char-faction empire", view_box: "0 0 100 100", role: "img",
                title { "Empire" }
                circle { cx: "50", cy: "50", r: "50", fill: "#141414" }
                path { fill: "#d83a32", fill_rule: "evenodd", d: "M50.0 3.0L90.7 26.5L90.7 73.5L50.0 97.0L9.3 73.5L9.3 26.5ZM15.4 30.0L15.4 70.0L50.0 90.0L84.6 70.0L84.6 30.0L50.0 10.0ZM50.0 37.0L61.3 43.5L61.3 56.5L50.0 63.0L38.7 56.5L38.7 43.5ZM45.5 37.0L45.5 28.0L37.0 28.0L33.0 16.0L43.0 16.0L45.0 21.0L55.0 21.0L57.0 16.0L67.0 16.0L63.0 28.0L54.5 28.0L54.5 37.0ZM59.0 39.6L66.8 35.1L62.6 27.7L70.9 18.3L75.9 26.9L72.6 31.2L77.6 39.8L82.9 39.1L87.9 47.7L75.6 50.3L71.3 42.9L63.5 47.4ZM63.5 52.6L71.3 57.1L75.6 49.7L87.9 52.3L82.9 60.9L77.6 60.2L72.6 68.8L75.9 73.1L70.9 81.7L62.6 72.3L66.8 64.9L59.0 60.4ZM54.5 63.0L54.5 72.0L63.0 72.0L67.0 84.0L57.0 84.0L55.0 79.0L45.0 79.0L43.0 84.0L33.0 84.0L37.0 72.0L45.5 72.0L45.5 63.0ZM41.0 60.4L33.2 64.9L37.4 72.3L29.1 81.7L24.1 73.1L27.4 68.8L22.4 60.2L17.1 60.9L12.1 52.3L24.4 49.7L28.7 57.1L36.5 52.6ZM36.5 47.4L28.7 42.9L24.4 50.3L12.1 47.7L17.1 39.1L22.4 39.8L27.4 31.2L24.1 26.9L29.1 18.3L37.4 27.7L33.2 35.1L41.0 39.6Z" }
            }
        },
        Faction::Republic => rsx! {
            svg { class: "char-faction republic", view_box: "0 0 100 100", role: "img",
                title { "Republic" }
                circle { cx: "50", cy: "50", r: "50", fill: "#1f5fb8" }
                path { fill: "#ffffff", d: "M48.4 74.0L51.6 74.0L50.9 6.0L49.1 6.0ZM50.0 65.0L51.3 72.8L57.8 68.2L53.2 74.7L61.0 76.0L53.2 77.3L57.8 83.8L51.3 79.2L50.0 87.0L48.7 79.2L42.2 83.8L46.8 77.3L39.0 76.0L46.8 74.7L42.2 68.2L48.7 72.8ZM46 96 C26 93 8 76 9 52 C10 38 16 27 25 18 C22 30 24 40 31 48 C27 37 30 28 38 22 C35 33 38 43 44 52 C42 62 43 78 46 96 ZM54 96 C74 93 92 76 91 52 C90 38 84 27 75 18 C78 30 76 40 69 48 C73 37 70 28 62 22 C65 33 62 43 56 52 C58 62 57 78 54 96 Z" }
            }
        },
        Faction::Unknown => rsx! { span { class: "char-faction unknown", title: "Faction not set", "—" } },
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Server,
    LastSeen,
    Lockouts,
}

pub fn sort_rows(rows: &mut [CharacterRow], key: SortKey, desc: bool) {
    rows.sort_by(|a, b| {
        let ord = match key {
            SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortKey::Server => a.server.cmp(&b.server).then_with(|| a.name.cmp(&b.name)),
            SortKey::LastSeen => iso_to_ms(&a.last_seen).cmp(&iso_to_ms(&b.last_seen)),
            SortKey::Lockouts => a.lockouts.len().cmp(&b.lockouts.len()),
        };
        if desc { ord.reverse() } else { ord }
    });
}

#[component]
pub fn CharacterTable(
    rows: Vec<CharacterRow>,
    sort_key: Signal<SortKey>,
    sort_desc: Signal<bool>,
    on_edit: EventHandler<CharacterRow>,
    on_delete: EventHandler<CharacterRow>,
    on_restore: EventHandler<CharacterRow>,
) -> Element {
    let sort_class = move |k: SortKey| {
        if *sort_key.read() == k {
            if *sort_desc.read() { "sortable-header sorted desc" } else { "sortable-header sorted asc" }
        } else {
            "sortable-header"
        }
    };
    let on_sort = move |k: SortKey| {
        let (mut sort_key, mut sort_desc) = (sort_key, sort_desc);
        if *sort_key.read() == k {
            sort_desc.toggle();
        } else {
            sort_key.set(k);
            sort_desc.set(matches!(k, SortKey::LastSeen | SortKey::Lockouts));
        }
    };

    rsx! {
        div { class: "pull-history-table-wrap characters-table-wrap",
            table { class: "pull-history-table characters-table",
                thead {
                    tr {
                        th { class: sort_class(SortKey::Name), onclick: move |_| on_sort(SortKey::Name), "Character" }
                        th { class: sort_class(SortKey::Server), onclick: move |_| on_sort(SortKey::Server), "Server" }
                        th { "Faction" }
                        th { "Classes" }
                        th { "Roles" }
                        th { "Guild" }
                        th { "Crew Skills" }
                        th { class: sort_class(SortKey::LastSeen), onclick: move |_| on_sort(SortKey::LastSeen), "Last Seen" }
                        th { class: sort_class(SortKey::Lockouts), onclick: move |_| on_sort(SortKey::Lockouts), "Lockouts" }
                        th { "Note" }
                        th { }
                    }
                }
                tbody {
                    if rows.is_empty() {
                        tr { td { colspan: "11", class: "library-empty", "No characters match. Characters appear here once a log names them, or add one by hand." } }
                    }
                    for row in rows.iter() {
                        {
                            let (r1, r2, r3) = (row.clone(), row.clone(), row.clone());
                            rsx! {
                                tr { key: "{row.key}", class: if row.hidden { "hidden-row" } else { "" },
                                    td { class: "char-name",
                                        span { "{row.name}" }
                                        if !row.saved { i { class: "fa-regular fa-circle char-auto", title: "Found in your logs; not saved yet. Edit to add details." } }
                                    }
                                    td { if row.server.is_empty() { span { class: "text-muted", "Unknown" } } else { "{row.server}" } }
                                    td { class: "char-faction-cell", FactionIcon { faction: row.faction } }
                                    td {
                                        div { class: "char-icons", title: if row.classes_auto { "From your logs" } else { "Assigned" },
                                            for c in row.classes.iter() {
                                                span { class: "char-class", title: "{c.name}",
                                                    if let Some(icon) = get_class_icon(&c.icon) { img { src: *icon, alt: "" } }
                                                    "{c.name}"
                                                }
                                            }
                                            if row.classes.is_empty() { span { class: "text-muted", "—" } }
                                        }
                                    }
                                    td {
                                        div { class: "char-icons",
                                            for r in row.roles.iter() {
                                                if let Some(icon) = get_role_icon(&r.icon) { img { class: "role-icon", src: *icon, alt: "{r.name}", title: "{r.name}" } }
                                            }
                                        }
                                    }
                                    td { "{row.guild}" }
                                    td {
                                        for s in row.crew_skills.iter() { span { class: "run-tag small", "{s}" } }
                                    }
                                    td { title: "{row.last_seen}", {ago(&row.last_seen)} }
                                    td { LockoutChips { lockouts: row.lockouts.clone() } }
                                    td { class: "char-note", title: "{row.note}", "{row.note}" }
                                    td { class: "char-actions",
                                        button { class: "btn btn-xs btn-ghost", title: "Edit", onclick: move |_| on_edit.call(r1.clone()), i { class: "fa-solid fa-pen" } }
                                        if row.hidden {
                                            button { class: "btn btn-xs btn-ghost", title: "Show this character again", onclick: move |_| on_restore.call(r3.clone()), i { class: "fa-solid fa-rotate-left" } }
                                        } else {
                                            button { class: "btn btn-xs btn-ghost", title: "Remove (can be restored from hidden characters)", onclick: move |_| on_delete.call(r2.clone()), i { class: "fa-solid fa-trash" } }
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
