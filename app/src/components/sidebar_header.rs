//! Top of an `explorer-sidebar`: title plus the fold toggle. Folded, only the
//! toggle (and `badge`, if any) shows; the caller adds `collapsed` to the aside.

use dioxus::prelude::*;

#[component]
pub fn SidebarHeader(
    icon: &'static str,
    title: &'static str,
    collapsed: bool,
    on_toggle: EventHandler<()>,
    /// Shown under the toggle while folded (e.g. active filter count)
    #[props(default)] badge: Option<String>,
    /// Beside the toggle while open (e.g. a "New" button)
    #[props(default = VNode::empty())] action: Element,
) -> Element {
    rsx! {
        div { class: "sidebar-header",
            div { class: "sidebar-header-row",
                if !collapsed {
                    h3 { i { class: "{icon}" } " {title}" }
                    {action}
                }
                button {
                    class: "sidebar-collapse-btn",
                    title: if collapsed { "Expand {title} (S)" } else { "Collapse {title} (S)" },
                    "data-shortcut": "s",
                    onclick: move |_| on_toggle.call(()),
                    i { class: if collapsed { "fa-solid fa-angles-right" } else { "fa-solid fa-angles-left" } }
                }
            }
            if collapsed && let Some(b) = badge {
                span { class: "sidebar-collapsed-badge", "{b}" }
            }
        }
    }
}
