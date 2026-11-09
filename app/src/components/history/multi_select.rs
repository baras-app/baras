//! Multi-select filter for the history toolbars: a select-looking button over
//! a checklist. Each entry can stand for several logged values (mirror
//! disciplines are one entry), and any number of entries can be picked.

use dioxus::prelude::*;

use baras_types::history::discipline_mirror;

/// A pickable entry: its label and every logged value it stands for
pub type Entry = (String, Vec<String>);

/// One entry per value, as is
pub fn plain_entries(values: &[String]) -> Vec<Entry> {
    values.iter().map(|v| (v.clone(), vec![v.clone()])).collect()
}

/// The logged names a discipline stands for: itself and its mirror
fn names_of(name: &str) -> Vec<String> {
    match discipline_mirror(name) {
        Some((imp, rep)) => vec![imp.to_string(), rep.to_string()],
        None => vec![name.to_string()],
    }
}

/// Disciplines played, one entry per mirror pair ("Madness / Balance"), by name
pub fn discipline_entries(played: &[String]) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for d in played {
        let names = names_of(d);
        let label = names.join(" / ");
        if !out.iter().any(|(l, _)| *l == label) {
            out.push((label, names));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// `picked` with `values` added, or removed when any of them is already in
fn toggle(picked: &[String], values: &[String]) -> Vec<String> {
    let on = values.iter().any(|v| picked.contains(v));
    let mut next: Vec<String> = picked.iter().filter(|p| !values.contains(p)).cloned().collect();
    if !on {
        next.extend(values.iter().cloned());
    }
    next
}

/// Toggle one value in `picked` (a board click); a discipline brings its mirror
pub fn toggle_value(picked: &[String], value: &str, mirrored: bool) -> Vec<String> {
    let values = if mirrored { names_of(value) } else { vec![value.to_string()] };
    toggle(picked, &values)
}

#[component]
pub fn MultiSelect(
    /// Label when nothing is picked, e.g. "All characters"
    any_label: &'static str,
    /// Plural noun for several picks, e.g. "characters"
    noun: &'static str,
    entries: Vec<Entry>,
    picked: Vec<String>,
    #[props(default = "")] title: &'static str,
    on_change: EventHandler<Vec<String>>,
) -> Element {
    let mut open = use_signal(|| false);
    let on: Vec<&Entry> = entries.iter().filter(|(_, vals)| vals.iter().any(|v| picked.contains(v))).collect();
    let button_label = match on.as_slice() {
        [] => any_label.to_string(),
        [(label, _)] => label.clone(),
        many => format!("{} {noun}", many.len()),
    };

    rsx! {
        div { class: "nav-menu multi-select",
            button {
                class: if on.is_empty() { "select-trigger" } else { "select-trigger active" },
                title,
                onclick: move |_| open.toggle(),
                "{button_label}"
            }
            if open() {
                div { class: "nav-menu-backdrop", onclick: move |_| open.set(false) }
                div { class: "nav-menu-list multi-select-list",
                    button {
                        class: if on.is_empty() { "nav-menu-item active" } else { "nav-menu-item" },
                        onclick: move |_| on_change.call(Vec::new()),
                        i { class: if on.is_empty() { "fa-solid fa-square-check" } else { "fa-regular fa-square" } }
                        " {any_label}"
                    }
                    div { class: "nav-menu-divider" }
                    for (label, values) in entries.iter().cloned() {
                        {
                            let checked = values.iter().any(|v| picked.contains(v));
                            let current = picked.clone();
                            rsx! {
                                button {
                                    key: "{label}",
                                    class: if checked { "nav-menu-item active" } else { "nav-menu-item" },
                                    // The list stays open for more picks
                                    onclick: move |_| on_change.call(toggle(&current, &values)),
                                    i { class: if checked { "fa-solid fa-square-check" } else { "fa-regular fa-square" } }
                                    " {label}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
