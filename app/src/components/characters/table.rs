//! The roster table: one row per character, sortable by the header cells.

use dioxus::prelude::*;

use baras_types::ClassColorConfig;
use baras_types::characters::{CharacterRow, Faction};
use baras_types::history::iso_to_ms;

use crate::components::class_icons::{get_class_icon, get_role_icon};

use super::lockouts::ago;

static ICON_EMPIRE: Asset = asset!("/assets/icons/faction/empire.png");
static ICON_REPUBLIC: Asset = asset!("/assets/icons/faction/republic.png");

/// Faction emblem images
#[component]
pub fn FactionIcon(faction: Faction) -> Element {
    match faction {
        Faction::Empire => rsx! {
            img { class: "char-faction empire", src: ICON_EMPIRE, alt: "Empire", title: "Empire" }
        },
        Faction::Republic => rsx! {
            img { class: "char-faction republic", src: ICON_REPUBLIC, alt: "Republic", title: "Republic" }
        },
        Faction::Unknown => rsx! { span { class: "char-faction unknown", title: "Faction not set", "—" } },
    }
}

/// Class glyph recolored to its default overlay class color (as in the overlays).
/// Recolored with an SVG filter rather than a CSS mask: masks are downsampled
/// without smoothing in WebKit, which breaks thin glyphs apart at 24px.
#[component]
fn ClassGlyph(name: String, icon: String) -> Element {
    let Some(src) = get_class_icon(&icon) else { return rsx! {} };
    let color = ClassColorConfig::default()
        .for_class_name(&name)
        .map(|[r, g, b, _]| format!("rgb({r}, {g}, {b})"))
        .unwrap_or_else(|| "currentColor".into());
    let id = format!("class-tint-{icon}");
    rsx! {
        svg { class: "char-class-glyph", view_box: "0 0 24 24",
            defs {
                filter { id: "{id}", color_interpolation_filters: "sRGB",
                    feFlood { flood_color: "{color}" }
                    feComposite { in2: "SourceAlpha", operator: "in" }
                }
            }
            image { href: "{src}", width: "24", height: "24", filter: "url(#{id})" }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Server,
    LastSeen,
}

pub fn sort_rows(rows: &mut [CharacterRow], key: SortKey, desc: bool) {
    rows.sort_by(|a, b| {
        let ord = match key {
            SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortKey::Server => a.server.cmp(&b.server).then_with(|| a.name.cmp(&b.name)),
            SortKey::LastSeen => iso_to_ms(&a.last_seen).cmp(&iso_to_ms(&b.last_seen)),
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
            sort_desc.set(matches!(k, SortKey::LastSeen));
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
                                                span {
                                                    class: "char-class",
                                                    title: "{c.name}",
                                                    ClassGlyph { name: c.name.clone(), icon: c.icon.clone() }
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
