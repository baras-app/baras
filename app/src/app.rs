#![allow(non_snake_case)]
#![allow(clippy::too_many_arguments)]

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

// Import components
use crate::components::{HistoryPanel, SettingsPanel};

static CSS: Asset = asset!("/assets/styles.css");

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    async fn invoke(cmd: &str, args: JsValue) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "event"], js_name = "listen")]
    async fn tauri_listen(event: &str, handler: &Closure<dyn FnMut(JsValue)>) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "dialog"], js_name = "open")]
    async fn open_dialog(options: JsValue) -> JsValue;
}

// ─────────────────────────────────────────────────────────────────────────────
// Data Types (pub for use by components)
// ─────────────────────────────────────────────────────────────────────────────

// Re-export shared types from baras-types
pub use baras_types::{
    AppConfig, BossHealthConfig, Color, OverlayAppearanceConfig, OverlaySettings,
    PersonalOverlayConfig, PersonalStat, RaidOverlaySettings, MAX_PROFILES,
};

/// Parse a hex color string (e.g., "#ff0000") to RGBA bytes
pub fn parse_hex_color(hex: &str) -> Option<Color> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some([r, g, b, 255])
}

// ─────────────────────────────────────────────────────────────────────────────
// Frontend-Only Types
// ─────────────────────────────────────────────────────────────────────────────

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
    pub personal_running: bool,
    pub personal_enabled: bool,
    pub raid_running: bool,
    pub raid_enabled: bool,
    pub boss_health_running: bool,
    pub boss_health_enabled: bool,
    pub overlays_visible: bool,
    pub move_mode: bool,
    pub rearrange_mode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MetricType {
    Dps,
    EDps,
    BossDps,
    Hps,
    EHps,
    Abs,
    Dtps,
    Tps,
}

impl MetricType {
    pub fn label(&self) -> &'static str {
        match self {
            MetricType::Dps => "Damage",
            MetricType::EDps => "Effective Damage",
            MetricType::BossDps => "Boss Damage",
            MetricType::Hps => "Healing",
            MetricType::EHps => "Effective Healing",
            MetricType::Tps => "Threat",
            MetricType::Dtps => "Damage Taken",
            MetricType::Abs => "Shielding Given",
        }
    }

    pub fn config_key(&self) -> &'static str {
        match self {
            MetricType::Dps => "dps",
            MetricType::EDps => "edps",
            MetricType::BossDps => "bossdps",
            MetricType::Hps => "hps",
            MetricType::EHps => "ehps",
            MetricType::Tps => "tps",
            MetricType::Dtps => "dtps",
            MetricType::Abs => "abs",
        }
    }

    /// All metric overlay types (for iteration)
    pub fn all_metrics() -> &'static [MetricType] {
        &[
            MetricType::Dps,
            MetricType::EDps,
            MetricType::BossDps,
            MetricType::Hps,
            MetricType::EHps,
            MetricType::Abs,
            MetricType::Dtps,
            MetricType::Tps,
        ]
    }

}

/// Unified overlay kind - matches backend OverlayType
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "type", content = "value")]
pub enum OverlayType {
    Metric(MetricType),
    Personal,
    Raid,
    BossHealth,
}


// ─────────────────────────────────────────────────────────────────────────────
// App Component
// ─────────────────────────────────────────────────────────────────────────────

