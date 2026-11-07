//! Overlay toggle button with an attached settings gear.

use dioxus::prelude::*;

/// A split button: the wide segment toggles the overlay, the narrow gear
/// segment opens that overlay's tab in the settings panel. Overlays without a
/// settings tab (`tab_key` = None) render only the toggle.
#[derive(Props, Clone, PartialEq)]
pub struct OverlayToggleProps {
    pub label: &'static str,
    /// Font Awesome class list for the toggle icon
    pub icon: &'static str,
    #[props(default)]
    pub title: &'static str,
    pub active: bool,
    pub tab_key: Option<&'static str>,
    pub on_toggle: EventHandler<()>,
    /// Tab key of the overlay whose settings were requested
    pub on_settings: EventHandler<&'static str>,
    /// Tab key of the overlay the settings drawer is showing; lights up that gear
    #[props(default)]
    pub drawer_overlay: String,
}

#[component]
pub fn OverlayToggle(props: OverlayToggleProps) -> Element {
    let OverlayToggleProps { label, icon, title, active, tab_key, on_toggle, on_settings, drawer_overlay } = props;
    let gear_open = tab_key.is_some_and(|key| key == drawer_overlay);
    let gear_class = match (active, gear_open) {
        (true, true) => "btn btn-overlay-gear btn-active open",
        (true, false) => "btn btn-overlay-gear btn-active",
        (false, true) => "btn btn-overlay-gear open",
        (false, false) => "btn btn-overlay-gear",
    };
    rsx! {
        div { class: "overlay-toggle",
            button {
                class: if active { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                title: "{title}",
                onclick: move |_| on_toggle.call(()),
                i { class: "{icon} overlay-btn-icon" }
                "{label}"
            }
            if tab_key.is_none() {
                span { class: "btn-overlay-gear-spacer" }
            }
            if let Some(key) = tab_key {
                button {
                    class: "{gear_class}",
                    title: if gear_open { "Close settings" } else { "{label} settings" },
                    onclick: move |_| on_settings.call(key),
                    i { class: "fa-solid fa-gear" }
                }
            }
        }
    }
}
