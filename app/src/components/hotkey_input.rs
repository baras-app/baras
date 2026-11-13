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

/// Convert a physical key code to its hotkey string representation.
///
/// Uses the physical code rather than the produced character so modifier
/// combos capture the base key (Shift+8 → "8", not the shifted "*" which
/// the global-shortcut parser can't register).
fn code_to_string(code: Code) -> Option<String> {
    use Code::*;
    let name = code.to_string();
    // Letters and digits: code names are "KeyA".."KeyZ" / "Digit0".."Digit9"
    if let Some(c) = name.strip_prefix("Key").filter(|c| c.len() == 1) {
        return Some(c.to_string());
    }
    if let Some(d) = name.strip_prefix("Digit").filter(|d| d.len() == 1) {
        return Some(d.to_string());
    }
    // Numpad digits: keep the full "Numpad8" token (parsed by the shortcut backend)
    if name.strip_prefix("Numpad").is_some_and(|d| d.len() == 1 && d.chars().all(|c| c.is_ascii_digit())) {
        return Some(name);
    }
    let s = match code {
        F1 | F2 | F3 | F4 | F5 | F6 | F7 | F8 | F9 | F10 | F11 | F12 => return Some(name),
        ArrowUp => "Up",
        ArrowDown => "Down",
        ArrowLeft => "Left",
        ArrowRight => "Right",
        Home => "Home",
        End => "End",
        PageUp => "PageUp",
        PageDown => "PageDown",
        Insert => "Insert",
        Tab => "Tab",
        Enter => "Enter",
        Minus => "-",
        Equal => "=",
        Comma => ",",
        Period => ".",
        Slash => "/",
        Backslash => "\\",
        Semicolon => ";",
        Quote => "'",
        BracketLeft => "[",
        BracketRight => "]",
        Backquote => "`",
        _ => return None,
    };
    Some(s.to_string())
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
            // Captured keys must not trigger in-app shortcuts
            "data-no-shortcuts": true,
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

                // Convert the physical key code to a string
                if let Some(key_str) = code_to_string(e.code()) {
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