pub fn App() -> Element {
    // Overlay enabled states - HashMap for all metric overlays
    let mut metric_overlays_enabled = use_signal(|| {
        let mut map = std::collections::HashMap::new();
        for ot in MetricType::all_metrics() {
            map.insert(*ot, false);
        }
        map
    });
    let mut personal_enabled = use_signal(|| false);
    let mut raid_enabled = use_signal(|| false);
    let mut boss_health_enabled = use_signal(|| false);

    // Global visibility toggle (persisted)
    let mut overlays_visible = use_signal(|| true);

    let mut move_mode = use_signal(|| false);
    let mut rearrange_mode = use_signal(|| false);

    // Status
    let mut status_msg = use_signal(String::new);

    // Directory/file state
    let mut log_directory = use_signal(String::new);
    let mut active_file = use_signal(String::new);
    let mut is_watching = use_signal(|| false);

    // Session info
    let mut session_info = use_signal(|| None::<SessionInfo>);

    // Settings panel state
    let mut settings_open = use_signal(|| false);
    let mut general_settings_open = use_signal(|| false);

    // Main view tab state: "session" or "overlays"
    let mut active_tab = use_signal(|| "session".to_string());
    let mut overlay_settings = use_signal(OverlaySettings::default);
    let selected_overlay_tab = use_signal(|| "dps".to_string());

    // Draggable panel state
    let mut settings_panel_pos = use_signal(|| (100i32, 50i32)); // (x, y)
    let mut settings_dragging = use_signal(|| false);
    let mut settings_drag_offset = use_signal(|| (0i32, 0i32));

    // Hotkey settings state
    let mut hotkey_visibility = use_signal(String::new);
    let mut hotkey_move_mode = use_signal(String::new);
    let mut hotkey_rearrange = use_signal(String::new);
    let mut hotkey_save_status = use_signal(String::new);

    // Profile state (for main page dropdown)
    let mut profile_names = use_signal(Vec::<String>::new);
    let mut active_profile = use_signal(|| None::<String>);

    // Fetch initial state from backend
    use_future(move || async move {
        // Get config
        let result = invoke("get_config", JsValue::NULL).await;
        if let Ok(config) = serde_wasm_bindgen::from_value::<AppConfig>(result) {
            log_directory.set(config.log_directory.clone());
            overlay_settings.set(config.overlay_settings);
            // Load hotkey settings
            if let Some(v) = config.hotkeys.toggle_visibility { hotkey_visibility.set(v); }
            if let Some(v) = config.hotkeys.toggle_move_mode { hotkey_move_mode.set(v); }
            if let Some(v) = config.hotkeys.toggle_rearrange_mode { hotkey_rearrange.set(v); }
            // Load profile data
            let names: Vec<String> = config.profiles.iter().map(|p| p.name.clone()).collect();
            profile_names.set(names);
            active_profile.set(config.active_profile_name);
        }

        // Get watcher status
        let watching_result = invoke("get_watching_status", JsValue::NULL).await;
        if let Ok(watching) = serde_wasm_bindgen::from_value::<bool>(watching_result) {
            is_watching.set(watching);
        }

        // Get active file
        let file_result = invoke("get_active_file", JsValue::NULL).await;
        if let Some(file) = file_result.as_string() {
            active_file.set(file);
        }

        // Get overlay status
        let status_result = invoke("get_overlay_status", JsValue::NULL).await;
        if let Ok(status) = serde_wasm_bindgen::from_value::<OverlayStatus>(status_result) {
            // Set enabled states from config for all metric overlays
            let mut new_map = std::collections::HashMap::new();
            for ot in MetricType::all_metrics() {
                let key = ot.config_key().to_string();
                new_map.insert(*ot, status.enabled.contains(&key));
            }
            metric_overlays_enabled.set(new_map);
            personal_enabled.set(status.personal_enabled);
            raid_enabled.set(status.raid_enabled);
            boss_health_enabled.set(status.boss_health_enabled);
            // Set global visibility
            overlays_visible.set(status.overlays_visible);
            move_mode.set(status.move_mode);
            rearrange_mode.set(status.rearrange_mode);
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
            if let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
                && let Some(file_path) = payload.as_string() {
                    active_file.set(file_path);
                }
        });
        tauri_listen("active-file-changed", &closure).await;
        closure.forget();
    });

    // Poll session info and watcher status periodically
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(2000).await;

            // Session info
            let session_result = invoke("get_session_info", JsValue::NULL).await;
            if let Ok(info) = serde_wasm_bindgen::from_value::<Option<SessionInfo>>(session_result) {
                session_info.set(info);
            }

            // Watcher status
            let watching_result = invoke("get_watching_status", JsValue::NULL).await;
            if let Ok(watching) = serde_wasm_bindgen::from_value::<bool>(watching_result) {
                is_watching.set(watching);
            }
        }
    });

    // Read signals
    let enabled_map = metric_overlays_enabled();
    let personal_on = personal_enabled();
    let raid_on = raid_enabled();
    let boss_health_on = boss_health_enabled();
    let any_metric_enabled = enabled_map.values().any(|&v| v);
    let any_enabled = any_metric_enabled || personal_on || raid_on || boss_health_on;
    let is_visible = overlays_visible();
    let is_move_mode = move_mode();
    let is_rearrange_mode = rearrange_mode();
    let current_directory = log_directory();
    let watching = is_watching();
    let current_file = active_file();
    let current_filename = current_file.rsplit(['/', '\\']).next().unwrap_or(&current_file).to_string();
    let session = session_info();

    // Toggle metric overlay enabled state
    let enabled_map_for_toggle = enabled_map.clone();
    let make_toggle_overlay = move |overlay_type: MetricType| {
        let current = enabled_map_for_toggle.get(&overlay_type).copied().unwrap_or(false);
        move |_| {
            let cmd = if current { "hide_overlay" } else { "show_overlay" };
            let kind = OverlayType::Metric(overlay_type);

            async move {
                let args = serde_wasm_bindgen::to_value(&kind).unwrap_or(JsValue::NULL);
                let obj = js_sys::Object::new();
                js_sys::Reflect::set(&obj, &JsValue::from_str("kind"), &args).unwrap();

                let result = invoke(cmd, obj.into()).await;
                if let Some(success) = result.as_bool() && success {
                    let mut new_map = metric_overlays_enabled();
                    new_map.insert(overlay_type, !current);
                    metric_overlays_enabled.set(new_map);
                }
            }
        }
    };

    // Toggle personal overlay
    let toggle_personal = move |_| {
        let current = personal_on;
        async move {
            let cmd = if current { "hide_overlay" } else { "show_overlay" };
            let kind = OverlayType::Personal;

            let args = serde_wasm_bindgen::to_value(&kind).unwrap_or(JsValue::NULL);
            let obj = js_sys::Object::new();
            js_sys::Reflect::set(&obj, &JsValue::from_str("kind"), &args).unwrap();

            let result = invoke(cmd, obj.into()).await;
            if let Some(success) = result.as_bool()
                && success
            {
                personal_enabled.set(!current);
            }
        }
    };

    // Toggle raid overlay
    let toggle_raid = move |_| {
        let current = raid_on;
        async move {
            let cmd = if current { "hide_overlay" } else { "show_overlay" };
            let kind = OverlayType::Raid;

            let args = serde_wasm_bindgen::to_value(&kind).unwrap_or(JsValue::NULL);
            let obj = js_sys::Object::new();
            js_sys::Reflect::set(&obj, &JsValue::from_str("kind"), &args).unwrap();

            let result = invoke(cmd, obj.into()).await;
            if let Some(success) = result.as_bool()
                && success
            {
                raid_enabled.set(!current);
                // If hiding raid overlay, also clear rearrange mode
                if current {
                    rearrange_mode.set(false);
                }
            }
        }
    };

    // Toggle boss health overlay
    let toggle_boss_health = move |_| {
        let current = boss_health_on;
        async move {
            let cmd = if current { "hide_overlay" } else { "show_overlay" };
            let kind = OverlayType::BossHealth;

            let args = serde_wasm_bindgen::to_value(&kind).unwrap_or(JsValue::NULL);
            let obj = js_sys::Object::new();
            js_sys::Reflect::set(&obj, &JsValue::from_str("kind"), &args).unwrap();

            let result = invoke(cmd, obj.into()).await;
            if let Some(success) = result.as_bool()
                && success
            {
                boss_health_enabled.set(!current);
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
                // Move mode overrides rearrange mode
                if new_mode {
                    rearrange_mode.set(false);
                }
                status_msg.set(String::new());
            } else if let Some(err) = result.as_string() {
                status_msg.set(format!("Error: {}", err));
            }
        }
    };

    let toggle_rearrange = move |_| {
        async move {
            if !raid_on {
                status_msg.set("Raid overlay must be enabled".to_string());
                return;
            }

            let result = invoke("toggle_raid_rearrange", JsValue::NULL).await;
            if let Some(new_mode) = result.as_bool() {
                rearrange_mode.set(new_mode);
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

    // Browse and set directory using native dialog
    let browse_directory = move |_| {
        async move {
            // Open native directory picker
            let options = js_sys::Object::new();
            js_sys::Reflect::set(&options, &JsValue::from_str("directory"), &JsValue::TRUE).unwrap();
            js_sys::Reflect::set(&options, &JsValue::from_str("title"), &JsValue::from_str("Select Log Directory")).unwrap();

            let result = open_dialog(options.into()).await;

            // Result is the selected path string or null
            if let Some(path) = result.as_string() {
                log_directory.set(path.clone());

                // Get current config and update only log_directory
                let config_result = invoke("get_config", JsValue::NULL).await;
                if let Ok(mut config) = serde_wasm_bindgen::from_value::<AppConfig>(config_result) {
                    config.log_directory = path.clone();

                    let args = serde_wasm_bindgen::to_value(&config).unwrap_or(JsValue::NULL);
                    let obj = js_sys::Object::new();
                    js_sys::Reflect::set(&obj, &JsValue::from_str("config"), &args).unwrap();

                    let save_result = invoke("update_config", obj.into()).await;
                    if save_result.is_undefined() || save_result.is_null() {
                        is_watching.set(true);
                        status_msg.set(format!("Watching: {}", path));
                    } else if let Some(err) = save_result.as_string() {
                        status_msg.set(format!("Error: {}", err));
                    }
                }
            }
        }
    };

    rsx! {
        link { rel: "stylesheet", href: CSS }
        link { rel: "stylesheet", href: "https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css" }
        main { class: "container",
            // App header
            header { class: "app-header",
                div { class: "header-content",
                    h1 { "BARAS" }
                    p { class: "subtitle", "Battle Analysis and Raid Assessment System" }
                }
                button {
                    class: "btn btn-header-settings",
                    onclick: move |_| general_settings_open.set(true),
                    i { class: "fa-solid fa-gear" }
                }
            }

            // Main navigation tabs
            nav { class: "main-tabs",
                button {
                    class: if active_tab() == "session" { "tab-btn active" } else { "tab-btn" },
                    onclick: move |_| active_tab.set("session".to_string()),
                    i { class: "fa-solid fa-chart-line" }
                    " Session"
                }
                button {
                    class: if active_tab() == "overlays" { "tab-btn active" } else { "tab-btn" },
                    onclick: move |_| active_tab.set("overlays".to_string()),
                    i { class: "fa-solid fa-layer-group" }
                    " Overlays"
                }
            }

            // Tab content container
            div { class: "tab-content",

            // SESSION TAB: Session info + History
            if active_tab() == "session" {

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
                        if let Some(ref area) = info.area_name {
                            div { class: "session-item",
                                span { class: "label", "Area" }
                                span { class: "value", "{area}" }
                            }
                        }
                        if let Some(ref class_name) = info.player_class {
                            div { class: "session-item",
                                span { class: "label", "Class" }
                                span { class: "value", "{class_name}" }
                            }
                        }
                         div { class: "session-item",
                            span { class: "label", "Combat" }
                            span {
                                class: if info.in_combat { "value status-warning" } else { "value" },
                                if info.in_combat { "In Combat" } else { "Out of Combat" }
                            }
                        }
                        if let Some(ref disc) = info.player_discipline {
                            div { class: "session-item",
                                span { class: "label", "Discipline" }
                                span { class: "value", "{disc}" }
                            }
                        }
                    }
                }
            }

            // Active file info panel (moved from Overlays tab)
            section { class: "active-file-panel",
                div { class: "file-info",
                    span { class: "label",
                        i { class: "fa-solid fa-folder-open" }
                        " Directory: "
                    }
                    span { class: "value", "{current_directory}" }
                }
                if !current_file.is_empty() {
                    div { class: "file-info",
                        span { class: "label",
                            i { class: "fa-solid fa-file-lines" }
                            " Active: "
                        }
                        span { class: "value filename", "{current_filename}" }
                    }
                }
                // Watcher status indicator
                div { class: "watcher-status",
                    span {
                        class: if watching { "status-dot watching" } else { "status-dot not-watching" }
                    }
                    span {
                        class: "status-text",
                        if watching { "Watching" } else { "Not Watching" }
                    }
                    button {
                        class: "btn-restart-watcher",
                        title: "Restart file directory watcher",
                        onclick: move |_| {
                            spawn(async move {
                                let _ = invoke("restart_watcher", JsValue::NULL).await;
                            });
                        },
                        i { class: "fa-solid fa-rotate" }
                    }
                }
            }

            // Encounter history panel (larger in session tab)
            div { class: "history-container-large",
                HistoryPanel {}
            }

            } // end SESSION TAB

            // OVERLAYS TAB: Overlay controls
            if active_tab() == "overlays" {

            // Overlay controls section
            section { class: "overlay-controls",
                div { class: "overlays-header",
                    h3 { "Overlays" }

                    // Quick profile selector
                    if !profile_names().is_empty() {
                        div { class: "profile-selector",
                            i { class: "fa-solid fa-user-gear" }
                            select {
                                class: "profile-dropdown",
                                value: active_profile().unwrap_or_default(),
                                onchange: move |e| {
                                    let selected = e.value();
                                    if selected.is_empty() { return; }

                                    spawn(async move {
                                        let obj = js_sys::Object::new();
                                        js_sys::Reflect::set(&obj, &JsValue::from_str("name"), &JsValue::from_str(&selected)).unwrap();
                                        let result = invoke("load_profile", obj.into()).await;
                                        if result.is_undefined() || result.is_null() {
                                            active_profile.set(Some(selected.clone()));
                                            // Refresh overlay settings
                                            let config_result = invoke("get_config", JsValue::NULL).await;
                                            if let Ok(config) = serde_wasm_bindgen::from_value::<AppConfig>(config_result) {
                                                overlay_settings.set(config.overlay_settings);
                                            }
                                            // Refresh running overlays
                                            let _ = invoke("refresh_overlay_settings", JsValue::NULL).await;
                                            // Update UI button states from actual overlay status
                                            let status_result = invoke("get_overlay_status", JsValue::NULL).await;
                                            if let Ok(status) = serde_wasm_bindgen::from_value::<OverlayStatus>(status_result) {
                                                let mut new_map = std::collections::HashMap::new();
                                                for ot in MetricType::all_metrics() {
                                                    let key = ot.config_key().to_string();
                                                    new_map.insert(*ot, status.enabled.contains(&key));
                                                }
                                                metric_overlays_enabled.set(new_map);
                                                personal_enabled.set(status.personal_enabled);
                                                raid_enabled.set(status.raid_enabled);
                                                overlays_visible.set(status.overlays_visible);
                                            }
                                        }
                                    });
                                },
                                for name in profile_names().iter() {
                                    {
                                        let pname = name.clone();
                                        let is_selected = active_profile().as_ref() == Some(&pname);
                                        rsx! {
                                            option {
                                                value: "{pname}",
                                                selected: is_selected,
                                                "{pname}"
                                            }
                                        }
                                    }
                                }
                            }
                            // Quick save button (only show if a profile is selected)
                            if active_profile().is_some() {
                                button {
                                    class: "profile-save-btn",
                                    title: "Save current settings to profile",
                                    onclick: move |_| {
                                        if let Some(profile_name) = active_profile() {
                                            spawn(async move {
                                                let obj = js_sys::Object::new();
                                                js_sys::Reflect::set(&obj, &JsValue::from_str("name"), &JsValue::from_str(&profile_name)).unwrap();
                                                let _ = invoke("save_profile", obj.into()).await;
                                            });
                                        }
                                    },
                                    i { class: "fa-solid fa-floppy-disk" }
                                }
                            }
                        }
                    }
                }

                // Overlay Controls row (Hide/Show, Lock, Rearrange Frames)
                h4 { class: "subsection-title", "Overlay Controls" }
                div { class: "settings-controls",
                    // Show/hide toggle (or placeholder if none enabled)
                    if any_enabled {
                        button {
                            class: if is_visible { "btn btn-control btn-visible" } else { "btn btn-control btn-hidden" },
                            onclick: toggle_visibility,
                            if is_visible {
                                i { class: "fa-solid fa-eye" }
                                span { " Visible" }
                            } else {
                                i { class: "fa-solid fa-eye-slash" }
                                span { " Hidden" }
                            }
                        }
                    } else {
                        button {
                            class: "btn btn-control btn-visibility-placeholder",
                            disabled: true,
                            i { class: "fa-solid fa-eye-slash" }
                            span { " Hidden" }
                        }
                    }
                    button {
                        class: if is_move_mode { "btn btn-control btn-unlocked" } else { "btn btn-control btn-locked" },
                        disabled: !is_visible || !any_enabled || is_rearrange_mode,
                        onclick: toggle_move,
                        if is_move_mode {
                            i { class: "fa-solid fa-lock-open" }
                            span { " Unlocked" }
                        } else {
                            i { class: "fa-solid fa-lock" }
                            span { " Locked" }
                        }
                    }
                    button {
                        class: if is_rearrange_mode { "btn btn-control btn-rearrange btn-active" } else { "btn btn-control btn-rearrange" },
                        disabled: !raid_on || is_move_mode,
                        onclick: toggle_rearrange,
                        i { class: "fa-solid fa-grip" }
                        span { " Rearrange" }
                    }
                    button {
                        class: "btn btn-control btn-clear-frames",
                        disabled: !raid_on,
                        onclick: move |_| {
                            spawn(async move {
                                let _ = invoke("clear_raid_registry", JsValue::NULL).await;
                            });
                        },
                        i { class: "fa-solid fa-trash" }
                        span { " Clear Frames" }
                    }
                }

                // General section (Personal Stats + Raid Frames + Boss Health)
                h4 { class: "subsection-title", "General" }
                div { class: "overlay-grid",
                    button {
                        class: if personal_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                        onclick: toggle_personal,
                        "Personal Stats"
                    }
                    button {
                        class: if raid_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                        onclick: toggle_raid,
                        "Raid Frames"
                    }
                    button {
                        class: if boss_health_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                        onclick: toggle_boss_health,
                        "Boss Health"
                    }
                }

                // Metrics section - 3 per row grid
                h4 { class: "subsection-title", "Metrics" }
                div { class: "overlay-grid",
                    for overlay_type in MetricType::all_metrics() {
                        {
                            let ot = *overlay_type;
                            let is_enabled = enabled_map.get(&ot).copied().unwrap_or(false);
                            rsx! {
                                button {
                                    class: if is_enabled { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                    onclick: make_toggle_overlay(ot),
                                    "{ot.label()}"
                                }
                            }
                        }
                    }
                }

                // Customize button (own section)
                div { class: "customize-section",
                    button {
                        class: "btn btn-control btn-settings",
                        onclick: move |_| settings_open.set(!settings_open()),
                        i { class: "fa-solid fa-screwdriver-wrench" }
                        span { " Customize" }
                    }
                }
            }

            // Settings modal (floating, draggable, non-blocking)
            if settings_open() {
                // Drag overlay - captures mouse events during drag
                if settings_dragging() {
                    div {
                        style: "position: fixed; top: 0; left: 0; right: 0; bottom: 0; z-index: 999; cursor: grabbing;",
                        onmousemove: move |e| {
                            let (offset_x, offset_y) = settings_drag_offset();
                            let new_x = e.client_coordinates().x as i32 - offset_x;
                            let new_y = e.client_coordinates().y as i32 - offset_y;
                            settings_panel_pos.set((new_x, new_y));
                        },
                        onmouseup: move |_| {
                            settings_dragging.set(false);
                        }
                    }
                }

                div {
                    class: "floating-panel-wrapper",
                    style: "position: fixed; left: {settings_panel_pos().0}px; top: {settings_panel_pos().1}px; z-index: 1000;",
                    onmousemove: move |e| {
                        if settings_dragging() {
                            let (offset_x, offset_y) = settings_drag_offset();
                            let new_x = e.client_coordinates().x as i32 - offset_x;
                            let new_y = e.client_coordinates().y as i32 - offset_y;
                            settings_panel_pos.set((new_x, new_y));
                        }
                    },
                    onmouseup: move |_| {
                        settings_dragging.set(false);
                    },
                    SettingsPanel {
                        settings: overlay_settings,
                        selected_tab: selected_overlay_tab,
                        profile_names: profile_names,
                        active_profile: active_profile,
                        metric_overlays_enabled: metric_overlays_enabled,
                        personal_enabled: personal_enabled,
                        raid_enabled: raid_enabled,
                        overlays_visible: overlays_visible,
                        on_close: move |_| settings_open.set(false),
                        on_header_mousedown: move |e: MouseEvent| {
                            let (panel_x, panel_y) = settings_panel_pos();
                            let offset_x = e.client_coordinates().x as i32 - panel_x;
                            let offset_y = e.client_coordinates().y as i32 - panel_y;
                            settings_drag_offset.set((offset_x, offset_y));
                            settings_dragging.set(true);
                        },
                    }
                }
            }

            } // end OVERLAYS TAB

            } // end tab-content

            // General settings modal
            if general_settings_open() {
                div {
                    class: "modal-backdrop",
                    onclick: move |_| general_settings_open.set(false),
                    div {
                        onclick: move |e| e.stop_propagation(),
                        section { class: "settings-panel general-settings",
                            div { class: "settings-header",
                                h3 { "Settings" }
                                button {
                                    class: "btn btn-close",
                                    onclick: move |_| general_settings_open.set(false),
                                    "X"
                                }
                            }

                            div { class: "settings-section",
                                h4 { "Log Directory" }
                                p { class: "hint", "Select the directory containing your SWTOR combat logs." }

                                div { class: "directory-picker",
                                    div { class: "directory-display",
                                        i { class: "fa-solid fa-folder" }
                                        span {
                                            class: "directory-path",
                                            if current_directory.is_empty() {
                                                "No directory selected"
                                            } else {
                                                "{current_directory}"
                                            }
                                        }
                                    }
                                    button {
                                        class: "btn btn-browse",
                                        onclick: browse_directory,
                                        i { class: "fa-solid fa-folder-open" }
                                        " Browse"
                                    }
                                }

                                if watching {
                                    div { class: "directory-status",
                                        span { class: "status-dot status-on" }
                                        span { "Watching for new log files" }
                                    }
                                }
                            }

                            // Hotkey Settings Section
                            div { class: "settings-section",
                                h4 { "Global Hotkeys" }
                                p { class: "hint", "Configure keyboard shortcuts. Format: Ctrl+Shift+Key (Windows only)" }
                                p { class: "hint hint-warning",
                                    i { class: "fa-solid fa-triangle-exclamation" }
                                    "Restart app on hotkey changes."
                                }

                                div { class: "hotkey-grid",
                                    div { class: "setting-row",
                                        label { "Show/Hide Overlays" }
                                        input {
                                            r#type: "text",
                                            class: "hotkey-input",
                                            placeholder: "e.g., Ctrl+Shift+O",
                                            value: hotkey_visibility,
                                            oninput: move |e| hotkey_visibility.set(e.value())
                                        }
                                    }

                                    div { class: "setting-row",
                                        label { "Toggle Move Mode" }
                                        input {
                                            r#type: "text",
                                            class: "hotkey-input",
                                            placeholder: "e.g., Ctrl+Shift+M",
                                            value: hotkey_move_mode,
                                            oninput: move |e| hotkey_move_mode.set(e.value())
                                        }
                                    }

                                    div { class: "setting-row",
                                        label { "Toggle Rearrange Mode" }
                                        input {
                                            r#type: "text",
                                            class: "hotkey-input",
                                            placeholder: "e.g., Ctrl+Shift+R",
                                            value: hotkey_rearrange,
                                            oninput: move |e| hotkey_rearrange.set(e.value())
                                        }
                                    }
                                }

                                div { class: "settings-footer",
                                    button {
                                        class: "btn btn-save",
                                        onclick: move |_| {
                                            let vis = hotkey_visibility();
                                            let mov = hotkey_move_mode();
                                            let rea = hotkey_rearrange();

                                            spawn(async move {
                                                let result = invoke("get_config", JsValue::NULL).await;
                                                if let Ok(mut config) = serde_wasm_bindgen::from_value::<AppConfig>(result) {
                                                    config.hotkeys.toggle_visibility = if vis.is_empty() { None } else { Some(vis) };
                                                    config.hotkeys.toggle_move_mode = if mov.is_empty() { None } else { Some(mov) };
                                                    config.hotkeys.toggle_rearrange_mode = if rea.is_empty() { None } else { Some(rea) };

                                                    let args = serde_wasm_bindgen::to_value(&config).unwrap_or(JsValue::NULL);
                                                    let obj = js_sys::Object::new();
                                                    js_sys::Reflect::set(&obj, &JsValue::from_str("config"), &args).unwrap();
                                                    let _ = invoke("update_config", obj.into()).await;
                                                    hotkey_save_status.set("Saved! Restart to apply.".to_string());
                                                }
                                            });
                                        },
                                        "Save Hotkeys"
                                    }
                                    span { class: "save-status", "{hotkey_save_status}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

