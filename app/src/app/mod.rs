#![allow(non_snake_case)]

use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

use crate::api::{self, BossNotesInfo};
use crate::components::class_icons::{get_class_icon, get_role_icon};
use crate::components::toggle_switch::ToggleSwitch;
use crate::components::{
    AlacrityPill, BossNotesMenu, CharactersPanel, CleanupRules, ContributorsModal, DataExplorerPanel,
    EffectEditorPanel, EncounterEditorPanel, FileOpenBanner, HistoryPanel, HotkeyInput, LockoutResetSetting, NavMenu,
    NumberInput, OpTimerControl, OverlayToggle, ParselyUploadModal, ProfileUnsavedHint, SessionStatusChip,
    SettingsPanel, SettingsSection, Slider, ToastFrame, ToastSeverity, use_file_opener_provider, use_parsely_upload,
    use_parsely_upload_provider, use_toast, use_toast_provider,
};
use crate::types::{
    AreaVisitInfo, LogFileInfo, MainTab, MetricType, OverlaySettings, OverlayType, SessionInfo, UiSessionState,
    UpdateInfo, WEB_OVERLAY_URL,
};

mod helpers;

use helpers::{
    OVERLAY_CATEGORIES, add_parsely_guild, apply_status, is_linux, is_macos, save_parsely_guild_fields,
    set_light_theme, set_overlay_enabled,
};

