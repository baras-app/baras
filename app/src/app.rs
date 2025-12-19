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

// ─────────────────────────────────────────────────────────────────────────────
// Data Types
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub log_directory: String,
    #[serde(default)]
    pub auto_delete_empty_files: bool,
    #[serde(default)]
    pub log_retention_days: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub player_name: Option<String>,
    pub player_class: Option<String>,
    pub player_discipline: Option<String>,
    pub area_name: Option<String>,
    pub in_combat: bool,
    pub encounter_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayStatus {
    pub running: Vec<String>,
    pub enabled: Vec<String>,
    pub overlays_visible: bool,
    pub move_mode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverlayType {
    Dps,
    Hps,
    Tps,
}


// ─────────────────────────────────────────────────────────────────────────────
// App Component
// ─────────────────────────────────────────────────────────────────────────────

pub fn App() -> Element {
    // Overlay enabled states (user preference, persisted)
    let mut dps_enabled = use_signal(|| false);
    let mut hps_enabled = use_signal(|| false);
    let mut tps_enabled = use_signal(|| false);

    // Global visibility toggle (persisted)
    let mut overlays_visible = use_signal(|| true);

    let mut move_mode = use_signal(|| false);

    // Status
    let mut status_msg = use_signal(String::new);

    // Directory/file state
    let mut log_directory = use_signal(String::new);
    let mut active_file = use_signal(String::new);
    let mut is_watching = use_signal(|| false);

    // Session info
    let mut session_info = use_signal(|| None::<SessionInfo>);

    // Fetch initial state from backend
    use_future(move || async move {
        // Get config
        let result = invoke("get_config", JsValue::NULL).await;
        if let Ok(config) = serde_wasm_bindgen::from_value::<AppConfig>(result) {
            log_directory.set(config.log_directory.clone());
            if !config.log_directory.is_empty() {
                is_watching.set(true);
            }
        }

        // Get active file
        let file_result = invoke("get_active_file", JsValue::NULL).await;
        if let Some(file) = file_result.as_string() {
            active_file.set(file);
        }

        // Get overlay status
        let status_result = invoke("get_overlay_status", JsValue::NULL).await;
        if let Ok(status) = serde_wasm_bindgen::from_value::<OverlayStatus>(status_result) {
            // Set enabled states from config
            dps_enabled.set(status.enabled.contains(&"dps".to_string()));
            hps_enabled.set(status.enabled.contains(&"hps".to_string()));
            tps_enabled.set(status.enabled.contains(&"tps".to_string()));
            // Set global visibility
            overlays_visible.set(status.overlays_visible);
            move_mode.set(status.move_mode);
        }

        // Get session info
        let session_result = invoke("get_session_info", JsValue::NULL).await;
        if let Ok(info) = serde_wasm_bindgen::from_value::<Option<SessionInfo>>(session_result) {
            session_info.set(info);
        }
    });

    // Listen for active file changes
    use_future(move || async move {
        let closure = Closure::new(move |event: JsValue| {
            if let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload")) {
                if let Some(file_path) = payload.as_string() {
                    active_file.set(file_path);
                }
            }
        });
        tauri_listen("active-file-changed", &closure).await;
        closure.forget();
    });

    // Poll session info periodically
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(2000).await;
            let session_result = invoke("get_session_info", JsValue::NULL).await;
            if let Ok(info) = serde_wasm_bindgen::from_value::<Option<SessionInfo>>(session_result) {
                session_info.set(info);
            }
        }
    });

    // Read signals
    let dps_on = dps_enabled();
    let hps_on = hps_enabled();
    let tps_on = tps_enabled();
    let any_enabled = dps_on || hps_on || tps_on;
    let is_visible = overlays_visible();
    let is_move_mode = move_mode();
    let status = status_msg();
    let current_directory = log_directory();
    let watching = is_watching();
    let current_file = active_file();
    let session = session_info();

    // Toggle overlay enabled state
    let make_toggle_overlay = |overlay_type: OverlayType, current: bool, enabled_signal: Signal<bool>| {
        move |_| {
            let cmd = if current { "hide_overlay" } else { "show_overlay" };
            let mut enabled_signal = enabled_signal;

            async move {
                let args = serde_wasm_bindgen::to_value(&overlay_type).unwrap_or(JsValue::NULL);
                let obj = js_sys::Object::new();
                js_sys::Reflect::set(&obj, &JsValue::from_str("overlayType"), &args).unwrap();

                let result = invoke(cmd, obj.into()).await;
                if let Some(success) = result.as_bool() {
                    if success {
                        enabled_signal.set(!current);
                    }
                }
            }
        }
    };

    let toggle_move = move |_| {
        async move {
            if !is_visible || !any_enabled {
                status_msg.set("Overlays must be visible".to_string());
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

    // Single toggle for show/hide all overlays
    let toggle_visibility = move |_| {
        let currently_visible = is_visible;
        async move {
            let cmd = if currently_visible { "hide_all_overlays" } else { "show_all_overlays" };
            let result = invoke(cmd, JsValue::NULL).await;
            // Check for success (bool for hide, array for show)
            let success = result.as_bool().unwrap_or(false) || result.is_array();
            if success {
                overlays_visible.set(!currently_visible);
                if currently_visible {
                    move_mode.set(false);
                }
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
            h1 { "BARAS" }
            p { class: "subtitle", "Battle Analysis and Raid Assessment System" }


            // Session info panel
            if let Some(ref info) = session {
                section { class: "session-panel",
                    h3 { "Session" }
                    div { class: "session-grid",
                        if let Some(ref name) = info.player_name {
                            div { class: "session-item",
                                span { class: "label", "Player" }
                                span { class: "value", "{name}" }
                            }
                        }
                        if let Some(ref class_name) = info.player_class {
                            div { class: "session-item",
                                span { class: "label", "Class" }
                                span { class: "value", "{class_name}" }
                            }
                        }
                        if let Some(ref disc) = info.player_discipline {
                            div { class: "session-item",
                                span { class: "label", "Discipline" }
                                span { class: "value", "{disc}" }
                            }
                        }
                        if let Some(ref area) = info.area_name {
                            div { class: "session-item",
                                span { class: "label", "Area" }
                                span { class: "value", "{area}" }
                            }
                        }
                        div { class: "session-item",
                            span { class: "label", "Combat" }
                            span {
                                class: if info.in_combat { "value status-warning" } else { "value" },
                                if info.in_combat { "In Combat" } else { "Out of Combat" }
                            }
                        }
                        div { class: "session-item",
                            span { class: "label", "Encounters" }
                            span { class: "value", "{info.encounter_count}" }
                        }
                    }
                }
            }


            // Overlay toggles
            section { class: "overlay-controls",
                h3 { "Meters" }
                div { class: "overlay-meters",
                    button {
                        class: if dps_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                        onclick: make_toggle_overlay(OverlayType::Dps, dps_on, dps_enabled),
                        "DPS"
                    }
                    button {
                        class: if hps_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                        onclick: make_toggle_overlay(OverlayType::Hps, hps_on, hps_enabled),
                        "HPS"
                    }
                    button {
                        class: if tps_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                        onclick: make_toggle_overlay(OverlayType::Tps, tps_on, tps_enabled),
                        "TPS"
                    }
                }
            }
            section { class: "overlay-controls",
                div { class: "overlay-toggles",
                    // Single show/hide toggle
                    if any_enabled {
                        button {
                            class: if is_visible { "btn btn-hide-all" } else { "btn btn-show-all" },
                            onclick: toggle_visibility,
                            if is_visible { "Hide Overlays" } else { "Show Overlays" }
                        }
                    }
                    button {
                        class: if is_move_mode { "btn btn-lock btn-warning" } else { "btn btn-lock" },
                        disabled: !is_visible || !any_enabled,
                        onclick: toggle_move,
                        if is_move_mode { "Unlocked" } else { "Locked" }
                    }
                }
            }


            // Log directory section
            section { class: "log-section",
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
                        "Set"
                    }
                }
                if watching && !current_file.is_empty() {
                    div { class: "active-file",
                        span { class: "label", "Active: " }
                        span { class: "filename", "{current_file}" }
                    }
                }
            }

            // Status messages
            if !status.is_empty() {
                p {
                    class: if status.starts_with("Error") { "error" } else { "info" },
                    "{status}"
                }
            }

            // Footer status
            footer { class: "status-footer",
                span {
                    class: if watching { "status-dot status-on" } else { "status-dot status-off" }
                }
                span {
                    if watching { "Watching directory" } else { "Not watching" }
                }
            }
        }
    }
}
