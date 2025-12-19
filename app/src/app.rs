#![allow(non_snake_case)]

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

static CSS: Asset = asset!("/assets/styles.css");

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], js_name = "listen")]
    async fn tauri_listen(event: &str, handler: &Closure<dyn FnMut(JsValue)>) -> JsValue;
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub log_directory: String,
    #[serde(default)]
    pub auto_delete_empty_files: bool,
    #[serde(default)]
    pub log_retention_days: u32,
}

pub fn App() -> Element {
    let mut overlay_visible = use_signal(|| true);
    let mut move_mode = use_signal(|| false);
    let mut status_msg = use_signal(String::new);
    let mut log_directory = use_signal(String::new);
    let mut active_file = use_signal(String::new);
    let mut is_watching = use_signal(|| false);

    // Fetch config from backend on mount
    use_future(move || async move {
        let result = invoke("get_config", JsValue::NULL).await;
        if let Ok(config) = serde_wasm_bindgen::from_value::<AppConfig>(result) {
            log_directory.set(config.log_directory.clone());
            if !config.log_directory.is_empty() {
                is_watching.set(true);
            }
        }

        // Also fetch initial active file
        let file_result = invoke("get_active_file", JsValue::NULL).await;
        if let Some(file) = file_result.as_string() {
            active_file.set(file);
        }
    });

    // Listen for active file changes from backend
    use_future(move || async move {
        let closure = Closure::new(move |event: JsValue| {
            if let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload")) {
                if let Some(file_path) = payload.as_string() {
                    active_file.set(file_path);
                }
            }
        });

        tauri_listen("active-file-changed", &closure).await;

        // Keep the closure alive for the lifetime of the app
        closure.forget();
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
    let current_directory = log_directory();
    let watching = is_watching();

    let toggle_overlay = move |_| {
        let current = overlay_visible();
        let cmd = if current {
            "hide_overlay"
        } else {
            "show_overlay"
        };

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

    let set_directory = move |_| {
        let dir = log_directory();

        async move {
            if dir.is_empty() {
                status_msg.set("Please enter a log directory path".to_string());
                return;
            }

            let config = AppConfig {
                log_directory: dir.clone(),
                auto_delete_empty_files: false,
                log_retention_days: 0,
            };

            let args = serde_wasm_bindgen::to_value(&config).unwrap_or(JsValue::NULL);
            let obj = js_sys::Object::new();
            js_sys::Reflect::set(&obj, &JsValue::from_str("config"), &args).unwrap();

            let result = invoke("update_config", obj.into()).await;
            if result.is_undefined() || result.is_null() {
                // Active file will be updated via event listener
                is_watching.set(true);
                status_msg.set(format!("Watching: {}", dir));
            } else if let Some(err) = result.as_string() {
                status_msg.set(format!("Error: {}", err));
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
                    if is_visible { "Hide Overlays" } else { "Show Overlays" }
                }

                button {
                    class: if is_move_mode { "btn btn-warning" } else { "btn" },
                    disabled: !is_visible,
                    onclick: toggle_move,
                    if is_move_mode { "Lock Overlays" } else { "Unlock Overlays" }
                }
            }

            // Log directory selection
            div { class: "log-section",
                h3 { "Log Directory" }
                div { class: "log-controls",
                    input {
                        r#type: "text",
                        class: "log-input",
                        placeholder: "Path to SWTOR combat log directory...",
                        value: "{current_directory}",
                        oninput: move |e| log_directory.set(e.value())
                    }
                    button {
                        class: "btn",
                        onclick: set_directory,
                        "Set Directory"
                    }
                }
                div {class: "active-file",
                    p { "{active_file}" }
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
                    "Watching: "
                    span { class: if watching { "status-on" } else { "status-off" },
                        if watching { "Active" } else { "Not set" }
                    }
                }
            }
        }
    }
}
