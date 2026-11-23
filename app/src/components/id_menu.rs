//! Right-click menu that copies game ids (ability, effect, entity) to the clipboard

use dioxus::prelude::*;

use crate::components::toast::{ToastManager, ToastSeverity, use_toast};
use crate::utils::copy_to_clipboard;

#[derive(Clone, PartialEq)]
pub struct IdMenu {
    x: f64,
    y: f64,
    items: Vec<(&'static str, i64)>,
}

impl IdMenu {
    /// Menu at the pointer in place of the browser's own; zero ids are left
    /// out, and `None` when nothing is left to copy
    pub fn at(e: &MouseEvent, items: impl IntoIterator<Item = (&'static str, i64)>) -> Option<Self> {
        e.prevent_default();
        let items: Vec<_> = items.into_iter().filter(|(_, id)| *id != 0).collect();
        let p = e.client_coordinates();
        (!items.is_empty()).then_some(Self { x: p.x, y: p.y, items })
    }
}

/// Copy an id and confirm it with a toast
pub fn copy_id(mut toast: ToastManager, id: i64) {
    copy_to_clipboard(&id.to_string());
    toast.show(format!("Copied {id}"), ToastSeverity::Success);
}

/// Menu item that copies `id`, showing the id beside its label
#[component]
pub fn CopyIdItem(label: String, id: i64, on_done: EventHandler<()>) -> Element {
    let toast = use_toast();
    rsx! {
        div {
            class: "context-menu-item",
            onclick: move |_| {
                copy_id(toast, id);
                on_done.call(());
            },
            "{label}"
            span { class: "context-menu-id", "{id}" }
        }
    }
}

#[component]
pub fn IdContextMenu(menu: Signal<Option<IdMenu>>) -> Element {
    let Some(m) = menu() else { return rsx! {} };
    rsx! {
        div {
            class: "context-menu-backdrop",
            onclick: move |_| menu.set(None),
            oncontextmenu: move |e| {
                e.prevent_default();
                menu.set(None);
            },
        }
        div {
            class: "log-context-menu",
            // Kept inside the window when opened near its right or bottom edge
            style: "position: fixed; left: min({m.x}px, calc(100vw - 17em)); top: min({m.y}px, calc(100vh - 6em)); z-index: 1000;",
            for (label, id) in m.items {
                CopyIdItem { label, id, on_done: move |_| menu.set(None) }
            }
        }
    }
}
