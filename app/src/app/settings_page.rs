//! The Settings tab: log directory, application options, global hotkeys, audio,
//! Parsely, characters, log cleanup and name detection. Every control saves as
//! it changes. State only this page edits lives here and is read from the
//! config when the page opens; state the rest of the app also uses comes in as props.

use dioxus::prelude::*;

use super::helpers::{
    add_parsely_guild, is_linux, is_macos, save_parsely_guild_fields, save_setting, set_light_theme,
};
use crate::api;
use crate::components::{
    CleanupRules, HotkeyInput, LockoutResetSetting, NumberInput, Slider, ToastSeverity, use_toast,
};
use crate::types::{UiSessionState, WEB_OVERLAY_URL};

#[component]
pub(super) fn SettingsPage(
    mut ui_state: Signal<UiSessionState>,
    mut log_directory: Signal<String>,
    mut is_watching: Signal<bool>,
    mut european_number_format: Signal<bool>,
    light_theme: Signal<bool>,
    mut parsely_guilds: Signal<Vec<String>>,
    mut parsely_selected_guild: Signal<Option<String>>,
) -> Element {
    // Log directory stats
    let mut log_dir_size = use_signal(|| 0u64);
    let mut log_file_count = use_signal(|| 0usize);

    // Application
    let mut minimize_to_tray = use_signal(|| true);
    let mut web_overlay_enabled = use_signal(|| false);
    let mut parsely_percentiles = use_signal(|| true);

    // Global hotkeys
    let mut hotkey_visibility = use_signal(String::new);
    let mut hotkey_move_mode = use_signal(String::new);
    let mut hotkey_rearrange = use_signal(String::new);
    let mut hotkey_op_timer = use_signal(String::new);
    let mut hotkey_live_mode = use_signal(String::new);
    let mut hotkey_detect_raid = use_signal(String::new);
    let mut is_wayland_session = use_signal(|| false);

    // Audio
    let mut audio_enabled = use_signal(|| true);
    let mut audio_volume = use_signal(|| 80u8);
    let mut audio_tts_enabled = use_signal(|| true);

    // Parsely
    let mut parsely_username = use_signal(String::new);
    let mut parsely_password = use_signal(String::new);
    let mut parsely_guild_input = use_signal(String::new);

    // Name detection
    let mut ocr_debug_dump = use_signal(|| false);
    let mut ocr_debug_max_dumps = use_signal(|| 100u32);

    // Nothing renders until the stored values are in, so controls never flash their defaults
    let mut loaded = use_signal(|| false);
    use_future(move || async move {
        if let Some(config) = api::get_config().await {
            minimize_to_tray.set(config.minimize_to_tray);
            web_overlay_enabled.set(config.web_overlay_enabled);
            parsely_percentiles.set(config.parsely_percentiles);
            hotkey_visibility.set(config.hotkeys.toggle_visibility.unwrap_or_default());
            hotkey_move_mode.set(config.hotkeys.toggle_move_mode.unwrap_or_default());
            hotkey_rearrange.set(config.hotkeys.toggle_rearrange_mode.unwrap_or_default());
            hotkey_op_timer.set(config.hotkeys.toggle_operation_timer.unwrap_or_default());
            hotkey_live_mode.set(config.hotkeys.toggle_live_mode.unwrap_or_default());
            hotkey_detect_raid.set(config.hotkeys.detect_raid_names.unwrap_or_default());
            audio_enabled.set(config.audio.enabled);
            audio_volume.set(config.audio.volume);
            audio_tts_enabled.set(config.audio.tts_enabled);
            parsely_username.set(config.parsely.username);
            parsely_password.set(config.parsely.password);
            ocr_debug_dump.set(config.ocr_debug_dump);
            ocr_debug_max_dumps.set(config.ocr_debug_max_dumps);
        }
        loaded.set(true);
        is_wayland_session.set(api::is_wayland_session().await);
        log_dir_size.set(api::get_log_directory_size().await);
        log_file_count.set(api::get_log_file_count().await);
    });

    let current_dir = log_directory();
    let watching = is_watching();

    if !loaded() {
        return rsx! { section { class: "settings-page" } };
    }

    rsx! {
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
                                            if let Err(err) = api::restart_watcher().await {
                                                toast.show(format!("Couldn't restart the log watcher: {err}"), ToastSeverity::Normal);
                                            }
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
                            save_setting("settings", move |cfg| cfg.minimize_to_tray = checked);
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
                label { class: "settings-check",
                    input {
                        r#type: "checkbox",
                        checked: parsely_percentiles(),
                        onchange: move |e| {
                            let checked = e.checked();
                            parsely_percentiles.set(checked);
                            ui_state.write().parsely_percentiles = checked;
                            save_setting("settings", move |cfg| cfg.parsely_percentiles = checked);
                        }
                    }
                    "Parsely percentiles"
                }
                p { class: "hint", "Rates each player against their discipline on operation boss kills in the Data Explorer. Downloads the boss's rating table from parsely.io at most once a day." }
            }

            div { class: "settings-section",
                h4 { "Global Hotkeys" }
                p { class: "hint", "Click a field and press a key combination; Backspace clears it. Saved as you set them; they take effect after a restart." }
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
                            on_change: move |v: String| {
                                hotkey_visibility.set(v.clone());
                                save_setting("hotkey", move |cfg| cfg.hotkeys.toggle_visibility = (!v.is_empty()).then_some(v));
                            },
                        }
                    }
                    div { class: "setting-row",
                        label { "Lock/Unlock Overlays" }
                        HotkeyInput {
                            value: hotkey_move_mode(),
                            on_change: move |v: String| {
                                hotkey_move_mode.set(v.clone());
                                save_setting("hotkey", move |cfg| cfg.hotkeys.toggle_move_mode = (!v.is_empty()).then_some(v));
                            },
                        }
                    }
                    div { class: "setting-row",
                        label { "Toggle Raid Frame Interactivity" }
                        HotkeyInput {
                            value: hotkey_rearrange(),
                            on_change: move |v: String| {
                                hotkey_rearrange.set(v.clone());
                                save_setting("hotkey", move |cfg| cfg.hotkeys.toggle_rearrange_mode = (!v.is_empty()).then_some(v));
                            },
                        }
                    }
                    div { class: "setting-row",
                        label { "Start/Stop Operations Timer" }
                        HotkeyInput {
                            value: hotkey_op_timer(),
                            on_change: move |v: String| {
                                hotkey_op_timer.set(v.clone());
                                save_setting("hotkey", move |cfg| cfg.hotkeys.toggle_operation_timer = (!v.is_empty()).then_some(v));
                            },
                        }
                    }
                    div { class: "setting-row",
                        label { "Run Raid Frame OCR Name Recognition" }
                        HotkeyInput {
                            value: hotkey_detect_raid(),
                            on_change: move |v: String| {
                                hotkey_detect_raid.set(v.clone());
                                save_setting("hotkey", move |cfg| cfg.hotkeys.detect_raid_names = (!v.is_empty()).then_some(v));
                            },
                        }
                    }
                    div { class: "setting-row",
                        label { "Resume Live Tailing" }
                        HotkeyInput {
                            value: hotkey_live_mode(),
                            on_change: move |v: String| {
                                hotkey_live_mode.set(v.clone());
                                save_setting("hotkey", move |cfg| cfg.hotkeys.toggle_live_mode = (!v.is_empty()).then_some(v));
                            },
                        }
                    }
                }
                if is_macos() {
                    p { class: "hint hint-warning",
                        i { class: "fa-solid fa-triangle-exclamation" }
                        " Raid Frame OCR Name Recognition is not supported on macOS."
                    }
                }
                div { class: "app-shortcuts",
                    h5 { "In-app Shortcuts" }
                    ul {
                        for (key, action) in [
                            ("Esc", "Close the topmost dialog, drawer or fullscreen view"),
                            ("L", "Toggle the log file browser"),
                            ("[  ]", "Previous / next tab"),
                            ("↑  ↓", "Previous / next encounter in Analysis"),
                            ("1 – 0", "Pick an Analysis view"),
                            ("Ctrl+S", "Save the open timer or effect form"),
                            ("S", "Collapse / expand the sidebar"),
                            ("F", "Toggle Analysis fullscreen"),
                            ("X", "Clear all filters"),
                        ] {
                            li { kbd { "{key}" } span { "{action}" } }
                        }
                    }
                    p { class: "hint", "Ignored while typing in a field, except Ctrl+S." }
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
                            save_setting("settings", move |cfg| cfg.audio.enabled = checked);
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
                        save_setting("settings", move |cfg| cfg.audio.volume = val);
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
                            save_setting("settings", move |cfg| cfg.audio.tts_enabled = checked);
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
                            let mut toast = use_toast();
                            spawn(async move {
                                if let Err(err) = api::preview_sound("Alert.mp3").await {
                                    toast.show(format!("Couldn't play the test sound: {err}"), ToastSeverity::Normal);
                                }
                            });
                        },
                        "Test sound"
                    }
                    button {
                        class: "btn",
                        disabled: !audio_enabled() || !audio_tts_enabled(),
                        onclick: move |_| {
                            let mut toast = use_toast();
                            spawn(async move {
                                if let Err(err) = api::preview_tts().await {
                                    toast.show(format!("Couldn't play text-to-speech: {err}"), ToastSeverity::Normal);
                                }
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
                        oninput: move |e| parsely_username.set(e.value()),
                        // Saved when the field is left
                        onchange: move |e| {
                            let v = e.value();
                            save_setting("Parsely username", move |cfg| cfg.parsely.username = v);
                        },
                    }
                }
                div { class: "setting-row",
                    label { "Password" }
                    input {
                        r#type: "password",
                        placeholder: "Optional",
                        value: parsely_password,
                        oninput: move |e| parsely_password.set(e.value()),
                        onchange: move |e| {
                            let v = e.value();
                            save_setting("Parsely password", move |cfg| cfg.parsely.password = v);
                        },
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
                                    );
                                },
                                "Add"
                            }
                        }
                    }
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
                            save_setting("settings", move |cfg| cfg.ocr_debug_dump = checked);
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
                            save_setting("settings", move |cfg| cfg.ocr_debug_max_dumps = count);
                        }
                    }
                }
                p { class: "hint", "The oldest images are removed once this limit is reached." }
            }

            } // settings-col
            } // settings-content
        }
    }
}