static CSS: Asset = asset!("/assets/styles.css");
static DATA_EXPLORER_CSS: Asset = asset!("/assets/data-explorer.css");
static HISTORY_CSS: Asset = asset!("/assets/history.css");
static CHARACTERS_CSS: Asset = asset!("/assets/characters.css");
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
    // Config keys of the enabled overlays
    let mut overlays_enabled = use_signal(HashSet::<String>::new);
    // Operation timer state from Tauri events
    let mut op_timer_secs = use_signal(|| 0u64);
    let mut op_timer_running = use_signal(|| false);
    let mut op_timer_name = use_signal(|| None::<String>);
    let mut overlays_visible = use_signal(|| true);
    let mut move_mode = use_signal(|| false);
    let mut rearrange_mode = use_signal(|| false);
    let mut auto_hidden = use_signal(|| false);

    // Directory and file state
    let mut log_directory = use_signal(String::new);
    let mut active_file = use_signal(String::new);
    let mut is_watching = use_signal(|| false);
    let mut is_live_tailing = use_signal(|| true);
    let opener = use_file_opener_provider(is_live_tailing);
    let mut session_info = use_signal(|| None::<SessionInfo>);
    let mut session_ended = use_signal(|| false); // True when player logged out but data preserved

    // Boss notes selector state
    let mut area_bosses = use_signal(Vec::<BossNotesInfo>::new);
    let selected_boss_id = use_signal(|| None::<String>);

    // File browser state
    let mut file_browser_open = use_signal(|| false);
    let mut log_files = use_signal(Vec::<LogFileInfo>::new);
    let mut upload_status = use_signal(HashMap::<String, (bool, String)>::new); // path -> (success, message)
    let mut file_browser_filter = use_signal(String::new);
    let mut hide_small_log_files = use_signal(|| true);
    let mut ocr_debug_dump = use_signal(|| false);
    let mut ocr_debug_max_dumps = use_signal(|| 100u32);

    // UI Session State - unified state that persists across tab switches
    let mut ui_state = use_signal(UiSessionState::default);
    crate::shortcuts::use_app_shortcuts(ui_state);
    // Render reads only this, so explorer filter and sort writes to `ui_state` don't re-render App
    let active_tab = use_memo(move || ui_state.read().active_tab);

    // Other UI state (not part of session persistence)
    let mut overlay_settings = use_signal(OverlaySettings::default);
    let mut selected_overlay_tab = use_signal(|| "dps".to_string());
    let mut settings_section = use_signal(|| SettingsSection::Global);
    // Gear on an overlay toggle: show that overlay's options in the settings panel
    let open_overlay_settings = move |key: &'static str| {
        selected_overlay_tab.set(key.to_string());
        settings_section.set(SettingsSection::Overlays);
    };
    // Overlay whose options the panel currently shows, for the gear highlight
    let drawer_overlay = if settings_section() == SettingsSection::Overlays {
        selected_overlay_tab()
    } else {
        String::new()
    };

    // Hotkey state
    let mut hotkey_visibility = use_signal(String::new);
    let mut hotkey_move_mode = use_signal(String::new);
    let mut hotkey_rearrange = use_signal(String::new);
    let mut hotkey_op_timer = use_signal(String::new);
    let mut hotkey_live_mode = use_signal(String::new);
    let mut hotkey_detect_raid = use_signal(String::new);
    let mut hotkey_save_status = use_signal(String::new);
    let mut is_wayland_session = use_signal(|| false);
    use_future(move || async move {
        is_wayland_session.set(api::is_wayland_session().await);
    });

    // Log management state
    let mut log_dir_size = use_signal(|| 0u64);
    let mut log_file_count = use_signal(|| 0usize);

    // Application settings
    let mut minimize_to_tray = use_signal(|| true);
    let mut web_overlay_enabled = use_signal(|| false);
    let mut european_number_format = use_signal(|| false);
    let mut light_theme = use_signal(|| false);
    use_context_provider(|| crate::utils::LightTheme(light_theme));
    let mut app_version = use_signal(String::new);

    // Update state
    let mut update_available = use_signal(|| None::<UpdateInfo>);
    let mut update_installing = use_signal(|| false);

    // Changelog state
    let mut changelog_open = use_signal(|| false);
    let mut changelog_html = use_signal(String::new);
    let mut contributors_open = use_signal(|| false);
    let mut screen_capture_permission_open = use_signal(|| false);
    let mut screen_capture_starting = use_signal(|| false);
    let mut screen_capture_error = use_signal(|| None::<String>);

    // Audio settings
    let mut audio_enabled = use_signal(|| true);
    let mut audio_volume = use_signal(|| 80u8);
    let mut audio_tts_enabled = use_signal(|| true);

    // Profile state
    let mut profile_names = use_signal(Vec::<String>::new);
    let mut active_profile = use_signal(|| None::<String>);
    let mut profile_dirty = use_signal(|| false);

    // Parsely settings
    let mut parsely_username = use_signal(String::new);
    let mut parsely_password = use_signal(String::new);
    let mut parsely_guilds = use_signal(Vec::<String>::new);
    let mut parsely_selected_guild = use_signal(|| None::<String>);
    let mut parsely_guild_input = use_signal(String::new);
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
            if let Some(v) = config.hotkeys.toggle_operation_timer {
                hotkey_op_timer.set(v);
            }
            if let Some(v) = config.hotkeys.toggle_live_mode {
                hotkey_live_mode.set(v);
            }
            if let Some(v) = config.hotkeys.detect_raid_names {
                hotkey_detect_raid.set(v);
            }
            profile_names.set(config.profiles.iter().map(|p| p.name.clone()).collect());
            active_profile.set(config.active_profile_name);
            // Auto-create a "Default" profile for new users
            if config.profiles.is_empty() && api::save_profile("Default").await.is_ok() {
                profile_names.set(vec!["Default".to_string()]);
                active_profile.set(Some("Default".to_string()));
            }
            hide_small_log_files.set(config.hide_small_log_files);
            ocr_debug_dump.set(config.ocr_debug_dump);
            ocr_debug_max_dumps.set(config.ocr_debug_max_dumps);
            minimize_to_tray.set(config.minimize_to_tray);
            web_overlay_enabled.set(config.web_overlay_enabled);
            european_number_format.set(config.european_number_format);
            light_theme.set(config.light_theme);
            crate::utils::apply_theme(config.light_theme);
            parsely_username.set(config.parsely.username);
            parsely_password.set(config.parsely.password);
            parsely_guilds.set(config.parsely.guilds);
            parsely_selected_guild.set(config.parsely.selected_guild);
            // Audio settings
            audio_enabled.set(config.audio.enabled);
            audio_volume.set(config.audio.volume);
            audio_tts_enabled.set(config.audio.tts_enabled);
            // UI preferences - now in unified state
            ui_state.write().data_explorer.show_only_bosses = config.show_only_bosses;
            ui_state.write().history.filter.flashpoints = config.raid_summaries_include_flashpoints;
            ui_state.write().data_explorer.auto_live = config.data_explorer_auto_live;
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
                status,
                &mut overlays_enabled,
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
                opener.loaded(&path);
                // Use try_write to handle signal being dropped when component unmounts
                let _ = active_file.try_write().map(|mut w| *w = path);
            }
        });
        api::tauri_listen("active-file-changed", &closure).await;
        closure.forget();
    });

    use_future(move || async move {
        let closure = Closure::new(move |_event: JsValue| {
            let _ = screen_capture_error.try_write().map(|mut value| *value = None);
            let _ = screen_capture_permission_open
                .try_write()
                .map(|mut value| *value = true);
        });
        api::tauri_listen("screen-capture-permission-required", &closure).await;
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

    // Listen for profile auto-switch events (role-based default profiles)
    use_future(move || async move {
        let closure = Closure::new(move |_event: JsValue| {
            spawn_local(async move {
                // Refresh profile state from backend
                let names = api::get_profile_names().await;
                let active = api::get_active_profile().await;
                profile_names.set(names);
                active_profile.set(active);
                profile_dirty.set(false);
                // Refresh overlay settings to match the newly loaded profile
                if let Some(cfg) = api::get_config().await {
                    overlay_settings.set(cfg.overlay_settings);
                }
                api::refresh_overlay_settings().await;
                if let Some(status) = api::get_overlay_status().await {
                    apply_status(
                        status,
                        &mut overlays_enabled,
                        &mut overlays_visible,
                        &mut move_mode,
                        &mut rearrange_mode,
                        &mut auto_hidden,
                    );
                }
            });
        });
        api::tauri_listen("profile-auto-switched", &closure).await;
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
                    if let Ok(data) = js_sys::Reflect::get(&global, &"__parsely_upload_result".into())
                        && let Some(data_str) = data.as_string()
                    {
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
                });
            });
            let _ =
                window.add_event_listener_with_callback("parsely-upload-complete", closure.as_ref().unchecked_ref());
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

    // Listen for encounter focus requests (History tab → Data Explorer). Fires after
    // the file has loaded, so it lands after the new-session reset above. The Data
    // Explorer reads the selection from ui_state when it mounts on the tab switch.
    use_future(move || async move {
        let closure = Closure::new(move |event: JsValue| {
            if let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
                && let Some(id) = payload.as_f64()
            {
                let _ = ui_state.try_write().map(|mut state| {
                    state.data_explorer.selected_encounter = Some(id as u32);
                    state.active_tab = MainTab::DataExplorer;
                });
            }
        });
        api::tauri_listen("select-encounter", &closure).await;
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
                update_failed_toast.show(format!("Update failed: {}", msg), ToastSeverity::Critical);
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
                    format!(
                        "Global hotkeys unavailable: {}. Configure shortcuts in compositor settings.",
                        msg
                    ),
                    ToastSeverity::Normal,
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

    // Backend emits this only when locking persisted a position that actually changed
    use_future(move || async move {
        let closure = Closure::new(move |_event: JsValue| {
            spawn_local(async move {
                let dirty = api::is_profile_dirty().await;
                let _ = profile_dirty.try_write().map(|mut w| *w = dirty);
            });
        });
        api::tauri_listen("overlay-positions-changed", &closure).await;
        closure.forget();
    });

    // Listen for operation timer tick events
    use_future(move || async move {
        let closure = Closure::new(move |event: JsValue| {
            // event.payload has { elapsed_secs, is_running, operation_name }
            if let Ok(payload) = js_sys::Reflect::get(&event, &"payload".into()) {
                // try_write: this runs outside the Dioxus runtime, where .set() can panic
                if let Some(secs) = js_sys::Reflect::get(&payload, &"elapsed_secs".into())
                    .ok()
                    .and_then(|v| v.as_f64())
                {
                    let _ = op_timer_secs.try_write().map(|mut w| *w = secs as u64);
                }
                if let Some(running) = js_sys::Reflect::get(&payload, &"is_running".into())
                    .ok()
                    .and_then(|v| v.as_bool())
                {
                    let _ = op_timer_running.try_write().map(|mut w| *w = running);
                }
                let name = js_sys::Reflect::get(&payload, &"operation_name".into())
                    .ok()
                    .and_then(|v| v.as_string());
                let _ = op_timer_name.try_write().map(|mut w| *w = name);
            }
        });
        api::tauri_listen("operation-timer-tick", &closure).await;
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
        if let Some(response) = api::get_changelog().await
            && response.should_show
            && let Some(html) = response.html
        {
            changelog_html.set(html);
            changelog_open.set(true);
        }
    });

    // ─────────────────────────────────────────────────────────────────────────
    // Computed Values
    // ─────────────────────────────────────────────────────────────────────────

    let enabled_set = overlays_enabled();
    let is_on = |key: &str| enabled_set.contains(key);
    let raid_on = is_on("raid");
    let pvp_metrics = MetricType::pvp();
    let any_enabled = OVERLAY_CATEGORIES
        .iter()
        .flat_map(|(_, defs)| defs.iter())
        .any(|d| is_on(d.key))
        || MetricType::all().iter().any(|m| is_on(m.config_key()))
        || is_on("enemy_frames");
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
    // Amber treatment for the whole status row when the view is not live
    let status_historical = !live_tailing || session_ended() || session.as_ref().is_some_and(|s| s.stale_session);
    // When the session ran: shown at the end of the collapsible row
    let session_time_text = session.as_ref().and_then(|s| {
        if session_ended() || s.stale_session {
            s.session_start.clone()
        } else if live_tailing {
            s.session_start_short.as_ref().map(|t| format!("since {t}"))
        } else {
            match (&s.session_start, &s.session_end) {
                (Some(start), Some(end)) => Some(format!("{start} – {end}")),
                (Some(start), None) => Some(start.clone()),
                _ => None,
            }
        }
    });

    // Enabled counts for the category card headers on the Overlays page
    let metrics_on = MetricType::all()
        .iter()
        .filter(|m| !pvp_metrics.contains(m) && is_on(m.config_key()))
        .count();
    let metrics_total = MetricType::all().len() - pvp_metrics.len();
    let enemy_frames_on = is_on("enemy_frames");
    let pvp_on = usize::from(enemy_frames_on) + pvp_metrics.iter().filter(|m| is_on(m.config_key())).count();
    let pvp_total = 1 + pvp_metrics.len();

    // Session status chip actions (shared by every session bar variant)
    let restart_watcher = move |_| {
        spawn(async move {
            api::restart_watcher().await;
            is_live_tailing.set(true);
        });
    };
    let resume_live = move |_| {
        let mut toast = use_toast();
        spawn(async move {
            if let Err(err) = api::resume_live_tailing().await {
                toast.show(format!("Failed to resume live tailing: {}", err), ToastSeverity::Normal);
            } else {
                is_live_tailing.set(true);
            }
        });
    };

    // ─────────────────────────────────────────────────────────────────────────
    // Render
    // ─────────────────────────────────────────────────────────────────────────

    rsx! {
        link { rel: "stylesheet", href: CSS }
        link { rel: "stylesheet", href: DATA_EXPLORER_CSS }
        link { rel: "stylesheet", href: HISTORY_CSS }
        link { rel: "stylesheet", href: CHARACTERS_CSS }
        link { rel: "stylesheet", href: "https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css" }
        style { "@font-face {{ font-family: 'StarJedi'; src: url('{FONT}') format('truetype'); font-display: block; font-weight: normal; font-style: normal; }}" }

        main { class: "container",
            // Header
            header { class: "app-header",
                div { class: "header-bar",
                    div { class: "header-content",
                        h1 { "BARAS" }
                        img { class: "header-logo", src: LOGO, alt: "BARAS mascot" }
                    }
                    // Navigation: live view and overlays as tabs, the editors and everything else behind menus
                    div { class: "header-nav",
                        div { class: "header-tabs",
                            button {
                                class: if active_tab() == MainTab::DataExplorer { "tab-btn active" } else { "tab-btn" },
                                title: "Pull breakdowns for currently selected file",
                                onclick: move |_| ui_state.write().active_tab = MainTab::DataExplorer,
                                i { class: "fa-solid fa-magnifying-glass-chart" }
                                span { class: "tab-label", " Analysis" }
                            }
                            button {
                                class: if active_tab() == MainTab::Overlays { "tab-btn active" } else { "tab-btn" },
                                title: "Enable, arrange and customize overlays",
                                onclick: move |_| ui_state.write().active_tab = MainTab::Overlays,
                                i { class: "fa-solid fa-layer-group" }
                                span { class: "tab-label", " Overlays" }
                            }
                            for (tab, icon, label, tip) in [
                                (MainTab::EncounterBuilder, "fa-solid fa-stopwatch", "Timers", "Boss timers, phases, challenges and more"),
                                (MainTab::Effects, "fa-solid fa-heart-pulse", "Effects", "HoTs, DoTs, DCDs and other effects that are tracked across all encounters"),
                            ] {
                                button {
                                    class: if active_tab() == tab { "tab-btn active" } else { "tab-btn" },
                                    title: tip,
                                    onclick: move |_| ui_state.write().active_tab = tab,
                                    i { class: icon }
                                    span { class: "tab-label", " {label}" }
                                }
                            }
                            NavMenu {
                                label: "Statistics",
                                icon: "fa-solid fa-chart-column",
                                items: vec![
                                    (MainTab::BossStats, "fa-solid fa-chart-column", "Boss Statistics", "One boss across every log"),
                                    (MainTab::RaidSummaries, "fa-solid fa-dungeon", "Raid Summaries", ""),
                                    (MainTab::PvpStats, "fa-solid fa-crosshairs", "PvP Statistics", "Warzone and arena record, kills and deaths, trends"),
                                ],
                                state: ui_state,
                            }
                            for (tab, icon, label, tip) in [
                                (MainTab::LogManagement, "fa-solid fa-folder-tree", "Logs", "Manage log files"),
                                (MainTab::Characters, "fa-solid fa-users", "Characters", "Track character information and lockouts"),
                                (MainTab::Settings, "fa-solid fa-gear", "Settings", "Log directory, hotkeys, audio and application options"),
                            ] {
                                button {
                                    class: if active_tab() == tab { "tab-btn active" } else { "tab-btn" },
                                    title: tip,
                                    onclick: move |_| ui_state.write().active_tab = tab,
                                    i { class: icon }
                                    span { class: "tab-label", " {label}" }
                                }
                            }
                            NavMenu {
                                label: "Links",
                                icon: "fa-solid fa-ellipsis",
                                icon_only: true,
                                items: vec![],
                                actions: vec![
                                    ("fa-solid fa-circle-question", "Documentation".into(), "Documentation & Help",
                                        EventHandler::new(|_| { spawn(api::open_url("https://baras-app.github.io/features/overview")); })),
                                    ("fa-brands fa-discord", "Discord".into(), "Questions, bugs, or feedback",
                                        EventHandler::new(|_| { spawn(api::open_url("https://discord.gg/zmtkYkhSM4")); })),
                                    ("fa-brands fa-github", "GitHub".into(), "Source code",
                                        EventHandler::new(|_| { spawn(api::open_url("https://github.com/baras-app/baras")); })),
                                    ("fa-solid fa-heart", "Contributors".into(), "The people behind BARAS",
                                        EventHandler::new(move |_| contributors_open.set(true))),
                                    ("fa-solid fa-scroll", format!("Changelog (v{})", app_version()), "What changed in this version",
                                        EventHandler::new(move |_| {
                                            spawn(async move {
                                                if let Some(html) = api::get_changelog().await.and_then(|r| r.html) {
                                                    changelog_html.set(html);
                                                }
                                                changelog_open.set(true);
                                            });
                                        })),
                                ],
                                state: ui_state,
                            }
                        }
                        button {
                            class: if file_browser_open() { "tab-btn header-files-btn active" } else { "tab-btn header-files-btn" },
                            title: if file_browser_open() { "Close log files (L)" } else { "Browse log files (L)" },
                            "data-shortcut": "l",
                            onclick: move |_| {
                                // Second click closes the drawer again
                                if file_browser_open() {
                                    file_browser_open.set(false);
                                    return;
                                }
                                file_browser_filter.set(String::new());
                                file_browser_open.set(true);
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
                    }
                    // Version, update notice and project links
                    div { class: "header-meta",
                        div { class: "header-version-group",
                            // Only an available update earns header space; the version and
                            // changelog live in the links menu
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
                                        });
                                    },
                                    if update_installing() {
                                        i { class: "fa-solid fa-spinner fa-spin" }
                                        " Updating..."
                                    } else {
                                        i { class: "fa-solid fa-arrow-up" }
                                        " Update" span { class: "update-long", " Available!" }
                                    }
                                }
                            }
                            div { class: "header-links",
                                a {
                                    class: "header-link",
                                    href: "#",
                                    title: if light_theme() { "Switch to dark theme" } else { "Switch to light theme" },
                                    onclick: move |e| {
                                        e.prevent_default();
                                        set_light_theme(light_theme, !light_theme());
                                    },
                                    i { class: if light_theme() { "fa-solid fa-moon" } else { "fa-solid fa-sun" } }
                                }
                            }
                        }
                    }
                }

                // Status row: overlay quick actions, then the session bar beside them
                div {
                    class: if status_historical { "status-row historical" } else { "status-row" },
                        div { class: "header-actions",
                            div { class: "header-overlay-controls",
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
                                if is_rearrange {
                                    button {
                                        class: "btn btn-header-overlay",
                                        title: "Clear raid frame assignments",
                                        disabled: !raid_on,
                                        onclick: move |_| { spawn(async move { api::clear_raid_registry().await; }); },
                                        i { class: "fa-solid fa-eraser" }
                                    }
                                }
                                // Profile selector
                                select {
                                    class: "header-profile-dropdown",
                                    title: "Switch overlay profile",
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
                                                if let Some(status) = api::get_overlay_status().await {
                                                    apply_status(status, &mut overlays_enabled, &mut overlays_visible, &mut move_mode, &mut rearrange_mode, &mut auto_hidden);
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
                        }
                    div { class: "status-row-session",
                        // ─────────────────────────────────────────────────────────────
                        // Collapsible Session Dashboard Bar (visible on all tabs)
                        // ─────────────────────────────────────────────────────────────

                        // Empty states: show status when no player data yet
                        if show_empty_state && !session.as_ref().is_some_and(|s| s.missing_area) {
                            div { class: "session-dashboard-bar",
                            div { class: "dashboard-toggle-row dashboard-empty-state",
                                if !live_tailing {
                                    i { class: "fa-solid fa-spinner fa-spin dashboard-status-icon" }
                                    span { class: "dashboard-empty-text", "Loading file..." }
                                } else if log_files().is_empty() {
                                    i { class: "fa-solid fa-triangle-exclamation dashboard-status-icon" }
                                    span { class: "dashboard-empty-text", "No log files found — " }
                                    span {
                                        class: "settings-link",
                                        onclick: move |_| ui_state.write().active_tab = MainTab::Settings,
                                        "set log directory"
                                    }
                                } else if watching {
                                    i { class: "fa-solid fa-hourglass-half dashboard-status-icon" }
                                    span { class: "dashboard-empty-text", "Waiting for player..." }
                                } else {
                                    i { class: "fa-solid fa-inbox dashboard-status-icon" }
                                    span { class: "dashboard-empty-text", "No active session" }
                                }
                                ProfileUnsavedHint { active_profile, profile_dirty }
                                SessionStatusChip { live_tailing, watching, prior: false, on_restart: restart_watcher, on_resume: resume_live }
                            }
                            }
                        }

                        // Incomplete log file warning
                        if session.as_ref().is_some_and(|s| s.missing_area) {
                            div { class: "session-dashboard-bar",
                                div { class: "dashboard-toggle-row dashboard-empty-state",
                                    i { class: "fa-solid fa-triangle-exclamation dashboard-status-icon warning" }
                                    if session.as_ref().is_some_and(|s| s.area_resume_pending) {
                                        span { class: "dashboard-empty-text", "Log resumed after a disconnect — engage an NPC to restore the area, boss encounters and timers" }
                                    } else {
                                        span { class: "dashboard-empty-text", "Incomplete log file — boss encounters and timers unavailable" }
                                    }
                                    ProfileUnsavedHint { active_profile, profile_dirty }
                                    SessionStatusChip { live_tailing, watching, prior: false, on_restart: restart_watcher, on_resume: resume_live }
                                }
                            }
                        }

                        if let Some(ref info) = session {
                            if has_player && !info.missing_area {
                            {
                            let is_historical = session_ended() || info.stale_session || !live_tailing;
                            rsx! {
                            div {
                                class: {
                                    let mut cls = String::from("session-dashboard-bar");
                                    if is_historical { cls.push_str(" historical"); }
                                    cls
                                },

                                // Character mismatch warning (corrupted log file)
                                if info.character_mismatch {
                                    div { class: "session-mismatch-warning",
                                        i { class: "fa-solid fa-triangle-exclamation" }
                                        " This log file contains multiple characters. Data may be inaccurate. Please relog to start a new session."
                                    }
                                }

                                // ── Always-visible header row (clickable to toggle collapse) ──
                                div {
                                    class: "dashboard-toggle-row",

                                    // Session tools on the left; character, duration and status on the right
                                    div { class: "dashboard-summary",
                                        AlacrityPill {
                                            player_name: session_info().and_then(|s| s.player_name),
                                            player_discipline: session_info().and_then(|s| s.player_discipline),
                                        }
                                        OpTimerControl { op_timer_secs, op_timer_running, live: live_tailing }
                                        if is_on("notes") {
                                            BossNotesMenu { area_bosses, selected_boss_id }
                                        }

                                        // Parsely upload (inline in summary)
                                        if !current_file.is_empty() {
                                            {
                                                let path = current_file.clone();
                                                let upload_result = upload_status().get(&path).cloned().or_else(|| {
                                                    log_files().iter().find(|f| f.path == path).and_then(|f| f.parsely.clone()).map(|link| (true, link))
                                                });
                                                rsx! {
                                                    div { class: "session-upload-group",
                                                        button {
                                                            class: "btn btn-session-upload",
                                                            title: "Upload session to Parsely",
                                                            onclick: {
                                                                let p = path.clone();
                                                                move |e| {
                                                                    e.stop_propagation();
                                                                    let filename = p.split('/').next_back()
                                                                        .or_else(|| p.split('\\').next_back())
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
                                                                        move |e| {
                                                                            e.stop_propagation();
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

                                        ProfileUnsavedHint { active_profile, profile_dirty }

                                        // Session facts on the right: who, how long, then live / prior
                                        span { class: "dashboard-identity",
                                            // Role icon (small)
                                            if let Some(ref role_name) = info.role_icon {
                                                if let Some(role_asset) = get_role_icon(role_name) {
                                                    img { class: "dashboard-role-icon", src: *role_asset }
                                                }
                                            }
                                            // Class icon (small)
                                            if let Some(ref icon_name) = info.class_icon {
                                                if let Some(icon_asset) = get_class_icon(icon_name) {
                                                    img { class: "dashboard-class-icon", src: *icon_asset }
                                                }
                                            }

                                            // Player name
                                            if let Some(ref name) = info.player_name {
                                                span { class: "dashboard-player-name", "{name}" }
                                            }

                                            // Duration badge
                                            if let Some(ref duration) = info.duration_formatted {
                                                span { class: "session-duration-badge",
                                                    title: session_time_text.as_deref().map(|t| format!("Session: {t}")).unwrap_or_default(),
                                                    i { class: "fa-regular fa-clock" }
                                                    " {duration}"
                                                }
                                            }
                                        }
                                        span { class: "header-controls-divider" }
                                        SessionStatusChip {
                                            live_tailing,
                                            watching,
                                            prior: session_ended() || info.stale_session,
                                            on_restart: restart_watcher,
                                            on_resume: resume_live,
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

            } // end app-header

            // Tab Content
            div { class: "tab-content",
                // ─────────────────────────────────────────────────────────────
                // Overlays Tab
                // ─────────────────────────────────────────────────────────────
                if active_tab() == MainTab::Overlays {
                    section { class: "overlay-controls",
                        // Global controls as the header of the overlay card
                        div { class: "overlay-toolbar",
                            span { class: "overlay-toolbar-label", "Controls:" }
                            button {
                                class: if is_visible && any_enabled { "btn btn-control btn-visible" } else { "btn btn-control btn-hidden" },
                                title: if is_visible { "Hide all overlays" } else { "Show enabled overlays" },
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
                                title: if is_move_mode { "Lock overlays in place" } else { "Unlock overlays to move and resize them" },
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
                                title: "Drag raid frames into a new order",
                                disabled: !is_visible || !raid_on || is_move_mode || overlays_auto_hidden,
                                onclick: move |_| { spawn(async move {
                                    if let Ok(new_mode) = api::toggle_raid_rearrange().await {
                                        rearrange_mode.set(new_mode);
                                    }
                                }); },
                                i { class: "fa-solid fa-grip" }
                                span { " Rearrange" }
                            }
                            button {
                                class: "btn btn-control btn-clear-frames",
                                title: "Clear raid frame assignments",
                                disabled: !is_visible || !raid_on,
                                onclick: move |_| { spawn(async move { api::clear_raid_registry().await; }); },
                                i { class: "fa-solid fa-eraser" }
                                span { " Clear" }
                            }
                            if !any_enabled {
                                span { class: "overlay-toolbar-hint",
                                    i { class: "fa-solid fa-circle-info" }
                                    " Enable at least one overlay to toggle visibility and movement modes."
                                }
                            }
                        }

                        // Overlay categories in columns
                        div { class: "overlay-categories",
                            for (category, defs) in OVERLAY_CATEGORIES {
                                div { class: "overlay-category",
                                    div { class: "category-header",
                                        h4 { class: "category-title", "{category}" }
                                        span { class: "category-count",
                                            "{defs.iter().filter(|d| is_on(d.key)).count()} of {defs.len()}"
                                        }
                                    }
                                    div { class: "category-buttons",
                                        for def in defs {
                                            {
                                                let (kind, key) = (def.kind, def.key);
                                                let on = is_on(key);
                                                rsx! {
                                                    OverlayToggle {
                                                        key: "{key}",
                                                        label: def.label,
                                                        icon: def.icon,
                                                        title: def.title,
                                                        active: on,
                                                        tab_key: def.has_settings.then_some(key),
                                                        on_toggle: move |_| { spawn(async move {
                                                            if api::toggle_overlay(kind, on).await {
                                                                set_overlay_enabled(&mut overlays_enabled, key, !on);
                                                                if on && key == "raid" { rearrange_mode.set(false); }
                                                                profile_dirty.set(api::is_profile_dirty().await);
                                                            }
                                                        }); },
                                                        on_settings: open_overlay_settings,
                                                        drawer_overlay: drawer_overlay.clone(),
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "overlay-category",
                                div { class: "category-header",
                                    h4 { class: "category-title", "Metrics" }
                                    span { class: "category-count", "{metrics_on} of {metrics_total}" }
                                }
                                div { class: "category-buttons",
                                    for mt in MetricType::all().iter().filter(|mt| !MetricType::pvp().contains(mt)) {
                                        {
                                            let ot = *mt;
                                            let on = is_on(ot.config_key());
                                            rsx! {
                                                OverlayToggle {
                                                    key: "{ot.config_key()}",
                                                    label: ot.label(),
                                                    icon: ot.icon_class(),
                                                    title: "",
                                                    active: on,
                                                    tab_key: ot.config_key(),
                                                    on_toggle: move |_| { spawn(async move {
                                                        if api::toggle_overlay(OverlayType::Metric(ot), on).await {
                                                            set_overlay_enabled(&mut overlays_enabled, ot.config_key(), !on);
                                                            profile_dirty.set(api::is_profile_dirty().await);
                                                        }
                                                    }); },
                                                    on_settings: open_overlay_settings,
                                                    drawer_overlay: drawer_overlay.clone(),
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "overlay-category",
                                div { class: "category-header",
                                    h4 { class: "category-title", "PvP" }
                                    span { class: "category-count", "{pvp_on} of {pvp_total}" }
                                }
                                div { class: "category-buttons",
                                    OverlayToggle {
                                        label: "Enemy Frames",
                                        icon: "fa-solid fa-heart-pulse",
                                        title: "Enemy team HP frames with disciplines and targets",
                                        active: enemy_frames_on,
                                        tab_key: "enemy_frames",
                                        on_toggle: move |_| { spawn(async move {
                                            if api::toggle_overlay(OverlayType::EnemyFrames, enemy_frames_on).await {
                                                set_overlay_enabled(&mut overlays_enabled, "enemy_frames", !enemy_frames_on);
                                                profile_dirty.set(api::is_profile_dirty().await);
                                            }
                                        }); },
                                        on_settings: open_overlay_settings,
                                        drawer_overlay: drawer_overlay.clone(),
                                    }
                                    for mt in MetricType::pvp() {
                                        {
                                            let ot = *mt;
                                            let on = is_on(ot.config_key());
                                            rsx! {
                                                OverlayToggle {
                                                    key: "{ot.config_key()}",
                                                    label: ot.label(),
                                                    icon: ot.icon_class(),
                                                    title: "Show your DTPS over the past 20 seconds by source. Optionally enable total DTPS by source for the entire fight",
                                                    active: on,
                                                    tab_key: ot.config_key(),
                                                    on_toggle: move |_| { spawn(async move {
                                                        if api::toggle_overlay(OverlayType::Metric(ot), on).await {
                                                            set_overlay_enabled(&mut overlays_enabled, ot.config_key(), !on);
                                                            profile_dirty.set(api::is_profile_dirty().await);
                                                        }
                                                    }); },
                                                    on_settings: open_overlay_settings,
                                                    drawer_overlay: drawer_overlay.clone(),
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }

                    }
                }

                // ─────────────────────────────────────────────────────────────
                // Encounter Editor Tab
                // ─────────────────────────────────────────────────────────────
                if active_tab() == MainTab::EncounterBuilder {
                    EncounterEditorPanel {
                        state: ui_state,
                    }
                }

                // ─────────────────────────────────────────────────────────────
                // Effects Tab
                // ─────────────────────────────────────────────────────────────
                if active_tab() == MainTab::Effects {
                    EffectEditorPanel {
                        state: ui_state,
                    }
                }

                // ─────────────────────────────────────────────────────────────
                // Settings Tab
                // ─────────────────────────────────────────────────────────────
                if active_tab() == MainTab::Settings {
                    section { class: "settings-page",
                        div { class: "settings-header",
                            h3 { "Settings" }
                        }
                        div { class: "settings-content",
                        div { class: "settings-col",
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
                                    class: "btn btn-primary btn-browse",
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
                            {
                                let count = log_file_count();
                                let size_mb = log_dir_size() as f64 / 1_000_000.0;
                                rsx! {
                                    p { class: "hint", "{count} files • {size_mb:.1} MB" }
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
                            h4 { "Application" }
                            label { class: "settings-check",
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
                                "Minimize to tray on close"
                            }
                            p { class: "hint", "Closing the window hides BARAS to the tray instead of quitting." }
                            label { class: "settings-check",
                                input {
                                    r#type: "checkbox",
                                    checked: light_theme(),
                                    onchange: move |e| set_light_theme(light_theme, e.checked()),
                                }
                                "Light theme"
                            }
                            label { class: "settings-check",
                                input {
                                    r#type: "checkbox",
                                    checked: web_overlay_enabled(),
                                    onchange: move |e| {
                                        let checked = e.checked();
                                        let previous = web_overlay_enabled();
                                        web_overlay_enabled.set(checked);
                                        let mut toast = use_toast();
                                        spawn(async move {
                                            if let Err(err) = api::set_web_overlay_enabled(checked).await {
                                                web_overlay_enabled.set(previous);
                                                toast.show(format!("Failed to change web overlay: {}", err), ToastSeverity::Normal);
                                            }
                                        });
                                    }
                                }
                                "Overlay Streaming Source"
                            }
                            p { class: "hint", "Mirrors visible overlays to a local web page for use as an OBS browser source." }
                            if web_overlay_enabled() {
                                code { class: "web-overlay-url", "{WEB_OVERLAY_URL}" }
                            }
                            label { class: "settings-check",
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
                                "European number format"
                            }
                            p { class: "hint", "Swaps the decimal point and thousands separator (1.50K → 1,50K). Editor inputs still use '.' for decimals." }
                        }

                        div { class: "settings-section",
                            h4 { "Global Hotkeys" }
                            p { class: "hint", "Click a field and press a key combination; Backspace clears it. Changes apply after a restart." }
                            if is_wayland_session() {
                                p { class: "hint hint-warning",
                                    i { class: "fa-solid fa-triangle-exclamation" }
                                    " Wayland support is experimental and needs a compositor that implements the global shortcuts portal."
                                }
                            }
                            div { class: "hotkey-grid",
                                div { class: "setting-row",
                                    label { "Show/Hide Overlays" }
                                    HotkeyInput {
                                        value: hotkey_visibility(),
                                        on_change: move |v| hotkey_visibility.set(v),
                                    }
                                }
                                div { class: "setting-row",
                                    label { "Lock/Unlock Overlays" }
                                    HotkeyInput {
                                        value: hotkey_move_mode(),
                                        on_change: move |v| hotkey_move_mode.set(v),
                                    }
                                }
                                div { class: "setting-row",
                                    label { "Toggle Raid Frame Interactivity" }
                                    HotkeyInput {
                                        value: hotkey_rearrange(),
                                        on_change: move |v| hotkey_rearrange.set(v),
                                    }
                                }
                                div { class: "setting-row",
                                    label { "Start/Stop Operations Timer" }
                                    HotkeyInput {
                                        value: hotkey_op_timer(),
                                        on_change: move |v| hotkey_op_timer.set(v),
                                    }
                                }
                                div { class: "setting-row",
                                    label { "Run Raid Frame OCR Name Recognition" }
                                    HotkeyInput {
                                        value: hotkey_detect_raid(),
                                        on_change: move |v| hotkey_detect_raid.set(v),
                                    }
                                }
                                div { class: "setting-row",
                                    label { "Resume Live Tailing" }
                                    HotkeyInput {
                                        value: hotkey_live_mode(),
                                        on_change: move |v| hotkey_live_mode.set(v),
                                    }
                                }
                            }
                            if is_macos() {
                                p { class: "hint hint-warning",
                                    i { class: "fa-solid fa-triangle-exclamation" }
                                    " Raid Frame OCR Name Recognition is not supported on macOS."
                                }
                            }
                            div { class: "settings-footer",
                                button {
                                    class: "btn btn-save",
                                    onclick: move |_| {
                                        let v = hotkey_visibility(); let m = hotkey_move_mode(); let r = hotkey_rearrange(); let ot = hotkey_op_timer(); let lm = hotkey_live_mode(); let dr = hotkey_detect_raid();
                                        let mut toast = use_toast();
                                        spawn(async move {
                                            if let Some(mut cfg) = api::get_config().await {
                                                cfg.hotkeys.toggle_visibility = if v.is_empty() { None } else { Some(v) };
                                                cfg.hotkeys.toggle_move_mode = if m.is_empty() { None } else { Some(m) };
                                                cfg.hotkeys.toggle_rearrange_mode = if r.is_empty() { None } else { Some(r) };
                                                cfg.hotkeys.toggle_operation_timer = if ot.is_empty() { None } else { Some(ot) };
                                                cfg.hotkeys.toggle_live_mode = if lm.is_empty() { None } else { Some(lm) };
                                                cfg.hotkeys.detect_raid_names = if dr.is_empty() { None } else { Some(dr) };
                                                if let Err(err) = api::update_config(&cfg).await {
                                                    toast.show(format!("Failed to save hotkeys: {}", err), ToastSeverity::Normal);
                                                } else {
                                                    hotkey_save_status.set("Saved".to_string());
                                                }
                                            }
                                        });
                                    },
                                    "Save Hotkeys"
                                }
                                span { class: "save-status", "{hotkey_save_status}" }
                            }
                            div { class: "app-shortcuts",
                                h5 { "In-app Shortcuts" }
                                ul {
                                    for (key, action) in [
                                        ("Esc", "Close the topmost dialog, drawer or fullscreen view"),
                                        ("L", "Toggle the log file browser"),
                                        ("[  ]", "Previous / next tab"),
                                        ("S", "Collapse / expand the sidebar"),
                                        ("F", "Toggle Analysis fullscreen"),
                                        ("X", "Clear all filters"),
                                    ] {
                                        li { kbd { "{key}" } span { "{action}" } }
                                    }
                                }
                                p { class: "hint", "Ignored while typing in a field." }
                            }
                        }

                        div { class: "settings-section",
                            h4 { "Audio" }

                            label { class: "settings-check",
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
                                "Enable Audio"
                            }

                            Slider {
                                label: "Volume",
                                value: audio_volume() as f64,
                                min: 0.0,
                                max: 200.0,
                                suffix: "%",
                                disabled: !audio_enabled(),
                                on_change: move |v: f64| {
                                    let val = v.round() as u8;
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
                                },
                            }

                            label { class: "settings-check",
                                input {
                                    r#type: "checkbox",
                                    checked: audio_tts_enabled(),
                                    disabled: !audio_enabled(),
                                    onchange: move |e| {
                                        let checked = e.checked();
                                        audio_tts_enabled.set(checked);
                                        let mut toast = use_toast();
                                        spawn(async move {
                                            if let Some(mut cfg) = api::get_config().await {
                                                cfg.audio.tts_enabled = checked;
                                                if let Err(err) = api::update_config(&cfg).await {
                                                    toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                }
                                            }
                                        });
                                    }
                                }
                                "Text-to-Speech"
                            }
                            p { class: "hint",
                                "Reads alerts and countdowns aloud when no sound file or voice pack is set"
                                if is_linux() {
                                    " (requires espeak to be installed)"
                                }
                                "."
                            }
                            div { class: "settings-footer",
                                button {
                                    class: "btn",
                                    disabled: !audio_enabled(),
                                    onclick: move |_| {
                                        spawn(async move {
                                            api::preview_sound("Alert.mp3").await;
                                        });
                                    },
                                    "Test sound"
                                }
                                button {
                                    class: "btn",
                                    disabled: !audio_enabled() || !audio_tts_enabled(),
                                    onclick: move |_| {
                                        spawn(async move {
                                            api::preview_tts().await;
                                        });
                                    },
                                    "Test TTS"
                                }
                            }
                        }

                        } // settings-col
                        div { class: "settings-col",
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
                                label { "Guilds" }
                                div { class: "parsely-guilds-field",
                                    if !parsely_guilds.read().is_empty() {
                                        div { class: "parsely-guilds-chips",
                                            for (idx, guild) in parsely_guilds.read().iter().enumerate() {
                                                {
                                                    let name = guild.clone();
                                                    rsx! {
                                                        span {
                                                            key: "{name}-{idx}",
                                                            class: "chip",
                                                            "{name}"
                                                            button {
                                                                class: "chip-remove",
                                                                title: "Remove guild",
                                                                onclick: move |_| {
                                                                    let (guilds_snapshot, selected_snapshot) = {
                                                                        let mut list = parsely_guilds.write();
                                                                        list.retain(|g| g != &name);
                                                                        let still_has_selected = parsely_selected_guild
                                                                            .read()
                                                                            .as_deref()
                                                                            .map(|s| list.iter().any(|g| g == s))
                                                                            .unwrap_or(false);
                                                                        if !still_has_selected {
                                                                            parsely_selected_guild.set(list.first().cloned());
                                                                        }
                                                                        (list.clone(), parsely_selected_guild.read().clone())
                                                                    };
                                                                    save_parsely_guild_fields(
                                                                        guilds_snapshot,
                                                                        selected_snapshot,
                                                                        parsely_save_status,
                                                                    );
                                                                },
                                                                "×"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    div { class: "parsely-guilds-add",
                                        input {
                                            r#type: "text",
                                            placeholder: "Add a guild...",
                                            value: parsely_guild_input,
                                            oninput: move |e| parsely_guild_input.set(e.value()),
                                            onkeydown: move |e| {
                                                if e.key() == Key::Enter {
                                                    add_parsely_guild(
                                                        parsely_guild_input,
                                                        parsely_guilds,
                                                        parsely_selected_guild,
                                                        parsely_save_status,
                                                    );
                                                }
                                            }
                                        }
                                        button {
                                            class: "btn btn-secondary btn-sm",
                                            onclick: move |_| {
                                                add_parsely_guild(
                                                    parsely_guild_input,
                                                    parsely_guilds,
                                                    parsely_selected_guild,
                                                    parsely_save_status,
                                                );
                                            },
                                            "Add"
                                        }
                                    }
                                }
                            }
                            div { class: "settings-footer",
                                button {
                                    class: "btn btn-save",
                                    onclick: move |_| {
                                        let u = parsely_username();
                                        let p = parsely_password();
                                        let guilds = parsely_guilds.read().clone();
                                        let selected = parsely_selected_guild.read().clone();
                                        let mut toast = use_toast();
                                        spawn(async move {
                                            if let Some(mut cfg) = api::get_config().await {
                                                cfg.parsely.username = u;
                                                cfg.parsely.password = p;
                                                cfg.parsely.guild = String::new();
                                                cfg.parsely.guilds = guilds;
                                                cfg.parsely.selected_guild = selected;
                                                if let Err(err) = api::update_config(&cfg).await {
                                                    toast.show(format!("Failed to save Parsely settings: {}", err), ToastSeverity::Normal);
                                                } else {
                                                    parsely_save_status.set("Saved".to_string());
                                                }
                                            }
                                        });
                                    },
                                    "Save Parsely Settings"
                                }
                                span { class: "save-status", "{parsely_save_status}" }
                            }
                        }

                        div { class: "settings-section",
                            h4 { "Characters" }
                            LockoutResetSetting {}
                        }

                        div { class: "settings-section",
                            h4 { "Logs" }
                            CleanupRules {}
                        }

                        div { class: "settings-section",
                            h4 { "Name Detection" }
                            if is_macos() {
                                p { class: "hint hint-warning",
                                    i { class: "fa-solid fa-triangle-exclamation" }
                                    " Name detection is not supported on macOS."
                                }
                            }
                            label { class: "settings-check",
                                input {
                                    r#type: "checkbox",
                                    checked: ocr_debug_dump(),
                                    onchange: move |e| {
                                        let checked = e.checked();
                                        ocr_debug_dump.set(checked);
                                        let mut toast = use_toast();
                                        spawn(async move {
                                            if let Some(mut cfg) = api::get_config().await {
                                                cfg.ocr_debug_dump = checked;
                                                if let Err(err) = api::update_config(&cfg).await {
                                                    toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                }
                                            }
                                        });
                                    }
                                }
                                "Save detection images"
                            }
                            p { class: "hint", "Saves what name detection saw to baras/ocr-debug, so misread names can be diagnosed." }
                            div { class: "setting-row",
                                label { "Detections to keep" }
                                NumberInput {
                                    value: ocr_debug_max_dumps() as f64,
                                    min: 1.0,
                                    max: 1000.0,
                                    on_change: move |v: f64| {
                                        let count = v as u32;
                                        ocr_debug_max_dumps.set(count);
                                        let mut toast = use_toast();
                                        spawn(async move {
                                            if let Some(mut cfg) = api::get_config().await {
                                                cfg.ocr_debug_max_dumps = count;
                                                if let Err(err) = api::update_config(&cfg).await {
                                                    toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                }
                                            }
                                        });
                                    }
                                }
                            }
                            p { class: "hint", "The oldest images are removed once this limit is reached." }
                        }

                        } // settings-col
                        } // settings-content
                    }
                }

                // ─────────────────────────────────────────────────────────────
                // History Tab
                // ─────────────────────────────────────────────────────────────
                if let Some(view) = active_tab().history_view() {
                    HistoryPanel {
                        european: *european_number_format.read(),
                        state: ui_state,
                        view,
                    }
                }

                // ─────────────────────────────────────────────────────────────
                // Characters Tab
                // ─────────────────────────────────────────────────────────────
                if active_tab() == MainTab::Characters {
                    CharactersPanel {}
                }

                // ─────────────────────────────────────────────────────────────
                // Data Explorer Tab
                // ─────────────────────────────────────────────────────────────
                if active_tab() == MainTab::DataExplorer {
                    ErrorBoundary {
                        handle_error: |_errors: ErrorContext| {
                            rsx! {
                                div { class: "error-boundary-fallback",
                                    style: "display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; gap: 12px; color: var(--text-secondary, #999);",
                                    i { class: "fa-solid fa-triangle-exclamation", style: "font-size: 2rem;" }
                                    p { "Something went wrong." }
                                    button {
                                        class: "btn",
                                        onclick: move |_| {
                                            // Full page reload to recover from broken state
                                            if let Some(window) = web_sys::window() {
                                                let _ = window.location().reload();
                                            }
                                        },
                                        "Reload page"
                                    }
                                }
                            }
                        },
                        DataExplorerPanel {
                            state: ui_state,
                        }
                    }
                }
            }

            // Overlay settings: docked beside the Overlays page whenever it is open
            if active_tab() == MainTab::Overlays {
                SettingsPanel {
                    settings: overlay_settings,
                    selected_tab: selected_overlay_tab,
                    profile_names: profile_names,
                    active_profile: active_profile,
                    overlays_enabled,
                    overlays_visible: overlays_visible,
                    profile_dirty: profile_dirty,
                    section: settings_section,
                    on_settings_saved: move |_| {},
                }
            }

            // File browser modal
            if file_browser_open() {
                section { class: "drawer file-browser-drawer",

                    div { class: "file-browser-header",
                        div { class: "file-browser-header-top",
                            h3 {
                                i { class: "fa-solid fa-folder-open" }
                                " Log Files"
                            }
                            ToggleSwitch {
                                label: "Hide <1MB",
                                class: "file-browser-filter-toggle",
                                title: "Hide files smaller than 1MB",
                                checked: hide_small_log_files(),
                                on_change: move |checked: bool| {
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
                            button {
                                class: "btn btn-close",
                                title: "Close (Esc)",
                                "data-esc-close": true,
                                onclick: move |_| file_browser_open.set(false),
                                "X"
                            }
                        }
                        input {
                            class: "file-browser-search",
                            r#type: "text",
                            placeholder: "Filter by name, date, day, or operation (comma = AND, e.g. \"ToonName,Dxun\")...",
                            value: "{file_browser_filter}",
                            oninput: move |e| file_browser_filter.set(e.value()),
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
                                    // Text filter: comma-separated terms are ANDed
                                    // together (e.g. "X,Y" matches files with both X and Y).
                                    let terms: Vec<&str> = filter
                                        .split(',')
                                        .map(|t| t.trim())
                                        .filter(|t| !t.is_empty())
                                        .collect();
                                    if terms.is_empty() {
                                        return true;
                                    }
                                    let name = f.character_name.as_deref().unwrap_or("").to_lowercase();
                                    let date = f.date.to_lowercase();
                                    let day = f.day_of_week.to_lowercase();
                                    terms.iter().all(|term| {
                                        // Also check area names for operation search
                                        let areas_match = f.areas.as_ref().is_some_and(|areas| {
                                            areas.iter().any(|a| {
                                                a.display.to_lowercase().contains(term)
                                                    || a.area_name.to_lowercase().contains(term)
                                            })
                                        });
                                        name.contains(term) || date.contains(term) || day.contains(term) || areas_match
                                    })
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
                                    let upload_result = upload_status().get(&path).cloned()
                                        .or_else(|| file.parsely.clone().map(|link| (true, link)));
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
                                                    // Helper to get difficulty CSS class; prefers the
                                                    // ID-derived tier (language-independent), falls back
                                                    // to English string matching for unknown IDs
                                                    fn difficulty_class(area: &AreaVisitInfo) -> &'static str {
                                                        match area.difficulty_tier.as_deref() {
                                                            Some("fp") => return "area-tag diff-fp",
                                                            Some("nim") => return "area-tag diff-nim",
                                                            Some("hm") => return "area-tag diff-hm",
                                                            Some("sm") => return "area-tag diff-sm",
                                                            _ => {}
                                                        }
                                                        let lower = area.difficulty.to_lowercase();
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
                                                                        span { class: "{difficulty_class(area)}", "{area.display}" }
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
                                                                            li { class: "{difficulty_class(area)}", "{area.display}" }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                                // Failed upload this run; a link gets its own button
                                                if let Some((false, ref msg)) = upload_result {
                                                    span { class: "upload-status error", "{msg}" }
                                                }
                                            }
                                            div { class: "file-actions",
                                                button {
                                                    class: "btn btn-open",
                                                    disabled: is_empty || opener.busy(),
                                                    onclick: move |_| {
                                                        file_browser_open.set(false);
                                                        opener.open(path.clone(), None);
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
                                                if let Some((true, url)) = upload_result.clone() {
                                                    button {
                                                        class: "btn btn-upload",
                                                        title: "Open on Parsely: {url}",
                                                        onclick: move |_| {
                                                            let u = url.clone();
                                                            spawn(async move { api::open_url(&u).await });
                                                        },
                                                        i { class: "fa-solid fa-up-right-from-square" }
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

            if screen_capture_permission_open() {
                div {
                    class: "modal-backdrop",
                    div {
                        class: "changelog-modal screen-capture-modal",
                        onclick: move |e| e.stop_propagation(),
                        div { class: "changelog-header screen-capture-header",
                            h3 {
                                i { class: "fa-solid fa-display" }
                                " Screen capture permission"
                            }
                            button {
                                class: "btn btn-close",
                                disabled: screen_capture_starting(),
                                onclick: move |_| screen_capture_permission_open.set(false),
                                "X"
                            }
                        }
                        div { class: "screen-capture-content",
                            p { "BARAS needs access to your screen to detect raid-frame names." }
                            p {
                                "Your desktop doesn't support BARAS's direct capture method. You will be asked to choose the monitor where SWTOR is running."
                            }
                            div { class: "screen-capture-note",
                                i { class: "fa-solid fa-shield-halved" }
                                div {
                                    strong { "What BARAS captures" }
                                    span { "Only the configured raid-frame area is processed for OCR. Debug captures are saved only when OCR debug dumps are enabled." }
                                }
                            }
                            if let Some(error) = screen_capture_error() {
                                div { class: "screen-capture-error", "{error}" }
                            }
                        }
                        div { class: "screen-capture-footer",
                            button {
                                class: "btn btn-secondary",
                                disabled: screen_capture_starting(),
                                onclick: move |_| screen_capture_permission_open.set(false),
                                "Not now"
                            }
                            button {
                                class: "btn btn-primary",
                                disabled: screen_capture_starting(),
                                onclick: move |_| {
                                    screen_capture_starting.set(true);
                                    screen_capture_error.set(None);
                                    spawn(async move {
                                        match api::enable_screen_capture().await {
                                            Ok(()) => screen_capture_permission_open.set(false),
                                            Err(error) if error.to_lowercase().contains("declined") => {
                                                screen_capture_permission_open.set(false);
                                            }
                                            Err(error) => screen_capture_error.set(Some(error)),
                                        }
                                        screen_capture_starting.set(false);
                                    });
                                },
                                i { class: "fa-solid fa-display" }
                                if screen_capture_starting() { " Waiting..." } else { " Choose monitor" }
                            }
                        }
                    }
                }
            }

            // Contributors modal
            ContributorsModal { open: contributors_open }

            // Parsely upload modal
            ParselyUploadModal {
                guilds: parsely_guilds.read().clone(),
                selected_guild: parsely_selected_guild,
            }

            // Toast notifications (rendered on top of everything)
            FileOpenBanner {}
            ToastFrame {}
        }
    }
}
