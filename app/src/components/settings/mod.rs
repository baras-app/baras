//! Overlay settings drawer: profiles, global options, and one tab per overlay.
//!
//! Slides in from the right so the overlay control panel stays usable beside
//! it; the gear on any overlay toggle opens it on that overlay.

mod ability_cast;
mod alerts;
mod boss_health;
mod challenges;
mod combat_time;
mod cooldowns;
mod dot_tracker;
mod draft;
mod effects;
mod enemy_frames;
mod frame_style;
mod global;
mod metric;
mod metrics_global;
mod notes;
mod operation_timer;
mod overrides;
mod personal;
mod profiles;
mod raid;
mod timers;
mod widgets;

use dioxus::prelude::*;

use crate::api;
use crate::components::slider::Slider;
use crate::components::{ToastSeverity, use_toast};
use crate::types::{MetricType, OverlaySettings};
use baras_types::{MAX_LIST_SCALE, MIN_LIST_SCALE};

use ability_cast::AbilityCastTab;
use alerts::AlertsTab;
use boss_health::BossHealthTab;
use challenges::ChallengesTab;
use combat_time::CombatTimeTab;
use cooldowns::{CooldownGroup, CooldownsTab};
use dot_tracker::DotTrackerTab;
use draft::ProfileSwitchGuard;
pub use draft::use_profile_switch_guard;
use draft::use_settings_draft;
use effects::{EffectsGroup, EffectsTab};
use enemy_frames::EnemyFramesTab;
use global::GlobalTab;
use metric::MetricTab;
use notes::NotesTab;
use operation_timer::OperationTimerTab;
use overrides::OverridesSection;
use personal::PersonalTab;
use profiles::ProfilesSection;
use raid::RaidTab;
use timers::{TimerGroup, TimersTab};
use widgets::TabButton;

/// Top-level drawer tabs
#[derive(Clone, Copy, PartialEq)]
pub enum SettingsSection {
    Overlays,
    Profiles,
    Global,
}

/// Non-metric overlays as the selector groups them: `(group, [(label, tab key)])`
const OVERLAY_GROUPS: &[(&str, &[(&str, &str)])] = &[
    (
        "General",
        &[
            ("Personal Stats", "personal"),
            ("Raid Frames", "raid"),
            ("Alerts", "alerts"),
            ("Combat Time", "combat_time"),
            ("Op Timer", "operation_timer"),
            ("Abilities Cast", "ability_cast"),
        ],
    ),
    (
        "Encounters",
        &[
            ("Boss Health", "boss_health"),
            ("Timers A", "timers_a"),
            ("Timers B", "timers_b"),
            ("Challenges", "challenges"),
            ("Notes", "notes"),
        ],
    ),
    (
        "Effects",
        &[
            ("Effects A", "effects_a"),
            ("Effects B", "effects_b"),
            ("Effects C", "effects_c"),
            ("Cooldowns A", "cooldowns"),
            ("Cooldowns B", "cooldowns_b"),
            ("DOT Tracker", "dot_tracker"),
        ],
    ),
    ("PvP", &[("Enemy Frames", "enemy_frames")]),
];

/// Every selectable overlay, with the metric overlays appended to their groups
fn overlay_groups() -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    let metric = |m: &MetricType| (m.label(), m.config_key());
    let mut groups: Vec<_> = OVERLAY_GROUPS.iter().map(|(g, items)| (*g, items.to_vec())).collect();
    if let Some((_, pvp)) = groups.iter_mut().find(|(g, _)| *g == "PvP") {
        pvp.extend(MetricType::pvp().iter().map(metric));
    }
    let metrics = MetricType::all()
        .iter()
        .filter(|m| !MetricType::pvp().contains(m))
        .map(metric)
        .collect();
    groups.push(("Metrics", metrics));
    groups
}

fn overlay_label(key: &str) -> &'static str {
    overlay_groups()
        .iter()
        .flat_map(|(_, items)| items.iter())
        .find(|(_, k)| *k == key)
        .map_or("Overlay", |(label, _)| *label)
}

