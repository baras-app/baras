#![allow(non_snake_case)]

use dioxus::prelude::*;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api::{self, BossNotesInfo};
use crate::components::{
    DataExplorerPanel, EffectEditorPanel, EncounterEditorPanel, HistoryPanel,
    HotkeyInput, ParselyUploadModal, SettingsPanel, ToastFrame, ToastSeverity, use_parsely_upload,
    use_parsely_upload_provider, use_toast, use_toast_provider,
};
use crate::components::class_icons::{get_class_icon, get_role_icon};
use crate::types::{
    CombatLogSessionState, DataExplorerState, EffectsEditorState, EncounterBuilderState,
    LogFileInfo, MainTab, MetricType, OverlaySettings, OverlayStatus, OverlayType,
    SessionInfo, UiSessionState, UpdateInfo, ViewMode,
};

static CSS: Asset = asset!("/assets/styles.css");
static DATA_EXPLORER_CSS: Asset = asset!("/assets/data-explorer.css");
static LOGO: Asset = asset!("/assets/logo.png");
static FONT: Asset = asset!("/assets/StarJedi.ttf");

// ─────────────────────────────────────────────────────────────────────────────
// App Component
// ─────────────────────────────────────────────────────────────────────────────

pub fn App() -> Element {
    // Initialize toast system at app root
    let _toast_manager = use_toast_provider();
    // Initialize Parsely upload modal at app root
    let _parsely_upload_manager = use_parsely_upload_provider();
    // Get parsely upload manager for use in event handlers
    let mut parsely_upload = use_parsely_upload();

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
    let mut timers_b_enabled = use_signal(|| false);
    let mut challenges_enabled = use_signal(|| false);
    let mut alerts_enabled = use_signal(|| false);
    let mut effects_a_enabled = use_signal(|| false);
    let mut effects_b_enabled = use_signal(|| false);
    let mut cooldowns_enabled = use_signal(|| false);
    let mut dot_tracker_enabled = use_signal(|| false);
    let mut notes_enabled = use_signal(|| false);
    let mut combat_time_enabled = use_signal(|| false);
    let mut overlays_visible = use_signal(|| true);
    let mut move_mode = use_signal(|| false);
    let mut rearrange_mode = use_signal(|| false);
    let mut auto_hidden = use_signal(|| false);

    // Directory and file state
    let mut log_directory = use_signal(String::new);
    let mut active_file = use_signal(String::new);
    let mut is_watching = use_signal(|| false);
    let mut is_live_tailing = use_signal(|| true);
    let mut session_info = use_signal(|| None::<SessionInfo>);
    let mut session_ended = use_signal(|| false); // True when player logged out but data preserved

    // Boss notes selector state
    let mut area_bosses = use_signal(Vec::<BossNotesInfo>::new);
    let mut selected_boss_id = use_signal(|| None::<String>);

    // File browser state
    let mut file_browser_open = use_signal(|| false);
    let mut log_files = use_signal(Vec::<LogFileInfo>::new);
    let mut upload_status = use_signal(HashMap::<String, (bool, String)>::new); // path -> (success, message)
    let mut file_browser_filter = use_signal(String::new);
    let mut hide_small_log_files = use_signal(|| true);

    // UI Session State - unified state that persists across tab switches
    let mut ui_state = use_signal(UiSessionState::default);

    // Other UI state (not part of session persistence)
    let mut settings_open = use_signal(|| false);
    let mut general_settings_open = use_signal(|| false);
    let mut overlay_settings = use_signal(OverlaySettings::default);
    let selected_overlay_tab = use_signal(|| "dps".to_string());

    // Hotkey state
    let mut hotkey_visibility = use_signal(String::new);
    let mut hotkey_move_mode = use_signal(String::new);
    let mut hotkey_rearrange = use_signal(String::new);
    let mut hotkey_save_status = use_signal(String::new);

    // Log management state
    let mut log_dir_size = use_signal(|| 0u64);
    let mut log_file_count = use_signal(|| 0usize);
    let mut auto_delete_empty = use_signal(|| false);
    let mut auto_delete_small = use_signal(|| false);
    let mut auto_delete_old = use_signal(|| false);
    let mut retention_days = use_signal(|| 21u32);
    let mut cleanup_status = use_signal(String::new);

    // Application settings
    let mut minimize_to_tray = use_signal(|| true);
    let mut european_number_format = use_signal(|| false);
    let mut app_version = use_signal(String::new);

    // Update state
    let mut update_available = use_signal(|| None::<UpdateInfo>);
    let mut update_installing = use_signal(|| false);

    // Changelog state
    let mut changelog_open = use_signal(|| false);
    let mut changelog_html = use_signal(String::new);

    // Audio settings
    let mut audio_enabled = use_signal(|| true);
    let mut audio_volume = use_signal(|| 80u8);
    let mut audio_countdown_enabled = use_signal(|| true);
    let mut audio_alerts_enabled = use_signal(|| true);

    // Profile state
    let mut profile_names = use_signal(Vec::<String>::new);
    let mut active_profile = use_signal(|| None::<String>);
    let mut profile_dirty = use_signal(|| false);

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
            auto_delete_small.set(config.auto_delete_small_files);
            auto_delete_old.set(config.auto_delete_old_files);
            retention_days.set(config.log_retention_days);
            hide_small_log_files.set(config.hide_small_log_files);
            minimize_to_tray.set(config.minimize_to_tray);
            european_number_format.set(config.european_number_format);
            parsely_username.set(config.parsely.username);
            parsely_password.set(config.parsely.password);
            parsely_guild.set(config.parsely.guild);
            // Audio settings
            audio_enabled.set(config.audio.enabled);
            audio_volume.set(config.audio.volume);
            audio_countdown_enabled.set(config.audio.countdown_enabled);
            audio_alerts_enabled.set(config.audio.alerts_enabled);
            // UI preferences - now in unified state
            ui_state.write().data_explorer.show_only_bosses = config.show_only_bosses;
            ui_state.write().combat_log.show_ids = config.show_log_ids;
            ui_state.write().european_number_format = config.european_number_format;
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
                &mut timers_b_enabled,
                &mut challenges_enabled,
                &mut alerts_enabled,
                &mut effects_a_enabled,
                &mut effects_b_enabled,
                &mut cooldowns_enabled,
                &mut dot_tracker_enabled,
                &mut notes_enabled,
                &mut combat_time_enabled,
                &mut overlays_visible,
                &mut move_mode,
                &mut rearrange_mode,
                &mut auto_hidden,
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
        // Also fetch initial boss list for current area
        area_bosses.set(api::get_area_bosses_for_notes().await);

        // Listen for updates (no more polling!)
        let closure = Closure::new(move |_event: JsValue| {
            // Use spawn_local for JS callbacks (no Dioxus runtime context available)
            spawn_local(async move {
                let info = api::get_session_info().await;
                let watching = api::get_watching_status().await;
                let tailing = api::is_live_tailing().await;
                // Clear session_ended flag if we have new player data
                if info.as_ref().is_some_and(|i| i.player_name.is_some()) {
                    let _ = session_ended.try_write().map(|mut w| *w = false);
                }
                let _ = session_info.try_write().map(|mut w| *w = info);
                let _ = is_watching.try_write().map(|mut w| *w = watching);
                let _ = is_live_tailing.try_write().map(|mut w| *w = tailing);
                // Refresh boss list for current area (in case area changed)
                let bosses = api::get_area_bosses_for_notes().await;
                let _ = area_bosses.try_write().map(|mut w| *w = bosses);
            });
        });
        api::tauri_listen("session-updated", &closure).await;
        closure.forget();
    });

    // Periodic refresh for live session duration (every 60 seconds)
    use_future(move || async move {
        loop {
            gloo_timers::future::TimeoutFuture::new(60_000).await;
            // Only refresh if live tailing (duration ticks for live sessions)
            if is_live_tailing() {
                spawn_local(async move {
                    let info = api::get_session_info().await;
                    let _ = session_info.try_write().map(|mut w| *w = info);
                });
            }
        }
    });

    // Listen for Parsely upload completion (to update inline UI status)
    use_future(move || async move {
        if let Some(window) = web_sys::window() {
            let closure = Closure::<dyn Fn(web_sys::Event)>::new(move |_event: web_sys::Event| {
                spawn_local(async move {
                    let global = js_sys::global();
                    if let Ok(data) = js_sys::Reflect::get(&global, &"__parsely_upload_result".into()) {
                        if let Some(data_str) = data.as_string() {
                            // Parse format: "path|success|message"
                            let parts: Vec<&str> = data_str.splitn(3, '|').collect();
                            if parts.len() == 3 {
                                let path = parts[0].to_string();
                                let success = parts[1] == "true";
                                let message = parts[2].to_string();
                                let _ = upload_status.try_write().map(|mut w| {
                                    w.insert(path, (success, message));
                                });
                            }
                            // Clear after reading
                            js_sys::Reflect::delete_property(&global, &"__parsely_upload_result".into()).ok();
                        }
                    }
                });
            });
            let _ = window.add_event_listener_with_callback(
                "parsely-upload-complete",
                closure.as_ref().unchecked_ref()
            );
            closure.forget();
        }
    });

    // Listen for session ended (player logged out, data preserved for upload)
    use_future(move || async move {
        let closure = Closure::new(move |_event: JsValue| {
            let _ = session_ended.try_write().map(|mut w| *w = true);
        });
        api::tauri_listen("session-ended", &closure).await;
        closure.forget();
    });

    // Listen for new session started (reset UI state for fresh start)
    use_future(move || async move {
        let closure = Closure::new(move |_event: JsValue| {
            // Reset session-specific state when a new file starts being parsed.
            // Preserves user preferences like show_only_bosses, show_ids, etc.
            let _ = ui_state.try_write().map(|mut w| w.reset_session());
            // Clear the "session ended" flag — a new session starting means
            // the previous logout/empty-file state is no longer relevant.
            let _ = session_ended.try_write().map(|mut w| *w = false);
        });
        api::tauri_listen("new-session-started", &closure).await;
        closure.forget();
    });

    // Listen for app updates
    use_future(move || async move {
        let closure = Closure::new(move |event: JsValue| {
            if let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
                && let Ok(info) = serde_wasm_bindgen::from_value::<UpdateInfo>(payload)
            {
                let _ = update_available.try_write().map(|mut w| *w = Some(info));
            }
        });
        api::tauri_listen("update-available", &closure).await;
        closure.forget();
    });

    // Listen for update failures
    let mut update_failed_toast = use_toast();
    use_future(move || async move {
        let closure = Closure::new(move |event: JsValue| {
            if let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
                && let Some(msg) = payload.as_string()
            {
                update_failed_toast
                    .show(format!("Update failed: {}", msg), ToastSeverity::Critical);
            }
            // Reset installing state so user can retry
            let _ = update_installing.try_write().map(|mut w| *w = false);
        });
        api::tauri_listen("update-failed", &closure).await;
        closure.forget();
    });

    // Listen for hotkeys unavailable (Wayland portal failure)
    let mut hotkeys_toast = use_toast();
    use_future(move || async move {
        let closure = Closure::new(move |event: JsValue| {
            if let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
                && let Some(msg) = payload.as_string()
            {
                hotkeys_toast.show(
                    format!("Global hotkeys unavailable: {}. Configure shortcuts in compositor settings.", msg),
                    ToastSeverity::Normal
                );
            }
        });
        api::tauri_listen("hotkeys-unavailable", &closure).await;
        closure.forget();
    });

    // Listen for overlay status changes (from hotkeys or other sources)
    // This ensures UI buttons stay in sync when overlay state changes
    use_future(move || async move {
        let closure = Closure::new(move |_event: JsValue| {
            // Use spawn_local for JS callbacks (no Dioxus runtime context available)
            spawn_local(async move {
                if let Some(status) = api::get_overlay_status().await {
                    // Use try_write for signals - safe outside Dioxus runtime context
                    let _ = overlays_visible.try_write().map(|mut w| *w = status.overlays_visible);
                    let _ = move_mode.try_write().map(|mut w| *w = status.move_mode);
                    let _ = rearrange_mode.try_write().map(|mut w| *w = status.rearrange_mode);
                    let _ = auto_hidden.try_write().map(|mut w| *w = status.auto_hidden);
                }
            });
        });
        api::tauri_listen("overlay-status-changed", &closure).await;
        closure.forget();
    });

    // Listen for auto-hidden toast events (from hotkey/tray show while auto-hidden)
    use_future(move || async move {
        let closure = Closure::new(move |_event: JsValue| {
            spawn_local(async move {
                let mut toast = use_toast();
                toast.show("Overlays are currently auto-hidden".to_string(), ToastSeverity::Normal);
            });
        });
        api::tauri_listen("overlays-auto-hidden-toast", &closure).await;
        closure.forget();
    });

    // Check for changelog on startup
    use_future(move || async move {
        if let Some(response) = api::get_changelog().await {
            if response.should_show {
                if let Some(html) = response.html {
                    changelog_html.set(html);
                    changelog_open.set(true);
                }
            }
        }
    });

    // ─────────────────────────────────────────────────────────────────────────
    // Computed Values
    // ─────────────────────────────────────────────────────────────────────────

    let enabled_map = metric_overlays_enabled();
    let personal_on = personal_enabled();
    let raid_on = raid_enabled();
    let boss_health_on = boss_health_enabled();
    let timers_on = timers_enabled();
    let timers_b_on = timers_b_enabled();
    let challenges_on = challenges_enabled();
    let alerts_on = alerts_enabled();
    let effects_a_on = effects_a_enabled();
    let effects_b_on = effects_b_enabled();
    let cooldowns_on = cooldowns_enabled();
    let dot_tracker_on = dot_tracker_enabled();
    let notes_on = notes_enabled();
    let combat_time_on = combat_time_enabled();
    let any_enabled = enabled_map.values().any(|&v| v)
        || personal_on
        || raid_on
        || boss_health_on
        || timers_on
        || timers_b_on
        || challenges_on
        || alerts_on
        || effects_a_on
        || effects_b_on
        || cooldowns_on
        || dot_tracker_on
        || notes_on
        || combat_time_on;
    let is_visible = overlays_visible();
    let is_move_mode = move_mode();
    let is_rearrange = rearrange_mode();
    let current_dir = log_directory();
    let watching = is_watching();
    let live_tailing = is_live_tailing();
    let current_file = active_file();

    // Session state for the session tab
    let session = session_info();
    let has_player = session
        .as_ref()
        .map(|s| s.player_name.as_ref().is_some_and(|n| !n.is_empty()))
        .unwrap_or(false);
    let show_empty_state = !has_player;

    // Auto-hide state is provided by the backend as the single source of truth
    let overlays_auto_hidden = auto_hidden();

    // ─────────────────────────────────────────────────────────────────────────
    // Render
    // ─────────────────────────────────────────────────────────────────────────

    rsx! {
        link { rel: "stylesheet", href: CSS }
        link { rel: "stylesheet", href: DATA_EXPLORER_CSS }
        link { rel: "stylesheet", href: "https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css" }
        style { "@font-face {{ font-family: 'StarJedi'; src: url('{FONT}') format('truetype'); font-display: block; font-weight: normal; font-style: normal; }}" }

        main { class: "container",
            // Header
            header { class: "app-header",
                div { class: "header-content",
                    h1 { "BARAS" }
                    img { class: "header-logo", src: LOGO, alt: "BARAS mascot" }
                    div { class: "header-version-group",
                        div { class: "header-links",
                            a {
                                class: "header-link",
                                href: "#",
                                title: "Discord — Questions, bugs, or feedback",
                                onclick: move |e| {
                                    e.prevent_default();
                                    spawn(async move {
                                        api::open_url("https://discord.gg/zmtkYkhSM4").await;
                                    });
                                },
                                i { class: "fa-brands fa-discord" }
                            }
                            a {
                                class: "header-link",
                                href: "#",
                                title: "Documentation & Help",
                                onclick: move |e| {
                                    e.prevent_default();
                                    spawn(async move {
                                        api::open_url("https://baras-app.github.io/features/overview").await;
                                    });
                                },
                                i { class: "fa-solid fa-circle-question" }
                            }
                        }
                        if !app_version().is_empty() {
                            if let Some(ref update) = update_available() {
                                // Update available - show clickable notification
                                button {
                                    class: if update_installing() { "header-version update-available updating" } else { "header-version update-available" },
                                    title: update.notes.as_deref().unwrap_or("Update available"),
                                    disabled: update_installing(),
                                    onclick: move |_| {
                                        update_installing.set(true);
                                        let mut toast = use_toast();
                                        spawn(async move {
                                            if let Err(e) = api::install_update().await {
                                                toast.show(format!("Update failed: {}", e), ToastSeverity::Critical);
                                                update_installing.set(false);
                                            }
                                            // On success, app will restart automatically
                                        });
                                    },
                                    if update_installing() {
                                        i { class: "fa-solid fa-spinner fa-spin" }
                                        " Updating..."
                                    } else {
                                        i { class: "fa-solid fa-arrow-up" }
                                        " Update Available!"
                                    }
                                }
                            } else {
                                // No update - show current version (clickable for changelog)
                                button {
                                    class: "header-version clickable",
                                    title: "View changelog",
                                    onclick: move |_| {
                                        spawn(async move {
                                            if let Some(response) = api::get_changelog().await {
                                                if let Some(html) = response.html {
                                                    changelog_html.set(html);
                                                }
                                            }
                                            changelog_open.set(true);
                                        });
                                    },
                                    "v{app_version}"
                                }
                            }
                        }
                    }
                    p { class: "subtitle", "Battle Analysis and Raid Assessment System" }
                }
                // Session indicator wrapper (resume button on left + indicator box)
                div { class: "header-session-wrapper",
                    // Resume Live button on the left
                    if !live_tailing {
                        button {
                            class: "btn-resume-live",
                            title: "Resume live tailing",
                            onclick: move |_| {
                                let mut toast = use_toast();
                                spawn(async move {
                                    if let Err(err) = api::resume_live_tailing().await {
                                        toast.show(format!("Failed to resume live tailing: {}", err), ToastSeverity::Normal);
                                    } else {
                                        is_live_tailing.set(true);
                                    }
                                });
                            },
                            "Resume Live "
                            i { class: "fa-solid fa-play" }
                        }
                    }
                    // Session indicator box
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
                                .map(|f| {
                                    f.character_name.clone().unwrap_or_else(|| {
                                        // Show different text for historical vs live when no character
                                        if live_tailing { "Waiting for player...".to_string() }
                                        else { "Loading file...".to_string() }
                                    })
                                })
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
                                if selected.is_empty() { return; }
                                let previous = active_profile();
                                active_profile.set(Some(selected.clone()));
                                let mut toast = use_toast();
                                spawn(async move {
                                    if let Err(err) = api::load_profile(&selected).await {
                                        active_profile.set(previous);
                                        toast.show(format!("Failed to load profile: {}", err), ToastSeverity::Normal);
                                    } else {
                                        if let Some(cfg) = api::get_config().await {
                                            overlay_settings.set(cfg.overlay_settings);
                                        }
                                        profile_dirty.set(false);
                                        api::refresh_overlay_settings().await;
                                        if let Some(status) = api::get_overlay_status().await {
                                            apply_status(&status, &mut metric_overlays_enabled, &mut personal_enabled,
                                                &mut raid_enabled, &mut boss_health_enabled, &mut timers_enabled,
                                                &mut timers_b_enabled, &mut challenges_enabled, &mut alerts_enabled,
                                                &mut effects_a_enabled, &mut effects_b_enabled,
                                                &mut cooldowns_enabled, &mut dot_tracker_enabled, &mut notes_enabled,
                                                &mut combat_time_enabled,
                                                &mut overlays_visible, &mut move_mode, &mut rearrange_mode, &mut auto_hidden);
                                        }
                                    }
                                });
                            },
                            for name in profile_names().iter() {
                                option {
                                    value: "{name}",
                                    selected: active_profile().as_deref() == Some(name.as_str()),
                                    "{name}"
                                }
                            }
                        }
                    }
                    div { class: "header-controls-divider" }
                    button {
                        class: if is_visible { "btn btn-header-overlay active" } else { "btn btn-header-overlay" },
                        title: if is_visible { "Hide overlays" } else { "Show overlays" },
                        disabled: !any_enabled,
                        onclick: move |_| {
                            let mut toast = use_toast();
                            spawn(async move {
                                if api::toggle_visibility(is_visible).await {
                                    overlays_visible.set(!is_visible);
                                    if is_visible { move_mode.set(false); }
                                    // If trying to show but auto-hide is active, inform user
                                    if !is_visible && auto_hidden() {
                                        toast.show("Overlays are currently auto-hidden".to_string(), ToastSeverity::Normal);
                                    }
                                }
                            });
                        },
                        i { class: if is_visible { "fa-solid fa-eye" } else { "fa-solid fa-eye-slash" } }
                    }
                    if overlays_auto_hidden {
                        span {
                            class: "auto-hide-indicator",
                            title: "Overlays auto-hidden (not live)",
                            i { class: "fa-solid fa-eye-slash" }
                            " Auto"
                        }
                    }
                    button {
                        class: if is_move_mode { "btn btn-header-overlay active" } else { "btn btn-header-overlay" },
                        title: if is_move_mode { "Lock overlays" } else { "Unlock overlays (move/resize)" },
                        disabled: !is_visible || !any_enabled || is_rearrange || overlays_auto_hidden,
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
                        disabled: !is_visible || !raid_on || is_move_mode || overlays_auto_hidden,
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
                            // Refresh file sizes then fetch files
                            spawn(async move {
                                api::refresh_file_sizes().await;
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
                    class: if ui_state.read().active_tab == MainTab::Session { "tab-btn active" } else { "tab-btn" },
                    onclick: move |_| ui_state.write().active_tab = MainTab::Session,
                    i { class: "fa-solid fa-chart-line" }
                    " Session"
                }
               button {
                    class: if ui_state.read().active_tab == MainTab::DataExplorer { "tab-btn active" } else { "tab-btn" },
                    onclick: move |_| ui_state.write().active_tab = MainTab::DataExplorer,
                    i { class: "fa-solid fa-magnifying-glass-chart" }
                    " Data Explorer"
                }
                button {
                    class: if ui_state.read().active_tab == MainTab::Overlays { "tab-btn active" } else { "tab-btn" },
                    onclick: move |_| ui_state.write().active_tab = MainTab::Overlays,
                    i { class: "fa-solid fa-layer-group" }
                    " Overlays"
                }
                button {
                    class: if ui_state.read().active_tab == MainTab::EncounterBuilder { "tab-btn active" } else { "tab-btn" },
                    onclick: move |_| ui_state.write().active_tab = MainTab::EncounterBuilder,
                    i { class: "fa-solid fa-hammer" }
                    " Encounter Builder"
                }
                button {
                    class: if ui_state.read().active_tab == MainTab::Effects { "tab-btn active" } else { "tab-btn" },
                    onclick: move |_| ui_state.write().active_tab = MainTab::Effects,
                    i { class: "fa-solid fa-heart-pulse" }
                    " Effects"
                }

            }

            // Tab Content
            div { class: "tab-content",
                // ─────────────────────────────────────────────────────────────
                // Session Tab
                // ─────────────────────────────────────────────────────────────
                if ui_state.read().active_tab == MainTab::Session {
                    // Empty states: show when no player data yet (but not if missing_area already detected)
                    if show_empty_state && !session.as_ref().is_some_and(|s| s.missing_area) {
                        if !live_tailing {
                            // Loading a historical file
                            div { class: "session-empty",
                                i { class: "fa-solid fa-spinner fa-spin" }
                                p { "Loading file..." }
                                p { class: "hint", "Reading historical session data" }
                            }
                        } else if log_files().is_empty() {
                            // No log files found - prompt user to configure directory
                            div { class: "session-empty alert",
                                i { class: "fa-solid fa-triangle-exclamation" }
                                p { "No log files found" }
                                p { class: "hint",
                                    "Choose a log directory in "
                                    span {
                                        class: "settings-link",
                                        onclick: move |_| general_settings_open.set(true),
                                        "settings"
                                    }
                                }
                            }
                        } else if watching {
                            // Live: Log file detected but no character data yet
                            div { class: "session-empty",
                                i { class: "fa-solid fa-hourglass-half" }
                                p { "Waiting for player..." }
                                p { class: "hint", "Log file detected, waiting for character login" }
                            }
                        } else {
                            // Not watching - watcher may have stopped
                            div { class: "session-empty",
                                i { class: "fa-solid fa-inbox" }
                                p { "No Active Session" }
                                p { class: "hint", "File watcher is not running" }
                            }
                        }
                    }

                    // Incomplete log file: no AreaEntered event at start of file
                    if session.as_ref().is_some_and(|s| s.missing_area) {
                        div { class: "session-empty alert",
                            i { class: "fa-solid fa-triangle-exclamation" }
                            p { "Incomplete Log File" }
                            p { class: "hint", "No area data found. Boss encounters and timers are unavailable. Relog or toggle combat logging in game settings to start a new log." }
                        }
                    }

                    // Session panel with info - only show when we have player data
                    if let Some(ref info) = session {
                    if has_player && !info.missing_area {
                        section {
                            class: if live_tailing && !session_ended() && !info.stale_session { "session-panel" } else { "session-panel historical" },

                            // Character mismatch warning (corrupted log file)
                            if info.character_mismatch {
                                div { class: "session-mismatch-warning",
                                    i { class: "fa-solid fa-triangle-exclamation" }
                                    " This log file contains multiple characters. Data may be inaccurate. Please relog to start a new session."
                                }
                            }

                            // ── Header row: status + time info + duration badge + combat icon + Parsely ──
                            div { class: "session-toolbar",
                                // Left: session status with inline time
                                div { class: "session-header-left",
                                    if session_ended() || info.stale_session {
                                        h3 { class: "session-header historical",
                                            i { class: "fa-solid fa-clock" }
                                            " Prior Session"
                                            if let Some(ref start) = info.session_start {
                                                span { class: "session-time-inline",
                                                    " — {start}"
                                                }
                                            }
                                        }
                                    } else if live_tailing {
                                        h3 { class: "session-header live",
                                            i { class: "fa-solid fa-circle-play" }
                                            " Live Session"
                                            if let Some(ref start_short) = info.session_start_short {
                                                span { class: "session-time-inline",
                                                    " — since {start_short}"
                                                }
                                            }
                                        }
                                    } else {
                                        h3 { class: "session-header historical",
                                            i { class: "fa-solid fa-circle-pause" }
                                            " Historical"
                                            // Show date range inline
                                            if let (Some(start), Some(end)) = (&info.session_start, &info.session_end) {
                                                span { class: "session-time-inline",
                                                    " — {start} – {end}"
                                                }
                                            } else if let Some(ref start) = info.session_start {
                                                span { class: "session-time-inline",
                                                    " — {start}"
                                                }
                                            }
                                        }
                                    }
                                }

                                // Right: duration badge + combat icon + Parsely
                                div { class: "session-toolbar-right",
                                    // Duration badge (always shown)
                                    if let Some(ref duration) = info.duration_formatted {
                                        span { class: "session-duration-badge",
                                            title: "Session duration",
                                            i { class: "fa-solid fa-stopwatch" }
                                            " {duration}"
                                        }
                                    }

                                    // Combat status icon
                                    if live_tailing && !session_ended() && !info.stale_session {
                                        span {
                                            class: if info.in_combat { "combat-indicator in-combat" } else { "combat-indicator" },
                                            title: if info.in_combat { "In Combat" } else { "Out of Combat" },
                                            if info.in_combat {
                                                i { class: "fa-solid fa-burst" }
                                            } else {
                                                i { class: "fa-solid fa-shield-halved" }
                                            }
                                        }
                                    }

                                    // Parsely upload button
                                    if !current_file.is_empty() {
                                        {
                                            let path = current_file.clone();
                                            let upload_result = upload_status().get(&path).cloned();
                                            rsx! {
                                                div { class: "session-upload-group",
                                                    button {
                                                        class: "btn btn-session-upload",
                                                        title: "Upload to Parsely",
                                                        onclick: {
                                                            let p = path.clone();
                                                            move |_| {
                                                                let filename = p.split('/').last()
                                                                    .or_else(|| p.split('\\').last())
                                                                    .unwrap_or("combat.txt")
                                                                    .to_string();
                                                                parsely_upload.open_file(p.clone(), filename);
                                                            }
                                                        },
                                                        i { class: "fa-solid fa-cloud-arrow-up" }
                                                        " Parsely"
                                                    }
                                                    if let Some((success, ref msg)) = upload_result {
                                                        if success {
                                                            button {
                                                                class: "btn btn-session-upload-result",
                                                                title: "Open in browser",
                                                                onclick: {
                                                                    let url = msg.clone();
                                                                    move |_| {
                                                                        let u = url.clone();
                                                                        spawn(async move { api::open_url(&u).await; });
                                                                    }
                                                                },
                                                                i { class: "fa-solid fa-external-link-alt" }
                                                            }
                                                        } else {
                                                            span { class: "upload-error", title: "{msg}",
                                                                i { class: "fa-solid fa-triangle-exclamation" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // ── Player identity row: icons + name + class/discipline ──
                            div { class: "session-identity",
                                // Role icon
                                if let Some(ref role_name) = info.role_icon {
                                    if let Some(role_asset) = get_role_icon(role_name) {
                                        img { class: "session-role-icon", src: *role_asset, title: "Role" }
                                    }
                                }
                                // Discipline icon
                                if let Some(ref icon_name) = info.class_icon {
                                    if let Some(icon_asset) = get_class_icon(icon_name) {
                                        img {
                                            class: "session-class-icon",
                                            src: *icon_asset,
                                            title: "{info.player_discipline.as_deref().unwrap_or(\"\")}",
                                        }
                                    }
                                }
                                // Player name and class/discipline text
                                div { class: "session-identity-text",
                                    if let Some(ref name) = info.player_name {
                                        span { class: "session-player-name", "{name}" }
                                    }
                                    {
                                        let class_str = info.player_class.as_deref().unwrap_or("");
                                        let disc_str = info.player_discipline.as_deref().unwrap_or("");
                                        let detail = if !class_str.is_empty() && !disc_str.is_empty() {
                                            format!("{class_str} — {disc_str}")
                                        } else if !class_str.is_empty() {
                                            class_str.to_string()
                                        } else if !disc_str.is_empty() {
                                            disc_str.to_string()
                                        } else {
                                            String::new()
                                        };
                                        if !detail.is_empty() {
                                            rsx! {
                                                span { class: "session-player-detail", "{detail}" }
                                            }
                                        } else {
                                            rsx! {}
                                        }
                                    }
                                }
                            }

                            // ── Settings row: alacrity/latency + notes selector ──
                            div { class: "session-settings-row",
                                // Left: player stats (alacrity / latency)
                                PlayerStatsBar {}

                                // Divider
                                span { class: "session-settings-divider" }

                                // Right: boss notes selector (always visible)
                                {
                                    let bosses_with_notes: Vec<_> = area_bosses().iter()
                                        .filter(|b| b.has_notes)
                                        .cloned()
                                        .collect();
                                    rsx! {
                                        div { class: "session-notes-selector",
                                            span { class: "label",
                                                i { class: "fa-solid fa-note-sticky" }
                                                " Notes:"
                                            }
                                            if bosses_with_notes.is_empty() {
                                                select {
                                                    class: "notes-boss-select",
                                                    disabled: true,
                                                    option { value: "", "No notes available" }
                                                }
                                            } else {
                                                select {
                                                    class: "notes-boss-select",
                                                    onchange: move |evt| {
                                                        let boss_id = evt.value();
                                                        if !boss_id.is_empty() {
                                                            selected_boss_id.set(Some(boss_id.clone()));
                                                            spawn(async move {
                                                                let _ = api::select_boss_notes(&boss_id).await;
                                                            });
                                                        }
                                                    },
                                                    option { value: "", "Select boss..." }
                                                    for boss in bosses_with_notes.iter() {
                                                        option {
                                                            value: "{boss.id}",
                                                            selected: selected_boss_id().as_ref() == Some(&boss.id),
                                                            "{boss.name}"
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

                    div { class: "history-container-large", HistoryPanel { state: ui_state } }
                }

                // ─────────────────────────────────────────────────────────────
                // Overlays Tab
                // ─────────────────────────────────────────────────────────────
                if ui_state.read().active_tab == MainTab::Overlays {
                    section { class: "overlay-controls",
                        // Profile unsaved changes indicator
                        if profile_dirty() && active_profile().is_some() {
                            div { class: "profile-unsaved-indicator",
                                i { class: "fa-solid fa-circle-exclamation" }
                                span { " Changes not saved to profile" }
                            }
                        }
                        // Top bar: Customize button + Profile selector
                        div { class: "overlays-top-bar",
                            button {
                                class: "btn btn-customize",
                                title: "Open overlay appearance and behavior settings",
                                onclick: move |_| settings_open.set(!settings_open()),
                                i { class: "fa-solid fa-screwdriver-wrench" }
                                span { " Customize" }
                            }
                            div { class: "overlay-auto-hide-toggles",
                                label {
                                    class: "toggle-switch-label",
                                    title: "Automatically hide overlays when viewing historical files or when logged out",
                                    span { class: "toggle-switch",
                                        input {
                                            r#type: "checkbox",
                                            checked: overlay_settings().hide_when_not_live,
                                            onchange: move |e| {
                                                let enabled = e.checked();
                                                let mut toast = use_toast();
                                                spawn(async move {
                                                    if let Some(mut cfg) = api::get_config().await {
                                                        cfg.overlay_settings.hide_when_not_live = enabled;
                                                        if let Err(err) = api::update_config(&cfg).await {
                                                            toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                        } else {
                                                            overlay_settings.set(cfg.overlay_settings);
                                                            api::apply_not_live_auto_hide().await;
                                                        }
                                                    }
                                                });
                                            },
                                        }
                                        span { class: "toggle-slider" }
                                    }
                                    span { class: "toggle-text", "Auto-hide when not live" }
                                }
                                label {
                                    class: "toggle-switch-label",
                                    title: "Automatically hide overlays during in-game conversations",
                                    span { class: "toggle-switch",
                                        input {
                                            r#type: "checkbox",
                                            checked: overlay_settings().hide_during_conversations,
                                            onchange: move |e| {
                                                let enabled = e.checked();
                                                let mut toast = use_toast();
                                                spawn(async move {
                                                    if let Some(mut cfg) = api::get_config().await {
                                                        cfg.overlay_settings.hide_during_conversations = enabled;
                                                        if let Err(err) = api::update_config(&cfg).await {
                                                            toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                        } else {
                                                            overlay_settings.set(cfg.overlay_settings);
                                                        }
                                                    }
                                                });
                                            },
                                        }
                                        span { class: "toggle-slider" }
                                    }
                                    span { class: "toggle-text", "Hide in conversations" }
                                }
                            }
                            div { class: "profile-selector",
                                if profile_names().is_empty() {
                                    // Empty state: no profiles exist
                                    span { class: "profile-label", "Profile:" }
                                    span { class: "profile-current", "Default" }
                                    button {
                                        class: "btn-create-profile",
                                        title: "Save current settings as a profile",
                                        onclick: move |_| {
                                            let mut toast = use_toast();
                                            spawn(async move {
                                                // Generate a unique profile name
                                                let name = "Profile 1".to_string();
                                                match api::save_profile(&name).await {
                                                    Err(err) => {
                                                        toast.show(format!("Failed to create profile: {}", err), ToastSeverity::Normal);
                                                    }
                                                    Ok(_) => {
                                                        // Refresh profile list and set as active
                                                        let names = api::get_profile_names().await;
                                                        profile_names.set(names);
                                                        active_profile.set(Some(name));
                                                        profile_dirty.set(false);
                                                    }
                                                }
                                            });
                                        },
                                        i { class: "fa-solid fa-plus" }
                                        " Save as Profile"
                                    }
                                } else {
                                    // Profiles exist: show dropdown
                                    span { class: "profile-label", "Profiles:" }
                                    select {
                                        class: "profile-dropdown",
                                        value: active_profile().unwrap_or_default(),
                                        onchange: move |e| {
                                            let selected = e.value();
                                            if selected.is_empty() { return; }
                                            let previous = active_profile();
                                            active_profile.set(Some(selected.clone()));
                                            let mut toast = use_toast();
                                            spawn(async move {
                                                if let Err(err) = api::load_profile(&selected).await {
                                                    active_profile.set(previous);
                                                    toast.show(format!("Failed to load profile: {}", err), ToastSeverity::Normal);
                                                } else {
                                                    if let Some(cfg) = api::get_config().await {
                                                        overlay_settings.set(cfg.overlay_settings);
                                                    }
                                                    profile_dirty.set(false);
                                                    api::refresh_overlay_settings().await;
                                                    if let Some(status) = api::get_overlay_status().await {
                                                        apply_status(&status, &mut metric_overlays_enabled, &mut personal_enabled,
                                                            &mut raid_enabled, &mut boss_health_enabled, &mut timers_enabled,
                                                            &mut timers_b_enabled, &mut challenges_enabled, &mut alerts_enabled,
                                                            &mut effects_a_enabled, &mut effects_b_enabled,
                                                            &mut cooldowns_enabled, &mut dot_tracker_enabled, &mut notes_enabled,
                                                            &mut combat_time_enabled,
                                                            &mut overlays_visible, &mut move_mode, &mut rearrange_mode, &mut auto_hidden);
                                                    }
                                                }
                                            });
                                        },
                                        for name in profile_names().iter() {
                                            option {
                                                value: "{name}",
                                                selected: active_profile().as_deref() == Some(name.as_str()),
                                                "{name}"
                                            }
                                        }
                                    }
                                    if active_profile().is_some() {
                                        button {
                                            class: "profile-save-btn",
                                            title: "Save to profile",
                                            onclick: move |_| {
                                                if let Some(ref name) = active_profile() {
                                                    let n = name.clone();
                                                    let mut toast = use_toast();
                                                    spawn(async move {
                                                        if let Err(err) = api::save_profile(&n).await {
                                                            toast.show(format!("Failed to save profile: {}", err), ToastSeverity::Normal);
                                                        } else {
                                                            profile_dirty.set(false);
                                                        }
                                                    });
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
                                onclick: move |_| {
                                    let mut toast = use_toast();
                                    spawn(async move {
                                        if api::toggle_visibility(is_visible).await {
                                            overlays_visible.set(!is_visible);
                                            if is_visible { move_mode.set(false); }
                                            if !is_visible && auto_hidden() {
                                                toast.show("Overlays are currently auto-hidden".to_string(), ToastSeverity::Normal);
                                            }
                                        }
                                    });
                                },
                                if is_visible { i { class: "fa-solid fa-eye" } span { " Visible" } }
                                else { i { class: "fa-solid fa-eye-slash" } span { " Hidden" } }
                            }
                            button {
                                class: if is_move_mode { "btn btn-control btn-unlocked" } else { "btn btn-control btn-locked" },
                                disabled: !is_visible || !any_enabled || is_rearrange || overlays_auto_hidden,
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
                                disabled: !is_visible || !raid_on || is_move_mode || overlays_auto_hidden,
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

                        // Overlay categories in columns
                        div { class: "overlay-categories",
                            // General column
                            div { class: "overlay-category",
                                h4 { class: "category-title", "General" }
                                div { class: "category-buttons",
                                    button {
                                        class: if personal_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                        title: "Shows your personal combat statistics",
                                        onclick: move |_| { spawn(async move {
                                            if api::toggle_overlay(OverlayType::Personal, personal_on).await {
                                                personal_enabled.set(!personal_on);
                                                profile_dirty.set(true);
                                            }
                                        }); },
                                        "Personal Stats"
                                    }
                                    button {
                                        class: if raid_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                        title: "Displays party/raid member health bars with effect tracking",
                                        onclick: move |_| { spawn(async move {
                                            if api::toggle_overlay(OverlayType::Raid, raid_on).await {
                                                raid_enabled.set(!raid_on);
                                                if raid_on { rearrange_mode.set(false); }
                                                profile_dirty.set(true);
                                            }
                                        }); },
                                        "Raid Frames"
                                    }
                                    button {
                                        class: if alerts_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                        title: "Shows combat alerts and notifications",
                                        onclick: move |_| { spawn(async move {
                                            if api::toggle_overlay(OverlayType::Alerts, alerts_on).await {
                                                alerts_enabled.set(!alerts_on);
                                                profile_dirty.set(true);
                                            }
                                        }); },
                                        "Alerts"
                                    }
                                    button {
                                        class: if combat_time_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                        title: "Displays the current encounter combat time",
                                        onclick: move |_| { spawn(async move {
                                            if api::toggle_overlay(OverlayType::CombatTime, combat_time_on).await {
                                                combat_time_enabled.set(!combat_time_on);
                                                profile_dirty.set(true);
                                            }
                                        }); },
                                        "Combat Time"
                                    }
                                }
                            }

                            // Encounter column
                            div { class: "overlay-category",
                                h4 { class: "category-title", "Encounter" }
                                div { class: "category-buttons",
                                    button {
                                        class: if boss_health_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                        title: "Shows boss health bars and cast timers",
                                        onclick: move |_| { spawn(async move {
                                            if api::toggle_overlay(OverlayType::BossHealth, boss_health_on).await {
                                                boss_health_enabled.set(!boss_health_on);
                                                profile_dirty.set(true);
                                            }
                                        }); },
                                        "Boss Health"
                                    }
                                    button {
                                        class: if challenges_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                        title: "Tracks raid challenge objectives and progress",
                                        onclick: move |_| { spawn(async move {
                                            if api::toggle_overlay(OverlayType::Challenges, challenges_on).await {
                                                challenges_enabled.set(!challenges_on);
                                                profile_dirty.set(true);
                                            }
                                        }); },
                                        "Challenges"
                                    }
                                    button {
                                        class: if timers_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                        title: "Displays encounter-specific timers and phase markers (Group A)",
                                        onclick: move |_| { spawn(async move {
                                            if api::toggle_overlay(OverlayType::TimersA, timers_on).await {
                                                timers_enabled.set(!timers_on);
                                                profile_dirty.set(true);
                                            }
                                        }); },
                                        "Timers A"
                                    }
                                    button {
                                        class: if timers_b_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                        title: "Displays encounter-specific timers and phase markers (Group B)",
                                        onclick: move |_| { spawn(async move {
                                            if api::toggle_overlay(OverlayType::TimersB, timers_b_on).await {
                                                timers_b_enabled.set(!timers_b_on);
                                                profile_dirty.set(true);
                                            }
                                        }); },
                                        "Timers B"
                                    }
                                    button {
                                        class: if notes_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                        title: "Displays encounter notes written in the Encounter Editor",
                                        onclick: move |_| { spawn(async move {
                                            if api::toggle_overlay(OverlayType::Notes, notes_on).await {
                                                notes_enabled.set(!notes_on);
                                                profile_dirty.set(true);
                                            }
                                        }); },
                                        "Notes"
                                    }
                                }
                            }

                            // Effects column
                            div { class: "overlay-category",
                                h4 { class: "category-title", "Effects" }
                                div { class: "category-buttons",
                                    button {
                                        class: if effects_a_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                        title: "Displays tracked buffs and effects (Group A)",
                                        onclick: move |_| { spawn(async move {
                                            if api::toggle_overlay(OverlayType::EffectsA, effects_a_on).await {
                                                effects_a_enabled.set(!effects_a_on);
                                                profile_dirty.set(true);
                                            }
                                        }); },
                                        "Effects A"
                                    }
                                    button {
                                        class: if effects_b_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                        title: "Displays tracked buffs and effects (Group B)",
                                        onclick: move |_| { spawn(async move {
                                            if api::toggle_overlay(OverlayType::EffectsB, effects_b_on).await {
                                                effects_b_enabled.set(!effects_b_on);
                                                profile_dirty.set(true);
                                            }
                                        }); },
                                        "Effects B"
                                    }
                                    button {
                                        class: if cooldowns_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                        title: "Tracks ability cooldowns",
                                        onclick: move |_| { spawn(async move {
                                            if api::toggle_overlay(OverlayType::Cooldowns, cooldowns_on).await {
                                                cooldowns_enabled.set(!cooldowns_on);
                                                profile_dirty.set(true);
                                            }
                                        }); },
                                        "Cooldowns"
                                    }
                                    button {
                                        class: if dot_tracker_on { "btn btn-overlay btn-active" } else { "btn btn-overlay" },
                                        title: "Tracks damage-over-time effects on targets",
                                        onclick: move |_| { spawn(async move {
                                            if api::toggle_overlay(OverlayType::DotTracker, dot_tracker_on).await {
                                                dot_tracker_enabled.set(!dot_tracker_on);
                                                profile_dirty.set(true);
                                            }
                                        }); },
                                        "DOT Tracker"
                                    }
                                }
                            }

                            // Metrics column
                            div { class: "overlay-category",
                                h4 { class: "category-title", "Metrics" }
                                div { class: "category-buttons",
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
                                                            profile_dirty.set(true);
                                                        }
                                                    }); },
                                                    "{ot.label()}"
                                                }
                                            }
                                        }
                                    }
                                }
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
                                    profile_dirty: profile_dirty,
                                    on_close: move |_| settings_open.set(false),
                                    on_header_mousedown: move |_| {},
                                    on_settings_saved: move |_| profile_dirty.set(true),
                                }
                            }
                        }
                    }
                }

                // ─────────────────────────────────────────────────────────────
                // Encounter Editor Tab
                // ─────────────────────────────────────────────────────────────
                if ui_state.read().active_tab == MainTab::EncounterBuilder {
                    EncounterEditorPanel {
                        state: ui_state,
                    }
                }

                // ─────────────────────────────────────────────────────────────
                // Effects Tab
                // ─────────────────────────────────────────────────────────────
                if ui_state.read().active_tab == MainTab::Effects {
                    EffectEditorPanel {
                        state: ui_state,
                    }
                }

                // ─────────────────────────────────────────────────────────────
                // Data Explorer Tab
                // ─────────────────────────────────────────────────────────────
                if ui_state.read().active_tab == MainTab::DataExplorer {
                    DataExplorerPanel {
                        state: ui_state,
                    }
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

                            div { class: "settings-content",
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
                                        onclick: move |_| {
                                            let mut toast = use_toast();
                                            spawn(async move {
                                                if let Some(path) = api::pick_log_directory().await {
                                                    log_directory.set(path.clone());
                                                    if let Some(mut cfg) = api::get_config().await {
                                                        cfg.log_directory = path;
                                                        if let Err(err) = api::update_config(&cfg).await {
                                                            toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                        } else {
                                                            // Restart watcher and rebuild index for new directory
                                                            api::restart_watcher().await;
                                                            api::refresh_log_index().await;
                                                            is_watching.set(true);
                                                            // Now fetch updated stats
                                                            log_dir_size.set(api::get_log_directory_size().await);
                                                            log_file_count.set(api::get_log_file_count().await);
                                                        }
                                                    }
                                                }
                                            });
                                        },
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
                                            let mut toast = use_toast();
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.auto_delete_empty_files = checked;
                                                    if let Err(err) = api::update_config(&cfg).await {
                                                        toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                    }
                                                }
                                            });
                                        }
                                    }
                                }

                                div { class: "setting-row",
                                    label { "Auto-delete small files (<1MB)" }
                                    input {
                                        r#type: "checkbox",
                                        checked: auto_delete_small(),
                                        onchange: move |e| {
                                            let checked = e.checked();
                                            auto_delete_small.set(checked);
                                            let mut toast = use_toast();
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.auto_delete_small_files = checked;
                                                    if let Err(err) = api::update_config(&cfg).await {
                                                        toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                    }
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
                                            let mut toast = use_toast();
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.auto_delete_old_files = checked;
                                                    if let Err(err) = api::update_config(&cfg).await {
                                                        toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                    }
                                                }
                                            });
                                        }
                                    }
                                }

                                div { class: "setting-row",
                                    label { "Retention days" }
                                    input {
                                        r#type: "number",
                                        min: "7",
                                        max: "365",
                                        value: "{retention_days()}",
                                        onchange: move |e| {
                                            if let Ok(days) = e.value().parse::<u32>() {
                                                let days = days.clamp(7, 365);
                                                retention_days.set(days);
                                                let mut toast = use_toast();
                                                spawn(async move {
                                                    if let Some(mut cfg) = api::get_config().await {
                                                        cfg.log_retention_days = days;
                                                        if let Err(err) = api::update_config(&cfg).await {
                                                            toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                        }
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
                                            let del_small = auto_delete_small();
                                            let del_old = auto_delete_old();
                                            let days = retention_days();
                                            spawn(async move {
                                                cleanup_status.set("Cleaning...".to_string());
                                                let retention = if del_old { Some(days) } else { None };
                                                let (empty, small, old) = api::cleanup_logs(del_empty, del_small, retention).await;
                                                cleanup_status.set(format!("Deleted {} empty, {} small, {} old files", empty, small, old));
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
                                            let mut toast = use_toast();
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.minimize_to_tray = checked;
                                                    if let Err(err) = api::update_config(&cfg).await {
                                                        toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                    }
                                                }
                                            });
                                        }
                                    }
                                }
                                p { class: "hint", "When enabled, closing the window hides to system tray instead of quitting." }
                                div { class: "setting-row",
                                    label { "European number format" }
                                    input {
                                        r#type: "checkbox",
                                        checked: european_number_format(),
                                        onchange: move |e| {
                                            let checked = e.checked();
                                            european_number_format.set(checked);
                                            ui_state.write().european_number_format = checked;
                                            let mut toast = use_toast();
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.european_number_format = checked;
                                                    if let Err(err) = api::update_config(&cfg).await {
                                                        toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                    } else {
                                                        api::refresh_overlay_settings().await;
                                                    }
                                                }
                                            });
                                        }
                                    }
                                }
                                p { class: "hint", "Swap decimal point and thousands separator (e.g., 1.50K becomes 1,50K)." }
                                p { class: "hint hint-warning",
                                    i { class: "fa-solid fa-triangle-exclamation" }
                                    strong { " Editor inputs still use '.' for decimals." }
                                }
                            }

                            div { class: "settings-section",
                                h4 { "Global Hotkeys" }
                                p { class: "hint", "Click to capture a key combination. Backspace to clear." }
                                p { class: "hint hint-warning",
                                    i { class: "fa-solid fa-triangle-exclamation" }
                                    " Linux Wayland: Experimental, requires compositor support for freedesktop global shortcuts portal."
                                }
                                p { class: "hint hint-warning",
                                    i { class: "fa-solid fa-info-circle" }
                                    " Restart app after changes."
                                }
                                div { class: "hotkey-grid",
                                    div { class: "setting-row",
                                        label { "Show/Hide" }
                                        HotkeyInput {
                                            value: hotkey_visibility(),
                                            on_change: move |v| hotkey_visibility.set(v),
                                        }
                                    }
                                    div { class: "setting-row",
                                        label { "Move Mode" }
                                        HotkeyInput {
                                            value: hotkey_move_mode(),
                                            on_change: move |v| hotkey_move_mode.set(v),
                                        }
                                    }
                                    div { class: "setting-row",
                                        label { "Rearrange" }
                                        HotkeyInput {
                                            value: hotkey_rearrange(),
                                            on_change: move |v| hotkey_rearrange.set(v),
                                        }
                                    }
                                }
                                div { class: "settings-footer",
                                    button {
                                        class: "btn btn-save",
                                        onclick: move |_| {
                                            let v = hotkey_visibility(); let m = hotkey_move_mode(); let r = hotkey_rearrange();
                                            let mut toast = use_toast();
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.hotkeys.toggle_visibility = if v.is_empty() { None } else { Some(v) };
                                                    cfg.hotkeys.toggle_move_mode = if m.is_empty() { None } else { Some(m) };
                                                    cfg.hotkeys.toggle_rearrange_mode = if r.is_empty() { None } else { Some(r) };
                                                    if let Err(err) = api::update_config(&cfg).await {
                                                        toast.show(format!("Failed to save hotkeys: {}", err), ToastSeverity::Normal);
                                                    } else {
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
                                            let mut toast = use_toast();
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.audio.enabled = checked;
                                                    if let Err(err) = api::update_config(&cfg).await {
                                                        toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                    }
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
                                                let mut toast = use_toast();
                                                spawn(async move {
                                                    if let Some(mut cfg) = api::get_config().await {
                                                        cfg.audio.volume = val;
                                                        if let Err(err) = api::update_config(&cfg).await {
                                                            toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                        }
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
                                            let mut toast = use_toast();
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.audio.countdown_enabled = checked;
                                                    if let Err(err) = api::update_config(&cfg).await {
                                                        toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                    }
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
                                            let mut toast = use_toast();
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.audio.alerts_enabled = checked;
                                                    if let Err(err) = api::update_config(&cfg).await {
                                                        toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                    }
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
                                            let mut toast = use_toast();
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.parsely.username = u;
                                                    cfg.parsely.password = p;
                                                    cfg.parsely.guild = g;
                                                    if let Err(err) = api::update_config(&cfg).await {
                                                        toast.show(format!("Failed to save Parsely settings: {}", err), ToastSeverity::Normal);
                                                    } else {
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

                            StarParseImportSection {}
                            } // settings-content
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
                                placeholder: "Filter by name, date, day, or operation...",
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
                                        let mut toast = use_toast();
                                        spawn(async move {
                                            if let Some(mut cfg) = api::get_config().await {
                                                cfg.hide_small_log_files = checked;
                                                if let Err(err) = api::update_config(&cfg).await {
                                                    toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                }
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
                                        let day = f.day_of_week.to_lowercase();
                                        // Also check area names for operation search
                                        let areas_match = f.areas.as_ref().map_or(false, |areas| {
                                            areas.iter().any(|a| {
                                                a.display.to_lowercase().contains(&filter)
                                                    || a.area_name.to_lowercase().contains(&filter)
                                            })
                                        });
                                        name.contains(&filter) || date.contains(&filter) || day.contains(&filter) || areas_match
                                    }).cloned().collect();
                                    rsx! {
                                        for file in filtered.iter() {
                                    {
                                        let path = file.path.clone();
                                        let path_for_upload = file.path.clone();
                                        let char_name = file.character_name.clone().unwrap_or_else(|| "Unknown".to_string());
                                        let date = file.date.clone();
                                        let day_of_week = file.day_of_week.clone();
                                        let size_str = if file.file_size >= 1024 * 1024 {
                                            format!("{:.1}mb", file.file_size as f64 / (1024.0 * 1024.0))
                                        } else {
                                            format!("{}kb", file.file_size / 1024)
                                        };
                                        let is_empty = file.is_empty;
                                        let is_current = path == current_file;
                                        let upload_result = upload_status().get(&path).cloned();
                                        rsx! {
                                            div {
                                                class: match (is_empty, is_current) {
                                                    (true, true) => "file-item empty current",
                                                    (true, false) => "file-item empty",
                                                    (false, true) => "file-item current",
                                                    (false, false) => "file-item",
                                                },
                                                div { class: "file-info",
                                                    span { class: "file-date",
                                                        "{date}"
                                                        if !day_of_week.is_empty() {
                                                            span { class: "file-day", " - {day_of_week}" }
                                                        }
                                                    }
                                                    div { class: "file-meta",
                                                        span { class: "file-char", "{char_name}" }
                                                        span { class: "file-sep", " • " }
                                                        span { class: "file-size", "{size_str}" }
                                                    }
                                                    // Show areas/operations visited in this file
                                                    // Only show areas with difficulty (actual instances, not open world)
                                                    {
                                                        // Helper to get difficulty CSS class from difficulty string
                                                        fn difficulty_class(difficulty: &str) -> &'static str {
                                                            let lower = difficulty.to_lowercase();
                                                            // 4-player content (flashpoints) gets blue
                                                            if lower.contains("4 player") {
                                                                "area-tag diff-fp"
                                                            // 8/16 player operations get colored by difficulty
                                                            } else if lower.contains("master") {
                                                                "area-tag diff-nim"
                                                            } else if lower.contains("veteran") {
                                                                "area-tag diff-hm"
                                                            } else if lower.contains("story") {
                                                                "area-tag diff-sm"
                                                            } else {
                                                                "area-tag"
                                                            }
                                                        }

                                                        let instanced_areas: Vec<_> = file.areas.as_ref()
                                                            .map(|areas| areas.iter().filter(|a| !a.difficulty.is_empty()).collect())
                                                            .unwrap_or_default();
                                                        let area_count = instanced_areas.len();
                                                        rsx! {
                                                            if !instanced_areas.is_empty() {
                                                                details { class: "file-areas",
                                                                    summary {
                                                                        // Show first 3 badges in summary
                                                                        for area in instanced_areas.iter().take(3) {
                                                                            span { class: "{difficulty_class(&area.difficulty)}", "{area.display}" }
                                                                        }
                                                                        // Show "+N more" if there are more than 3
                                                                        if area_count > 3 {
                                                                            span { class: "area-tag area-more", "+{area_count - 3} more" }
                                                                        }
                                                                    }
                                                                    // When expanded, show all areas
                                                                    if area_count > 3 {
                                                                        ul { class: "area-list",
                                                                            for area in instanced_areas.iter() {
                                                                                li { class: "{difficulty_class(&area.difficulty)}", "{area.display}" }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                    // Show upload result for this file
                                                    if let Some((success, ref msg)) = upload_result {
                                                        if success {
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
                                                        } else {
                                                            span { class: "upload-status error", "{msg}" }
                                                        }
                                                    }
                                                }
                                                div { class: "file-actions",
                                                    button {
                                                        class: "btn btn-open",
                                                        disabled: is_empty,
                                                        onclick: move |_| {
                                                            let p = path.clone();
                                                            let mut toast = use_toast();
                                                            file_browser_open.set(false);
                                                            spawn(async move {
                                                                if let Err(err) = api::open_historical_file(&p).await {
                                                                    toast.show(format!("Failed to open log file: {}", err), ToastSeverity::Normal);
                                                                } else {
                                                                    is_live_tailing.set(false);
                                                                }
                                                            });
                                                        },
                                                        i { class: "fa-solid fa-eye" }
                                                        " Open"
                                                    }
                                                    button {
                                                        class: "btn btn-upload",
                                                        disabled: is_empty,
                                                        title: "Upload to Parsely.io",
                                                        onclick: {
                                                            let p = path_for_upload.clone();
                                                            let display_name = format!("{} - {}", char_name, date);
                                                            move |_| {
                                                                parsely_upload.open_file(p.clone(), display_name.clone());
                                                            }
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

            // Changelog modal (What's New)
            if changelog_open() {
                div {
                    class: "modal-backdrop",
                    onclick: move |_| {
                        spawn(async move {
                            api::mark_changelog_viewed().await;
                            changelog_open.set(false);
                        });
                    },
                    div {
                        class: "changelog-modal",
                        onclick: move |e| e.stop_propagation(),
                        div { class: "changelog-header",
                            h3 {
                                i { class: "fa-solid fa-sparkles" }
                                " What's New"
                            }
                            button {
                                class: "btn btn-close",
                                onclick: move |_| {
                                    spawn(async move {
                                        api::mark_changelog_viewed().await;
                                        changelog_open.set(false);
                                    });
                                },
                                "X"
                            }
                        }
                        div {
                            class: "changelog-content",
                            dangerous_inner_html: "{changelog_html}"
                        }
                        div { class: "changelog-footer",
                            button {
                                class: "btn btn-primary",
                                onclick: move |_| {
                                    spawn(async move {
                                        api::mark_changelog_viewed().await;
                                        changelog_open.set(false);
                                    });
                                },
                                "Got it!"
                            }
                        }
                    }
                }
            }

            // Parsely upload modal
            ParselyUploadModal {}

            // Toast notifications (rendered on top of everything)
            ToastFrame {}
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Player Stats Bar
// ─────────────────────────────────────────────────────────────────────────────

/// Inline bar for alacrity and latency settings
#[component]
fn PlayerStatsBar() -> Element {
    let mut alacrity = use_signal(|| 0.0f32);
    let mut latency = use_signal(|| 0u16);
    let mut loaded = use_signal(|| false);

    // Load from config on mount
    use_effect(move || {
        if !loaded() {
            spawn(async move {
                if let Some(config) = api::get_config().await {
                    alacrity.set(config.alacrity_percent);
                    latency.set(config.latency_ms);
                    loaded.set(true);
                }
            });
        }
    });

    let save_config = move || {
        let new_alacrity = alacrity();
        let new_latency = latency();
        let mut toast = use_toast();
        spawn(async move {
            if let Some(mut config) = api::get_config().await {
                config.alacrity_percent = new_alacrity;
                config.latency_ms = new_latency;
                if let Err(err) = api::update_config(&config).await {
                    toast.show(
                        format!("Failed to save settings: {}", err),
                        ToastSeverity::Normal,
                    );
                }
            }
        });
    };

    rsx! {
        div { class: "player-stats-bar",
            div { class: "stat-input",
                label { "Alacrity %" }
                input {
                    r#type: "text",
                    title: "Your alacrity percentage for GCD calculations",
                    value: "{alacrity():.1}",
                    onchange: move |e| {
                        if let Ok(val) = e.value().parse::<f32>() {
                            alacrity.set(val.clamp(0.0, 30.0));
                            save_config();
                        }
                    }
                }
            }
            div { class: "stat-input",
                label { "Latency (ms)" }
                input {
                    r#type: "text",
                    title: "Your network latency in milliseconds for ability timing",
                    value: "{latency()}",
                    onchange: move |e| {
                        if let Ok(val) = e.value().parse::<u16>() {
                            latency.set(val.clamp(0, 500));
                            save_config();
                        }
                    }
                }
            }
            span {
                class: "stats-help-icon",
                title: "Alacrity and Latency affect duration of certain HoTs and effects",
                i { class: "fa-solid fa-circle-question" }
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
    timers_b_enabled: &mut Signal<bool>,
    challenges_enabled: &mut Signal<bool>,
    alerts_enabled: &mut Signal<bool>,
    effects_a_enabled: &mut Signal<bool>,
    effects_b_enabled: &mut Signal<bool>,
    cooldowns_enabled: &mut Signal<bool>,
    dot_tracker_enabled: &mut Signal<bool>,
    notes_enabled: &mut Signal<bool>,
    combat_time_enabled: &mut Signal<bool>,
    overlays_visible: &mut Signal<bool>,
    move_mode: &mut Signal<bool>,
    rearrange_mode: &mut Signal<bool>,
    auto_hidden: &mut Signal<bool>,
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
    timers_b_enabled.set(status.timers_b_enabled);
    challenges_enabled.set(status.challenges_enabled);
    alerts_enabled.set(status.alerts_enabled);
    effects_a_enabled.set(status.effects_a_enabled);
    effects_b_enabled.set(status.effects_b_enabled);
    cooldowns_enabled.set(status.cooldowns_enabled);
    dot_tracker_enabled.set(status.dot_tracker_enabled);
    notes_enabled.set(status.notes_enabled);
    combat_time_enabled.set(status.combat_time_enabled);
    overlays_visible.set(status.overlays_visible);
    move_mode.set(status.move_mode);
    rearrange_mode.set(status.rearrange_mode);
    auto_hidden.set(status.auto_hidden);
}

// ─────────────────────────────────────────────────────────────────────────────
// StarParse Import
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq)]
enum ImportState {
    Idle,
    Previewing,
    Ready(String, crate::types::StarParsePreview),
    Importing,
    Done(crate::types::StarParseImportResult),
    Error(String),
}

#[component]
fn StarParseImportSection() -> Element {
    let toast = use_toast();
    let mut state = use_signal(|| ImportState::Idle);

    rsx! {
        div { class: "settings-section",
            h4 { "Import" }
            p { class: "hint", "Import timers and effects from StarParse XML exports." }

            match state() {
                ImportState::Idle | ImportState::Error(_) => rsx! {
                    if let ImportState::Error(ref msg) = state() {
                        p { class: "hint hint-warning", "{msg}" }
                    }
                    button {
                        class: "btn",
                        onclick: move |_| {
                            let mut state = state.clone();
                            let mut toast = toast.clone();
                            spawn(async move {
                                let Some(path) = api::open_xml_file_dialog().await else {
                                    return;
                                };
                                state.set(ImportState::Previewing);
                                match api::preview_starparse_import(&path).await {
                                    Ok(preview) => state.set(ImportState::Ready(path, preview)),
                                    Err(e) => {
                                        toast.show(format!("Failed to parse XML: {}", e), ToastSeverity::Normal);
                                        state.set(ImportState::Error(e));
                                    }
                                }
                            });
                        },
                        "Import StarParse XML..."
                    }
                },
                ImportState::Previewing => rsx! {
                    p { class: "hint", "Parsing XML..." }
                },
                ImportState::Ready(ref path, ref preview) => {
                    let path = path.clone();
                    let preview = preview.clone();
                    rsx! {
                        div { class: "import-preview",
                            if preview.encounter_timers > 0 {
                                p {
                                    strong { "{preview.encounter_timers}" }
                                    " encounter timers across "
                                    strong { "{preview.operations.len()}" }
                                    " operations"
                                }
                            }
                            if preview.effect_timers > 0 {
                                p {
                                    strong { "{preview.effect_timers}" }
                                    " personal effects"
                                }
                            }
                            if preview.skipped_builtin > 0 {
                                p { class: "hint",
                                    "{preview.skipped_builtin} built-in timers skipped"
                                }
                            }
                            if preview.skipped_unsupported_effects > 0 {
                                p { class: "hint hint-warning",
                                    "{preview.skipped_unsupported_effects} personal timers skipped (non-effect triggers not supported)"
                                }
                            }
                            if !preview.unmapped_bosses.is_empty() {
                                p { class: "hint hint-warning",
                                    "Unmapped bosses: {preview.unmapped_bosses.join(\", \")}"
                                }
                            }
                            div { class: "button-row",
                                button {
                                    class: "btn btn-primary",
                                    onclick: move |_| {
                                        let path = path.clone();
                                        let mut state = state.clone();
                                        let mut toast = toast.clone();
                                        spawn(async move {
                                            state.set(ImportState::Importing);
                                            match api::import_starparse_timers(&path).await {
                                                Ok(result) => {
                                                    toast.show(
                                                        format!(
                                                            "Imported {} encounter timers and {} effects",
                                                            result.encounter_timers_imported,
                                                            result.effects_imported,
                                                        ),
                                                        ToastSeverity::Normal,
                                                    );
                                                    state.set(ImportState::Done(result));
                                                }
                                                Err(e) => {
                                                    toast.show(format!("Import failed: {}", e), ToastSeverity::Normal);
                                                    state.set(ImportState::Error(e));
                                                }
                                            }
                                        });
                                    },
                                    "Confirm Import"
                                }
                                button {
                                    class: "btn",
                                    onclick: move |_| state.set(ImportState::Idle),
                                    "Cancel"
                                }
                            }
                        }
                    }
                },
                ImportState::Importing => rsx! {
                    p { class: "hint", "Importing..." }
                },
                ImportState::Done(ref result) => rsx! {
                    p { class: "hint",
                        "Imported {result.encounter_timers_imported} encounter timers and {result.effects_imported} effects to {result.files_written} files"
                    }
                    button {
                        class: "btn",
                        onclick: move |_| state.set(ImportState::Idle),
                        "Import Another"
                    }
                },
            }
        }
    }
}
