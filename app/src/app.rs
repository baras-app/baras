#![allow(non_snake_case)]

use dioxus::prelude::*;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::console;

use crate::api;
use crate::components::{
    DataExplorerPanel, EffectEditorPanel, EncounterEditorPanel, HistoryPanel, SettingsPanel,
};
use crate::types::{
    LogFileInfo, MetricType, OverlaySettings, OverlayStatus, OverlayType, SessionInfo, UpdateInfo,
};

static CSS: Asset = asset!("/assets/styles.css");
static DATA_EXPLORER_CSS: Asset = asset!("/assets/data-explorer.css");
static LOGO: Asset = asset!("/assets/logo.png");
static FONT: Asset = asset!("/assets/StarJedi.ttf");

// ─────────────────────────────────────────────────────────────────────────────
// App Component
// ─────────────────────────────────────────────────────────────────────────────

pub fn App() -> Element {
    // Overlay state
    let mut metric_overlays_enabled = use_signal(|| {
        MetricType::all()
            .iter()
            .map(|ot| (*ot, false))
            .collect::<HashMap<_, _>>()
    });
    let mut personal_enabled = use_signal(|| false);
    let mut raid_enabled = use_signal(|| false);
    let mut boss_health_enabled = use_signal(|| false);
    let mut timers_enabled = use_signal(|| false);
    let mut challenges_enabled = use_signal(|| false);
    let mut alerts_enabled = use_signal(|| false);
    let mut effects_a_enabled = use_signal(|| false);
    let mut effects_b_enabled = use_signal(|| false);
    let mut cooldowns_enabled = use_signal(|| false);
    let mut dot_tracker_enabled = use_signal(|| false);
    let mut overlays_visible = use_signal(|| true);
    let mut move_mode = use_signal(|| false);
    let mut rearrange_mode = use_signal(|| false);

    // Directory and file state
    let mut log_directory = use_signal(String::new);
    let mut active_file = use_signal(String::new);
    let mut is_watching = use_signal(|| false);
    let mut is_live_tailing = use_signal(|| true);
    let mut session_info = use_signal(|| None::<SessionInfo>);

    // File browser state
    let mut file_browser_open = use_signal(|| false);
    let mut log_files = use_signal(Vec::<LogFileInfo>::new);
    let mut upload_status = use_signal(|| None::<(String, bool, String)>); // (path, success, message)
    let mut file_browser_filter = use_signal(String::new);
    let mut hide_small_log_files = use_signal(|| true);

    // UI state
    let mut active_tab = use_signal(|| "session".to_string());
    let mut settings_open = use_signal(|| false);
    let mut general_settings_open = use_signal(|| false);
    let mut overlay_settings = use_signal(OverlaySettings::default);
    let selected_overlay_tab = use_signal(|| "dps".to_string());
    let mut show_only_bosses = use_signal(|| false);

    // Hotkey state
    let mut hotkey_visibility = use_signal(String::new);
    let mut hotkey_move_mode = use_signal(String::new);
    let mut hotkey_rearrange = use_signal(String::new);
    let mut hotkey_save_status = use_signal(String::new);

    // Log management state
    let mut log_dir_size = use_signal(|| 0u64);
    let mut log_file_count = use_signal(|| 0usize);
    let mut auto_delete_empty = use_signal(|| false);
    let mut auto_delete_old = use_signal(|| false);
    let mut retention_days = use_signal(|| 21u32);
    let mut cleanup_status = use_signal(String::new);

    // Application settings
    let mut minimize_to_tray = use_signal(|| true);
    let mut app_version = use_signal(String::new);

    // Update state
    let mut update_available = use_signal(|| None::<UpdateInfo>);
    let mut update_installing = use_signal(|| false);

    // Audio settings
    let mut audio_enabled = use_signal(|| true);
    let mut audio_volume = use_signal(|| 80u8);
    let mut audio_countdown_enabled = use_signal(|| true);
    let mut audio_alerts_enabled = use_signal(|| true);

    // Profile state
    let mut profile_names = use_signal(Vec::<String>::new);
    let mut active_profile = use_signal(|| None::<String>);

    // Parsely settings
    let mut parsely_username = use_signal(String::new);
    let mut parsely_password = use_signal(String::new);
    let mut parsely_guild = use_signal(String::new);
    let mut parsely_save_status = use_signal(String::new);

    // ─────────────────────────────────────────────────────────────────────────
    // Initial Load
    // ─────────────────────────────────────────────────────────────────────────

    use_future(move || async move {
        if let Some(config) = api::get_config().await {
            log_directory.set(config.log_directory.clone());
            overlay_settings.set(config.overlay_settings);
            if let Some(v) = config.hotkeys.toggle_visibility {
                hotkey_visibility.set(v);
            }
            if let Some(v) = config.hotkeys.toggle_move_mode {
                hotkey_move_mode.set(v);
            }
            if let Some(v) = config.hotkeys.toggle_rearrange_mode {
                hotkey_rearrange.set(v);
            }
            profile_names.set(config.profiles.iter().map(|p| p.name.clone()).collect());
            active_profile.set(config.active_profile_name);
            auto_delete_empty.set(config.auto_delete_empty_files);
            auto_delete_old.set(config.auto_delete_old_files);
            retention_days.set(config.log_retention_days);
            hide_small_log_files.set(config.hide_small_log_files);
            minimize_to_tray.set(config.minimize_to_tray);
            parsely_username.set(config.parsely.username);
            parsely_password.set(config.parsely.password);
            parsely_guild.set(config.parsely.guild);
            // Audio settings
            audio_enabled.set(config.audio.enabled);
            audio_volume.set(config.audio.volume);
            audio_countdown_enabled.set(config.audio.countdown_enabled);
            audio_alerts_enabled.set(config.audio.alerts_enabled);
            // UI preferences
            show_only_bosses.set(config.show_only_bosses);
        }

        app_version.set(api::get_app_version().await);
        log_dir_size.set(api::get_log_directory_size().await);
        log_file_count.set(api::get_log_file_count().await);

        // Fetch log files list for Latest/Current display
        let result = api::get_log_files().await;
        if let Ok(files) = serde_wasm_bindgen::from_value::<Vec<LogFileInfo>>(result) {
            log_files.set(files);
        }

        is_watching.set(api::get_watching_status().await);
        if let Some(file) = api::get_active_file().await {
            active_file.set(file);
        }

        if let Some(status) = api::get_overlay_status().await {
            apply_status(
                &status,
                &mut metric_overlays_enabled,
                &mut personal_enabled,
                &mut raid_enabled,
                &mut boss_health_enabled,
                &mut timers_enabled,
                &mut challenges_enabled,
                &mut alerts_enabled,
                &mut effects_a_enabled,
                &mut effects_b_enabled,
                &mut cooldowns_enabled,
                &mut dot_tracker_enabled,
                &mut overlays_visible,
                &mut move_mode,
                &mut rearrange_mode,
            );
        }

        session_info.set(api::get_session_info().await);
    });

    // Listen for file changes
    use_future(move || async move {
        let closure = Closure::new(move |event: JsValue| {
            if let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
                && let Some(path) = payload.as_string()
            {
                // Use try_write to handle signal being dropped when component unmounts
                let _ = active_file.try_write().map(|mut w| *w = path);
            }
        });
        api::tauri_listen("active-file-changed", &closure).await;
        closure.forget();
    });

    // Listen for log file changes (event-driven from watcher)
    use_future(move || async move {
        let closure = Closure::new(move |_event: JsValue| {
            // Use spawn_local for JS callbacks (no Dioxus runtime context available)
            spawn_local(async move {
                let result = api::get_log_files().await;
                if let Ok(files) = serde_wasm_bindgen::from_value::<Vec<LogFileInfo>>(result) {
                    let _ = log_files.try_write().map(|mut w| *w = files);
                }
            });
        });
        api::tauri_listen("log-files-changed", &closure).await;
        closure.forget();
    });

    // Listen for session updates (event-driven from backend signals)
    use_future(move || async move {
        // Initial fetch on mount
        session_info.set(api::get_session_info().await);
        is_watching.set(api::get_watching_status().await);
        is_live_tailing.set(api::is_live_tailing().await);

        // Listen for updates (no more polling!)
        let closure = Closure::new(move |_event: JsValue| {
            // Use spawn_local for JS callbacks (no Dioxus runtime context available)
            spawn_local(async move {
                let info = api::get_session_info().await;
                let watching = api::get_watching_status().await;
                let tailing = api::is_live_tailing().await;
                let _ = session_info.try_write().map(|mut w| *w = info);
                let _ = is_watching.try_write().map(|mut w| *w = watching);
                let _ = is_live_tailing.try_write().map(|mut w| *w = tailing);
            });
        });
        api::tauri_listen("session-updated", &closure).await;
        closure.forget();
    });

    // Listen for app updates
    use_future(move || async move {
        let closure = Closure::new(move |event: JsValue| {
            if let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload")) {
                if let Ok(info) = serde_wasm_bindgen::from_value::<UpdateInfo>(payload) {
                    let _ = update_available.try_write().map(|mut w| *w = Some(info));
                }
            }
        });
        api::tauri_listen("update-available", &closure).await;
        closure.forget();
    });

    // Listen for update failures
    use_future(move || async move {
        let closure = Closure::new(move |event: JsValue| {
            if let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload")) {
                if let Some(msg) = payload.as_string() {
                    console::error_1(&format!("Update failed: {}", msg).into());
                }
            }
            // Reset installing state so user can retry
            let _ = update_installing.try_write().map(|mut w| *w = false);
        });
        api::tauri_listen("update-failed", &closure).await;
        closure.forget();
    });

    // ─────────────────────────────────────────────────────────────────────────
    // Computed Values
    // ─────────────────────────────────────────────────────────────────────────

    let enabled_map = metric_overlays_enabled();
    let personal_on = personal_enabled();
    let raid_on = raid_enabled();
    let boss_health_on = boss_health_enabled();
    let timers_on = timers_enabled();
    let challenges_on = challenges_enabled();
    let alerts_on = alerts_enabled();
    let effects_a_on = effects_a_enabled();
    let effects_b_on = effects_b_enabled();
    let cooldowns_on = cooldowns_enabled();
    let dot_tracker_on = dot_tracker_enabled();
    let any_enabled = enabled_map.values().any(|&v| v)
        || personal_on
        || raid_on
        || boss_health_on
        || timers_on
        || challenges_on
        || alerts_on
        || effects_a_on
        || effects_b_on
        || cooldowns_on
        || dot_tracker_on;
    let is_visible = overlays_visible();
    let is_move_mode = move_mode();
    let is_rearrange = rearrange_mode();
    let current_dir = log_directory();
    let watching = is_watching();
    let live_tailing = is_live_tailing();
    let current_file = active_file();
    let session = session_info();

    // ─────────────────────────────────────────────────────────────────────────
    // Render
    // ─────────────────────────────────────────────────────────────────────────

    rsx! {
        link { rel: "stylesheet", href: CSS }
        link { rel: "stylesheet", href: DATA_EXPLORER_CSS }
        link { rel: "stylesheet", href: "https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css" }
        style { "@font-face {{ font-family: 'StarJedi'; src: url('{FONT}') format('truetype'); }}" }

        main { class: "container",
            // Header
            header { class: "app-header",
                div { class: "header-content",
                    h1 { "BARAS" }
                    img { class: "header-logo", src: LOGO, alt: "BARAS mascot" }
                    if !app_version().is_empty() {
                        if let Some(ref update) = update_available() {
                            // Update available - show clickable notification
                            button {
                                class: if update_installing() { "header-version update-available updating" } else { "header-version update-available" },
                                title: update.notes.as_deref().unwrap_or("Update available"),
                                disabled: update_installing(),
                                onclick: move |_| {
                                    update_installing.set(true);
                                    spawn(async move {
                                        if let Err(e) = api::install_update().await {
                                            console::error_1(&format!("Update failed: {}", e).into());
                                            update_installing.set(false);
                                        }
                                        // On success, app will restart automatically
                                    });
                                },
                                if update_installing() {
                                    i { class: "fa-solid fa-spinner fa-spin" }
                                    " Updating..."
                                } else {
                                    i { class: "fa-solid fa-download" }
                                    " v{update.version}"
                                }
                            }
                        } else {
                            // No update - show current version
                            span { class: "header-version", "v{app_version}" }
                        }
                    }
                    p { class: "subtitle", "Battle Analysis and Raid Assessment System" }
                    a {
                        class: "header-help",
                        href: "#",
                        title: "Documentation & Help",
                        onclick: move |e| {
                            e.prevent_default();
                            spawn(async move {
                                api::open_url("https://github.com/baras-app/baras/wiki").await;
                            });
                        },
                        i { class: "fa-solid fa-circle-question" }
                    }
                }
                // Session indicator (always visible)
                div { class: "header-session-indicator",
                    // Watcher status dot
                    span {
                        class: if !live_tailing { "status-dot paused" }
                            else if watching { "status-dot watching" }
                            else { "status-dot not-watching" },
                        title: if !live_tailing { "Paused" } else if watching { "Watching" } else { "Not watching" }
                    }
                    // Viewing indicator
                    {
                        let current_meta = log_files().iter().find(|f| f.path == current_file).cloned();
                        let display = current_meta.as_ref()
                            .map(|f| f.character_name.clone().unwrap_or_else(|| f.display_name.clone()))
                            .unwrap_or_else(|| "None".to_string());
                        let date = current_meta.as_ref().map(|f| f.date.clone()).unwrap_or_default();
                        let is_latest = log_files().first().map(|f| f.path == current_file).unwrap_or(false);
                        rsx! {
                            span {
                                class: if is_latest { "session-file latest" } else { "session-file" },
                                title: if is_latest { format!("Viewing latest: {} - {}", display, date) } else { format!("Viewing: {} - {}", display, date) },
                                if is_latest {
                                    i { class: "fa-solid fa-clock" }
                                } else {
                                    i { class: "fa-solid fa-file-lines" }
                                }
                                " {display}"
                            }
                        }
                    }
                    // Resume button when paused
                    if !live_tailing {
                        button {
                            class: "btn-header-resume",
                            title: "Resume live tailing",
                            onclick: move |_| {
                                spawn(async move {
                                    api::resume_live_tailing().await;
                                    is_live_tailing.set(true);
                                });
                            },
                            i { class: "fa-solid fa-play" }
                        }
                    }
                    // Restart watcher button
                    button {
                        class: "btn-header-restart",
                        title: "Restart watcher",
                        onclick: move |_| {
                            spawn(async move {
                                api::restart_watcher().await;
                                is_live_tailing.set(true);
                            });
                        },
                        i { class: "fa-solid fa-rotate" }
                    }
                }

                // Quick overlay controls with profile dropdown
                div { class: "header-overlay-controls",
                    // Profile dropdown (no label, compact)
                    if !profile_names().is_empty() {
                        select {
                            class: "header-profile-dropdown",
                            title: "Switch profile",
                            value: active_profile().unwrap_or_default(),
                            onchange: move |e| {
                                let selected = e.value();
                                spawn(async move {
                                    if !selected.is_empty() && api::load_profile(&selected).await {
                                        active_profile.set(Some(selected));
                                        if let Some(cfg) = api::get_config().await {
                                            overlay_settings.set(cfg.overlay_settings);
                                        }
                                        api::refresh_overlay_settings().await;
                                        if let Some(status) = api::get_overlay_status().await {
                                            apply_status(&status, &mut metric_overlays_enabled, &mut personal_enabled,
                                                &mut raid_enabled, &mut boss_health_enabled, &mut timers_enabled,
                                                &mut challenges_enabled, &mut alerts_enabled,
                                                &mut effects_a_enabled, &mut effects_b_enabled,
                                                &mut cooldowns_enabled, &mut dot_tracker_enabled,
                                                &mut overlays_visible, &mut move_mode, &mut rearrange_mode);
                                        }
                                    }
                                });
                            },
                            for name in profile_names().iter() {
                                option { value: "{name}", "{name}" }
                            }
                        }
                    }
                    div { class: "header-controls-divider" }
                    button {
                        class: if is_visible { "btn btn-header-overlay active" } else { "btn btn-header-overlay" },
                        title: if is_visible { "Hide overlays" } else { "Show overlays" },
                        disabled: !any_enabled,
                        onclick: move |_| { spawn(async move {
                            if api::toggle_visibility(is_visible).await {
                                overlays_visible.set(!is_visible);
                                if is_visible { move_mode.set(false); }
                            }
                        }); },
                        i { class: if is_visible { "fa-solid fa-eye" } else { "fa-solid fa-eye-slash" } }
                    }
                    button {
                        class: if is_move_mode { "btn btn-header-overlay active" } else { "btn btn-header-overlay" },
                        title: if is_move_mode { "Lock overlays" } else { "Unlock overlays (move/resize)" },
                        disabled: !is_visible || !any_enabled || is_rearrange,
                        onclick: move |_| { spawn(async move {
                            if let Ok(new_mode) = api::toggle_move_mode().await {
                                move_mode.set(new_mode);
                                if new_mode { rearrange_mode.set(false); }
                            }
                        }); },
                        i { class: if is_move_mode { "fa-solid fa-lock-open" } else { "fa-solid fa-lock" } }
                    }
                    button {
                        class: if is_rearrange { "btn btn-header-overlay active" } else { "btn btn-header-overlay" },
                        title: "Rearrange raid frames",
                        disabled: !is_visible || !raid_on || is_move_mode,
                        onclick: move |_| { spawn(async move {
                            if let Ok(new_mode) = api::toggle_raid_rearrange().await {
                                rearrange_mode.set(new_mode);
                            }
                        }); },
                        i { class: "fa-solid fa-grip" }
                    }
                    button {
                        class: "btn btn-header-overlay",
                        title: "Clear raid frame assignments",
                        disabled: !raid_on,
                        onclick: move |_| { spawn(async move { api::clear_raid_registry().await; }); },
                        i { class: "fa-solid fa-eraser" }
                    }
                }

                div { class: "header-buttons",
                    button {
                        class: "btn btn-header-files",
                        title: "Browse log files",
                        onclick: move |_| {
                            file_browser_filter.set(String::new()); // Clear filter on open
                            file_browser_open.set(true);
                            // Fetch files when opening
                            spawn(async move {
                                let result = api::get_log_files().await;
                                if let Ok(files) = serde_wasm_bindgen::from_value::<Vec<LogFileInfo>>(result) {
                                    log_files.set(files);
                                }
                            });
                        },
                        i { class: "fa-solid fa-folder-open" }
                    }
                    button {
                        class: "btn btn-header-settings",
                        title: "Settings",
                        onclick: move |_| general_settings_open.set(true),
                        i { class: "fa-solid fa-gear" }
                    }
                }
            }

            // Tabs
            nav { class: "main-tabs",
                button {
                    class: if active_tab() == "session" { "tab-btn active" } else { "tab-btn" },
                    onclick: move |_| active_tab.set("session".to_string()),
                    i { class: "fa-solid fa-chart-line" }
                    " Session"
                }
               button {
                    class: if active_tab() == "explorer" { "tab-btn active" } else { "tab-btn" },
                    onclick: move |_| active_tab.set("explorer".to_string()),
                    i { class: "fa-solid fa-magnifying-glass-chart" }
                    " Data Explorer"
                }
                button {
                    class: if active_tab() == "overlays" { "tab-btn active" } else { "tab-btn" },
                    onclick: move |_| active_tab.set("overlays".to_string()),
                    i { class: "fa-solid fa-layer-group" }
                    " Overlays"
                }
                button {
                    class: if active_tab() == "timers" { "tab-btn active" } else { "tab-btn" },
                    onclick: move |_| active_tab.set("timers".to_string()),
                    i { class: "fa-solid fa-skull" }
                    " Encounter Builder"
                }
                button {
                    class: if active_tab() == "effects" { "tab-btn active" } else { "tab-btn" },
                    onclick: move |_| active_tab.set("effects".to_string()),
                    i { class: "fa-solid fa-heart-pulse" }
                    " Effects"
                }

            }

            // Tab Content
            div { class: "tab-content",
                // ─────────────────────────────────────────────────────────────
                // Session Tab
                // ─────────────────────────────────────────────────────────────
                if active_tab() == "session" {
                    if let Some(ref info) = session {
                        section { class: "session-panel",
                            h3 { "Session" }
                            div { class: "session-grid",
                                // Row 1: Player - Area - Session Start
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
                                if let Some(ref start) = info.session_start {
                                    div { class: "session-item",
                                        span { class: "label", "Started" }
                                        span { class: "value", "{start}" }
                                    }
                                }
                                // Row 2: Class - Discipline - Combat
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
                                div { class: "session-item",
                                    span { class: "label", "Combat" }
                                    span {
                                        class: if info.in_combat { "value status-warning" } else { "value" },
                                        if info.in_combat { "In Combat" } else { "Out of Combat" }
                                    }
                                }
                            }
                        }
                    }

                    div { class: "history-container-large", HistoryPanel { show_only_bosses } }
                }

                // ─────────────────────────────────────────────────────────────
                // Overlays Tab
                // ─────────────────────────────────────────────────────────────
                if active_tab() == "overlays" {
                    section { class: "overlay-controls",
                        // Top bar: Customize button + Profile selector
                        div { class: "overlays-top-bar",
                            button {
                                class: "btn btn-customize",
                                onclick: move |_| settings_open.set(!settings_open()),
                                i { class: "fa-solid fa-screwdriver-wrench" }
                                span { " Customize" }
                            }
                            if !profile_names().is_empty() {
                                div { class: "profile-selector",
                                    span { class: "profile-label", "Profiles:" }
                                    select {
                                        class: "profile-dropdown",
                                        value: active_profile().unwrap_or_default(),
                                        onchange: move |e| {
                                            let selected = e.value();
                                            spawn(async move {
                                                if !selected.is_empty() && api::load_profile(&selected).await {
                                                    active_profile.set(Some(selected));
                                                    if let Some(cfg) = api::get_config().await {
                                                        overlay_settings.set(cfg.overlay_settings);
                                                    }
                                                    api::refresh_overlay_settings().await;
                                                    if let Some(status) = api::get_overlay_status().await {
                                                        apply_status(&status, &mut metric_overlays_enabled, &mut personal_enabled,
                                                            &mut raid_enabled, &mut boss_health_enabled, &mut timers_enabled,
                                                            &mut challenges_enabled, &mut alerts_enabled,
                                                            &mut effects_a_enabled, &mut effects_b_enabled,
                                                            &mut cooldowns_enabled, &mut dot_tracker_enabled,
                                                            &mut overlays_visible, &mut move_mode, &mut rearrange_mode);
                                                    }
                                                }
                                            });
                                        },
                                        for name in profile_names().iter() {
                                            option { value: "{name}", "{name}" }
                                        }
                                    }
                                    if active_profile().is_some() {
                                        button {
                                            class: "profile-save-btn",
                                            title: "Save to profile",
                                            onclick: move |_| {
                                                if let Some(ref name) = active_profile() {
                                                    let n = name.clone();
                                                    spawn(async move { api::save_profile(&n).await; });
                                                }
                                            },
                                            i { class: "fa-solid fa-floppy-disk" }
                                        }
                                    }
                                }
                            }
                        }

                        // Controls
                        h4 { class: "subsection-title", "Controls" }
                        div { class: "settings-controls",
                            button {
                                class: if is_visible && any_enabled { "btn btn-control btn-visible" } else { "btn btn-control btn-hidden" },
                                disabled: !any_enabled,
                                onclick: move |_| { spawn(async move {
                                    if api::toggle_visibility(is_visible).await {
                                        overlays_visible.set(!is_visible);
                                        if is_visible { move_mode.set(false); }
                                    }
                                }); },
                                if is_visible { i { class: "fa-solid fa-eye" } span { " Visible" } }
                                else { i { class: "fa-solid fa-eye-slash" } span { " Hidden" } }
                            }
                            button {
                                class: if is_move_mode { "btn btn-control btn-unlocked" } else { "btn btn-control btn-locked" },
                                disabled: !is_visible || !any_enabled || is_rearrange,
                                onclick: move |_| { spawn(async move {
                                    if let Ok(new_mode) = api::toggle_move_mode().await {
                                        move_mode.set(new_mode);
                                        if new_mode { rearrange_mode.set(false); }
                                    }
                                }); },
                                if is_move_mode { i { class: "fa-solid fa-lock-open" } span { " Unlocked" } }
                                else { i { class: "fa-solid fa-lock" } span { " Locked" } }
                            }
                            button {
                                class: if is_rearrange { "btn btn-control btn-rearrange btn-active" } else { "btn btn-control btn-rearrange" },
                                disabled: !is_visible || !raid_on || is_move_mode,
                                onclick: move |_| { spawn(async move {
                                    if let Ok(new_mode) = api::toggle_raid_rearrange().await {
                                        rearrange_mode.set(new_mode);
                                    }
                                }); },
                                i { class: "fa-solid fa-grip" }
                                span { " Rearrange Frames" }
                            }
                            button {
                                class: "btn btn-control btn-clear-frames",
                                disabled: !is_visible || !raid_on,
                                onclick: move |_| { spawn(async move { api::clear_raid_registry().await; }); },
                                i { class: "fa-solid fa-trash" }
                                span { " Clear Frames" }
                            }
                        }

                        // General overlays
                        h4 { class: "subsection-title", "General" }
                        div { class: "overlay-grid",
                            button {
                                class: if personal_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                onclick: move |_| { spawn(async move {
                                    if api::toggle_overlay(OverlayType::Personal, personal_on).await {
                                        personal_enabled.set(!personal_on);
                                    }
                                }); },
                                "Personal Stats"
                            }
                            button {
                                class: if raid_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                onclick: move |_| { spawn(async move {
                                    if api::toggle_overlay(OverlayType::Raid, raid_on).await {
                                        raid_enabled.set(!raid_on);
                                        if raid_on { rearrange_mode.set(false); }
                                    }
                                }); },
                                "Raid Frames"
                            }
                            button {
                                class: if boss_health_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                onclick: move |_| { spawn(async move {
                                    if api::toggle_overlay(OverlayType::BossHealth, boss_health_on).await {
                                        boss_health_enabled.set(!boss_health_on);
                                    }
                                }); },
                                "Boss Health"
                            }
                            button {
                                class: if timers_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                onclick: move |_| { spawn(async move {
                                    if api::toggle_overlay(OverlayType::Timers, timers_on).await {
                                        timers_enabled.set(!timers_on);
                                    }
                                }); },
                                "Encounter Timers"
                            }
                            button {
                                class: if challenges_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                onclick: move |_| { spawn(async move {
                                    if api::toggle_overlay(OverlayType::Challenges, challenges_on).await {
                                        challenges_enabled.set(!challenges_on);
                                    }
                                }); },
                                "Challenges"
                            }
                            button {
                                class: if alerts_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                onclick: move |_| { spawn(async move {
                                    if api::toggle_overlay(OverlayType::Alerts, alerts_on).await {
                                        alerts_enabled.set(!alerts_on);
                                    }
                                }); },
                                "Alerts"
                            }
                        }

                        // Effects overlays
                        h4 { class: "subsection-title", "Effects" }
                        div { class: "overlay-grid",
                            button {
                                class: if effects_a_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                onclick: move |_| { spawn(async move {
                                    if api::toggle_overlay(OverlayType::EffectsA, effects_a_on).await {
                                        effects_a_enabled.set(!effects_a_on);
                                    }
                                }); },
                                "Effects A"
                            }
                            button {
                                class: if effects_b_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                onclick: move |_| { spawn(async move {
                                    if api::toggle_overlay(OverlayType::EffectsB, effects_b_on).await {
                                        effects_b_enabled.set(!effects_b_on);
                                    }
                                }); },
                                "Effects B"
                            }
                            button {
                                class: if cooldowns_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                onclick: move |_| { spawn(async move {
                                    if api::toggle_overlay(OverlayType::Cooldowns, cooldowns_on).await {
                                        cooldowns_enabled.set(!cooldowns_on);
                                    }
                                }); },
                                "Cooldowns"
                            }
                            button {
                                class: if dot_tracker_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                onclick: move |_| { spawn(async move {
                                    if api::toggle_overlay(OverlayType::DotTracker, dot_tracker_on).await {
                                        dot_tracker_enabled.set(!dot_tracker_on);
                                    }
                                }); },
                                "DOT Tracker"
                            }
                        }

                        // Metric overlays
                        h4 { class: "subsection-title", "Metrics" }
                        div { class: "overlay-grid",
                            for mt in MetricType::all() {
                                {
                                    let ot = *mt;
                                    let is_on = enabled_map.get(&ot).copied().unwrap_or(false);
                                    rsx! {
                                        button {
                                            class: if is_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                            onclick: move |_| { spawn(async move {
                                                if api::toggle_overlay(OverlayType::Metric(ot), is_on).await {
                                                    let mut map = metric_overlays_enabled();
                                                    map.insert(ot, !is_on);
                                                    metric_overlays_enabled.set(map);
                                                }
                                            }); },
                                            "{ot.label()}"
                                        }
                                    }
                                }
                            }
                        }

                        // Behavior settings
                        h4 { class: "subsection-title text-muted", "Behavior" }
                        div { class: "settings-row",
                            label { class: "checkbox-label",
                                input {
                                    r#type: "checkbox",
                                    checked: overlay_settings().hide_during_conversations,
                                    onchange: move |e| {
                                        let enabled = e.checked();
                                        spawn(async move {
                                            if let Some(mut cfg) = api::get_config().await {
                                                cfg.overlay_settings.hide_during_conversations = enabled;
                                                let _ = api::update_config(&cfg).await;
                                            }
                                        });
                                    },
                                }
                                span { class: "text-button-style", "Hide during conversations" }
                            }
                        }

                    }

                    // Overlay settings modal
                    if settings_open() {
                        div {
                            class: "modal-backdrop",
                            onclick: move |_| settings_open.set(false),
                            div {
                                onclick: move |e| e.stop_propagation(),
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
                                    on_header_mousedown: move |_| {},
                                }
                            }
                        }
                    }
                }

                // ─────────────────────────────────────────────────────────────
                // Encounter Editor Tab
                // ─────────────────────────────────────────────────────────────
                if active_tab() == "timers" {
                    EncounterEditorPanel {}
                }

                // ─────────────────────────────────────────────────────────────
                // Effects Tab
                // ─────────────────────────────────────────────────────────────
                if active_tab() == "effects" {
                    EffectEditorPanel {}
                }

                // ─────────────────────────────────────────────────────────────
                // Data Explorer Tab
                // ─────────────────────────────────────────────────────────────
                if active_tab() == "explorer" {
                    DataExplorerPanel { show_only_bosses }
                }
            }

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
                                button { class: "btn btn-close", onclick: move |_| general_settings_open.set(false), "X" }
                            }

                            div { class: "settings-section",
                                h4 { "Log Directory" }
                                p { class: "hint", "Select the directory containing your SWTOR combat logs." }
                                div { class: "directory-picker",
                                    div { class: "directory-display",
                                        i { class: "fa-solid fa-folder" }
                                        span { class: "directory-path",
                                            if current_dir.is_empty() { "No directory selected" } else { "{current_dir}" }
                                        }
                                    }
                                    button {
                                        class: "btn btn-browse",
                                        onclick: move |_| { spawn(async move {
                                            if let Some(path) = api::pick_directory("Select Log Directory").await {
                                                log_directory.set(path.clone());
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.log_directory = path;
                                                    api::update_config(&cfg).await;
                                                    // Restart watcher and rebuild index for new directory
                                                    api::restart_watcher().await;
                                                    api::refresh_log_index().await;
                                                    is_watching.set(true);
                                                    // Now fetch updated stats
                                                    log_dir_size.set(api::get_log_directory_size().await);
                                                    log_file_count.set(api::get_log_file_count().await);
                                                }
                                            }
                                        }); },
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

                            div { class: "settings-section",
                                h4 { "Log Management" }
                                {
                                    let count = log_file_count();
                                    let size_mb = log_dir_size() as f64 / 1_000_000.0;
                                    rsx! {
                                        p { class: "hint", "{count} files • {size_mb:.1} MB" }
                                    }
                                }

                                div { class: "setting-row",
                                    label { "Auto-delete empty files" }
                                    input {
                                        r#type: "checkbox",
                                        checked: auto_delete_empty(),
                                        onchange: move |e| {
                                            let checked = e.checked();
                                            auto_delete_empty.set(checked);
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.auto_delete_empty_files = checked;
                                                    api::update_config(&cfg).await;
                                                }
                                            });
                                        }
                                    }
                                }

                                div { class: "setting-row",
                                    label { "Delete old files" }
                                    input {
                                        r#type: "checkbox",
                                        checked: auto_delete_old(),
                                        onchange: move |e| {
                                            let checked = e.checked();
                                            auto_delete_old.set(checked);
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.auto_delete_old_files = checked;
                                                    api::update_config(&cfg).await;
                                                }
                                            });
                                        }
                                    }
                                }

                                div { class: "setting-row",
                                    label { "Retention days" }
                                    input {
                                        r#type: "number",
                                        min: "1",
                                        max: "365",
                                        value: "{retention_days()}",
                                        onchange: move |e| {
                                            if let Ok(days) = e.value().parse::<u32>() {
                                                let days = days.clamp(1, 365);
                                                retention_days.set(days);
                                                spawn(async move {
                                                    if let Some(mut cfg) = api::get_config().await {
                                                        cfg.log_retention_days = days;
                                                        api::update_config(&cfg).await;
                                                    }
                                                });
                                            }
                                        }
                                    }
                                }

                                div { class: "settings-footer",
                                    button {
                                        class: "btn btn-control",
                                        onclick: move |_| {
                                            let del_empty = auto_delete_empty();
                                            let del_old = auto_delete_old();
                                            let days = retention_days();
                                            spawn(async move {
                                                cleanup_status.set("Cleaning...".to_string());
                                                let retention = if del_old { Some(days) } else { None };
                                                let (empty, old) = api::cleanup_logs(del_empty, retention).await;
                                                cleanup_status.set(format!("Deleted {} empty, {} old files", empty, old));
                                                log_dir_size.set(api::get_log_directory_size().await);
                                                log_file_count.set(api::get_log_file_count().await);
                                            });
                                        },
                                        i { class: "fa-solid fa-broom" }
                                        " Clean Now"
                                    }
                                    if !cleanup_status().is_empty() {
                                        span { class: "save-status", "{cleanup_status}" }
                                    }
                                }
                            }

                            div { class: "settings-section",
                                h4 { "Application" }
                                div { class: "setting-row",
                                    label { "Minimize to tray on close" }
                                    input {
                                        r#type: "checkbox",
                                        checked: minimize_to_tray(),
                                        onchange: move |e| {
                                            let checked = e.checked();
                                            minimize_to_tray.set(checked);
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.minimize_to_tray = checked;
                                                    api::update_config(&cfg).await;
                                                }
                                            });
                                        }
                                    }
                                }
                                p { class: "hint", "When enabled, closing the window hides to system tray instead of quitting." }
                            }

                            div { class: "settings-section",
                                h4 { "Global Hotkeys" }
                                p { class: "hint", "Format: Ctrl+Shift+Key (Windows only)" }
                                p { class: "hint hint-warning",
                                    i { class: "fa-solid fa-triangle-exclamation" }
                                    " Restart app after changes."
                                }
                                div { class: "hotkey-grid",
                                    div { class: "setting-row",
                                        label { "Show/Hide" }
                                        input { r#type: "text", class: "hotkey-input", placeholder: "e.g., Ctrl+Shift+O",
                                            value: hotkey_visibility, oninput: move |e| hotkey_visibility.set(e.value()) }
                                    }
                                    div { class: "setting-row",
                                        label { "Move Mode" }
                                        input { r#type: "text", class: "hotkey-input", placeholder: "e.g., Ctrl+Shift+M",
                                            value: hotkey_move_mode, oninput: move |e| hotkey_move_mode.set(e.value()) }
                                    }
                                    div { class: "setting-row",
                                        label { "Rearrange" }
                                        input { r#type: "text", class: "hotkey-input", placeholder: "e.g., Ctrl+Shift+R",
                                            value: hotkey_rearrange, oninput: move |e| hotkey_rearrange.set(e.value()) }
                                    }
                                }
                                div { class: "settings-footer",
                                    button {
                                        class: "btn btn-save",
                                        onclick: move |_| {
                                            let v = hotkey_visibility(); let m = hotkey_move_mode(); let r = hotkey_rearrange();
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.hotkeys.toggle_visibility = if v.is_empty() { None } else { Some(v) };
                                                    cfg.hotkeys.toggle_move_mode = if m.is_empty() { None } else { Some(m) };
                                                    cfg.hotkeys.toggle_rearrange_mode = if r.is_empty() { None } else { Some(r) };
                                                    if api::update_config(&cfg).await {
                                                        hotkey_save_status.set("Saved! Restart to apply.".to_string());
                                                    }
                                                }
                                            });
                                        },
                                        "Save Hotkeys"
                                    }
                                    span { class: "save-status", "{hotkey_save_status}" }
                                }
                            }

                            div { class: "settings-section",
                                h4 { "Audio" }
                                p { class: "hint", "TTS audio for timer countdowns and alerts." }

                                div { class: "setting-row",
                                    label { "Enable Audio" }
                                    input {
                                        r#type: "checkbox",
                                        checked: audio_enabled(),
                                        onchange: move |e| {
                                            let checked = e.checked();
                                            audio_enabled.set(checked);
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.audio.enabled = checked;
                                                    api::update_config(&cfg).await;
                                                }
                                            });
                                        }
                                    }
                                }

                                div { class: "setting-row",
                                    label { "Volume" }
                                    input {
                                        r#type: "range",
                                        min: "0",
                                        max: "100",
                                        value: "{audio_volume()}",
                                        disabled: !audio_enabled(),
                                        oninput: move |e| {
                                            if let Ok(val) = e.value().parse::<u8>() {
                                                audio_volume.set(val);
                                                spawn(async move {
                                                    if let Some(mut cfg) = api::get_config().await {
                                                        cfg.audio.volume = val;
                                                        api::update_config(&cfg).await;
                                                    }
                                                });
                                            }
                                        }
                                    }
                                    span { class: "value", "{audio_volume()}%" }
                                }

                                div { class: "setting-row",
                                    label { "Countdown Audio" }
                                    input {
                                        r#type: "checkbox",
                                        checked: audio_countdown_enabled(),
                                        disabled: !audio_enabled(),
                                        onchange: move |e| {
                                            let checked = e.checked();
                                            audio_countdown_enabled.set(checked);
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.audio.countdown_enabled = checked;
                                                    api::update_config(&cfg).await;
                                                }
                                            });
                                        }
                                    }
                                }

                                div { class: "setting-row",
                                    label { "Alert Audio" }
                                    input {
                                        r#type: "checkbox",
                                        checked: audio_alerts_enabled(),
                                        disabled: !audio_enabled(),
                                        onchange: move |e| {
                                            let checked = e.checked();
                                            audio_alerts_enabled.set(checked);
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.audio.alerts_enabled = checked;
                                                    api::update_config(&cfg).await;
                                                }
                                            });
                                        }
                                    }
                                }

                                p { class: "hint hint-subtle", "Countdowns speak timer name + seconds (e.g., \"Shield 3... 2... 1...\")" }
                            }

                            div { class: "settings-section",
                                h4 { "Parsely.io" }
                                p { class: "hint", "Upload logs to parsely.io for leaderboards and detailed analysis." }
                                div { class: "setting-row",
                                    label { "Username" }
                                    input {
                                        r#type: "text",
                                        placeholder: "Optional",
                                        value: parsely_username,
                                        oninput: move |e| parsely_username.set(e.value())
                                    }
                                }
                                div { class: "setting-row",
                                    label { "Password" }
                                    input {
                                        r#type: "password",
                                        placeholder: "Optional",
                                        value: parsely_password,
                                        oninput: move |e| parsely_password.set(e.value())
                                    }
                                }
                                div { class: "setting-row",
                                    label { "Guild" }
                                    input {
                                        r#type: "text",
                                        placeholder: "Optional",
                                        value: parsely_guild,
                                        oninput: move |e| parsely_guild.set(e.value())
                                    }
                                }
                                div { class: "settings-footer",
                                    button {
                                        class: "btn btn-save",
                                        onclick: move |_| {
                                            let u = parsely_username();
                                            let p = parsely_password();
                                            let g = parsely_guild();
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.parsely.username = u;
                                                    cfg.parsely.password = p;
                                                    cfg.parsely.guild = g;
                                                    if api::update_config(&cfg).await {
                                                        parsely_save_status.set("Saved!".to_string());
                                                    }
                                                }
                                            });
                                        },
                                        "Save Parsely Settings"
                                    }
                                    span { class: "save-status", "{parsely_save_status}" }
                                }
                            }
                        }
                    }
                }
            }

            // File browser modal
            if file_browser_open() {
                div {
                    class: "modal-backdrop",
                    onclick: move |_| file_browser_open.set(false),
                    div {
                        class: "file-browser-modal",
                        onclick: move |e| e.stop_propagation(),

                        div { class: "file-browser-header",
                            h3 {
                                i { class: "fa-solid fa-folder-open" }
                                " Log Files"
                            }
                            input {
                                class: "file-browser-search",
                                r#type: "text",
                                placeholder: "Filter by name or date...",
                                value: "{file_browser_filter}",
                                oninput: move |e| file_browser_filter.set(e.value()),
                            }
                            label {
                                class: "file-browser-filter-toggle",
                                title: "Hide files smaller than 1MB",
                                input {
                                    r#type: "checkbox",
                                    checked: hide_small_log_files(),
                                    onchange: move |e| {
                                        let checked = e.checked();
                                        hide_small_log_files.set(checked);
                                        spawn(async move {
                                            if let Some(mut cfg) = api::get_config().await {
                                                cfg.hide_small_log_files = checked;
                                                api::update_config(&cfg).await;
                                            }
                                        });
                                    },
                                }
                                " Hide <1MB"
                            }
                            button {
                                class: "btn btn-close",
                                onclick: move |_| file_browser_open.set(false),
                                "X"
                            }
                        }

                        div { class: "file-browser-list",
                            if log_files().is_empty() {
                                div { class: "file-browser-empty",
                                    i { class: "fa-solid fa-spinner fa-spin" }
                                    " Loading files..."
                                }
                            } else {
                                {
                                    let filter = file_browser_filter().to_lowercase();
                                    let hide_small = hide_small_log_files();
                                    let filtered: Vec<_> = log_files().iter().filter(|f| {
                                        // Size filter: hide files < 1MB if enabled
                                        if hide_small && f.file_size < 1024 * 1024 {
                                            return false;
                                        }
                                        // Text filter
                                        if filter.is_empty() {
                                            return true;
                                        }
                                        let name = f.character_name.as_deref().unwrap_or("").to_lowercase();
                                        let date = f.date.to_lowercase();
                                        name.contains(&filter) || date.contains(&filter)
                                    }).cloned().collect();
                                    rsx! {
                                        for file in filtered.iter() {
                                    {
                                        let path = file.path.clone();
                                        let path_for_upload = file.path.clone();
                                        let char_name = file.character_name.clone().unwrap_or_else(|| "Unknown".to_string());
                                        let date = file.date.clone();
                                        let size_str = if file.file_size >= 1024 * 1024 {
                                            format!("{:.1}mb", file.file_size as f64 / (1024.0 * 1024.0))
                                        } else {
                                            format!("{}kb", file.file_size / 1024)
                                        };
                                        let is_empty = file.is_empty;
                                        let upload_st = upload_status();
                                        let is_uploading = upload_st.as_ref().map(|(p, _, _)| p == &path).unwrap_or(false);
                                        rsx! {
                                            div {
                                                class: if is_empty { "file-item empty" } else { "file-item" },
                                                div { class: "file-info",
                                                    span { class: "file-date", "{date}" }
                                                    div { class: "file-meta",
                                                        span { class: "file-char", "{char_name}" }
                                                        span { class: "file-sep", " • " }
                                                        span { class: "file-size", "{size_str}" }
                                                    }
                                                    // Show upload result for this file
                                                    if let Some((ref p, success, ref msg)) = upload_st {
                                                        if p == &path {
                                                            if success && msg != "Uploading..." {
                                                                // Show clickable link that opens in browser
                                                                {
                                                                    let url = msg.clone();
                                                                    rsx! {
                                                                        button {
                                                                            class: "upload-link",
                                                                            title: "Open in browser",
                                                                            onclick: move |_| {
                                                                                let u = url.clone();
                                                                                spawn(async move {
                                                                                    api::open_url(&u).await;
                                                                                });
                                                                            },
                                                                            i { class: "fa-solid fa-external-link-alt" }
                                                                            " {msg}"
                                                                        }
                                                                    }
                                                                }
                                                            } else if success {
                                                                span { class: "upload-status", "{msg}" }
                                                            } else {
                                                                span { class: "upload-status error", "{msg}" }
                                                            }
                                                        }
                                                    }
                                                }
                                                div { class: "file-actions",
                                                    button {
                                                        class: "btn btn-open",
                                                        disabled: is_empty,
                                                        onclick: move |_| {
                                                            let p = path.clone();
                                                            console::log_1(&format!("[DEBUG] Opening file: {}", p).into());
                                                            file_browser_open.set(false);
                                                            spawn(async move {
                                                                console::log_1(&"[DEBUG] spawn started".into());
                                                                let result = api::open_historical_file(&p).await;
                                                                console::log_1(&format!("[DEBUG] API returned: {}", result).into());
                                                                is_live_tailing.set(false);
                                                            });
                                                        },
                                                        i { class: "fa-solid fa-eye" }
                                                        " Open"
                                                    }
                                                    button {
                                                        class: "btn btn-upload",
                                                        disabled: is_empty || is_uploading,
                                                        title: "Upload to Parsely.io",
                                                        onclick: move |_| {
                                                            let p = path_for_upload.clone();
                                                            upload_status.set(Some((p.clone(), true, "Uploading...".to_string())));
                                                            spawn(async move {
                                                                if let Some(resp) = api::upload_to_parsely(&p).await {
                                                                    if resp.success {
                                                                        let link = resp.link.unwrap_or_default();
                                                                        upload_status.set(Some((p, true, link)));
                                                                    } else {
                                                                        let err = resp.error.unwrap_or_else(|| "Upload failed".to_string());
                                                                        upload_status.set(Some((p, false, err)));
                                                                    }
                                                                } else {
                                                                    upload_status.set(Some((p, false, "Upload failed".to_string())));
                                                                }
                                                            });
                                                        },
                                                        i { class: "fa-solid fa-cloud-arrow-up" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn apply_status(
    status: &OverlayStatus,
    metric_overlays_enabled: &mut Signal<HashMap<MetricType, bool>>,
    personal_enabled: &mut Signal<bool>,
    raid_enabled: &mut Signal<bool>,
    boss_health_enabled: &mut Signal<bool>,
    timers_enabled: &mut Signal<bool>,
    challenges_enabled: &mut Signal<bool>,
    alerts_enabled: &mut Signal<bool>,
    effects_a_enabled: &mut Signal<bool>,
    effects_b_enabled: &mut Signal<bool>,
    cooldowns_enabled: &mut Signal<bool>,
    dot_tracker_enabled: &mut Signal<bool>,
    overlays_visible: &mut Signal<bool>,
    move_mode: &mut Signal<bool>,
    rearrange_mode: &mut Signal<bool>,
) {
    let map: HashMap<MetricType, bool> = MetricType::all()
        .iter()
        .map(|ot| (*ot, status.enabled.contains(&ot.config_key().to_string())))
        .collect();
    metric_overlays_enabled.set(map);
    personal_enabled.set(status.personal_enabled);
    raid_enabled.set(status.raid_enabled);
    boss_health_enabled.set(status.boss_health_enabled);
    timers_enabled.set(status.timers_enabled);
    challenges_enabled.set(status.challenges_enabled);
    alerts_enabled.set(status.alerts_enabled);
    effects_a_enabled.set(status.effects_a_enabled);
    effects_b_enabled.set(status.effects_b_enabled);
    cooldowns_enabled.set(status.cooldowns_enabled);
    dot_tracker_enabled.set(status.dot_tracker_enabled);
    overlays_visible.set(status.overlays_visible);
    move_mode.set(status.move_mode);
    rearrange_mode.set(status.rearrange_mode);
}
