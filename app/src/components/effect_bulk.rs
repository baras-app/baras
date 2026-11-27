//! Bulk actions on the effects the list currently shows

use dioxus::prelude::*;

/// One bulk action over the shown effects
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum BulkAction {
    Enabled(bool),
    Audio(bool),
    Export,
    /// Every custom and modified effect, whatever the filters show
    ExportOwn,
}

const ACTIONS: [(BulkAction, &str, &str); 5] = [
    (BulkAction::Enabled(true), "fa-power-off", "Enable"),
    (BulkAction::Enabled(false), "fa-ban", "Disable"),
    (BulkAction::Audio(true), "fa-volume-high", "Audio on"),
    (BulkAction::Audio(false), "fa-volume-xmark", "Audio off"),
    (BulkAction::Export, "fa-file-export", "Export"),
];

/// The list's count as a button over the bulk menu: its entries act on the
/// `count` effects the search and filters show, out of `total`
#[component]
pub(crate) fn BulkMenu(count: usize, total: usize, on_pick: EventHandler<BulkAction>) -> Element {
    let mut open = use_signal(|| false);
    let filtered = count < total;
    let scope = if filtered {
        format!("the {count} filtered effects")
    } else {
        format!("all {count} effects")
    };
    rsx! {
        div { class: "nav-menu",
            button {
                class: if filtered { "btn btn-sm bulk-trigger filtered" } else { "btn btn-sm bulk-trigger" },
                title: "Bulk actions on {scope}. Narrow the list with the search and filters to act on fewer.",
                onclick: move |_| open.toggle(),
                if filtered { "{count} of {total} shown " } else { "All {total} shown " }
                i { class: "fa-solid fa-caret-down" }
            }
            if open() {
                div { class: "nav-menu-backdrop", onclick: move |_| open.set(false) }
                div { class: "nav-menu-list align-right", onclick: move |_| open.set(false),
                    div { class: "multi-select-group", "For {scope}" }
                    for (action, icon, label) in ACTIONS {
                        button { class: "nav-menu-item", disabled: count == 0, onclick: move |_| on_pick.call(action),
                            i { class: "fa-solid {icon}" }
                            "{label}"
                        }
                    }
                    div { class: "nav-menu-divider" }
                    button {
                        class: "nav-menu-item",
                        title: "Your custom and modified effects, whatever the filters show",
                        onclick: move |_| on_pick.call(BulkAction::ExportOwn),
                        i { class: "fa-solid fa-file-export" }
                        "Export all custom and modified"
                    }
                }
            }
        }
    }
}