#[component]
pub fn SettingsPanel(
    settings: Signal<OverlaySettings>,
    selected_tab: Signal<String>,
    profile_names: Signal<Vec<String>>,
    active_profile: Signal<Option<String>>,
    overlays_enabled: Signal<std::collections::HashSet<String>>,
    overlays_visible: Signal<bool>,
    /// Which top tab shows; lifted so a control-panel gear can switch to Overlays
    mut section: Signal<SettingsSection>,
    on_settings_saved: EventHandler<()>,
    /// Another tab is open: stay mounted but hidden
    #[props(default)]
    parked: bool,
    /// "Keep editing" on the leave prompt: back to the Overlays page
    on_return: EventHandler<()>,
    profile_guard: ProfileSwitchGuard,
) -> Element {
    let draft = use_settings_draft(settings);
    use_effect(move || profile_guard.set_dirty(draft.has_changes()));
    // The panel outlives the Overlays page (parked while another tab is open), so
    // unsaved edits keep previewing; dropping it puts the overlays back to the saved settings
    use_drop(move || {
        profile_guard.set_dirty(false);
        if draft.abandon() {
            wasm_bindgen_futures::spawn_local(async {
                api::refresh_overlay_settings().await;
            });
        }
    });
    let mut toast = use_toast();

    let tab = selected_tab();
    let is_metric_tab = MetricType::all().iter().any(|t| t.config_key() == tab);
    let showing_overlays = section() == SettingsSection::Overlays;
    let overlay_groups = overlay_groups();
    // Captured by the footer's reset button
    let reset_key = tab.clone();
    let reset_label = overlay_label(&tab);

    // Save settings to backend
    let save_to_backend = move || {
        let new_settings = draft.current();
        async move {
            if let Some(mut config) = api::get_config().await {
                // Preserve existing positions and enabled state
                let existing_positions = config.overlay_settings.positions.clone();
                let existing_enabled = config.overlay_settings.enabled.clone();

                config.overlay_settings.appearances = new_settings.appearances.clone();
                config.overlay_settings.frame_style = new_settings.frame_style;
                config.overlay_settings.frame_overrides = new_settings.frame_overrides.clone();
                config.overlay_settings.scales = new_settings.scales.clone();
                config.overlay_settings.personal_overlay = new_settings.personal_overlay.clone();
                config.overlay_settings.metric_show_empty_bars = new_settings.metric_show_empty_bars;
                config.overlay_settings.metric_stack_from_bottom = new_settings.metric_stack_from_bottom;
                config.overlay_settings.metric_scaling_factor = new_settings.metric_scaling_factor;
                config.overlay_settings.metric_bar_gap = new_settings.metric_bar_gap;
                config.overlay_settings.metric_bar_radius = new_settings.metric_bar_radius;
                config.overlay_settings.metric_font_scale = new_settings.metric_font_scale;
                config.overlay_settings.metric_gradient_intensity = new_settings.metric_gradient_intensity;
                config.overlay_settings.metric_dynamic_background = new_settings.metric_dynamic_background;
                config.overlay_settings.metric_show_background_bar = new_settings.metric_show_background_bar;
                config.overlay_settings.class_icon_mode = new_settings.class_icon_mode;
                config.overlay_settings.class_colors = new_settings.class_colors.clone();
                config.overlay_settings.raid_overlay = new_settings.raid_overlay.clone();
                config.overlay_settings.boss_health = new_settings.boss_health.clone();
                config.overlay_settings.timers_a_overlay = new_settings.timers_a_overlay.clone();
                config.overlay_settings.timers_b_overlay = new_settings.timers_b_overlay.clone();
                config.overlay_settings.effects_overlay = new_settings.effects_overlay.clone();
                config.overlay_settings.challenge_overlay = new_settings.challenge_overlay.clone();
                config.overlay_settings.alerts_overlay = new_settings.alerts_overlay.clone();
                config.overlay_settings.effects_a = new_settings.effects_a.clone();
                config.overlay_settings.effects_b = new_settings.effects_b.clone();
                config.overlay_settings.effects_c = new_settings.effects_c.clone();
                config.overlay_settings.cooldown_tracker = new_settings.cooldown_tracker.clone();
                config.overlay_settings.cooldown_tracker_b = new_settings.cooldown_tracker_b.clone();
                config.overlay_settings.dot_tracker = new_settings.dot_tracker.clone();
                config.overlay_settings.notes_overlay = new_settings.notes_overlay.clone();
                config.overlay_settings.combat_time = new_settings.combat_time.clone();
                config.overlay_settings.operation_timer = new_settings.operation_timer.clone();
                config.overlay_settings.ability_queue = new_settings.ability_queue.clone();
                config.overlay_settings.enemy_frames = new_settings.enemy_frames.clone();
                config.overlay_settings.ability_cast = new_settings.ability_cast.clone();
                config.overlay_settings.positions = existing_positions;
                config.overlay_settings.enabled = existing_enabled;

                if let Err(err) = api::update_config(&config).await {
                    draft.set_status(format!("Failed to save: {}", err));
                    false
                } else {
                    api::refresh_overlay_settings().await;
                    // Use the merged config (draft appearance + backend positions/enabled/
                    // behavioral flags) rather than the stale draft, so we don't overwrite
                    // global settings like hide_when_not_live or hide_during_conversations.
                    settings.set(config.overlay_settings.clone());
                    draft.set_status("Settings saved!".to_string());
                    on_settings_saved.call(());
                    true
                }
            } else {
                false
            }
        }
    };

    // Footer and leave prompt: drop the edits and put the overlays back as saved
    let discard = move |_| {
        draft.replace(settings());
        spawn(async move {
            api::refresh_overlay_settings().await;
        });
    };
    // Says where the changes go
    let save_label = match active_profile() {
        Some(profile) if draft.has_changes() => format!("Save to '{profile}'"),
        _ => "Save".to_string(),
    };
    // The prompts' buttons stay short, so their text names the profile
    let save_hint = match active_profile() {
        Some(profile) => format!("Saving writes them to '{profile}'."),
        None => String::new(),
    };

    // Sits beside each overlay tab's style reset
    let reset_position = rsx! {
        button {
            class: "btn btn-reset",
            title: "Move this overlay back to its default position (useful if it is off-screen)",
            onclick: move |_| {
                let key = reset_key.clone();
                async move {
                    match api::reset_overlay_position(&key).await {
                        Ok(_) => {
                            toast.show(format!("{reset_label} position reset"), ToastSeverity::Success);
                        }
                        Err(err) => toast.show(format!("Failed to reset position: {err}"), ToastSeverity::Normal),
                    }
                }
            },
            i { class: "fa-solid fa-arrows-to-dot" }
            span { " Reset Position" }
        }
    };

    let tab_content = match section() {
        SettingsSection::Profiles => rsx! {
            ProfilesSection {
                settings,
                profile_names,
                active_profile,
                overlays_enabled,
                overlays_visible,
                profile_guard,
            }
        },
        SettingsSection::Global => rsx! { GlobalTab { draft } },
        SettingsSection::Overlays => match tab.as_str() {
            "boss_health" => rsx! { BossHealthTab { draft, actions: reset_position } },
            "timers_a" => rsx! { TimersTab { draft, group: TimerGroup::A, actions: reset_position } },
            "timers_b" => rsx! { TimersTab { draft, group: TimerGroup::B, actions: reset_position } },
            "effects_a" => rsx! { EffectsTab { draft, group: EffectsGroup::A, actions: reset_position } },
            "effects_b" => rsx! { EffectsTab { draft, group: EffectsGroup::B, actions: reset_position } },
            "effects_c" => rsx! { EffectsTab { draft, group: EffectsGroup::C, actions: reset_position } },
            "cooldowns" => rsx! { CooldownsTab { draft, group: CooldownGroup::A, actions: reset_position } },
            "cooldowns_b" => rsx! { CooldownsTab { draft, group: CooldownGroup::B, actions: reset_position } },
            "dot_tracker" => rsx! { DotTrackerTab { draft, actions: reset_position } },
            "notes" => rsx! { NotesTab { draft, actions: reset_position } },
            "combat_time" => rsx! { CombatTimeTab { draft, actions: reset_position } },
            "ability_cast" => rsx! { AbilityCastTab { draft, actions: reset_position } },
            "operation_timer" => rsx! { OperationTimerTab { draft, actions: reset_position } },
            "challenges" => rsx! { ChallengesTab { draft, actions: reset_position } },
            "alerts" => rsx! { AlertsTab { draft, actions: reset_position } },
            "enemy_frames" => rsx! { EnemyFramesTab { draft, actions: reset_position } },
            "raid" => rsx! { RaidTab { draft, actions: reset_position } },
            "personal" => rsx! { PersonalTab { draft, actions: reset_position } },
            _ => rsx! { MetricTab { draft, tab_key: tab.clone(), actions: reset_position } },
        },
    };

    rsx! {
        // Parked: off-screen and out of the docked layout, with its draft intact
        section {
            class: if parked { "drawer settings-panel-parked" } else { "drawer settings-panel" },
            div { class: "settings-header",
                h3 { "Overlay Settings" }
                if draft.has_changes() {
                    span { class: "unsaved-indicator", title: "Style changes are previewing on the overlays but not saved yet" }
                }
            }

            // Outside the scrolling content, so the section and the overlay being
            // edited stay in view
            div { class: "settings-top-tabs",
                TabButton { label: "Overlays", active: section() == SettingsSection::Overlays, onclick: move |_| section.set(SettingsSection::Overlays) }
                TabButton { label: "Profiles", active: section() == SettingsSection::Profiles, onclick: move |_| section.set(SettingsSection::Profiles) }
                TabButton { label: "Global", active: section() == SettingsSection::Global, onclick: move |_| section.set(SettingsSection::Global) }
            }
            if showing_overlays {
                div { class: "setting-row overlay-select-row",
                    label { "Overlay" }
                    select {
                        value: "{tab}",
                        onchange: move |e: Event<FormData>| selected_tab.set(e.value()),
                        for (group, items) in overlay_groups.iter() {
                            optgroup { label: "{group}",
                                for (label, key) in items.iter() {
                                    option { key: "{key}", value: "{key}", selected: *key == tab, "{label}" }
                                }
                            }
                        }
                    }
                }
            }

            div { class: "settings-content",
                if showing_overlays {
                    h4 { class: "overlay-options-title", "{overlay_label(&tab)} Options" }
                    if let Some(scale) = draft.current().list_scale(&tab) {
                        div { class: "settings-section",
                            Slider {
                                label: "Scale",
                                value: (scale * 100.0).round() as f64,
                                min: (MIN_LIST_SCALE * 100.0) as f64,
                                max: (MAX_LIST_SCALE * 100.0) as f64,
                                suffix: "%",
                                tooltip: "Size of this overlay's rows, icons and text. Resizing the window only changes how much fits.",
                                on_change: move |v: f64| draft.edit(|s| s.set_list_scale(&selected_tab(), v as f32 / 100.0)),
                            }
                        }
                    }
                }
                {tab_content}
                if showing_overlays {
                    OverridesSection {
                        draft,
                        tab_key: selected_tab,
                        is_metric: is_metric_tab,
                        on_open_global: move |_| section.set(SettingsSection::Global),
                    }
                }
            }

            // Style edits are the only thing left to save (toggles and moves save
            // themselves), so their unsaved state is called out here, on their page
            div { class: if draft.has_changes() { "settings-footer unsaved" } else { "settings-footer" },
                if draft.has_changes() {
                    span { class: "settings-footer-note",
                        i { class: "fa-solid fa-circle-exclamation" }
                        " Unsaved style changes"
                    }
                }
                button {
                    class: if draft.has_changes() { "btn btn-save btn-unsaved" } else { "btn btn-save" },
                    disabled: !draft.has_changes(),
                    onclick: move |_| async move { save_to_backend().await; },
                    // The highlight is the unsaved marker
                    "{save_label}"
                }
                if draft.has_changes() {
                    button {
                        class: "btn",
                        title: "Drop the unsaved style changes and put the overlays back as saved",
                        onclick: discard,
                        "Discard"
                    }
                }
                if !draft.status().is_empty() {
                    span { class: "save-status", "{draft.status()}" }
                }
            }
        }

        // A profile switch would replace the draft: settle it first. The switch
        // reloads the overlays itself, so Discard skips the refresh
        if profile_guard.is_pending() {
            div { class: "modal-overlay",
                div { class: "modal-content discard-prompt",
                    div { class: "modal-header", h3 { "Unsaved overlay changes" } }
                    p { "Switching profiles replaces your unsaved overlay style edits. {save_hint}" }
                    if !draft.status().is_empty() {
                        p { class: "save-status", "{draft.status()}" }
                    }
                    div { class: "modal-footer",
                        button { class: "btn btn-sm", onclick: move |_| { profile_guard.take(); }, "Cancel" }
                        button {
                            class: "btn btn-danger btn-sm",
                            onclick: move |_| {
                                draft.replace(settings());
                                if let Some(switch) = profile_guard.take() { switch.call(()); }
                            },
                            "Discard"
                        }
                        button {
                            class: "btn btn-save btn-sm",
                            onclick: move |_| async move {
                                if save_to_backend().await && let Some(switch) = profile_guard.take() {
                                    switch.call(());
                                }
                            },
                            "Save and switch"
                        }
                    }
                }
            }
        } else if parked && draft.has_changes() {
            // Left the page mid-edit: settle the draft instead of leaving it
            // previewing out of sight. Outside the parked (hidden) section
            div { class: "modal-overlay",
                div { class: "modal-content discard-prompt",
                    div { class: "modal-header", h3 { "Unsaved overlay changes" } }
                    p { "Your overlay style edits are previewing but have not been saved. {save_hint}" }
                    if !draft.status().is_empty() {
                        p { class: "save-status", "{draft.status()}" }
                    }
                    div { class: "modal-footer",
                        button { class: "btn btn-sm", onclick: move |_| on_return.call(()), "Keep editing" }
                        button { class: "btn btn-danger btn-sm", onclick: discard, "Discard" }
                        button { class: "btn btn-save btn-sm", onclick: move |_| async move { save_to_backend().await; }, "Save" }
                    }
                }
            }
        }
    }
}
