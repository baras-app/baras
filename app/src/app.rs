#![allow(non_snake_case)]

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

static CSS: Asset = asset!("/assets/styles.css");

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;
}

/// Helper to create invoke args object
fn make_args(pairs: &[(&str, &str)]) -> JsValue {
    let obj = js_sys::Object::new();
    for (key, value) in pairs {
        js_sys::Reflect::set(&obj, &JsValue::from_str(key), &JsValue::from_str(value)).unwrap();
    }
    obj.into()
}

pub fn App() -> Element {
    let mut overlay_visible = use_signal(|| true);
    let mut move_mode = use_signal(|| false);
    let mut status_msg = use_signal(String::new);
    let mut log_path = use_signal(String::new);
    let mut is_tailing = use_signal(|| false);

    // Fetch default log path from backend on mount
    use_future(move || async move {
        let result = invoke("default_log_path", JsValue::NULL).await;
        if let Some(path) = result.as_string() {
            log_path.set(path);
        }
    });

    if overlay_visible() {
    use_future(move || async move {
        invoke("show_overlay", JsValue::NULL).await;
    });
    }



    // Read signals once at the top to avoid multiple borrow conflicts
    let is_visible = overlay_visible();
    let is_move_mode = move_mode();
    let status = status_msg();
    let current_path = log_path();
    let tailing = is_tailing();

    let toggle_overlay = move |_| {
        let current = overlay_visible();
        let cmd = if current { "hide_overlay" } else { "show_overlay" };

        async move {
            let result = invoke(cmd, JsValue::NULL).await;
            if let Some(success) = result.as_bool() {
                if success {
                    let new_state = !current;
                    overlay_visible.set(new_state);
                    if !new_state {
                        move_mode.set(false);
                    }
                    status_msg.set(String::new());
                }
            } else if let Some(err) = result.as_string() {
                status_msg.set(format!("Error: {}", err));
            }
        }
    };

    let toggle_move = move |_| {
        let current = overlay_visible();

        async move {
            if !current {
                status_msg.set("Overlay must be visible first".to_string());
                return;
            }

            let result = invoke("toggle_move_mode", JsValue::NULL).await;
            if let Some(new_mode) = result.as_bool() {
                move_mode.set(new_mode);
                status_msg.set(String::new());
            } else if let Some(err) = result.as_string() {
                status_msg.set(format!("Error: {}", err));
            }
        }
    };

    let toggle_tailing = move |_| {
        let currently_tailing = is_tailing();
        let path = log_path();

        async move {
            if currently_tailing {
                // Stop tailing
                let result = invoke("stop_tailing", JsValue::NULL).await;
                if result.is_undefined() || result.is_null() {
                    is_tailing.set(false);
                    status_msg.set("Stopped tailing".to_string());
                } else if let Some(err) = result.as_string() {
                    status_msg.set(format!("Error: {}", err));
                }
            } else {
                // Start tailing
                if path.is_empty() {
                    status_msg.set("Please enter a log file path".to_string());
                    return;
                }

                let args = make_args(&[("path", &path)]);
                let result = invoke("start_tailing", args).await;
                if result.is_undefined() || result.is_null() {
                    is_tailing.set(true);
                    status_msg.set(format!("Tailing: {}", path));
                } else if let Some(err) = result.as_string() {
                    status_msg.set(format!("Error: {}", err));
                }
            }
        }
    };

    rsx! {
        link { rel: "stylesheet", href: CSS }
        main { class: "container",
            h1 { "Baras" }
            p { class: "subtitle", "SWTOR Combat Log Parser" }

            // Overlay controls
            div { class: "controls",
                button {
                    class: if is_visible { "btn btn-active" } else { "btn" },
                    onclick: toggle_overlay,
                    if is_visible { "Hide Overlay" } else { "Show Overlay" }
                }

                button {
                    class: if is_move_mode { "btn btn-warning" } else { "btn" },
                    disabled: !is_visible,
                    onclick: toggle_move,
                    if is_move_mode { "Lock Position" } else { "Move Overlay" }
                }
            }

            // Log file tailing
            div { class: "log-section",
                h3 { "Combat Log" }
                div { class: "log-controls",
                    input {
                        r#type: "text",
                        class: "log-input",
                        placeholder: "Path to combat log file...",
                        value: "{current_path}",
                        disabled: tailing,
                        oninput: move |e| log_path.set(e.value())
                    }
                    button {
                        class: if tailing { "btn btn-warning" } else { "btn" },
                        onclick: toggle_tailing,
                        if tailing { "Stop" } else { "Start Tailing" }
                    }
                }
            }

            if !status.is_empty() {
                p { class: if status.starts_with("Error") { "error" } else { "info" }, "{status}" }
            }

            div { class: "status",
                p {
                    "Overlay: "
                    span { class: if is_visible { "status-on" } else { "status-off" },
                        if is_visible { "Visible" } else { "Hidden" }
                    }
                }
                p {
                    "Mode: "
                    span { class: if is_move_mode { "status-warning" } else { "" },
                        if is_move_mode { "Move Mode (drag to reposition)" } else { "Locked" }
                    }
                }
                p {
                    "Tailing: "
                    span { class: if tailing { "status-on" } else { "status-off" },
                        if tailing { "Active" } else { "Stopped" }
                    }
                }
            }
        }
    }
}
