//! Settings panel component for overlay configuration
//!
//! Floating, draggable panel for customizing overlay appearances,
//! personal stats, and raid frame settings.

use dioxus::prelude::*;
use std::collections::HashMap;

use crate::api;
use crate::types::{
    AlertsOverlayConfig, BossHealthConfig, ChallengeLayout, CooldownTrackerConfig,
    DotTrackerConfig, MAX_PROFILES, MetricType, OverlayAppearanceConfig, OverlaySettings,
    PersonalBuffsConfig, PersonalDebuffsConfig, PersonalOverlayConfig, PersonalStat,
    RaidOverlaySettings, TimerOverlayConfig,
};
use crate::utils::{color_to_hex, parse_hex_color};

#[component]
pub fn SettingsPanel(
    settings: Signal<OverlaySettings>,
    selected_tab: Signal<String>,
    profile_names: Signal<Vec<String>>,
    active_profile: Signal<Option<String>>,
    metric_overlays_enabled: Signal<HashMap<MetricType, bool>>,
    personal_enabled: Signal<bool>,
    raid_enabled: Signal<bool>,
    overlays_visible: Signal<bool>,
    on_close: EventHandler<()>,
    on_header_mousedown: EventHandler<MouseEvent>,
) -> Element {
    // Local draft of settings being edited
    #[allow(clippy::redundant_closure)]
    let mut draft_settings = use_signal(|| settings());
    let mut has_changes = use_signal(|| false);
    let mut save_status = use_signal(String::new);

    // Profile UI state
    let mut new_profile_name = use_signal(String::new);
    let mut profile_status = use_signal(String::new);

    let current_settings = draft_settings();
    let tab = selected_tab();

    // Get appearance for current tab
    let get_appearance = |key: &str| -> OverlayAppearanceConfig {
        current_settings
            .appearances
            .get(key)
            .cloned()
            .or_else(|| current_settings.default_appearances.get(key).cloned())
            .unwrap_or_default()
    };

    let current_appearance = get_appearance(&tab);

    // Pre-compute hex color strings
    let bar_color_hex = color_to_hex(&current_appearance.bar_color);
    let font_color_hex = color_to_hex(&current_appearance.font_color);
    let personal_font_color_hex = color_to_hex(&current_settings.personal_overlay.font_color);
    let personal_label_font_color_hex =
        color_to_hex(&current_settings.personal_overlay.label_color);
    let boss_bar_hex = color_to_hex(&current_settings.boss_health.bar_color);

    // Save settings to backend
    let save_to_backend = move |_| {
        let new_settings = draft_settings();
        async move {
            if let Some(mut config) = api::get_config().await {
                // Preserve existing positions and enabled state
                let existing_positions = config.overlay_settings.positions.clone();
                let existing_enabled = config.overlay_settings.enabled.clone();

                config.overlay_settings.appearances = new_settings.appearances.clone();
                config.overlay_settings.personal_overlay = new_settings.personal_overlay.clone();
                config.overlay_settings.metric_opacity = new_settings.metric_opacity;
                config.overlay_settings.metric_show_empty_bars = new_settings.metric_show_empty_bars;
                config.overlay_settings.metric_stack_from_bottom = new_settings.metric_stack_from_bottom;
                config.overlay_settings.metric_scaling_factor = new_settings.metric_scaling_factor;
                config.overlay_settings.personal_opacity = new_settings.personal_opacity;
                config.overlay_settings.raid_overlay = new_settings.raid_overlay.clone();
                config.overlay_settings.raid_opacity = new_settings.raid_opacity;
                config.overlay_settings.boss_health = new_settings.boss_health.clone();
                config.overlay_settings.boss_health_opacity = new_settings.boss_health_opacity;
                config.overlay_settings.timer_overlay = new_settings.timer_overlay.clone();
                config.overlay_settings.timer_opacity = new_settings.timer_opacity;
                config.overlay_settings.effects_overlay = new_settings.effects_overlay.clone();
                config.overlay_settings.effects_opacity = new_settings.effects_opacity;
                config.overlay_settings.challenge_overlay = new_settings.challenge_overlay.clone();
                config.overlay_settings.challenge_opacity = new_settings.challenge_opacity;
                config.overlay_settings.alerts_overlay = new_settings.alerts_overlay.clone();
                config.overlay_settings.alerts_opacity = new_settings.alerts_opacity;
                config.overlay_settings.personal_buffs = new_settings.personal_buffs.clone();
                config.overlay_settings.personal_buffs_opacity = new_settings.personal_buffs_opacity;
                config.overlay_settings.personal_debuffs = new_settings.personal_debuffs.clone();
                config.overlay_settings.personal_debuffs_opacity = new_settings.personal_debuffs_opacity;
                config.overlay_settings.cooldown_tracker = new_settings.cooldown_tracker.clone();
                config.overlay_settings.cooldown_tracker_opacity = new_settings.cooldown_tracker_opacity;
                config.overlay_settings.dot_tracker = new_settings.dot_tracker.clone();
                config.overlay_settings.dot_tracker_opacity = new_settings.dot_tracker_opacity;
                config.overlay_settings.positions = existing_positions;
                config.overlay_settings.enabled = existing_enabled;

                if api::update_config(&config).await {
                    api::refresh_overlay_settings().await;
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
            // Header
            div {
                class: "settings-header draggable",
                onmousedown: move |e| on_header_mousedown.call(e),
                h3 { "Overlay Settings" }
                button {
                    class: "btn btn-close",
                    onclick: move |_| on_close.call(()),
                    onmousedown: move |e| e.stop_propagation(),
                    "X"
                }
            }

            // ─────────────────────────────────────────────────────────────────
            // Profiles section (inline)
            // ─────────────────────────────────────────────────────────────────
            details { class: "settings-section collapsible",
                summary { class: "collapsible-summary",
                    i { class: "fa-solid fa-user-gear summary-icon" }
                    "Profiles"
                    if let Some(ref name) = active_profile() {
                        span { class: "profile-active-badge", "{name}" }
                    }
                }
                div { class: "collapsible-content",
                    // Profile list
                    if !profile_names().is_empty() {
                        div { class: "profile-list compact",
                            for name in profile_names().iter() {
                                {
                                    let profile_name = name.clone();
                                    let is_active = active_profile().as_ref() == Some(&profile_name);
                                    rsx! {
                                        div {
                                            key: "{profile_name}",
                                            class: if is_active { "profile-item active" } else { "profile-item" },
                                            span { class: "profile-name", "{profile_name}" }
                                            div { class: "profile-actions",
                                                // Load button
                                                button {
                                                    class: "btn btn-small btn-load",
                                                    disabled: is_active,
                                                    onclick: {
                                                        let pname = profile_name.clone();
                                                        move |_| {
                                                            let pname = pname.clone();
                                                            spawn(async move {
                                                                if api::load_profile(&pname).await {
                                                                    active_profile.set(Some(pname.clone()));
                                                                    profile_status.set(format!("Loaded '{}'", pname));
                                                                    if let Some(config) = api::get_config().await {
                                                                        draft_settings.set(config.overlay_settings.clone());
                                                                        settings.set(config.overlay_settings);
                                                                    }
                                                                    api::refresh_overlay_settings().await;
                                                                    if let Some(status) = api::get_overlay_status().await {
                                                                        let new_map: HashMap<MetricType, bool> = MetricType::all()
                                                                            .iter()
                                                                            .map(|ot| (*ot, status.enabled.contains(&ot.config_key().to_string())))
                                                                            .collect();
                                                                        metric_overlays_enabled.set(new_map);
                                                                        personal_enabled.set(status.personal_enabled);
                                                                        raid_enabled.set(status.raid_enabled);
                                                                        overlays_visible.set(status.overlays_visible);
                                                                    }
                                                                }
                                                            });
                                                        }
                                                    },
                                                    "Load"
                                                }
                                                // Save button
                                                button {
                                                    class: "btn btn-small btn-update",
                                                    title: "Overwrite profile with current settings",
                                                    onclick: {
                                                        let pname = profile_name.clone();
                                                        move |_| {
                                                            let pname = pname.clone();
                                                            spawn(async move {
                                                                if api::save_profile(&pname).await {
                                                                    active_profile.set(Some(pname.clone()));
                                                                    profile_status.set(format!("Saved '{}'", pname));
                                                                }
                                                            });
                                                        }
                                                    },
                                                    "Save"
                                                }
                                                // Delete button
                                                button {
                                                    class: "btn btn-small btn-delete",
                                                    onclick: {
                                                        let pname = profile_name.clone();
                                                        move |_| {
                                                            let pname = pname.clone();
                                                            spawn(async move {
                                                                if api::delete_profile(&pname).await {
                                                                    profile_names.set(api::get_profile_names().await);
                                                                    active_profile.set(api::get_active_profile().await);
                                                                    profile_status.set(format!("Deleted '{}'", pname));
                                                                }
                                                            });
                                                        }
                                                    },
                                                    "×"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Create new profile
                    div { class: "profile-create",
                        input {
                            r#type: "text",
                            class: "profile-name-input",
                            placeholder: "New profile name...",
                            maxlength: "32",
                            value: new_profile_name,
                            oninput: move |e| new_profile_name.set(e.value())
                        }
                        button {
                            class: "btn btn-small btn-save",
                            disabled: new_profile_name().trim().is_empty() || profile_names().len() >= MAX_PROFILES,
                            onclick: move |_| {
                                let name = new_profile_name().trim().to_string();
                                if name.is_empty() { return; }
                                spawn(async move {
                                    if api::save_profile(&name).await {
                                        profile_names.set(api::get_profile_names().await);
                                        active_profile.set(Some(name.clone()));
                                        new_profile_name.set(String::new());
                                        profile_status.set(format!("Created '{}'", name));
                                    }
                                });
                            },
                            "+ New"
                        }
                    }

                    if profile_names().len() >= MAX_PROFILES {
                        p { class: "hint hint-warning compact", "Maximum {MAX_PROFILES} profiles" }
                    }
                    if !profile_status().is_empty() {
                        p { class: "profile-status compact", "{profile_status}" }
                    }
                }
            }

            // ─────────────────────────────────────────────────────────────────
            // Tabs for overlay types
            // ─────────────────────────────────────────────────────────────────
            div { class: "settings-tabs",
                div { class: "tab-group",
                    span { class: "tab-group-label", "General" }
                    div { class: "tab-group-buttons",
                        TabButton { label: "Personal Stats", tab_key: "personal", selected_tab: selected_tab }
                        TabButton { label: "Raid Frames", tab_key: "raid", selected_tab: selected_tab }
                        TabButton { label: "Boss Health", tab_key: "boss_health", selected_tab: selected_tab }
                        TabButton { label: "Timers", tab_key: "timers", selected_tab: selected_tab }
                        TabButton { label: "Challenges", tab_key: "challenges", selected_tab: selected_tab }
                        TabButton { label: "Alerts", tab_key: "alerts", selected_tab: selected_tab }
                    }
                }
                div { class: "tab-group",
                    span { class: "tab-group-label", "Effects" }
                    div { class: "tab-group-buttons",
                        TabButton { label: "Personal Buffs", tab_key: "personal_buffs", selected_tab: selected_tab }
                        TabButton { label: "Personal Debuffs", tab_key: "personal_debuffs", selected_tab: selected_tab }
                        TabButton { label: "Cooldowns", tab_key: "cooldowns", selected_tab: selected_tab }
                        TabButton { label: "DOT Tracker", tab_key: "dot_tracker", selected_tab: selected_tab }
                    }
                }
                div { class: "tab-group",
                    span { class: "tab-group-label", "Metrics" }
                    div { class: "tab-group-buttons",
                        for overlay_type in MetricType::all() {
                            TabButton {
                                key: "{overlay_type.config_key()}",
                                label: overlay_type.label(),
                                tab_key: overlay_type.config_key(),
                                selected_tab: selected_tab,
                            }
                        }
                    }
                    details { class: "settings-section collapsible metrics-global",
                        summary { class: "collapsible-summary",
                            i { class: "fa-solid fa-sliders summary-icon" }
                            "Global Metrics Settings"
                        }
                        div { class: "collapsible-content",
                            div { class: "setting-row",
                                label { "Background Opacity" }
                                input {
                                    r#type: "range",
                                    min: "0",
                                    max: "255",
                                    value: "{current_settings.metric_opacity}",
                                    oninput: move |e| {
                                        if let Ok(val) = e.value().parse::<u8>() {
                                            let mut new_settings = draft_settings();
                                            new_settings.metric_opacity = val;
                                            update_draft(new_settings);
                                        }
                                    }
                                }
                                span { class: "value", "{current_settings.metric_opacity}" }
                            }

                            div { class: "setting-row",
                                label { "Bar Thickness" }
                                input {
                                    r#type: "range",
                                    min: "100",
                                    max: "200",
                                    value: "{(current_settings.metric_scaling_factor * 100.0) as i32}",
                                    oninput: move |e| {
                                        if let Ok(val) = e.value().parse::<i32>() {
                                            let mut new_settings = draft_settings();
                                            new_settings.metric_scaling_factor = val as f32 / 100.0;
                                            update_draft(new_settings);
                                        }
                                    }
                                }
                                span { class: "value", "{(current_settings.metric_scaling_factor * 100.0) as i32}%" }
                            }

                            div { class: "setting-row",
                                label { "Show Empty Bars" }
                                input {
                                    r#type: "checkbox",
                                    checked: current_settings.metric_show_empty_bars,
                                    onchange: move |e: Event<FormData>| {
                                        let mut new_settings = draft_settings();
                                        new_settings.metric_show_empty_bars = e.checked();
                                        update_draft(new_settings);
                                    }
                                }
                            }

                            div { class: "setting-row",
                                label { "Stack From Bottom" }
                                input {
                                    r#type: "checkbox",
                                    checked: current_settings.metric_stack_from_bottom,
                                    onchange: move |e: Event<FormData>| {
                                        let mut new_settings = draft_settings();
                                        new_settings.metric_stack_from_bottom = e.checked();
                                        update_draft(new_settings);
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ─────────────────────────────────────────────────────────────────
            // Per-overlay settings content (inline)
            // ─────────────────────────────────────────────────────────────────
            if tab == "boss_health" {
                // Boss Health Settings
                div { class: "settings-section",
                    h4 { "Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.boss_health_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft_settings();
                            new_settings.boss_health_opacity = val;
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Bar Color" }
                        input {
                            r#type: "color",
                            value: "{boss_bar_hex}",
                            class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft_settings();
                                    new_settings.boss_health.bar_color = color;
                                    update_draft(new_settings);
                                }
                            }
                        }
                    }

                            div { class: "setting-row",
                                label { "Show current target" }
                                input {
                                    r#type: "checkbox",
                                    checked: current_settings.boss_health.show_target,
                                    onchange: move |e: Event<FormData>| {
                                        let mut new_settings = draft_settings();
                                        new_settings.boss_health.show_target = e.checked();
                                        update_draft(new_settings);
                                    }
                                }
                            }

                    div { class: "setting-row reset-row",
                        button {
                            class: "btn btn-reset",
                            onclick: move |_| {
                                let mut new_settings = draft_settings();
                                new_settings.boss_health = BossHealthConfig::default();
                                new_settings.boss_health_opacity = 180;
                                update_draft(new_settings);
                            },
                            i { class: "fa-solid fa-rotate-left" }
                            span { " Reset Style" }
                        }
                    }
                }
            } else if tab == "timers" {
                // Timer Settings
                div { class: "settings-section",
                    h4 { "Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.timer_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft_settings();
                            new_settings.timer_opacity = val;
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Font Color" }
                        input {
                            r#type: "color",
                            value: "{color_to_hex(&current_settings.timer_overlay.font_color)}",
                            class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft_settings();
                                    new_settings.timer_overlay.font_color = color;
                                    update_draft(new_settings);
                                }
                            }
                        }
                    }

                    div { class: "setting-row reset-row",
                        button {
                            class: "btn btn-reset",
                            onclick: move |_| {
                                let mut new_settings = draft_settings();
                                new_settings.timer_overlay = TimerOverlayConfig::default();
                                new_settings.timer_opacity = 180;
                                update_draft(new_settings);
                            },
                            i { class: "fa-solid fa-rotate-left" }
                            span { " Reset Style" }
                        }
                    }
                }
            } else if tab == "personal_buffs" {
                // Personal Buffs Settings
                div { class: "settings-section",
                    h4 { "Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.personal_buffs_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft_settings();
                            new_settings.personal_buffs_opacity = val;
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Icon Size" }
                        input {
                            r#type: "range",
                            min: "16",
                            max: "64",
                            value: "{current_settings.personal_buffs.icon_size}",
                            oninput: move |e| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let mut new_settings = draft_settings();
                                    new_settings.personal_buffs.icon_size = val.clamp(16, 64);
                                    update_draft(new_settings);
                                }
                            }
                        }
                        span { class: "value", "{current_settings.personal_buffs.icon_size}px" }
                    }

                    div { class: "setting-row",
                        label { "Max Displayed" }
                        select {
                            class: "input-inline",
                            value: "{current_settings.personal_buffs.max_display}",
                            onchange: move |e: Event<FormData>| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let mut new_settings = draft_settings();
                                    new_settings.personal_buffs.max_display = val.clamp(1, 16);
                                    update_draft(new_settings);
                                }
                            },
                            for n in 1..=16u8 {
                                option { value: "{n}", selected: current_settings.personal_buffs.max_display == n, "{n}" }
                            }
                        }
                    }

                    h4 { style: "margin-top: 16px;", "Display Options" }

                    div { class: "setting-row",
                        label { "Show Countdown" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.personal_buffs.show_countdown,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.personal_buffs.show_countdown = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show Effect Names" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.personal_buffs.show_effect_names,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.personal_buffs.show_effect_names = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Prioritize Stacked Effects" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.personal_buffs.stack_priority,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.personal_buffs.stack_priority = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row reset-row",
                        button {
                            class: "btn btn-reset",
                            onclick: move |_| {
                                let mut new_settings = draft_settings();
                                new_settings.personal_buffs = PersonalBuffsConfig::default();
                                new_settings.personal_buffs_opacity = 180;
                                update_draft(new_settings);
                            },
                            i { class: "fa-solid fa-rotate-left" }
                            span { " Reset to Defaults" }
                        }
                    }
                }
            } else if tab == "personal_debuffs" {
                // Personal Debuffs Settings
                div { class: "settings-section",
                    h4 { "Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.personal_debuffs_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft_settings();
                            new_settings.personal_debuffs_opacity = val;
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Icon Size" }
                        input {
                            r#type: "range",
                            min: "16",
                            max: "64",
                            value: "{current_settings.personal_debuffs.icon_size}",
                            oninput: move |e| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let mut new_settings = draft_settings();
                                    new_settings.personal_debuffs.icon_size = val.clamp(16, 64);
                                    update_draft(new_settings);
                                }
                            }
                        }
                        span { class: "value", "{current_settings.personal_debuffs.icon_size}px" }
                    }

                    div { class: "setting-row",
                        label { "Max Displayed" }
                        select {
                            class: "input-inline",
                            value: "{current_settings.personal_debuffs.max_display}",
                            onchange: move |e: Event<FormData>| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let mut new_settings = draft_settings();
                                    new_settings.personal_debuffs.max_display = val.clamp(1, 16);
                                    update_draft(new_settings);
                                }
                            },
                            for n in 1..=16u8 {
                                option { value: "{n}", selected: current_settings.personal_debuffs.max_display == n, "{n}" }
                            }
                        }
                    }

                    h4 { style: "margin-top: 16px;", "Display Options" }

                    div { class: "setting-row",
                        label { "Show Countdown" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.personal_debuffs.show_countdown,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.personal_debuffs.show_countdown = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show Effect Names" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.personal_debuffs.show_effect_names,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.personal_debuffs.show_effect_names = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Highlight Cleansable" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.personal_debuffs.highlight_cleansable,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.personal_debuffs.highlight_cleansable = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show Source Name" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.personal_debuffs.show_source_name,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.personal_debuffs.show_source_name = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Prioritize Stacked Effects" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.personal_debuffs.stack_priority,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.personal_debuffs.stack_priority = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row reset-row",
                        button {
                            class: "btn btn-reset",
                            onclick: move |_| {
                                let mut new_settings = draft_settings();
                                new_settings.personal_debuffs = PersonalDebuffsConfig::default();
                                new_settings.personal_debuffs_opacity = 180;
                                update_draft(new_settings);
                            },
                            i { class: "fa-solid fa-rotate-left" }
                            span { " Reset to Defaults" }
                        }
                    }
                }
            } else if tab == "cooldowns" {
                // Cooldowns Settings
                div { class: "settings-section",
                    h4 { "Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.cooldown_tracker_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft_settings();
                            new_settings.cooldown_tracker_opacity = val;
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Icon Size" }
                        input {
                            r#type: "range",
                            min: "16",
                            max: "64",
                            value: "{current_settings.cooldown_tracker.icon_size}",
                            oninput: move |e| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let mut new_settings = draft_settings();
                                    new_settings.cooldown_tracker.icon_size = val.clamp(16, 64);
                                    update_draft(new_settings);
                                }
                            }
                        }
                        span { class: "value", "{current_settings.cooldown_tracker.icon_size}px" }
                    }

                    div { class: "setting-row",
                        label { "Max Displayed" }
                        select {
                            class: "input-inline",
                            value: "{current_settings.cooldown_tracker.max_display}",
                            onchange: move |e: Event<FormData>| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let mut new_settings = draft_settings();
                                    new_settings.cooldown_tracker.max_display = val.clamp(1, 20);
                                    update_draft(new_settings);
                                }
                            },
                            for n in 1..=20u8 {
                                option { value: "{n}", selected: current_settings.cooldown_tracker.max_display == n, "{n}" }
                            }
                        }
                    }

                    h4 { style: "margin-top: 16px;", "Display Options" }

                    div { class: "setting-row",
                        label { "Show Ability Names" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.cooldown_tracker.show_ability_names,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.cooldown_tracker.show_ability_names = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Sort by Remaining Time" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.cooldown_tracker.sort_by_remaining,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.cooldown_tracker.sort_by_remaining = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row reset-row",
                        button {
                            class: "btn btn-reset",
                            onclick: move |_| {
                                let mut new_settings = draft_settings();
                                new_settings.cooldown_tracker = CooldownTrackerConfig::default();
                                new_settings.cooldown_tracker_opacity = 180;
                                update_draft(new_settings);
                            },
                            i { class: "fa-solid fa-rotate-left" }
                            span { " Reset to Defaults" }
                        }
                    }
                }
            } else if tab == "dot_tracker" {
                // DOT Tracker Settings
                div { class: "settings-section",
                    h4 { "Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.dot_tracker_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft_settings();
                            new_settings.dot_tracker_opacity = val;
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Icon Size" }
                        input {
                            r#type: "range",
                            min: "12",
                            max: "48",
                            value: "{current_settings.dot_tracker.icon_size}",
                            oninput: move |e| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let mut new_settings = draft_settings();
                                    new_settings.dot_tracker.icon_size = val.clamp(12, 48);
                                    update_draft(new_settings);
                                }
                            }
                        }
                        span { class: "value", "{current_settings.dot_tracker.icon_size}px" }
                    }

                    div { class: "setting-row",
                        label { "Max Targets" }
                        select {
                            class: "input-inline",
                            value: "{current_settings.dot_tracker.max_targets}",
                            onchange: move |e: Event<FormData>| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let mut new_settings = draft_settings();
                                    new_settings.dot_tracker.max_targets = val.clamp(1, 10);
                                    update_draft(new_settings);
                                }
                            },
                            for n in 1..=10u8 {
                                option { value: "{n}", selected: current_settings.dot_tracker.max_targets == n, "{n}" }
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Prune Delay" }
                        input {
                            r#type: "range",
                            min: "0",
                            max: "10",
                            step: "0.5",
                            value: "{current_settings.dot_tracker.prune_delay_secs}",
                            oninput: move |e| {
                                if let Ok(val) = e.value().parse::<f32>() {
                                    let mut new_settings = draft_settings();
                                    new_settings.dot_tracker.prune_delay_secs = val.clamp(0.0, 10.0);
                                    update_draft(new_settings);
                                }
                            }
                        }
                        span { class: "value", "{current_settings.dot_tracker.prune_delay_secs:.1}s" }
                    }

                    h4 { style: "margin-top: 16px;", "Display Options" }

                    div { class: "setting-row",
                        label { "Show Effect Names" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.dot_tracker.show_effect_names,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.dot_tracker.show_effect_names = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Font Color" }
                        input {
                            r#type: "color",
                            value: "{color_to_hex(&current_settings.dot_tracker.font_color)}",
                            class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft_settings();
                                    new_settings.dot_tracker.font_color = color;
                                    update_draft(new_settings);
                                }
                            }
                        }
                    }

                    div { class: "setting-row reset-row",
                        button {
                            class: "btn btn-reset",
                            onclick: move |_| {
                                let mut new_settings = draft_settings();
                                new_settings.dot_tracker = DotTrackerConfig::default();
                                new_settings.dot_tracker_opacity = 180;
                                update_draft(new_settings);
                            },
                            i { class: "fa-solid fa-rotate-left" }
                            span { " Reset to Defaults" }
                        }
                    }
                }
            } else if tab == "challenges" {
                // Challenges Settings (global overlay settings)
                div { class: "settings-section",
                    h4 { "Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.challenge_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft_settings();
                            new_settings.challenge_opacity = val;
                            update_draft(new_settings);
                        },
                    }

                    h4 { style: "margin-top: 16px;", "Layout" }

                    {
                        let challenge_config = current_settings.challenge_overlay.clone();
                        let font_hex = color_to_hex(&challenge_config.font_color);
                        let bar_hex = color_to_hex(&challenge_config.default_bar_color);

                        rsx! {
                            // Layout direction
                            div { class: "setting-row",
                                label { "Direction" }
                                select {
                                    class: "input-inline",
                                    value: match challenge_config.layout {
                                        ChallengeLayout::Vertical => "vertical",
                                        ChallengeLayout::Horizontal => "horizontal",
                                    },
                                    onchange: move |e: Event<FormData>| {
                                        let mut new_settings = draft_settings();
                                        new_settings.challenge_overlay.layout = match e.value().as_str() {
                                            "horizontal" => ChallengeLayout::Horizontal,
                                            _ => ChallengeLayout::Vertical,
                                        };
                                        update_draft(new_settings);
                                    },
                                    option { value: "vertical", selected: matches!(challenge_config.layout, ChallengeLayout::Vertical), "Vertical (stacked)" }
                                    option { value: "horizontal", selected: matches!(challenge_config.layout, ChallengeLayout::Horizontal), "Horizontal (side-by-side)" }
                                }
                            }

                            // Max challenges to display
                            div { class: "setting-row",
                                label { "Max Displayed" }
                                select {
                                    class: "input-inline",
                                    value: "{challenge_config.max_display}",
                                    onchange: move |e: Event<FormData>| {
                                        if let Ok(val) = e.value().parse::<u8>() {
                                            let mut new_settings = draft_settings();
                                            new_settings.challenge_overlay.max_display = val.clamp(1, 8);
                                            update_draft(new_settings);
                                        }
                                    },
                                    for n in 1..=8u8 {
                                        option { value: "{n}", selected: challenge_config.max_display == n, "{n}" }
                                    }
                                }
                            }

                            h4 { style: "margin-top: 16px;", "Display Options" }

                            // Show footer
                            div { class: "setting-row",
                                label { "Show Footer Totals" }
                                input {
                                    r#type: "checkbox",
                                    checked: challenge_config.show_footer,
                                    onchange: move |e: Event<FormData>| {
                                        let mut new_settings = draft_settings();
                                        new_settings.challenge_overlay.show_footer = e.checked();
                                        update_draft(new_settings);
                                    }
                                }
                            }

                            // Show duration
                            div { class: "setting-row",
                                label { "Show Duration" }
                                input {
                                    r#type: "checkbox",
                                    checked: challenge_config.show_duration,
                                    onchange: move |e: Event<FormData>| {
                                        let mut new_settings = draft_settings();
                                        new_settings.challenge_overlay.show_duration = e.checked();
                                        update_draft(new_settings);
                                    }
                                }
                            }

                            h4 { style: "margin-top: 16px;", "Colors" }

                            // Default bar color
                            div { class: "setting-row",
                                label { "Default Bar Color" }
                                input {
                                    r#type: "color",
                                    value: "{bar_hex}",
                                    class: "color-picker",
                                    oninput: move |e: Event<FormData>| {
                                        if let Some(color) = parse_hex_color(&e.value()) {
                                            let mut new_settings = draft_settings();
                                            new_settings.challenge_overlay.default_bar_color = color;
                                            update_draft(new_settings);
                                        }
                                    }
                                }
                            }

                            // Font color
                            div { class: "setting-row",
                                label { "Font Color" }
                                input {
                                    r#type: "color",
                                    value: "{font_hex}",
                                    class: "color-picker",
                                    oninput: move |e: Event<FormData>| {
                                        if let Some(color) = parse_hex_color(&e.value()) {
                                            let mut new_settings = draft_settings();
                                            new_settings.challenge_overlay.font_color = color;
                                            update_draft(new_settings);
                                        }
                                    }
                                }
                            }

                            // Reset button
                            div { class: "setting-row reset-row",
                                button {
                                    class: "btn btn-reset",
                                    onclick: move |_| {
                                        let mut new_settings = draft_settings();
                                        new_settings.challenge_overlay = Default::default();
                                        new_settings.challenge_opacity = 180;
                                        update_draft(new_settings);
                                    },
                                    i { class: "fa-solid fa-rotate-left" }
                                    span { " Reset to Defaults" }
                                }
                            }

                            // Hint about per-challenge settings
                            p { class: "text-muted text-sm", style: "margin-top: 12px;",
                                i { class: "fa-solid fa-info-circle" }
                                " Per-challenge settings (columns, color, enabled) are configured in the Encounter Editor."
                            }
                        }
                    }
                }
            } else if tab == "alerts" {
                // Alerts Settings
                div { class: "settings-section",
                    h4 { "Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.alerts_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft_settings();
                            new_settings.alerts_opacity = val;
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Font Size" }
                        input {
                            r#type: "range",
                            min: "8",
                            max: "24",
                            value: "{current_settings.alerts_overlay.font_size}",
                            oninput: move |e| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let mut new_settings = draft_settings();
                                    new_settings.alerts_overlay.font_size = val.clamp(8, 24);
                                    update_draft(new_settings);
                                }
                            }
                        }
                        span { class: "value", "{current_settings.alerts_overlay.font_size}px" }
                    }

                    h4 { style: "margin-top: 16px;", "Display" }

                    div { class: "setting-row",
                        label { "Max Displayed" }
                        select {
                            class: "input-inline",
                            value: "{current_settings.alerts_overlay.max_display}",
                            onchange: move |e: Event<FormData>| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let mut new_settings = draft_settings();
                                    new_settings.alerts_overlay.max_display = val.clamp(1, 10);
                                    update_draft(new_settings);
                                }
                            },
                            for n in 1..=10u8 {
                                option { value: "{n}", selected: current_settings.alerts_overlay.max_display == n, "{n}" }
                            }
                        }
                    }

                    h4 { style: "margin-top: 16px;", "Timing" }

                    div { class: "setting-row",
                        label { "Display Duration" }
                        input {
                            r#type: "range",
                            min: "1",
                            max: "15",
                            step: "0.5",
                            value: "{current_settings.alerts_overlay.default_duration}",
                            oninput: move |e| {
                                if let Ok(val) = e.value().parse::<f32>() {
                                    let mut new_settings = draft_settings();
                                    new_settings.alerts_overlay.default_duration = val.clamp(1.0, 15.0);
                                    update_draft(new_settings);
                                }
                            }
                        }
                        span { class: "value", "{current_settings.alerts_overlay.default_duration:.1}s" }
                    }

                    div { class: "setting-row",
                        label { "Fade Duration" }
                        input {
                            r#type: "range",
                            min: "0",
                            max: "3",
                            step: "0.5",
                            value: "{current_settings.alerts_overlay.fade_duration}",
                            oninput: move |e| {
                                if let Ok(val) = e.value().parse::<f32>() {
                                    let mut new_settings = draft_settings();
                                    new_settings.alerts_overlay.fade_duration = val.clamp(0.0, 3.0);
                                    update_draft(new_settings);
                                }
                            }
                        }
                        span { class: "value", "{current_settings.alerts_overlay.fade_duration:.1}s" }
                    }

                    div { class: "setting-row reset-row",
                        button {
                            class: "btn btn-reset",
                            onclick: move |_| {
                                let mut new_settings = draft_settings();
                                new_settings.alerts_overlay = AlertsOverlayConfig::default();
                                new_settings.alerts_opacity = 180;
                                update_draft(new_settings);
                            },
                            i { class: "fa-solid fa-rotate-left" }
                            span { " Reset to Defaults" }
                        }
                    }

                    // Hint about per-alert settings
                    p { class: "text-muted text-sm", style: "margin-top: 12px;",
                        i { class: "fa-solid fa-info-circle" }
                        " Per-alert color can be set when defining timers with is_alert enabled."
                    }
                }
            } else if tab == "raid" {
                // Raid Settings
                {
                    let cols = current_settings.raid_overlay.grid_columns;
                    let rows = current_settings.raid_overlay.grid_rows;
                    let is_valid = current_settings.raid_overlay.is_valid_grid();

                    rsx! {
                        div { class: "settings-section",
                            h4 { "Grid Layout" }

                            div { class: "setting-row",
                                label { "Columns" }
                                select {
                                    value: "{cols}",
                                    onchange: move |e: Event<FormData>| {
                                        if let Ok(val) = e.value().parse::<u8>() {
                                            let mut new_settings = draft_settings();
                                            new_settings.raid_overlay.grid_columns = val.clamp(1, 4);
                                            update_draft(new_settings);
                                        }
                                    },
                                    option { value: "1", "1" }
                                    option { value: "2", "2" }
                                    option { value: "4", "4" }
                                }
                            }

                            div { class: "setting-row",
                                label { "Rows" }
                                select {
                                    value: "{rows}",
                                    onchange: move |e: Event<FormData>| {
                                        if let Ok(val) = e.value().parse::<u8>() {
                                            let mut new_settings = draft_settings();
                                            new_settings.raid_overlay.grid_rows = val.clamp(1, 8);
                                            update_draft(new_settings);
                                        }
                                    },
                                    option { value: "1", "1" }
                                    option { value: "2", "2" }
                                    option { value: "4", "4" }
                                    option { value: "8", "8" }
                                }
                            }

                            div { class: "setting-row",
                                span { class: "hint", "Total slots: {cols * rows}" }
                            }
                            if !is_valid {
                                div { class: "setting-row validation-error",
                                    "⚠ Grid must have 4, 8, or 16 total slots"
                                }
                            }
                            div { class: "setting-row",
                                span { class: "hint hint-subtle", "Grid changes require toggling overlay off/on" }
                            }

                            h4 { "Appearance" }

                            OpacitySlider {
                                label: "Background Opacity",
                                value: current_settings.raid_opacity,
                                on_change: move |val| {
                                    let mut new_settings = draft_settings();
                                    new_settings.raid_opacity = val;
                                    update_draft(new_settings);
                                },
                            }

                            div { class: "setting-row",
                                label { "Max Effects per Frame" }
                                input {
                                    r#type: "number",
                                    min: "1",
                                    max: "8",
                                    value: "{current_settings.raid_overlay.max_effects_per_frame}",
                                    onchange: move |e: Event<FormData>| {
                                        if let Ok(val) = e.value().parse::<u8>() {
                                            let mut new_settings = draft_settings();
                                            new_settings.raid_overlay.max_effects_per_frame = val.clamp(1, 8);
                                            update_draft(new_settings);
                                        }
                                    }
                                }
                            }

                            div { class: "setting-row",
                                label { "Effect Size" }
                                input {
                                    r#type: "range",
                                    min: "8",
                                    max: "24",
                                    value: "{current_settings.raid_overlay.effect_size as i32}",
                                    oninput: move |e| {
                                        if let Ok(val) = e.value().parse::<f32>() {
                                            let mut new_settings = draft_settings();
                                            new_settings.raid_overlay.effect_size = val.clamp(8.0, 24.0);
                                            update_draft(new_settings);
                                        }
                                    }
                                }
                                span { class: "value", "{current_settings.raid_overlay.effect_size:.0}px" }
                            }

                            div { class: "setting-row",
                                label { "Effect Vertical Offset" }
                                input {
                                    r#type: "range",
                                    min: "-10",
                                    max: "30",
                                    value: "{current_settings.raid_overlay.effect_vertical_offset as i32}",
                                    oninput: move |e| {
                                        if let Ok(val) = e.value().parse::<f32>() {
                                            let mut new_settings = draft_settings();
                                            new_settings.raid_overlay.effect_vertical_offset = val.clamp(-10.0, 30.0);
                                            update_draft(new_settings);
                                        }
                                    }
                                }
                                span { class: "value", "{current_settings.raid_overlay.effect_vertical_offset:.0}px" }
                            }

                            div { class: "setting-row",
                                label { "Show Role Icons" }
                                input {
                                    r#type: "checkbox",
                                    checked: current_settings.raid_overlay.show_role_icons,
                                    onchange: move |e: Event<FormData>| {
                                        let mut new_settings = draft_settings();
                                        new_settings.raid_overlay.show_role_icons = e.checked();
                                        update_draft(new_settings);
                                    }
                                }
                            }

                            div { class: "setting-row reset-row",
                                button {
                                    class: "btn btn-reset",
                                    onclick: move |_| {
                                        let mut new_settings = draft_settings();
                                        new_settings.raid_overlay = RaidOverlaySettings::default();
                                        new_settings.raid_opacity = 180;
                                        update_draft(new_settings);
                                    },
                                    i { class: "fa-solid fa-rotate-left" }
                                    span { " Reset Style" }
                                }
                            }
                        }
                    }
                }
            } else if tab == "personal" {
                // Personal Settings
                {
                    let visible_stats = current_settings.personal_overlay.visible_stats.clone();
                    let stat_count = visible_stats.len();

                    rsx! {
                        div { class: "settings-section",
                            p { class: "hint", "Displayed stats:" }

                            div { class: "stat-order-list",
                                for (idx, stat) in visible_stats.into_iter().enumerate() {
                                    div { class: "stat-order-item", key: "{stat:?}",
                                        span { class: "stat-name", "{stat.label()}" }
                                        div { class: "stat-controls",
                                            button {
                                                class: "btn-order",
                                                disabled: idx == 0,
                                                onclick: move |_| {
                                                    let mut new_settings = draft_settings();
                                                    let stats = &mut new_settings.personal_overlay.visible_stats;
                                                    if idx > 0 { stats.swap(idx, idx - 1); }
                                                    update_draft(new_settings);
                                                },
                                                "▲"
                                            }
                                            button {
                                                class: "btn-order",
                                                disabled: idx >= stat_count - 1,
                                                onclick: move |_| {
                                                    let mut new_settings = draft_settings();
                                                    let stats = &mut new_settings.personal_overlay.visible_stats;
                                                    if idx < stats.len() - 1 { stats.swap(idx, idx + 1); }
                                                    update_draft(new_settings);
                                                },
                                                "▼"
                                            }
                                            button {
                                                class: "btn-remove",
                                                onclick: move |_| {
                                                    let mut new_settings = draft_settings();
                                                    new_settings.personal_overlay.visible_stats.retain(|s| *s != stat);
                                                    update_draft(new_settings);
                                                },
                                                "✕"
                                            }
                                        }
                                    }
                                }
                            }

                            // Add stats section
                            div { class: "stat-add-section",
                                p { class: "hint", "Add stats:" }
                                div { class: "stat-add-grid",
                                    for stat in PersonalStat::all() {
                                        {
                                            let is_visible = current_settings.personal_overlay.visible_stats.contains(stat);
                                            if !is_visible {
                                                let stat = *stat;
                                                rsx! {
                                                    button {
                                                        class: "btn-add-stat",
                                                        onclick: move |_| {
                                                            let mut new_settings = draft_settings();
                                                            if !new_settings.personal_overlay.visible_stats.contains(&stat) {
                                                                new_settings.personal_overlay.visible_stats.push(stat);
                                                            }
                                                            update_draft(new_settings);
                                                        },
                                                        "+ {stat.label()}"
                                                    }
                                                }
                                            } else {
                                                rsx! {}
                                            }
                                        }
                                    }
                                }
                            }

                            h4 { "Appearance" }

                            OpacitySlider {
                                label: "Background Opacity",
                                value: current_settings.personal_opacity,
                                on_change: move |val| {
                                    let mut new_settings = draft_settings();
                                    new_settings.personal_opacity = val;
                                    update_draft(new_settings);
                                },
                            }

                            div { class: "setting-row",
                                label { "Value Font Color" }
                                input {
                                    r#type: "color",
                                    value: "{personal_font_color_hex}",
                                    class: "color-picker",
                                    oninput: move |e: Event<FormData>| {
                                        if let Some(color) = parse_hex_color(&e.value()) {
                                            let mut new_settings = draft_settings();
                                            new_settings.personal_overlay.font_color = color;
                                            update_draft(new_settings);
                                        }
                                    }
                                }
                            }

                            div { class: "setting-row",
                                label { "Label Font Color" }
                                input {
                                    r#type: "color",
                                    value: "{personal_label_font_color_hex}",
                                    class: "color-picker",
                                    oninput: move |e: Event<FormData>| {
                                        if let Some(color) = parse_hex_color(&e.value()) {
                                            let mut new_settings = draft_settings();
                                            new_settings.personal_overlay.label_color = color;
                                            update_draft(new_settings);
                                        }
                                    }
                                }
                            }

                            div { class: "setting-row reset-row",
                                button {
                                    class: "btn btn-reset",
                                    onclick: move |_| {
                                        let mut new_settings = draft_settings();
                                        new_settings.personal_overlay = PersonalOverlayConfig::default();
                                        new_settings.personal_opacity = 180;
                                        update_draft(new_settings);
                                    },
                                    i { class: "fa-solid fa-rotate-left" }
                                    span { " Reset Style" }
                                }
                            }
                        }
                    }
                }
            } else {
                // Metric Settings (default tab content)
                {
                    let tab_key = tab.clone();
                    rsx! {
                        div { class: "settings-section",
                            div { class: "setting-row",
                                label { "Show Per-Second" }
                                input {
                                    r#type: "checkbox",
                                    checked: current_appearance.show_per_second,
                                    onchange: {
                                        let tab = tab_key.clone();
                                        move |e: Event<FormData>| {
                                            let mut new_settings = draft_settings();
                                            let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                            let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                            appearance.show_per_second = e.checked();
                                            update_draft(new_settings);
                                        }
                                    }
                                }
                            }

                            div { class: "setting-row",
                                label { "Show Total" }
                                input {
                                    r#type: "checkbox",
                                    checked: current_appearance.show_total,
                                    onchange: {
                                        let tab = tab_key.clone();
                                        move |e: Event<FormData>| {
                                            let mut new_settings = draft_settings();
                                            let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                            let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                            appearance.show_total = e.checked();
                                            update_draft(new_settings);
                                        }
                                    }
                                }
                            }

                            div { class: "setting-row",
                                label { "Show Header" }
                                input {
                                    r#type: "checkbox",
                                    checked: current_appearance.show_header,
                                    onchange: {
                                        let tab = tab_key.clone();
                                        move |e: Event<FormData>| {
                                            let mut new_settings = draft_settings();
                                            let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                            let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                            appearance.show_header = e.checked();
                                            update_draft(new_settings);
                                        }
                                    }
                                }
                            }

                            div { class: "setting-row",
                                label { "Show Footer" }
                                input {
                                    r#type: "checkbox",
                                    checked: current_appearance.show_footer,
                                    onchange: {
                                        let tab = tab_key.clone();
                                        move |e: Event<FormData>| {
                                            let mut new_settings = draft_settings();
                                            let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                            let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                            appearance.show_footer = e.checked();
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
                                        let tab = tab_key.clone();
                                        move |e: Event<FormData>| {
                                            if let Ok(val) = e.value().parse::<u8>() {
                                                let mut new_settings = draft_settings();
                                                let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                                let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                                appearance.max_entries = val.clamp(1, 16);
                                                update_draft(new_settings);
                                            }
                                        }
                                    }
                                }
                            }

                            div { class: "setting-row",
                                label { "Bar Color" }
                                input {
                                    r#type: "color",
                                    value: "{bar_color_hex}",
                                    class: "color-picker",
                                    oninput: {
                                        let tab = tab_key.clone();
                                        move |e: Event<FormData>| {
                                            if let Some(color) = parse_hex_color(&e.value()) {
                                                let mut new_settings = draft_settings();
                                                let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                                let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                                appearance.bar_color = color;
                                                update_draft(new_settings);
                                            }
                                        }
                                    }
                                }
                            }

                            div { class: "setting-row",
                                label { "Font Color" }
                                input {
                                    r#type: "color",
                                    value: "{font_color_hex}",
                                    class: "color-picker",
                                    oninput: {
                                        let tab = tab_key.clone();
                                        move |e: Event<FormData>| {
                                            if let Some(color) = parse_hex_color(&e.value()) {
                                                let mut new_settings = draft_settings();
                                                let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                                let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                                appearance.font_color = color;
                                                update_draft(new_settings);
                                            }
                                        }
                                    }
                                }
                            }

                            div { class: "setting-row reset-row",
                                button {
                                    class: "btn btn-reset",
                                    onclick: {
                                        let tab = tab_key.clone();
                                        move |_| {
                                            let mut new_settings = draft_settings();
                                            let default_appearance = new_settings.default_appearances
                                                .get(&tab)
                                                .cloned()
                                                .unwrap_or_default();
                                            new_settings.appearances.insert(tab.clone(), default_appearance);
                                            update_draft(new_settings);
                                        }
                                    },
                                    i { class: "fa-solid fa-rotate-left" }
                                    span { " Reset Style" }
                                }
                            }
                        }
                    }
                }
            }

            // ─────────────────────────────────────────────────────────────────
            // Save button
            // ─────────────────────────────────────────────────────────────────
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

// ─────────────────────────────────────────────────────────────────────────────
// Simple Sub-Components (these work with simple props)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn TabButton(label: &'static str, tab_key: &'static str, selected_tab: Signal<String>) -> Element {
    let is_active = selected_tab() == tab_key;
    rsx! {
        button {
            class: if is_active { "tab-btn active" } else { "tab-btn" },
            onclick: move |_| selected_tab.set(tab_key.to_string()),
            "{label}"
        }
    }
}

#[component]
fn OpacitySlider(label: &'static str, value: u8, on_change: EventHandler<u8>) -> Element {
    rsx! {
        div { class: "setting-row",
            label { "{label}" }
            input {
                r#type: "range",
                min: "0",
                max: "255",
                value: "{value}",
                oninput: move |e| {
                    if let Ok(val) = e.value().parse::<u8>() {
                        on_change.call(val);
                    }
                }
            }
            span { class: "value", "{value}" }
        }
    }
}
