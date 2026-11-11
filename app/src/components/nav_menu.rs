//! Header navigation menu: one button standing for a group of main tabs.
//! It shows the group name, or the active view's name when one of its
//! tabs is open, and drops a list of the group's views on click.

use dioxus::prelude::*;

use crate::types::{MainTab, UiSessionState};

/// One destination: tab, icon class, label, tooltip
pub type NavItem = (MainTab, &'static str, &'static str, &'static str);
/// One extra entry below the destinations that runs a callback instead of
/// switching tabs: icon class, label (may carry runtime text), tooltip, handler
pub type NavAction = (&'static str, String, &'static str, EventHandler<()>);

#[component]
pub fn NavMenu(
    label: &'static str,
    icon: &'static str,
    items: Vec<NavItem>,
    #[props(default)] actions: Vec<NavAction>,
    /// A hairline above this item, separating the group it starts
    #[props(default)] divider_before: Option<MainTab>,
    /// Drop the group name when none of its views is open (the icon says it)
    #[props(default)] icon_only: bool,
    state: Signal<UiSessionState>,
) -> Element {
    let mut open = use_signal(|| false);
    // Last destination picked from this menu: one click on the button goes
    // straight back to it; the list is for switching
    // An actions-only menu (no destinations) just opens its list
    let mut last = use_signal(|| items.first().map(|(t, ..)| *t));
    let tabs: Vec<MainTab> = items.iter().map(|(t, ..)| *t).collect();
    use_effect(move || {
        let t = state.read().active_tab;
        if tabs.contains(&t) {
            last.set(Some(t));
        }
    });
    let active_tab = state.read().active_tab;
    let current = items.iter().find(|(t, ..)| *t == active_tab);
    let (btn_icon, btn_label) = current.map_or((icon, label), |(_, i, l, _)| (*i, *l));
    let in_menu = current.is_some();
    let last_tab = *last.read();
    let last_label = items.iter().find(|(t, ..)| Some(*t) == last_tab).map_or(label, |(_, _, l, _)| *l);
    let has_items = !items.is_empty();

    rsx! {
        div { class: "nav-menu",
            button {
                class: if in_menu { "tab-btn active" } else { "tab-btn" },
                title: if in_menu || !has_items { "{label}" } else { "{last_label} · use the arrow for the other views" },
                onclick: move |_| match *last.read() {
                    Some(t) if !in_menu => state.write().active_tab = t,
                    _ => open.toggle(),
                },
                i { class: "{btn_icon}" }
                if in_menu || !icon_only {
                    span { class: "tab-label", " {btn_label}" }
                }
                if has_items {
                    span {
                        class: "nav-menu-caret",
                        title: "{label}",
                        onclick: move |e| {
                            e.stop_propagation();
                            open.toggle();
                        },
                        i { class: "fa-solid fa-chevron-down" }
                    }
                }
            }
            if *open.read() {
                // Click anywhere else closes the list
                div { class: "nav-menu-backdrop", onclick: move |_| open.set(false) }
                div { class: "nav-menu-list",
                    for (tab, i, l, t) in items.iter().copied() {
                        if divider_before == Some(tab) {
                            div { class: "nav-menu-divider" }
                        }
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
                    if !actions.is_empty() {
                        if has_items {
                            div { class: "nav-menu-divider" }
                        }
                        for (i, l, t, on_pick) in actions.iter().cloned() {
                            button {
                                class: "nav-menu-item",
                                title: "{t}",
                                onclick: move |_| {
                                    on_pick.call(());
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
}
