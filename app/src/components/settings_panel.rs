//! Settings panel component for overlay configuration
//!
//! Floating, draggable panel for customizing overlay appearances,
//! personal stats, and raid frame settings.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;
use std::collections::HashMap;

use crate::api;
use crate::components::{Slider, ToastSeverity, use_toast};
use crate::types::{
    AlertsOverlayConfig, BossHealthConfig, ChallengeLayout, ClassIconMode, CooldownTrackerConfig,
    DotTrackerConfig, EffectsAConfig, EffectsBConfig,
    MAX_PROFILES, MetricType, OverlayAppearanceConfig, OverlaySettings,
    PersonalOverlayConfig, PersonalStat, RaidOverlaySettings, TimerOverlayConfig,
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
    profile_dirty: Signal<bool>,
    on_close: EventHandler<()>,
    on_header_mousedown: EventHandler<MouseEvent>,
    on_settings_saved: EventHandler<()>,
) -> Element {
    // Local draft of settings being edited
    #[allow(clippy::redundant_closure)]
    let mut draft_settings = use_signal(|| settings());
    let mut has_changes = use_signal(|| false);
    let mut save_status = use_signal(String::new);

    // Adopt external settings changes (e.g. an overlay resized/moved in move mode,
    // persisted by the backend on lock) into the editable draft — but only when
    // the user has no unsaved edits here, so an in-progress change is never
    // clobbered. `settings()` subscribes to prop changes; `has_changes.peek()`
    // reads the guard without subscribing.
    use_effect(move || {
        let incoming = settings();
        let unsaved = *has_changes.peek();
        dioxus_logger::tracing::info!(
            map_w = incoming.map.width,
            map_h = incoming.map.height,
            unsaved,
            "settings_panel: incoming overlay settings — {}",
            if unsaved { "keeping draft (unsaved edits)" } else { "re-seeding draft" }
        );
        if !unsaved {
            draft_settings.set(incoming);
        }
    });

    // Debounce counter for live preview - each request gets an ID
    let mut preview_request_id: Signal<u32> = use_signal(|| 0);

    // Profile UI state
    let mut new_profile_name = use_signal(String::new);
    let mut profile_status = use_signal(String::new);
    let mut renaming_profile: Signal<Option<String>> = use_signal(|| None);
    let mut rename_input = use_signal(String::new);
    let mut metrics_global_open = use_signal(|| false);
    let mut toast = use_toast();

    // Role default profile state
    let mut role_defaults: Signal<HashMap<String, String>> = use_signal(HashMap::new);
    use_future(move || async move {
        role_defaults.set(api::get_default_profiles_per_role().await);
    });

    // Available system fonts for the global font picker (loaded once)
    let mut system_fonts: Signal<Vec<String>> = use_signal(Vec::new);
    use_future(move || async move {
        system_fonts.set(api::list_system_fonts().await);
    });

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
    // Class color hex strings
    let cc = &current_settings.class_colors;
    let cc_sorc = color_to_hex(&cc.sorcerer_sage);
    let cc_asin = color_to_hex(&cc.assassin_shadow);
    let cc_jugg = color_to_hex(&cc.juggernaut_guardian);
    let cc_mara = color_to_hex(&cc.marauder_sentinel);
    let cc_merc = color_to_hex(&cc.mercenary_commando);
    let cc_pt   = color_to_hex(&cc.powertech_vanguard);
    let cc_oper = color_to_hex(&cc.operative_scoundrel);
    let cc_snip = color_to_hex(&cc.sniper_gunslinger);

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
                config.overlay_settings.metric_show_empty_bars =
                    new_settings.metric_show_empty_bars;
                config.overlay_settings.metric_stack_from_bottom =
                    new_settings.metric_stack_from_bottom;
                config.overlay_settings.metric_scaling_factor = new_settings.metric_scaling_factor;
                config.overlay_settings.metric_font_scale = new_settings.metric_font_scale;
                config.overlay_settings.metric_gradient_intensity =
                    new_settings.metric_gradient_intensity;
                config.overlay_settings.metric_dynamic_background = new_settings.metric_dynamic_background;
                config.overlay_settings.metric_show_background_bar = new_settings.metric_show_background_bar;
                config.overlay_settings.class_icon_mode = new_settings.class_icon_mode;
                config.overlay_settings.class_colors = new_settings.class_colors.clone();
                config.overlay_settings.personal_opacity = new_settings.personal_opacity;
                config.overlay_settings.raid_overlay = new_settings.raid_overlay.clone();
                config.overlay_settings.raid_opacity = new_settings.raid_opacity;
                config.overlay_settings.boss_health = new_settings.boss_health.clone();
                config.overlay_settings.boss_health_opacity = new_settings.boss_health_opacity;
                config.overlay_settings.timers_a_overlay = new_settings.timers_a_overlay.clone();
                config.overlay_settings.timers_a_opacity = new_settings.timers_a_opacity;
                config.overlay_settings.timers_b_overlay = new_settings.timers_b_overlay.clone();
                config.overlay_settings.timers_b_opacity = new_settings.timers_b_opacity;
                config.overlay_settings.effects_overlay = new_settings.effects_overlay.clone();
                config.overlay_settings.effects_opacity = new_settings.effects_opacity;
                config.overlay_settings.challenge_overlay = new_settings.challenge_overlay.clone();
                config.overlay_settings.challenge_opacity = new_settings.challenge_opacity;
                config.overlay_settings.alerts_overlay = new_settings.alerts_overlay.clone();
                config.overlay_settings.alerts_opacity = new_settings.alerts_opacity;
                config.overlay_settings.effects_a = new_settings.effects_a.clone();
                config.overlay_settings.effects_a_opacity = new_settings.effects_a_opacity;
                config.overlay_settings.effects_b = new_settings.effects_b.clone();
                config.overlay_settings.effects_b_opacity = new_settings.effects_b_opacity;
                config.overlay_settings.cooldown_tracker = new_settings.cooldown_tracker.clone();
                config.overlay_settings.cooldown_tracker_opacity =
                    new_settings.cooldown_tracker_opacity;
                config.overlay_settings.dot_tracker = new_settings.dot_tracker.clone();
                config.overlay_settings.dot_tracker_opacity = new_settings.dot_tracker_opacity;
                config.overlay_settings.notes_overlay = new_settings.notes_overlay.clone();
                config.overlay_settings.notes_opacity = new_settings.notes_opacity;
                config.overlay_settings.combat_time = new_settings.combat_time.clone();
                config.overlay_settings.combat_time_opacity = new_settings.combat_time_opacity;
                config.overlay_settings.operation_timer = new_settings.operation_timer.clone();
                config.overlay_settings.operation_timer_opacity = new_settings.operation_timer_opacity;
                config.overlay_settings.map = new_settings.map.clone();
                config.overlay_settings.map_opacity = new_settings.map_opacity;
                config.overlay_settings.positions = existing_positions;
                config.overlay_settings.enabled = existing_enabled;

                if let Err(err) = api::update_config(&config).await {
                    save_status.set(format!("Failed to save: {}", err));
                } else {
                    api::refresh_overlay_settings().await;
                    // Use the merged config (draft appearance + backend positions/enabled/
                    // behavioral flags) rather than the stale draft, so we don't overwrite
                    // global settings like hide_when_not_live or hide_during_conversations.
                    settings.set(config.overlay_settings.clone());
                    draft_settings.set(config.overlay_settings);
                    has_changes.set(false);
                    // Save to the active profile so it stays in sync
                    if let Some(ref profile_name) = active_profile() {
                        let _ = api::save_profile(profile_name).await;
                    }
                    profile_dirty.set(false);
                    save_status.set("Settings saved!".to_string());
                    on_settings_saved.call(());
                }
            }
        }
    };

    // Update draft settings helper with debounced live preview
    let mut update_draft = move |new_settings: OverlaySettings| {
        draft_settings.set(new_settings.clone());
        has_changes.set(true);
        save_status.set(String::new());

        // Increment request ID to cancel any pending preview
        let request_id = preview_request_id() + 1;
        preview_request_id.set(request_id);

        // Schedule a debounced preview (300ms delay)
        spawn(async move {
            TimeoutFuture::new(300).await;
            // Only execute if this is still the latest request
            if preview_request_id() == request_id {
                api::preview_overlay_settings(&new_settings).await;
            }
        });
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
                    onclick: move |_| {
                        // Cancel any pending preview by incrementing the ID
                        preview_request_id.set(preview_request_id() + 1);
                        let needs_revert = has_changes();
                        async move {
                            // Restore original settings if there were changes
                            if needs_revert {
                                api::refresh_overlay_settings().await;
                            }
                            on_close.call(());
                        }
                    },
                    onmousedown: move |e| e.stop_propagation(),
                    "X"
                }
            }

            // ─────────────────────────────────────────────────────────────────
            // Global font (applies to all overlays, live)
            // ─────────────────────────────────────────────────────────────────
            div { class: "settings-section",
                div { class: "setting-row",
                    label { "Overlay Font" }
                    select {
                        value: "{current_settings.overlay_font_family}",
                        onchange: move |e: Event<FormData>| {
                            let family = e.value();
                            let mut new_settings = draft_settings();
                            new_settings.overlay_font_family = family.clone();
                            update_draft(new_settings);
                            // Apply immediately across all overlays (also persists)
                            spawn(async move {
                                let _ = api::set_overlay_font_family(family).await;
                            });
                        },
                        for font in system_fonts().iter() {
                            // `selected` (not just the select's `value`) so the active
                            // font stays chosen even though options load asynchronously.
                            option {
                                key: "{font}",
                                value: "{font}",
                                selected: *font == current_settings.overlay_font_family,
                                "{font}"
                            }
                        }
                    }
                }
            }

            // Profile unsaved changes indicator
            if profile_dirty() {
                div { class: "profile-unsaved-indicator",
                    i { class: "fa-solid fa-circle-exclamation" }
                    span { " Changes not saved to profile" }
                }
            }

            // ─────────────────────────────────────────────────────────────────
            // Profiles section (inline)
            // ─────────────────────────────────────────────────────────────────
            details { class: "settings-section collapsible",
                summary { class: "collapsible-summary",
                    i { class: "fa-solid fa-user-gear summary-icon" }
                    "Profiles"
                    span { class: "profile-active-badge", "{active_profile().unwrap_or_default()}" }
                }
                div { class: "collapsible-content",
                    // Profile list
                    div { class: "profile-list compact",
                            for name in profile_names().iter() {
                                {
                                    let profile_name = name.clone();
                                    let is_active = active_profile().as_ref() == Some(&profile_name);
                                    let is_renaming = renaming_profile().as_ref() == Some(&profile_name);
                                    rsx! {
                                        div {
                                            key: "{profile_name}",
                                            class: if is_active { "profile-item active" } else { "profile-item" },
                                            // Profile name: inline rename input or static text
                                            if is_renaming {
                                                input {
                                                    class: "profile-rename-input",
                                                    r#type: "text",
                                                    maxlength: "32",
                                                    value: rename_input,
                                                    autofocus: true,
                                                    oninput: move |e| rename_input.set(e.value()),
                                                    onkeydown: {
                                                        let old_name = profile_name.clone();
                                                        move |e: KeyboardEvent| {
                                                            if e.key() == Key::Enter {
                                                                let new_name = rename_input().trim().to_string();
                                                                let old = old_name.clone();
                                                                if new_name.is_empty() || new_name == old {
                                                                    renaming_profile.set(None);
                                                                    return;
                                                                }
                                                                spawn(async move {
                                                                    if let Err(err) = api::rename_profile(&old, &new_name).await {
                                                                        toast.show(format!("Failed to rename: {}", err), ToastSeverity::Normal);
                                                                    } else {
                                                                        profile_names.set(api::get_profile_names().await);
                                                                        active_profile.set(api::get_active_profile().await);
                                                                        profile_status.set(format!("Renamed '{}' to '{}'", old, new_name));
                                                                    }
                                                                    renaming_profile.set(None);
                                                                });
                                                            } else if e.key() == Key::Escape {
                                                                renaming_profile.set(None);
                                                            }
                                                        }
                                                    },
                                                }
                                            } else {
                                                span { class: "profile-name", "{profile_name}" }
                                            }
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
                                                                if let Err(err) = api::load_profile(&pname).await {
                                                                    toast.show(format!("Failed to load profile: {}", err), ToastSeverity::Normal);
                                                                } else {
                                                                    active_profile.set(Some(pname.clone()));
                                                                    profile_dirty.set(false);
                                                                    profile_status.set(format!("Loaded '{}'", pname));
                                                                    if let Some(config) = api::get_config().await {
                                                                        draft_settings.set(config.overlay_settings.clone());
                                                                        settings.set(config.overlay_settings);
                                                                    }
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
                                                                if let Err(err) = api::save_profile(&pname).await {
                                                                    toast.show(format!("Failed to save profile: {}", err), ToastSeverity::Normal);
                                                                } else {
                                                                    active_profile.set(Some(pname.clone()));
                                                                    profile_dirty.set(false);
                                                                    profile_status.set(format!("Saved '{}'", pname));
                                                                }
                                                            });
                                                        }
                                                    },
                                                    "Save"
                                                }
                                                // Rename button
                                                button {
                                                    class: "btn btn-small btn-rename",
                                                    title: "Rename profile",
                                                    onclick: {
                                                        let pname = profile_name.clone();
                                                        move |_| {
                                                            rename_input.set(pname.clone());
                                                            renaming_profile.set(Some(pname.clone()));
                                                        }
                                                    },
                                                    i { class: "fa-solid fa-pen" }
                                                }
                                                // Delete button
                                                button {
                                                    class: "btn btn-small btn-delete",
                                                    onclick: {
                                                        let pname = profile_name.clone();
                                                        move |_| {
                                                            let pname = pname.clone();
                                                            spawn(async move {
                                                                if let Err(err) = api::delete_profile(&pname).await {
                                                                    toast.show(format!("Failed to delete profile: {}", err), ToastSeverity::Normal);
                                                                } else {
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

                    // Create new profile
                    {
                        let names = profile_names();
                        let suggested_name = {
                            let mut n = names.len() + 1;
                            loop {
                                let candidate = format!("Profile {n}");
                                if !names.iter().any(|existing| existing == &candidate) {
                                    break candidate;
                                }
                                n += 1;
                            }
                        };
                        rsx! {
                            div { class: "profile-create",
                                input {
                                    r#type: "text",
                                    class: "profile-name-input",
                                    placeholder: "{suggested_name}",
                                    maxlength: "32",
                                    value: new_profile_name,
                                    oninput: move |e| new_profile_name.set(e.value()),
                                    onfocus: {
                                        let suggested = suggested_name.clone();
                                        move |_| {
                                            if new_profile_name().is_empty() {
                                                new_profile_name.set(suggested.clone());
                                            }
                                        }
                                    },
                                }
                                button {
                                    class: "btn btn-small btn-save",
                                    disabled: new_profile_name().trim().is_empty() || profile_names().len() >= MAX_PROFILES,
                                    onclick: move |_| {
                                        let name = new_profile_name().trim().to_string();
                                        if name.is_empty() { return; }
                                        spawn(async move {
                                            if let Err(err) = api::save_profile(&name).await {
                                                toast.show(format!("Failed to create profile: {}", err), ToastSeverity::Normal);
                                            } else {
                                                profile_names.set(api::get_profile_names().await);
                                                active_profile.set(Some(name.clone()));
                                                new_profile_name.set(String::new());
                                                profile_dirty.set(false);
                                                profile_status.set(format!("Created '{}'", name));
                                            }
                                        });
                                    },
                                    "+ New"
                                }
                            }
                        }
                    }

                    if profile_names().len() >= MAX_PROFILES {
                        p { class: "hint hint-warning compact", "Maximum {MAX_PROFILES} profiles" }
                    }
                    if !profile_status().is_empty() {
                        p { class: "profile-status compact", "{profile_status}" }
                    }

                    // Role default profiles
                    if !profile_names().is_empty() {
                        div { class: "role-defaults-section",
                            div { class: "role-defaults-header",
                                "Role Defaults"
                            }
                            p { class: "hint compact", "Auto-switch profile when your role changes" }
                            for (role, label) in [("Tank", "Tank"), ("Healer", "Healer"), ("Dps", "DPS")] {
                                {
                                    let current_default = role_defaults().get(role).cloned().unwrap_or_default();
                                    let role_key = role.to_string();
                                    rsx! {
                                        div { class: "role-default-row",
                                            label { class: "role-default-label", "{label}" }
                                            select {
                                                class: "role-default-select",
                                                value: "{current_default}",
                                                onchange: {
                                                    let role_key = role_key.clone();
                                                    move |e: Event<FormData>| {
                                                        let selected = e.value();
                                                        let role = role_key.clone();
                                                        spawn(async move {
                                                            let profile = if selected.is_empty() { None } else { Some(selected.as_str()) };
                                                            if let Err(err) = api::set_default_profile_for_role(&role, profile).await {
                                                                toast.show(format!("Failed to set default: {}", err), ToastSeverity::Normal);
                                                            } else {
                                                                role_defaults.set(api::get_default_profiles_per_role().await);
                                                            }
                                                        });
                                                    }
                                                },
                                                option { value: "", "None" }
                                                for pname in profile_names().iter() {
                                                    option {
                                                        value: "{pname}",
                                                        selected: current_default == *pname,
                                                        "{pname}"
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

            // ─────────────────────────────────────────────────────────────────
            // Scrollable content wrapper
            // ─────────────────────────────────────────────────────────────────
            div { class: "settings-content",

            // ─────────────────────────────────────────────────────────────────
            // Tabs for overlay types
            // ─────────────────────────────────────────────────────────────────
            div { class: "settings-tabs",
                div { class: "tab-group",
                    span { class: "tab-group-label", "General" }
                    div { class: "tab-group-buttons",
                        TabButton { label: "Personal Stats", tab_key: "personal", selected_tab: selected_tab, metrics_global_open: metrics_global_open }
                        TabButton { label: "Raid Frames", tab_key: "raid", selected_tab: selected_tab, metrics_global_open: metrics_global_open }
                        TabButton { label: "Alerts", tab_key: "alerts", selected_tab: selected_tab, metrics_global_open: metrics_global_open }
                        TabButton { label: "Combat Time", tab_key: "combat_time", selected_tab: selected_tab, metrics_global_open: metrics_global_open }
                        TabButton { label: "Op Timer", tab_key: "operation_timer", selected_tab: selected_tab, metrics_global_open: metrics_global_open }
                    }
                }
                div { class: "tab-group",
                    span { class: "tab-group-label", "Encounters" }
                    div { class: "tab-group-buttons",
                        TabButton { label: "Boss Health", tab_key: "boss_health", selected_tab: selected_tab, metrics_global_open: metrics_global_open }
                        TabButton { label: "Timers A", tab_key: "timers_a", selected_tab: selected_tab, metrics_global_open: metrics_global_open }
                        TabButton { label: "Timers B", tab_key: "timers_b", selected_tab: selected_tab, metrics_global_open: metrics_global_open }
                        TabButton { label: "Challenges", tab_key: "challenges", selected_tab: selected_tab, metrics_global_open: metrics_global_open }
                        TabButton { label: "Notes", tab_key: "notes", selected_tab: selected_tab, metrics_global_open: metrics_global_open }
                        TabButton { label: "Map", tab_key: "map", selected_tab: selected_tab, metrics_global_open: metrics_global_open }
                    }
                }
                div { class: "tab-group",
                    span { class: "tab-group-label", "Effects" }
                    div { class: "tab-group-buttons",
                        TabButton { label: "Effects A", tab_key: "effects_a", selected_tab: selected_tab, metrics_global_open: metrics_global_open }
                        TabButton { label: "Effects B", tab_key: "effects_b", selected_tab: selected_tab, metrics_global_open: metrics_global_open }
                        TabButton { label: "Cooldowns", tab_key: "cooldowns", selected_tab: selected_tab, metrics_global_open: metrics_global_open }
                        TabButton { label: "DOT Tracker", tab_key: "dot_tracker", selected_tab: selected_tab, metrics_global_open: metrics_global_open }
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
                                metrics_global_open: metrics_global_open,
                            }
                        }
                    }
                    div { class: if metrics_global_open() { "settings-section collapsible metrics-global open" } else { "settings-section collapsible metrics-global" },
                        div {
                            class: "collapsible-summary",
                            onclick: move |_| metrics_global_open.set(!metrics_global_open()),
                            i { class: "fa-solid fa-sliders summary-icon" }
                            "Global Metrics Settings"
                        }
                        if metrics_global_open() {
                            div { class: "collapsible-content",
                            OpacitySlider {
                                label: "Background Opacity",
                                value: current_settings.metric_opacity,
                                on_change: move |val| {
                                    let mut new_settings = draft_settings();
                                    new_settings.metric_opacity = val;
                                    update_draft(new_settings);
                                },
                            }

                            Slider {
                                label: "Bar Thickness",
                                value: (current_settings.metric_scaling_factor * 100.0) as i32 as f64,
                                min: 30.0,
                                max: 200.0,
                                step: 5.0,
                                suffix: "%",
                                on_change: move |v: f64| {
                                    let mut new_settings = draft_settings();
                                    new_settings.metric_scaling_factor = (v as f32 / 100.0).clamp(0.3, 2.0);
                                    update_draft(new_settings);
                                },
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

                            div { class: "setting-row",
                                label { "Class Icons" }
                                select {
                                    value: match current_settings.class_icon_mode {
                                        ClassIconMode::None => "none",
                                        ClassIconMode::Class => "class",
                                        ClassIconMode::Discipline => "discipline",
                                    },
                                    onchange: move |e: Event<FormData>| {
                                        let mut new_settings = draft_settings();
                                        new_settings.class_icon_mode = match e.value().as_str() {
                                            "class" => ClassIconMode::Class,
                                            "discipline" => ClassIconMode::Discipline,
                                            _ => ClassIconMode::None,
                                        };
                                        update_draft(new_settings);
                                    },
                                    option { value: "none", "None" }
                                    option { value: "class", "Class" }
                                    option { value: "discipline", "Discipline" }
                                }
                            }

                            // ── Class Colors ─────────────────────────────────
                            details { class: "collapsible",
                                summary { class: "collapsible-summary", "Class Colors" }
                                div { class: "setting-row",
                                    label { "Sorcerer / Sage" }
                                    input { r#type: "color", value: "{cc_sorc}", class: "color-picker",
                                        oninput: move |e: Event<FormData>| {
                                            if let Some(color) = parse_hex_color(&e.value()) {
                                                let mut s = draft_settings();
                                                s.class_colors.sorcerer_sage = color;
                                                update_draft(s);
                                            }
                                        }
                                    }
                                }
                                div { class: "setting-row",
                                    label { "Assassin / Shadow" }
                                    input { r#type: "color", value: "{cc_asin}", class: "color-picker",
                                        oninput: move |e: Event<FormData>| {
                                            if let Some(color) = parse_hex_color(&e.value()) {
                                                let mut s = draft_settings();
                                                s.class_colors.assassin_shadow = color;
                                                update_draft(s);
                                            }
                                        }
                                    }
                                }
                                div { class: "setting-row",
                                    label { "Juggernaut / Guardian" }
                                    input { r#type: "color", value: "{cc_jugg}", class: "color-picker",
                                        oninput: move |e: Event<FormData>| {
                                            if let Some(color) = parse_hex_color(&e.value()) {
                                                let mut s = draft_settings();
                                                s.class_colors.juggernaut_guardian = color;
                                                update_draft(s);
                                            }
                                        }
                                    }
                                }
                                div { class: "setting-row",
                                    label { "Marauder / Sentinel" }
                                    input { r#type: "color", value: "{cc_mara}", class: "color-picker",
                                        oninput: move |e: Event<FormData>| {
                                            if let Some(color) = parse_hex_color(&e.value()) {
                                                let mut s = draft_settings();
                                                s.class_colors.marauder_sentinel = color;
                                                update_draft(s);
                                            }
                                        }
                                    }
                                }
                                div { class: "setting-row",
                                    label { "Mercenary / Commando" }
                                    input { r#type: "color", value: "{cc_merc}", class: "color-picker",
                                        oninput: move |e: Event<FormData>| {
                                            if let Some(color) = parse_hex_color(&e.value()) {
                                                let mut s = draft_settings();
                                                s.class_colors.mercenary_commando = color;
                                                update_draft(s);
                                            }
                                        }
                                    }
                                }
                                div { class: "setting-row",
                                    label { "Powertech / Vanguard" }
                                    input { r#type: "color", value: "{cc_pt}", class: "color-picker",
                                        oninput: move |e: Event<FormData>| {
                                            if let Some(color) = parse_hex_color(&e.value()) {
                                                let mut s = draft_settings();
                                                s.class_colors.powertech_vanguard = color;
                                                update_draft(s);
                                            }
                                        }
                                    }
                                }
                                div { class: "setting-row",
                                    label { "Operative / Scoundrel" }
                                    input { r#type: "color", value: "{cc_oper}", class: "color-picker",
                                        oninput: move |e: Event<FormData>| {
                                            if let Some(color) = parse_hex_color(&e.value()) {
                                                let mut s = draft_settings();
                                                s.class_colors.operative_scoundrel = color;
                                                update_draft(s);
                                            }
                                        }
                                    }
                                }
                                div { class: "setting-row",
                                    label { "Sniper / Gunslinger" }
                                    input { r#type: "color", value: "{cc_snip}", class: "color-picker",
                                        oninput: move |e: Event<FormData>| {
                                            if let Some(color) = parse_hex_color(&e.value()) {
                                                let mut s = draft_settings();
                                                s.class_colors.sniper_gunslinger = color;
                                                update_draft(s);
                                            }
                                        }
                                    }
                                }
                            }

                            Slider {
                                label: "Font Scale",
                                value: (current_settings.metric_font_scale * 100.0) as i32 as f64,
                                min: 30.0,
                                max: 300.0,
                                step: 5.0,
                                suffix: "%",
                                on_change: move |v: f64| {
                                    let mut new_settings = draft_settings();
                                    new_settings.metric_font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                                    update_draft(new_settings);
                                },
                            }

                            Slider {
                                label: "Gradient Intensity",
                                value: (current_settings.metric_gradient_intensity * 100.0) as i32 as f64,
                                min: 0.0,
                                max: 60.0,
                                step: 2.0,
                                suffix: "%",
                                on_change: move |v: f64| {
                                    let mut new_settings = draft_settings();
                                    new_settings.metric_gradient_intensity = (v as f32 / 100.0).clamp(0.0, 0.6);
                                    update_draft(new_settings);
                                },
                            }

                            div { class: "setting-row",
                                label { "Dynamic Background" }
                                input {
                                    r#type: "checkbox",
                                    checked: current_settings.metric_dynamic_background,
                                    onchange: move |e: Event<FormData>| {
                                        let mut new_settings = draft_settings();
                                        new_settings.metric_dynamic_background = e.checked();
                                        update_draft(new_settings);
                                    }
                                }
                            }

                            div { class: "setting-row",
                                label { "Show Background Bar" }
                                input {
                                    r#type: "checkbox",
                                    checked: current_settings.metric_show_background_bar,
                                    onchange: move |e: Event<FormData>| {
                                        let mut new_settings = draft_settings();
                                        new_settings.metric_show_background_bar = e.checked();
                                        update_draft(new_settings);
                                    }
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

                            div { class: "setting-row",
                                label { "Show HP value" }
                                input {
                                    r#type: "checkbox",
                                    checked: current_settings.boss_health.show_hp_value,
                                    onchange: move |e: Event<FormData>| {
                                        let mut new_settings = draft_settings();
                                        new_settings.boss_health.show_hp_value = e.checked();
                                        update_draft(new_settings);
                                    }
                                }
                            }

                    Slider {
                        label: "Font Scale",
                        value: (current_settings.boss_health.font_scale * 100.0) as i32 as f64,
                        min: 30.0,
                        max: 300.0,
                        step: 10.0,
                        suffix: "%",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.boss_health.font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                            update_draft(new_settings);
                        },
                    }
                    div { class: "setting-row",
                        label { "Dynamic Background" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.boss_health.dynamic_background,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.boss_health.dynamic_background = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Clear After Combat" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.boss_health.clear_after_combat,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.boss_health.clear_after_combat = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Gradient Bars" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.boss_health.bar_gradient,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.boss_health.bar_gradient = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Show Border" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.boss_health.show_border,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.boss_health.show_border = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Border Color" }
                        input {
                            r#type: "color",
                            value: "{color_to_hex(&current_settings.boss_health.border_color)}",
                            class: "color-picker",
                            disabled: !current_settings.boss_health.show_border,
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft_settings();
                                    new_settings.boss_health.border_color = color;
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
                                new_settings.boss_health = BossHealthConfig::default();
                                new_settings.boss_health_opacity = 180;
                                update_draft(new_settings);
                            },
                            i { class: "fa-solid fa-rotate-left" }
                            span { " Reset Style" }
                        }
                    }
                }
            } else if tab == "timers_a" {
                // Timers A Settings
                div { class: "settings-section",
                    h4 { "Timers A Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.timers_a_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft_settings();
                            new_settings.timers_a_opacity = val;
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Font Color" }
                        input {
                            r#type: "color",
                            value: "{color_to_hex(&current_settings.timers_a_overlay.font_color)}",
                            class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft_settings();
                                    new_settings.timers_a_overlay.font_color = color;
                                    update_draft(new_settings);
                                }
                            }
                        }
                    }

                    Slider {
                        label: "Font Scale",
                        value: (current_settings.timers_a_overlay.font_scale * 100.0) as i32 as f64,
                        min: 30.0,
                        max: 300.0,
                        step: 10.0,
                        suffix: "%",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.timers_a_overlay.font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                            update_draft(new_settings);
                        },
                    }
                    div { class: "setting-row",
                        label { "Dynamic Background" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.timers_a_overlay.dynamic_background,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.timers_a_overlay.dynamic_background = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Stack from Bottom" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.timers_a_overlay.stack_from_bottom,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.timers_a_overlay.stack_from_bottom = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Gradient Bars" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.timers_a_overlay.bar_gradient,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.timers_a_overlay.bar_gradient = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Show Border" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.timers_a_overlay.show_border,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.timers_a_overlay.show_border = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Border Color" }
                        input {
                            r#type: "color",
                            value: "{color_to_hex(&current_settings.timers_a_overlay.border_color)}",
                            class: "color-picker",
                            disabled: !current_settings.timers_a_overlay.show_border,
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft_settings();
                                    new_settings.timers_a_overlay.border_color = color;
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
                                new_settings.timers_a_overlay = TimerOverlayConfig::default();
                                new_settings.timers_a_opacity = 180;
                                update_draft(new_settings);
                            },
                            i { class: "fa-solid fa-rotate-left" }
                            span { " Reset Style" }
                        }
                    }
                }
            } else if tab == "timers_b" {
                // Timers B Settings
                div { class: "settings-section",
                    h4 { "Timers B Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.timers_b_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft_settings();
                            new_settings.timers_b_opacity = val;
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Font Color" }
                        input {
                            r#type: "color",
                            value: "{color_to_hex(&current_settings.timers_b_overlay.font_color)}",
                            class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft_settings();
                                    new_settings.timers_b_overlay.font_color = color;
                                    update_draft(new_settings);
                                }
                            }
                        }
                    }

                    Slider {
                        label: "Font Scale",
                        value: (current_settings.timers_b_overlay.font_scale * 100.0) as i32 as f64,
                        min: 30.0,
                        max: 300.0,
                        step: 10.0,
                        suffix: "%",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.timers_b_overlay.font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                            update_draft(new_settings);
                        },
                    }
                    div { class: "setting-row",
                        label { "Dynamic Background" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.timers_b_overlay.dynamic_background,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.timers_b_overlay.dynamic_background = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Stack from Bottom" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.timers_b_overlay.stack_from_bottom,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.timers_b_overlay.stack_from_bottom = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Gradient Bars" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.timers_b_overlay.bar_gradient,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.timers_b_overlay.bar_gradient = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Show Border" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.timers_b_overlay.show_border,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.timers_b_overlay.show_border = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Border Color" }
                        input {
                            r#type: "color",
                            value: "{color_to_hex(&current_settings.timers_b_overlay.border_color)}",
                            class: "color-picker",
                            disabled: !current_settings.timers_b_overlay.show_border,
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft_settings();
                                    new_settings.timers_b_overlay.border_color = color;
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
                                new_settings.timers_b_overlay = TimerOverlayConfig::default();
                                new_settings.timers_b_opacity = 180;
                                update_draft(new_settings);
                            },
                            i { class: "fa-solid fa-rotate-left" }
                            span { " Reset Style" }
                        }
                    }
                }
            } else if tab == "effects_a" {
                // Effects A Settings
                div { class: "settings-section",
                    h4 { "Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.effects_a_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft_settings();
                            new_settings.effects_a_opacity = val;
                            update_draft(new_settings);
                        },
                    }

                    Slider {
                        label: "Icon/Bar Size",
                        value: current_settings.effects_a.icon_size as f64,
                        min: 16.0,
                        max: 64.0,
                        suffix: "px",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.effects_a.icon_size = (v as u8).clamp(16, 64);
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Max Displayed" }
                        select {
                            class: "input-inline",
                            value: "{current_settings.effects_a.max_display}",
                            onchange: move |e: Event<FormData>| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let mut new_settings = draft_settings();
                                    new_settings.effects_a.max_display = val.clamp(1, 16);
                                    update_draft(new_settings);
                                }
                            },
                            for n in 1..=16u8 {
                                option { value: "{n}", selected: current_settings.effects_a.max_display == n, "{n}" }
                            }
                        }
                    }

                    h4 { style: "margin-top: 16px;", "Layout" }

                    div { class: "setting-row",
                        label { "Bar Mode" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_a.layout_bar,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_a.layout_bar = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Vertical Layout" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_a.layout_vertical,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_a.layout_vertical = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    h4 { style: "margin-top: 16px;", "Display Options" }

                    div { class: "setting-row",
                        label { "Show Header" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_a.show_header,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_a.show_header = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show Countdown" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_a.show_countdown,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_a.show_countdown = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show Effect Names" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_a.show_effect_names,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_a.show_effect_names = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Prioritize Stacked Effects" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_a.stack_priority,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_a.stack_priority = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    Slider {
                        label: "Font Scale",
                        value: (current_settings.effects_a.font_scale * 100.0) as i32 as f64,
                        min: 30.0,
                        max: 300.0,
                        step: 10.0,
                        suffix: "%",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.effects_a.font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                            update_draft(new_settings);
                        },
                    }
                    div { class: "setting-row",
                        label { "Dynamic Background" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_a.dynamic_background,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_a.dynamic_background = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Stack from Bottom" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_a.stack_from_bottom,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_a.stack_from_bottom = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Gradient Bars (bar layout)" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_a.bar_gradient,
                            disabled: !current_settings.effects_a.layout_bar,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_a.bar_gradient = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Show Border (bar layout)" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_a.show_border,
                            disabled: !current_settings.effects_a.layout_bar,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_a.show_border = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Border Color" }
                        input {
                            r#type: "color",
                            value: "{color_to_hex(&current_settings.effects_a.border_color)}",
                            class: "color-picker",
                            disabled: !(current_settings.effects_a.layout_bar && current_settings.effects_a.show_border),
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft_settings();
                                    new_settings.effects_a.border_color = color;
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
                                new_settings.effects_a = EffectsAConfig::default();
                                new_settings.effects_a_opacity = 180;
                                update_draft(new_settings);
                            },
                            i { class: "fa-solid fa-rotate-left" }
                            span { " Reset to Defaults" }
                        }
                    }
                }
            } else if tab == "effects_b" {
                // Effects B Settings
                div { class: "settings-section",
                    h4 { "Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.effects_b_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft_settings();
                            new_settings.effects_b_opacity = val;
                            update_draft(new_settings);
                        },
                    }

                    Slider {
                        label: "Icon/Bar Size",
                        value: current_settings.effects_b.icon_size as f64,
                        min: 16.0,
                        max: 64.0,
                        suffix: "px",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.effects_b.icon_size = (v as u8).clamp(16, 64);
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Max Displayed" }
                        select {
                            class: "input-inline",
                            value: "{current_settings.effects_b.max_display}",
                            onchange: move |e: Event<FormData>| {
                                if let Ok(val) = e.value().parse::<u8>() {
                                    let mut new_settings = draft_settings();
                                    new_settings.effects_b.max_display = val.clamp(1, 16);
                                    update_draft(new_settings);
                                }
                            },
                            for n in 1..=16u8 {
                                option { value: "{n}", selected: current_settings.effects_b.max_display == n, "{n}" }
                            }
                        }
                    }

                    h4 { style: "margin-top: 16px;", "Layout" }

                    div { class: "setting-row",
                        label { "Bar Mode" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_b.layout_bar,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_b.layout_bar = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Vertical Layout" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_b.layout_vertical,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_b.layout_vertical = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    h4 { style: "margin-top: 16px;", "Display Options" }

                    div { class: "setting-row",
                        label { "Show Header" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_b.show_header,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_b.show_header = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show Countdown" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_b.show_countdown,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_b.show_countdown = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show Effect Names" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_b.show_effect_names,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_b.show_effect_names = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Prioritize Stacked Effects" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_b.stack_priority,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_b.stack_priority = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    Slider {
                        label: "Font Scale",
                        value: (current_settings.effects_b.font_scale * 100.0) as i32 as f64,
                        min: 30.0,
                        max: 300.0,
                        step: 10.0,
                        suffix: "%",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.effects_b.font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                            update_draft(new_settings);
                        },
                    }
                    div { class: "setting-row",
                        label { "Dynamic Background" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_b.dynamic_background,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_b.dynamic_background = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Stack from Bottom" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_b.stack_from_bottom,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_b.stack_from_bottom = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Gradient Bars (bar layout)" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_b.bar_gradient,
                            disabled: !current_settings.effects_b.layout_bar,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_b.bar_gradient = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Show Border (bar layout)" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.effects_b.show_border,
                            disabled: !current_settings.effects_b.layout_bar,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.effects_b.show_border = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Border Color" }
                        input {
                            r#type: "color",
                            value: "{color_to_hex(&current_settings.effects_b.border_color)}",
                            class: "color-picker",
                            disabled: !(current_settings.effects_b.layout_bar && current_settings.effects_b.show_border),
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft_settings();
                                    new_settings.effects_b.border_color = color;
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
                                new_settings.effects_b = EffectsBConfig::default();
                                new_settings.effects_b_opacity = 180;
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

                    Slider {
                        label: "Icon/Bar Size",
                        value: current_settings.cooldown_tracker.icon_size as f64,
                        min: 16.0,
                        max: 64.0,
                        suffix: "px",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.cooldown_tracker.icon_size = (v as u8).clamp(16, 64);
                            update_draft(new_settings);
                        },
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
                        label { "Bar Mode" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.cooldown_tracker.layout_bar,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.cooldown_tracker.layout_bar = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show Header" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.cooldown_tracker.show_header,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.cooldown_tracker.show_header = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

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

                    Slider {
                        label: "Font Scale",
                        value: (current_settings.cooldown_tracker.font_scale * 100.0) as i32 as f64,
                        min: 30.0,
                        max: 300.0,
                        step: 10.0,
                        suffix: "%",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.cooldown_tracker.font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                            update_draft(new_settings);
                        },
                    }
                    div { class: "setting-row",
                        label { "Dynamic Background" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.cooldown_tracker.dynamic_background,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.cooldown_tracker.dynamic_background = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Stack from Bottom" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.cooldown_tracker.stack_from_bottom,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.cooldown_tracker.stack_from_bottom = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Gradient Bars (bar layout)" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.cooldown_tracker.bar_gradient,
                            disabled: !current_settings.cooldown_tracker.layout_bar,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.cooldown_tracker.bar_gradient = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Show Border (bar layout)" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.cooldown_tracker.show_border,
                            disabled: !current_settings.cooldown_tracker.layout_bar,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.cooldown_tracker.show_border = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Border Color" }
                        input {
                            r#type: "color",
                            value: "{color_to_hex(&current_settings.cooldown_tracker.border_color)}",
                            class: "color-picker",
                            disabled: !(current_settings.cooldown_tracker.layout_bar && current_settings.cooldown_tracker.show_border),
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft_settings();
                                    new_settings.cooldown_tracker.border_color = color;
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

                    Slider {
                        label: "Icon/Bar Size",
                        value: current_settings.dot_tracker.icon_size as f64,
                        min: 12.0,
                        max: 48.0,
                        suffix: "px",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.dot_tracker.icon_size = (v as u8).clamp(12, 48);
                            update_draft(new_settings);
                        },
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

                    h4 { style: "margin-top: 16px;", "Display Options" }

                    div { class: "setting-row",
                        label { "Bar Mode" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.dot_tracker.layout_bar,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.dot_tracker.layout_bar = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show Header" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.dot_tracker.show_header,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.dot_tracker.show_header = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show Effect Names (bar layout)" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.dot_tracker.show_effect_names,
                            disabled: !current_settings.dot_tracker.layout_bar,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.dot_tracker.show_effect_names = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Show Countdown" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.dot_tracker.show_countdown,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.dot_tracker.show_countdown = e.checked();
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

                    Slider {
                        label: "Font Scale",
                        value: (current_settings.dot_tracker.font_scale * 100.0) as i32 as f64,
                        min: 30.0,
                        max: 300.0,
                        step: 10.0,
                        suffix: "%",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.dot_tracker.font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                            update_draft(new_settings);
                        },
                    }
                    div { class: "setting-row",
                        label { "Dynamic Background" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.dot_tracker.dynamic_background,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.dot_tracker.dynamic_background = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Stack from Bottom" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.dot_tracker.stack_from_bottom,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.dot_tracker.stack_from_bottom = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Gradient Bars (bar layout)" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.dot_tracker.bar_gradient,
                            disabled: !current_settings.dot_tracker.layout_bar,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.dot_tracker.bar_gradient = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Show Border (bar layout)" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.dot_tracker.show_border,
                            disabled: !current_settings.dot_tracker.layout_bar,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.dot_tracker.show_border = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }
                    div { class: "setting-row",
                        label { "Border Color" }
                        input {
                            r#type: "color",
                            value: "{color_to_hex(&current_settings.dot_tracker.border_color)}",
                            class: "color-picker",
                            disabled: !(current_settings.dot_tracker.layout_bar && current_settings.dot_tracker.show_border),
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft_settings();
                                    new_settings.dot_tracker.border_color = color;
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
            } else if tab == "notes" {
                // Notes Overlay Settings
                div { class: "settings-section",
                    h4 { "Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.notes_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft_settings();
                            new_settings.notes_opacity = val;
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Dynamic Background" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.notes_overlay.dynamic_background,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.notes_overlay.dynamic_background = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    Slider {
                        label: "Font Size",
                        value: current_settings.notes_overlay.font_size as f64,
                        min: 10.0,
                        max: 24.0,
                        suffix: "px",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.notes_overlay.font_size = (v as u8).clamp(10, 24);
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Font Color" }
                        input {
                            r#type: "color",
                            value: "{color_to_hex(&current_settings.notes_overlay.font_color)}",
                            class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft_settings();
                                    new_settings.notes_overlay.font_color = color;
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
                                new_settings.notes_overlay = Default::default();
                                new_settings.notes_opacity = 180;
                                update_draft(new_settings);
                            },
                            i { class: "fa-solid fa-rotate-left" }
                            span { " Reset to Defaults" }
                        }
                    }
                }
            } else if tab == "combat_time" {
                // Combat Time Overlay Settings
                div { class: "settings-section",
                    h4 { "Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.combat_time_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft_settings();
                            new_settings.combat_time_opacity = val;
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Show Title" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.combat_time.show_title,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.combat_time.show_title = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Dynamic Background" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.combat_time.dynamic_background,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.combat_time.dynamic_background = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Clear After Combat" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.combat_time.clear_after_combat,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.combat_time.clear_after_combat = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    Slider {
                        label: "Font Scale",
                        value: (current_settings.combat_time.font_scale * 100.0) as i32 as f64,
                        min: 30.0,
                        max: 300.0,
                        suffix: "%",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.combat_time.font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Font Color" }
                        input {
                            r#type: "color",
                            value: "{color_to_hex(&current_settings.combat_time.font_color)}",
                            class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft_settings();
                                    new_settings.combat_time.font_color = color;
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
                                new_settings.combat_time = Default::default();
                                new_settings.combat_time_opacity = 180;
                                update_draft(new_settings);
                            },
                            i { class: "fa-solid fa-rotate-left" }
                            span { " Reset to Defaults" }
                        }
                    }
                }
            } else if tab == "operation_timer" {
                // Operation Timer Overlay Settings
                div { class: "settings-section",
                    h4 { "Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.operation_timer_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft_settings();
                            new_settings.operation_timer_opacity = val;
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Show Title" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.operation_timer.show_title,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.operation_timer.show_title = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Dynamic Background" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.operation_timer.dynamic_background,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.operation_timer.dynamic_background = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    Slider {
                        label: "Font Scale",
                        value: (current_settings.operation_timer.font_scale * 100.0) as i32 as f64,
                        min: 30.0,
                        max: 300.0,
                        suffix: "%",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.operation_timer.font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Font Color" }
                        input {
                            r#type: "color",
                            value: "{color_to_hex(&current_settings.operation_timer.font_color)}",
                            class: "color-picker",
                            oninput: move |e: Event<FormData>| {
                                if let Some(color) = parse_hex_color(&e.value()) {
                                    let mut new_settings = draft_settings();
                                    new_settings.operation_timer.font_color = color;
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
                                new_settings.operation_timer = Default::default();
                                new_settings.operation_timer_opacity = 180;
                                update_draft(new_settings);
                            },
                            i { class: "fa-solid fa-rotate-left" }
                            span { " Reset to Defaults" }
                        }
                    }
                }
            } else if tab == "map" {
                // Map Overlay Settings
                div { class: "settings-section",
                    h4 { "Appearance" }

                    OpacitySlider {
                        label: "Background Opacity",
                        value: current_settings.map_opacity,
                        on_change: move |val| {
                            let mut new_settings = draft_settings();
                            new_settings.map_opacity = val;
                            update_draft(new_settings);
                        },
                    }

                    div { class: "setting-row",
                        label { "Preserve Aspect Ratio" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.map.preserve_aspect,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.map.preserve_aspect = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Lock Size" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.map.lock_size,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.map.lock_size = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Width" }
                        input {
                            r#type: "number",
                            min: "50",
                            max: "2560",
                            value: "{current_settings.map.width}",
                            oninput: move |e: Event<FormData>| {
                                if let Ok(v) = e.value().parse::<u32>() {
                                    let mut new_settings = draft_settings();
                                    new_settings.map.width = v.clamp(50, 2560);
                                    update_draft(new_settings);
                                }
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Height" }
                        input {
                            r#type: "number",
                            min: "50",
                            max: "2048",
                            value: "{current_settings.map.height}",
                            oninput: move |e: Event<FormData>| {
                                if let Ok(v) = e.value().parse::<u32>() {
                                    let mut new_settings = draft_settings();
                                    new_settings.map.height = v.clamp(50, 2048);
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
                                new_settings.map = Default::default();
                                new_settings.map_opacity = 180;
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

                            div { class: "setting-row",
                                label { "Show Background Bar" }
                                input {
                                    r#type: "checkbox",
                                    checked: challenge_config.show_background_bar,
                                    onchange: move |e: Event<FormData>| {
                                        let mut new_settings = draft_settings();
                                        new_settings.challenge_overlay.show_background_bar = e.checked();
                                        update_draft(new_settings);
                                    }
                                }
                            }

                            div { class: "setting-row",
                                label { "Gradient Bars" }
                                input {
                                    r#type: "checkbox",
                                    checked: challenge_config.bar_gradient,
                                    onchange: move |e: Event<FormData>| {
                                        let mut new_settings = draft_settings();
                                        new_settings.challenge_overlay.bar_gradient = e.checked();
                                        update_draft(new_settings);
                                    }
                                }
                            }

                            Slider {
                                label: "Bar Spacing",
                                value: (challenge_config.bar_spacing_ratio * 100.0) as i32 as f64,
                                min: 0.0,
                                max: 60.0,
                                step: 5.0,
                                suffix: "%",
                                on_change: move |v: f64| {
                                    let mut new_settings = draft_settings();
                                    new_settings.challenge_overlay.bar_spacing_ratio = (v as f32 / 100.0).clamp(0.0, 0.6);
                                    update_draft(new_settings);
                                },
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

                    Slider {
                        label: "Font Scale",
                        value: (current_settings.challenge_overlay.font_scale * 100.0) as i32 as f64,
                        min: 30.0,
                        max: 300.0,
                        step: 10.0,
                        suffix: "%",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.challenge_overlay.font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                            update_draft(new_settings);
                        },
                    }
                    div { class: "setting-row",
                        label { "Dynamic Background" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.challenge_overlay.dynamic_background,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.challenge_overlay.dynamic_background = e.checked();
                                update_draft(new_settings);
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

                    Slider {
                        label: "Font Size",
                        value: current_settings.alerts_overlay.font_size as f64,
                        min: 8.0,
                        max: 24.0,
                        suffix: "px",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.alerts_overlay.font_size = (v as u8).clamp(8, 24);
                            update_draft(new_settings);
                        },
                    }

                    h4 { style: "margin-top: 16px;", "Display" }

                    div { class: "setting-row",
                        label { "Show Icons" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.alerts_overlay.show_icons,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.alerts_overlay.show_icons = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Dynamic Background" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.alerts_overlay.dynamic_background,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.alerts_overlay.dynamic_background = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

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

                    Slider {
                        label: "Display Duration",
                        value: current_settings.alerts_overlay.default_duration as f64,
                        min: 1.0,
                        max: 15.0,
                        step: 0.5,
                        suffix: "s",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.alerts_overlay.default_duration = (v as f32).clamp(1.0, 15.0);
                            update_draft(new_settings);
                        },
                    }

                    Slider {
                        label: "Fade Duration",
                        value: current_settings.alerts_overlay.fade_duration as f64,
                        min: 0.0,
                        max: 3.0,
                        step: 0.5,
                        suffix: "s",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.alerts_overlay.fade_duration = (v as f32).clamp(0.0, 3.0);
                            update_draft(new_settings);
                        },
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
                                            new_settings.raid_overlay.grid_columns = val.clamp(1, 6);
                                            update_draft(new_settings);
                                        }
                                    },
                                    for c in [1u8, 2, 3, 4, 5, 6] {
                                        option {
                                            value: "{c}",
                                            selected: cols == c,
                                            disabled: c as u16 * rows as u16 > 24,
                                            "{c}"
                                        }
                                    }
                                }
                            }

                            div { class: "setting-row",
                                label { "Rows" }
                                select {
                                    value: "{rows}",
                                    onchange: move |e: Event<FormData>| {
                                        if let Ok(val) = e.value().parse::<u8>() {
                                            let mut new_settings = draft_settings();
                                            new_settings.raid_overlay.grid_rows = val.clamp(1, 24);
                                            update_draft(new_settings);
                                        }
                                    },
                                    for r in [1u8, 2, 4, 8, 12, 16, 20, 24] {
                                        option {
                                            value: "{r}",
                                            selected: rows == r,
                                            disabled: cols as u16 * r as u16 > 24,
                                            "{r}"
                                        }
                                    }
                                }
                            }

                            Slider {
                                label: "Frame Spacing",
                                value: current_settings.raid_overlay.frame_spacing as i32 as f64,
                                min: 0.0,
                                max: 75.0,
                                suffix: "px",
                                on_change: move |v: f64| {
                                    let mut new_settings = draft_settings();
                                    new_settings.raid_overlay.frame_spacing = (v as f32).clamp(0.0, 75.0);
                                    update_draft(new_settings);
                                },
                            }

                            div { class: "setting-row",
                                span { class: "hint", "Total slots: {cols * rows}" }
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

                            Slider {
                                label: "Effect Size",
                                value: current_settings.raid_overlay.effect_size as i32 as f64,
                                min: 8.0,
                                max: 36.0,
                                suffix: "px",
                                on_change: move |v: f64| {
                                    let mut new_settings = draft_settings();
                                    new_settings.raid_overlay.effect_size = (v as f32).clamp(8.0, 36.0);
                                    update_draft(new_settings);
                                },
                            }

                            Slider {
                                label: "Effect Vertical Offset",
                                value: current_settings.raid_overlay.effect_vertical_offset as i32 as f64,
                                min: -10.0,
                                max: 30.0,
                                suffix: "px",
                                on_change: move |v: f64| {
                                    let mut new_settings = draft_settings();
                                    new_settings.raid_overlay.effect_vertical_offset = (v as f32).clamp(-10.0, 30.0);
                                    update_draft(new_settings);
                                },
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

                            div { class: "setting-row",
                                label { "Show Class Icons" }
                                input {
                                    r#type: "checkbox",
                                    checked: current_settings.raid_overlay.show_class_icons,
                                    onchange: move |e: Event<FormData>| {
                                        let mut new_settings = draft_settings();
                                        new_settings.raid_overlay.show_class_icons = e.checked();
                                        update_draft(new_settings);
                                    }
                                }
                            }

                            div { class: "setting-row",
                                label { "Show Effect Icons" }
                                input {
                                    r#type: "checkbox",
                                    checked: current_settings.raid_overlay.show_effect_icons,
                                    onchange: move |e: Event<FormData>| {
                                        let mut new_settings = draft_settings();
                                        new_settings.raid_overlay.show_effect_icons = e.checked();
                                        update_draft(new_settings);
                                    }
                                }
                            }
                            p { class: "hint", "Display ability icons instead of colored squares (requires icon pack)" }

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
                                    div { class: "stat-order-item", key: "{idx}",
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
                                                    let stats = &mut new_settings.personal_overlay.visible_stats;
                                                    if idx < stats.len() { stats.remove(idx); }
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
                                            let stat = *stat;
                                            // Separator can always be added (multiple times)
                                            // Other stats can only be added once
                                            let is_visible = stat != PersonalStat::Separator
                                                && current_settings.personal_overlay.visible_stats.contains(&stat);
                                            if !is_visible {
                                                rsx! {
                                                    button {
                                                        class: "btn-add-stat",
                                                        onclick: move |_| {
                                                            let mut new_settings = draft_settings();
                                                            if stat == PersonalStat::Separator
                                                                || !new_settings.personal_overlay.visible_stats.contains(&stat)
                                                            {
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

                    Slider {
                        label: "Font Scale",
                        value: (current_settings.personal_overlay.font_scale * 100.0) as i32 as f64,
                        min: 30.0,
                        max: 300.0,
                        step: 10.0,
                        suffix: "%",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.personal_overlay.font_scale = (v as f32 / 100.0).clamp(0.3, 3.0);
                            update_draft(new_settings);
                        },
                    }
                    div { class: "setting-row",
                        label { "Dynamic Background" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.personal_overlay.dynamic_background,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.personal_overlay.dynamic_background = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Auto-color Values" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.personal_overlay.auto_color_values,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.personal_overlay.auto_color_values = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    div { class: "setting-row",
                        label { "Hide Empty Values" }
                        input {
                            r#type: "checkbox",
                            checked: current_settings.personal_overlay.hide_empty_values,
                            onchange: move |e: Event<FormData>| {
                                let mut new_settings = draft_settings();
                                new_settings.personal_overlay.hide_empty_values = e.checked();
                                update_draft(new_settings);
                            }
                        }
                    }

                    Slider {
                        label: "Line Spacing",
                        value: (current_settings.personal_overlay.line_spacing * 100.0) as i32 as f64,
                        min: 70.0,
                        max: 150.0,
                        step: 10.0,
                        suffix: "%",
                        on_change: move |v: f64| {
                            let mut new_settings = draft_settings();
                            new_settings.personal_overlay.line_spacing = (v as f32 / 100.0).clamp(0.7, 1.5);
                            update_draft(new_settings);
                        },
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

                            div { class: "setting-row",
                                label { "Color Bars by Class" }
                                input {
                                    r#type: "checkbox",
                                    checked: current_appearance.use_class_color,
                                    onchange: {
                                        let tab = tab_key.clone();
                                        move |e: Event<FormData>| {
                                            let mut new_settings = draft_settings();
                                            let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                            let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                            appearance.use_class_color = e.checked();
                                            update_draft(new_settings);
                                        }
                                    }
                                }
                            }

                            div { class: "setting-row",
                                label { "Gradient Bars" }
                                input {
                                    r#type: "checkbox",
                                    checked: current_appearance.bar_gradient,
                                    onchange: {
                                        let tab = tab_key.clone();
                                        move |e: Event<FormData>| {
                                            let mut new_settings = draft_settings();
                                            let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                            let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                            appearance.bar_gradient = e.checked();
                                            update_draft(new_settings);
                                        }
                                    }
                                }
                            }

                            Slider {
                                label: "Bar Spacing",
                                value: (current_appearance.bar_spacing_ratio * 100.0) as i32 as f64,
                                min: 0.0,
                                max: 60.0,
                                step: 5.0,
                                suffix: "%",
                                on_change: {
                                    let tab = tab_key.clone();
                                    move |v: f64| {
                                        let mut new_settings = draft_settings();
                                        let default = new_settings.default_appearances.get(&tab).cloned().unwrap_or_default();
                                        let appearance = new_settings.appearances.entry(tab.clone()).or_insert(default);
                                        appearance.bar_spacing_ratio = (v as f32 / 100.0).clamp(0.0, 0.6);
                                        update_draft(new_settings);
                                    }
                                },
                            }

                            // Font Scale and Dynamic Background are in Global Metrics Settings

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

            } // End settings-content

            // ─────────────────────────────────────────────────────────────────
            // Save button
            // ─────────────────────────────────────────────────────────────────
            div { class: "settings-footer",
                button {
                    class: if has_changes() || profile_dirty() { "btn btn-save btn-unsaved" } else { "btn btn-save" },
                    disabled: !has_changes() && !profile_dirty(),
                    onclick: save_to_backend,
                    if has_changes() || profile_dirty() { "Save *" } else { "Save" }
                }
                if !save_status().is_empty() {
                    span { class: "save-status", "{save_status()}" }
                }
                div { class: "footer-spacer" }
                button {
                    class: "btn btn-reset-positions",
                    title: "Reset all overlay positions to defaults (useful if overlays are off-screen)",
                    onclick: move |_| {
                        async move {
                            if let Some(mut config) = api::get_config().await {
                                config.overlay_settings.positions.clear();
                                if let Err(err) = api::update_config(&config).await {
                                    toast.show(format!("Failed to reset positions: {}", err), ToastSeverity::Normal);
                                } else {
                                    api::refresh_overlay_settings().await;
                                    settings.set(config.overlay_settings.clone());
                                    draft_settings.set(config.overlay_settings);
                                    profile_dirty.set(true);
                                    toast.show("All overlay positions reset to defaults".to_string(), ToastSeverity::Normal);
                                }
                            }
                        }
                    },
                    i { class: "fa-solid fa-arrows-to-dot" }
                    span { " Reset Positions" }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Simple Sub-Components (these work with simple props)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn TabButton(label: &'static str, tab_key: &'static str, selected_tab: Signal<String>, metrics_global_open: Signal<bool>) -> Element {
    let is_active = selected_tab() == tab_key;
    rsx! {
        button {
            class: if is_active { "tab-btn active" } else { "tab-btn" },
            onclick: move |_| {
                selected_tab.set(tab_key.to_string());
                metrics_global_open.set(false);
            },
            "{label}"
        }
    }
}

#[component]
fn OpacitySlider(label: &'static str, value: u8, on_change: EventHandler<u8>) -> Element {
    rsx! {
        Slider {
            label,
            value: value as f64,
            min: 0.0,
            max: 255.0,
            on_change: move |v: f64| on_change.call(v.round() as u8),
        }
    }
}

