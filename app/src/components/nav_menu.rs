//! Header navigation menu: one button standing for a group of main tabs.
//! It shows the group name, or the active view's name when one of its
//! tabs is open, and drops a list of the group's views on click.

use dioxus::prelude::*;

use crate::types::{MainTab, UiSessionState};

/// One destination: tab, icon class, label, tooltip
pub type NavItem = (MainTab, &'static str, &'static str, &'static str);

#[component]
pub fn NavMenu(label: &'static str, icon: &'static str, items: Vec<NavItem>, state: Signal<UiSessionState>) -> Element {
    let mut open = use_signal(|| false);
    let active_tab = state.read().active_tab;
    let current = items.iter().find(|(t, ..)| *t == active_tab);
    let (btn_icon, btn_label) = current.map_or((icon, label), |(_, i, l, _)| (*i, *l));

    rsx! {
        div { class: "nav-menu",
            button {
                class: if current.is_some() { "tab-btn active" } else { "tab-btn" },
                title: "{label}",
                onclick: move |_| open.toggle(),
                i { class: "{btn_icon}" }
                span { class: "tab-label", " {btn_label}" }
                i { class: "fa-solid fa-chevron-down nav-menu-caret" }
            }
            if *open.read() {
                // Click anywhere else closes the list
                div { class: "nav-menu-backdrop", onclick: move |_| open.set(false) }
                div { class: "nav-menu-list",
                    for (tab, i, l, t) in items.iter().copied() {
                        button {
                            class: if tab == active_tab { "nav-menu-item active" } else { "nav-menu-item" },
                            title: "{t}",
                            onclick: move |_| {
                                state.write().active_tab = tab;
                                open.set(false);
                            },
                            i { class: "{i}" }
                            " {l}"
                        }
                    }
                }
            }
        }
    }
}
