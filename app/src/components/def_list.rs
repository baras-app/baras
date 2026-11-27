//! Pieces shared by the editors' definition lists: the column names above the
//! rows, sorting by a column, and the "Hide disabled" toggle

use dioxus::prelude::*;

/// (column index, descending); `None` keeps the list's own order
pub type ColSort = Option<(usize, bool)>;

/// One cell of the column header. A blank label holds the place of a column
/// that has no name (icon, origin dot, toggles) and does not sort
#[derive(Clone, Copy, PartialEq)]
pub struct Column {
    pub label: &'static str,
    /// Class shared with the rows' cells, for the columns that drop when narrow
    pub class: &'static str,
    pub title: &'static str,
}

impl Column {
    pub const BLANK: Self = Self::new("", "", "");

    pub const fn new(label: &'static str, class: &'static str, title: &'static str) -> Self {
        Self { label, class, title }
    }
}

/// Column names on the rows' grid (`cols` is the rows' grid class). A name
/// sorts by its column: ascending, then descending, then back to the list's order
#[component]
pub fn ListHead(cols: &'static str, columns: &'static [Column], sort: Signal<ColSort>) -> Element {
    let current = sort();
    rsx! {
        div { class: "def-row def-list-head {cols}",
            for (i, col) in columns.iter().enumerate() {
                if col.label.is_empty() {
                    span {}
                } else {
                    {
                        let desc = current.and_then(|(c, desc)| (c == i).then_some(desc));
                        rsx! {
                            span {
                                class: "{col.class} sortable",
                                class: if desc.is_some() { "active" },
                                title: "{col.title}",
                                onclick: move |_| sort.set(match desc {
                                    None => Some((i, false)),
                                    Some(false) => Some((i, true)),
                                    Some(true) => None,
                                }),
                                "{col.label}"
                                if let Some(desc) = desc {
                                    i { class: if desc { "fa-solid fa-caret-down" } else { "fa-solid fa-caret-up" } }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Order `items` by a column's text; ties keep their order
pub fn sort_by_column<T>(items: &mut [T], sort: ColSort, text: impl Fn(usize, &T) -> String) {
    if let Some((col, desc)) = sort {
        items.sort_by_cached_key(|t| text(col, t).to_lowercase());
        if desc {
            items.reverse();
        }
    }
}

/// How many entries the list holds, and how many of them the filters leave
#[component]
pub fn ListCount(shown: usize, total: usize, noun: &'static str) -> Element {
    rsx! {
        span { class: "text-sm text-secondary",
            if shown == total { "{total} {noun}" } else { "{shown} of {total} {noun}" }
        }
    }
}

/// Leaves disabled entries out of the list; only shown when there are some
#[component]
pub fn HideDisabled(hidden: Signal<bool>, count: usize, noun: &'static str) -> Element {
    rsx! {
        if count > 0 {
            button {
                class: "btn btn-xs editor-sort-toggle",
                class: if hidden() { "active" },
                title: if hidden() { "Disabled {noun} are hidden: click to list them" } else { "Leave disabled {noun} out of the list" },
                onclick: move |_| hidden.toggle(),
                i { class: "fa-solid fa-eye-slash" }
                " Hide disabled ({count})"
            }
        }
    }
}
