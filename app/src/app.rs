#![allow(non_snake_case)]
#![allow(clippy::too_many_arguments)]

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

pub type Color = [u8; 4];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverlayAppearanceConfig {
    #[serde(default = "default_true")]
    pub show_header: bool,
    #[serde(default = "default_true")]
    pub show_footer: bool,
    #[serde(default)]
    pub show_class_icons: bool,
    #[serde(default = "default_font_color")]
    pub font_color: Color,
    #[serde(default = "default_bar_color")]
    pub bar_color: Color,
    #[serde(default = "default_max_entries")]
    pub max_entries: u8,
}

fn default_true() -> bool { true }
fn default_font_color() -> Color { [255, 255, 255, 255] }
fn default_bar_color() -> Color { [180, 50, 50, 255] }
fn default_max_entries() -> u8 { 8 }

impl Default for OverlayAppearanceConfig {
    fn default() -> Self {
        Self {
            show_header: true,
            show_footer: true,
            show_class_icons: false,
            font_color: default_font_color(),
            bar_color: default_bar_color(),
            max_entries: 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PersonalStat {
    EncounterTime,
    EncounterCount,
    Apm,
    Dps,
    EDps,
    TotalDamage,
    Hps,
    EHps,
    TotalHealing,
    Dtps,
    EDtps,
    Tps,
    TotalThreat,
    DamageCritPct,
    HealCritPct,
    EffectiveHealPct,
    ClassDiscipline,
}

impl PersonalStat {
    pub fn label(&self) -> &'static str {
        match self {
            PersonalStat::EncounterTime => "Time",
            PersonalStat::EncounterCount => "Fight #",
            PersonalStat::Apm => "APM",
            PersonalStat::Dps => "DPS",
            PersonalStat::EDps => "eDPS",
            PersonalStat::TotalDamage => "Total Damage",
            PersonalStat::Hps => "HPS",
            PersonalStat::EHps => "eHPS",
            PersonalStat::TotalHealing => "Total Healing",
            PersonalStat::Dtps => "DTPS",
            PersonalStat::EDtps => "eDTPS",
            PersonalStat::Tps => "TPS",
            PersonalStat::TotalThreat => "Total Threat",
            PersonalStat::DamageCritPct => "Dmg Crit %",
            PersonalStat::HealCritPct => "Heal Crit %",
            PersonalStat::EffectiveHealPct => "Eff Heal %",
            PersonalStat::ClassDiscipline => "Spec",
        }
    }

    pub fn all() -> &'static [PersonalStat] {
        &[
            PersonalStat::EncounterTime,
            PersonalStat::EncounterCount,
            PersonalStat::ClassDiscipline,
            PersonalStat::Apm,
            PersonalStat::Dps,
            PersonalStat::EDps,
            PersonalStat::TotalDamage,
            PersonalStat::Hps,
            PersonalStat::EHps,
            PersonalStat::TotalHealing,
            PersonalStat::Dtps,
            PersonalStat::EDtps,
            PersonalStat::Tps,
            PersonalStat::TotalThreat,
            PersonalStat::DamageCritPct,
            PersonalStat::HealCritPct,
            PersonalStat::EffectiveHealPct,
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonalOverlayConfig {
    #[serde(default = "default_personal_stats")]
    pub visible_stats: Vec<PersonalStat>,
    #[serde(default = "default_font_color")]
    pub font_color: Color,
}

fn default_personal_stats() -> Vec<PersonalStat> {
    vec![
        PersonalStat::EncounterTime,
        PersonalStat::Dps,
        PersonalStat::Hps,
        PersonalStat::Dtps,
        PersonalStat::Apm,
    ]
}

impl Default for PersonalOverlayConfig {
    fn default() -> Self {
        Self {
            visible_stats: default_personal_stats(),
            font_color: default_font_color(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OverlayPositionConfig {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub monitor_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OverlaySettings {
    #[serde(default)]
    pub positions: std::collections::HashMap<String, OverlayPositionConfig>,
    #[serde(default)]
    pub appearances: std::collections::HashMap<String, OverlayAppearanceConfig>,
    #[serde(default)]
    pub enabled: std::collections::HashMap<String, bool>,
    #[serde(default)]
    pub personal_overlay: PersonalOverlayConfig,
    #[serde(default = "default_background_alpha")]
    pub background_alpha: u8,
}

fn default_background_alpha() -> u8 { 180 }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub log_directory: String,
    #[serde(default)]
    pub auto_delete_empty_files: bool,
    #[serde(default)]
    pub log_retention_days: u32,
    #[serde(default)]
    pub overlay_settings: OverlaySettings,
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
    pub personal_running: bool,
    pub personal_enabled: bool,
    pub overlays_visible: bool,
    pub move_mode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OverlayType {
    Dps,
    EDps,
    Hps,
    EHps,
    Tps,
    Dtps,
    EDtps,
    Abs,
}

impl OverlayType {
    pub fn label(&self) -> &'static str {
        match self {
            OverlayType::Dps => "DPS",
            OverlayType::EDps => "eDPS",
            OverlayType::Hps => "HPS",
            OverlayType::EHps => "eHPS",
            OverlayType::Tps => "TPS",
            OverlayType::Dtps => "DTPS",
            OverlayType::EDtps => "eDTPS",
            OverlayType::Abs => "ABS",
        }
    }

    pub fn config_key(&self) -> &'static str {
        match self {
            OverlayType::Dps => "dps",
            OverlayType::EDps => "edps",
            OverlayType::Hps => "hps",
            OverlayType::EHps => "ehps",
            OverlayType::Tps => "tps",
            OverlayType::Dtps => "dtps",
            OverlayType::EDtps => "edtps",
            OverlayType::Abs => "abs",
        }
    }

    /// All metric overlay types (for iteration)
    pub fn all_metrics() -> &'static [OverlayType] {
        &[
            OverlayType::Dps,
            OverlayType::EDps,
            OverlayType::Hps,
            OverlayType::EHps,
            OverlayType::Tps,
            OverlayType::Dtps,
            OverlayType::EDtps,
            OverlayType::Abs,
        ]
    }
}


// ─────────────────────────────────────────────────────────────────────────────
// App Component
// ─────────────────────────────────────────────────────────────────────────────

pub fn App() -> Element {
    // Overlay enabled states - HashMap for all metric overlays
    let mut metric_overlays_enabled = use_signal(|| {
        let mut map = std::collections::HashMap::new();
        for ot in OverlayType::all_metrics() {
            map.insert(*ot, false);
        }
        map
    });
    let mut personal_enabled = use_signal(|| false);

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

    // Settings panel state
    let mut settings_open = use_signal(|| false);
    let mut overlay_settings = use_signal(OverlaySettings::default);
    let selected_overlay_tab = use_signal(|| "dps".to_string());

    // Fetch initial state from backend
    use_future(move || async move {
        // Get config
        let result = invoke("get_config", JsValue::NULL).await;
        if let Ok(config) = serde_wasm_bindgen::from_value::<AppConfig>(result) {
            log_directory.set(config.log_directory.clone());
            overlay_settings.set(config.overlay_settings);
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
            // Set enabled states from config for all metric overlays
            let mut new_map = std::collections::HashMap::new();
            for ot in OverlayType::all_metrics() {
                let key = ot.config_key().to_string();
                new_map.insert(*ot, status.enabled.contains(&key));
            }
            metric_overlays_enabled.set(new_map);
            personal_enabled.set(status.personal_enabled);
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
            if let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
                && let Some(file_path) = payload.as_string() {
                    active_file.set(file_path);
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
    let enabled_map = metric_overlays_enabled();
    let personal_on = personal_enabled();
    let any_metric_enabled = enabled_map.values().any(|&v| v);
    let any_enabled = any_metric_enabled || personal_on;
    let is_visible = overlays_visible();
    let is_move_mode = move_mode();
    let status = status_msg();
    let current_directory = log_directory();
    let watching = is_watching();
    let current_file = active_file();
    let session = session_info();

    // Toggle metric overlay enabled state
    let enabled_map_for_toggle = enabled_map.clone();
    let make_toggle_overlay = move |overlay_type: OverlayType| {
        let current = enabled_map_for_toggle.get(&overlay_type).copied().unwrap_or(false);
        move |_| {
            let cmd = if current { "hide_overlay" } else { "show_overlay" };

            async move {
                let args = serde_wasm_bindgen::to_value(&overlay_type).unwrap_or(JsValue::NULL);
                let obj = js_sys::Object::new();
                js_sys::Reflect::set(&obj, &JsValue::from_str("overlayType"), &args).unwrap();

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
            let cmd = if current { "hide_personal_overlay" } else { "show_personal_overlay" };
            let result = invoke(cmd, JsValue::NULL).await;
            if let Some(success) = result.as_bool()
                && success
            {
                personal_enabled.set(!current);
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
        let current_overlay_settings = overlay_settings();

        async move {
            if dir.is_empty() {
                status_msg.set("Please enter a log directory path".to_string());
                return;
            }

            let config = AppConfig {
                log_directory: dir.clone(),
                auto_delete_empty_files: false,
                log_retention_days: 0,
                overlay_settings: current_overlay_settings,
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
                        div { class: "session-item",
                            span { class: "label", "Encounters" }
                            span { class: "value", "{info.encounter_count}" }
                        }
                    }
                }
            }


            // Overlay toggles
            section { class: "overlay-controls",
                h3 { "Overlays" }

                // Meters section
                h4 { class: "subsection-title", "Meters" }
                div { class: "overlay-meters",
                    for overlay_type in OverlayType::all_metrics() {
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

                // Personal section
                h4 { class: "subsection-title", "Personal" }
                div { class: "overlay-meters",
                    button {
                        class: if personal_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                        onclick: toggle_personal,
                        "Personal"
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
                    button {
                        class: "btn btn-settings",
                        onclick: move |_| settings_open.set(!settings_open()),
                        "Settings"
                    }
                }
            }

            // Settings panel
            if settings_open() {
                SettingsPanel {
                    settings: overlay_settings,
                    selected_tab: selected_overlay_tab,
                    on_close: move |_| settings_open.set(false),
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

// ─────────────────────────────────────────────────────────────────────────────
// Settings Panel Component
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn SettingsPanel(
    settings: Signal<OverlaySettings>,
    selected_tab: Signal<String>,
    on_close: EventHandler<()>,
) -> Element {
    // Local draft of settings being edited
    let mut draft_settings = use_signal(|| settings());
    let mut has_changes = use_signal(|| false);
    let mut save_status = use_signal(String::new);

    let current_settings = draft_settings();
    let tab = selected_tab();

    // Get appearance for current tab
    let get_appearance = |key: &str| -> OverlayAppearanceConfig {
        current_settings.appearances.get(key).cloned().unwrap_or_default()
    };

    let current_appearance = get_appearance(&tab);

    // Save settings to backend (preserves positions)
    let save_to_backend = move |_| {
        let new_settings = draft_settings();
        async move {
            // Get current full config first to preserve positions
            let result = invoke("get_config", JsValue::NULL).await;
            if let Ok(mut config) = serde_wasm_bindgen::from_value::<AppConfig>(result) {
                // Preserve existing positions - only update appearances and other settings
                let existing_positions = config.overlay_settings.positions.clone();
                let existing_enabled = config.overlay_settings.enabled.clone();

                config.overlay_settings.appearances = new_settings.appearances.clone();
                config.overlay_settings.personal_overlay = new_settings.personal_overlay.clone();
                config.overlay_settings.background_alpha = new_settings.background_alpha;
                // Keep positions and enabled state untouched
                config.overlay_settings.positions = existing_positions;
                config.overlay_settings.enabled = existing_enabled;

                let args = serde_wasm_bindgen::to_value(&config).unwrap_or(JsValue::NULL);
                let obj = js_sys::Object::new();
                js_sys::Reflect::set(&obj, &JsValue::from_str("config"), &args).unwrap();

                let result = invoke("update_config", obj.into()).await;
                if result.is_undefined() || result.is_null() {
                    // Refresh running overlays with new settings
                    let _ = invoke("refresh_overlay_settings", JsValue::NULL).await;

                    settings.set(new_settings);
                    has_changes.set(false);
                    save_status.set("Settings saved!".to_string());
                } else {
                    save_status.set("Failed to save".to_string());
                }
            }
        }
    };

    // Update draft settings helper
    let mut update_draft = move |new_settings: OverlaySettings| {
        draft_settings.set(new_settings);
        has_changes.set(true);
        save_status.set(String::new());
    };

    rsx! {
        section { class: "settings-panel",
            div { class: "settings-header",
                h3 { "Overlay Settings" }
                button {
                    class: "btn btn-close",
                    onclick: move |_| on_close.call(()),
                    "X"
                }
            }

            // Global settings
            div { class: "settings-section",
                h4 { "Global" }
                div { class: "setting-row",
                    label { "Background Opacity" }
                    input {
                        r#type: "range",
                        min: "0",
                        max: "255",
                        value: "{current_settings.background_alpha}",
                        oninput: move |e| {
                            if let Ok(val) = e.value().parse::<u8>() {
                                let mut new_settings = draft_settings();
                                new_settings.background_alpha = val;
                                update_draft(new_settings);
                            }
                        }
                    }
                    span { class: "value", "{current_settings.background_alpha}" }
                }
            }

            // Tabs for overlay types
            div { class: "settings-tabs",
                for overlay_type in OverlayType::all_metrics() {
                    {
                        let ot = *overlay_type;
                        let key = ot.config_key().to_string();
                        let label = ot.label();
                        rsx! {
                            button {
                                class: if tab == key { "tab-btn active" } else { "tab-btn" },
                                onclick: move |_| selected_tab.set(key.clone()),
                                "{label}"
                            }
                        }
                    }
                }
                button {
                    class: if tab == "personal" { "tab-btn active" } else { "tab-btn" },
                    onclick: move |_| selected_tab.set("personal".to_string()),
                    "Personal"
                }
            }

            // Per-overlay settings
            if tab != "personal" {
                div { class: "settings-section",
                    h4 { "{tab.to_uppercase()} Meter" }

                    div { class: "setting-row",
                        label { "Show Header" }
                        input {
                            r#type: "checkbox",
                            checked: current_appearance.show_header,
                            onchange: {
                                let tab = tab.clone();
                                move |e: Event<FormData>| {
                                    let mut new_settings = draft_settings();
                                    let mut appearance = new_settings.appearances
                                        .entry(tab.clone())
                                        .or_insert_with(OverlayAppearanceConfig::default)
                                        .clone();
                                    appearance.show_header = e.checked();
                                    new_settings.appearances.insert(tab.clone(), appearance);
                                    update_draft(new_settings);
                                }
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show Footer (Total)" }
                        input {
                            r#type: "checkbox",
                            checked: current_appearance.show_footer,
                            onchange: {
                                let tab = tab.clone();
                                move |e: Event<FormData>| {
                                    let mut new_settings = draft_settings();
                                    let mut appearance = new_settings.appearances
                                        .entry(tab.clone())
                                        .or_insert_with(OverlayAppearanceConfig::default)
                                        .clone();
                                    appearance.show_footer = e.checked();
                                    new_settings.appearances.insert(tab.clone(), appearance);
                                    update_draft(new_settings);
                                }
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Max Entries" }
                        input {
                            r#type: "number",
                            min: "1",
                            max: "16",
                            value: "{current_appearance.max_entries}",
                            onchange: {
                                let tab = tab.clone();
                                move |e: Event<FormData>| {
                                    if let Ok(val) = e.value().parse::<u8>() {
                                        let mut new_settings = draft_settings();
                                        let mut appearance = new_settings.appearances
                                            .entry(tab.clone())
                                            .or_insert_with(OverlayAppearanceConfig::default)
                                            .clone();
                                        appearance.max_entries = val.clamp(1, 16);
                                        new_settings.appearances.insert(tab.clone(), appearance);
                                        update_draft(new_settings);
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                // Personal overlay settings
                div { class: "settings-section",
                    h4 { "Personal Stats" }
                    p { class: "hint", "Select which stats to display:" }

                    div { class: "stat-grid",
                        for stat in PersonalStat::all() {
                            {
                                let is_visible = current_settings.personal_overlay.visible_stats.contains(stat);
                                rsx! {
                                    label { class: "stat-toggle",
                                        input {
                                            r#type: "checkbox",
                                            checked: is_visible,
                                            onchange: {
                                                let stat = *stat;
                                                move |e: Event<FormData>| {
                                                    let mut new_settings = draft_settings();
                                                    if e.checked() {
                                                        if !new_settings.personal_overlay.visible_stats.contains(&stat) {
                                                            new_settings.personal_overlay.visible_stats.push(stat);
                                                        }
                                                    } else {
                                                        new_settings.personal_overlay.visible_stats.retain(|s| *s != stat);
                                                    }
                                                    update_draft(new_settings);
                                                }
                                            }
                                        }
                                        span { "{stat.label()}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Save button and status
            div { class: "settings-footer",
                button {
                    class: if has_changes() { "btn btn-save" } else { "btn btn-save btn-disabled" },
                    disabled: !has_changes(),
                    onclick: save_to_backend,
                    "Save Settings"
                }
                if !save_status().is_empty() {
                    span { class: "save-status", "{save_status()}" }
                }
            }
        }
    }
}
