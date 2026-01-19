//! Hotkey capture input component
//!
//! A specialized input that captures keyboard shortcuts by listening
//! for key presses rather than requiring manual text entry.

use dioxus::prelude::*;

/// Props for the HotkeyInput component
#[derive(Props, Clone, PartialEq)]
pub struct HotkeyInputProps {
    /// Current hotkey value (e.g., "Ctrl+Shift+O")
    pub value: String,
    /// Callback when hotkey changes
    pub on_change: EventHandler<String>,
    /// Optional placeholder text
    #[props(default = "Click to set hotkey".to_string())]
    pub placeholder: String,
}

/// Convert key to string representation
fn key_to_string(key: &Key) -> Option<String> {
    match key {
        Key::Character(c) => Some(c.to_uppercase()),
        Key::F1 => Some("F1".to_string()),
        Key::F2 => Some("F2".to_string()),
        Key::F3 => Some("F3".to_string()),
        Key::F4 => Some("F4".to_string()),
        Key::F5 => Some("F5".to_string()),
        Key::F6 => Some("F6".to_string()),
        Key::F7 => Some("F7".to_string()),
        Key::F8 => Some("F8".to_string()),
        Key::F9 => Some("F9".to_string()),
        Key::F10 => Some("F10".to_string()),
        Key::F11 => Some("F11".to_string()),
        Key::F12 => Some("F12".to_string()),
        Key::ArrowUp => Some("Up".to_string()),
        Key::ArrowDown => Some("Down".to_string()),
        Key::ArrowLeft => Some("Left".to_string()),
        Key::ArrowRight => Some("Right".to_string()),
        Key::Home => Some("Home".to_string()),
        Key::End => Some("End".to_string()),
        Key::PageUp => Some("PageUp".to_string()),
        Key::PageDown => Some("PageDown".to_string()),
        Key::Insert => Some("Insert".to_string()),
        Key::Tab => Some("Tab".to_string()),
        Key::Enter => Some("Enter".to_string()),
        _ => None,
    }
}

/// Build modifier prefix string
fn build_modifier_prefix(modifiers: &Modifiers) -> Vec<String> {
    let mut parts = Vec::new();
    if modifiers.ctrl() {
        parts.push("Ctrl".to_string());
    }
    if modifiers.shift() {
        parts.push("Shift".to_string());
    }
    if modifiers.alt() {
        parts.push("Alt".to_string());
    }
    parts
}

/// A keyboard shortcut capture input
///
/// Click to enter capture mode, then press the desired key combination.
/// Press Escape to cancel, Backspace/Delete to clear.
#[component]
pub fn HotkeyInput(props: HotkeyInputProps) -> Element {
    let mut is_capturing = use_signal(|| false);
    let mut pending_display = use_signal(String::new);

    let display_value = if is_capturing() {
        let pending = pending_display();
        if pending.is_empty() {
            "Press a key...".to_string()
        } else {
            pending
        }
    } else if props.value.is_empty() {
        props.placeholder.clone()
    } else {
        props.value.clone()
    };

    let input_class = if is_capturing() {
        "hotkey-input hotkey-input--capturing"
    } else if props.value.is_empty() {
        "hotkey-input hotkey-input--empty"
    } else {
        "hotkey-input"
    };

    rsx! {
        div {
            class: "{input_class}",
            tabindex: 0,
            onclick: move |_| {
                is_capturing.set(true);
                pending_display.set(String::new());
            },
            onkeydown: move |e| {
                // Only process keys when in capture mode (entered via click)
                if !is_capturing() {
                    // Allow Enter/Space to start capture mode
                    if e.key() == Key::Enter {
                        is_capturing.set(true);
                        pending_display.set(String::new());
                        e.prevent_default();
                    }
                    // Otherwise let the event bubble for scrolling etc.
                    return;
                }

                let key = e.key();

                // Cancel on Escape
                if key == Key::Escape {
                    is_capturing.set(false);
                    pending_display.set(String::new());
                    return;
                }

                // Clear on Backspace/Delete (without modifiers)
                if (key == Key::Backspace || key == Key::Delete)
                    && !e.modifiers().ctrl()
                    && !e.modifiers().shift()
                    && !e.modifiers().alt()
                {
                    props.on_change.call(String::new());
                    is_capturing.set(false);
                    pending_display.set(String::new());
                    return;
                }

                // Skip if only modifier keys pressed - show pending state
                if matches!(key, Key::Control | Key::Shift | Key::Alt | Key::Meta) {
                    let parts = build_modifier_prefix(&e.modifiers());
                    if !parts.is_empty() {
                        pending_display.set(format!("{}+...", parts.join("+")));
                    }
                    e.prevent_default();
                    return;
                }

                // Try to convert the key to a string
                if let Some(key_str) = key_to_string(&key) {
                    let mut parts = build_modifier_prefix(&e.modifiers());
                    parts.push(key_str);
                    let hotkey = parts.join("+");
                    props.on_change.call(hotkey);
                    is_capturing.set(false);
                    pending_display.set(String::new());
                }

                e.prevent_default();
            },
            onblur: move |_| {
                is_capturing.set(false);
                pending_display.set(String::new());
            },
            span { class: "hotkey-display", "{display_value}" }
            if !props.value.is_empty() && !is_capturing() {
                button {
                    class: "hotkey-clear",
                    r#type: "button",
                    onclick: move |e| {
                        e.stop_propagation();
                        props.on_change.call(String::new());
                    },
                    "×"
                }
            }
        }
    }
}
