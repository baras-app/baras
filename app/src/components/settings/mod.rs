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
mod global;
mod metric;
mod metrics_global;
mod notes;
mod operation_timer;
mod personal;
mod profiles;
mod raid;
mod timers;
mod widgets;

use dioxus::prelude::*;
use std::collections::HashMap;

use crate::api;
use crate::components::{ToastSeverity, use_toast};
use crate::types::{MetricType, OverlaySettings};

use ability_cast::AbilityCastTab;
use alerts::AlertsTab;
use boss_health::BossHealthTab;
use challenges::ChallengesTab;
use combat_time::CombatTimeTab;
use cooldowns::{CooldownGroup, CooldownsTab};
use dot_tracker::DotTrackerTab;
use draft::use_settings_draft;
use effects::{EffectsGroup, EffectsTab};
use enemy_frames::EnemyFramesTab;
use global::GlobalTab;
use metric::MetricTab;
use metrics_global::MetricsGlobalSection;
use notes::NotesTab;
use operation_timer::OperationTimerTab;
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
    ("General", &[
        ("Personal Stats", "personal"),
        ("Raid Frames", "raid"),
        ("Alerts", "alerts"),
        ("Combat Time", "combat_time"),
        ("Op Timer", "operation_timer"),
        ("Abilities Cast", "ability_cast"),
    ]),
    ("Encounters", &[
        ("Boss Health", "boss_health"),
        ("Timers A", "timers_a"),
        ("Timers B", "timers_b"),
        ("Challenges", "challenges"),
        ("Notes", "notes"),
    ]),
    ("Effects", &[
        ("Effects A", "effects_a"),
        ("Effects B", "effects_b"),
        ("Effects C", "effects_c"),
        ("Cooldowns A", "cooldowns"),
        ("Cooldowns B", "cooldowns_b"),
        ("DOT Tracker", "dot_tracker"),
    ]),
    ("PvP", &[("Enemy Frames", "enemy_frames")]),
];

/// Every selectable overlay, with the metric overlays appended to their groups
fn overlay_groups() -> Vec<(&'static str, Vec<(&'static str, &'static str)>)> {
    let metric = |m: &MetricType| (m.label(), m.config_key());
    let mut groups: Vec<_> = OVERLAY_GROUPS.iter().map(|(g, items)| (*g, items.to_vec())).collect();
    if let Some((_, pvp)) = groups.iter_mut().find(|(g, _)| *g == "PvP") {
        pvp.extend(MetricType::pvp().iter().map(metric));
    }
    let metrics = MetricType::all().iter().filter(|m| !MetricType::pvp().contains(m)).map(metric).collect();
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
    metric_overlays_enabled: Signal<HashMap<MetricType, bool>>,
    personal_enabled: Signal<bool>,
    raid_enabled: Signal<bool>,
    overlays_visible: Signal<bool>,
    profile_dirty: Signal<bool>,
    /// Which top tab shows; lifted so a control-panel gear can switch to Overlays
    mut section: Signal<SettingsSection>,
    on_settings_saved: EventHandler<()>,
) -> Element {
    let draft = use_settings_draft(settings);
    // The panel lives as long as the Overlays page is open; leaving it drops
    // unsaved edits and puts the overlays back to the saved settings
    use_drop(move || {
        if draft.abandon() {
            wasm_bindgen_futures::spawn_local(async {
                api::refresh_overlay_settings().await;
            });
        }
    });
    let mut metrics_global_open = use_signal(|| false);
    let mut toast = use_toast();

    let tab = selected_tab();
    let is_metric_tab = MetricType::all().iter().any(|t| t.config_key() == tab);
    let showing_overlays = section() == SettingsSection::Overlays;
    let overlay_groups = overlay_groups();
    // Captured by the footer's reset button
    let reset_key = tab.clone();
    let reset_label = overlay_label(&tab);

    // Save settings to backend
    let save_to_backend = move |_| {
        let new_settings = draft.current();
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
                config.overlay_settings.effects_c = new_settings.effects_c.clone();
                config.overlay_settings.effects_c_opacity = new_settings.effects_c_opacity;
                config.overlay_settings.cooldown_tracker = new_settings.cooldown_tracker.clone();
                config.overlay_settings.cooldown_tracker_opacity =
                    new_settings.cooldown_tracker_opacity;
                config.overlay_settings.cooldown_tracker_b = new_settings.cooldown_tracker_b.clone();
                config.overlay_settings.cooldown_tracker_b_opacity =
                    new_settings.cooldown_tracker_b_opacity;
                config.overlay_settings.dot_tracker = new_settings.dot_tracker.clone();
                config.overlay_settings.dot_tracker_opacity = new_settings.dot_tracker_opacity;
                config.overlay_settings.notes_overlay = new_settings.notes_overlay.clone();
                config.overlay_settings.notes_opacity = new_settings.notes_opacity;
                config.overlay_settings.combat_time = new_settings.combat_time.clone();
                config.overlay_settings.combat_time_opacity = new_settings.combat_time_opacity;
                config.overlay_settings.operation_timer = new_settings.operation_timer.clone();
                config.overlay_settings.operation_timer_opacity = new_settings.operation_timer_opacity;
                config.overlay_settings.ability_queue = new_settings.ability_queue.clone();
                config.overlay_settings.ability_queue_opacity = new_settings.ability_queue_opacity;
                config.overlay_settings.enemy_frames = new_settings.enemy_frames.clone();
                config.overlay_settings.enemy_frames_opacity = new_settings.enemy_frames_opacity;
                config.overlay_settings.ability_cast = new_settings.ability_cast.clone();
                config.overlay_settings.ability_cast_opacity = new_settings.ability_cast_opacity;
                config.overlay_settings.positions = existing_positions;
                config.overlay_settings.enabled = existing_enabled;

                if let Err(err) = api::update_config(&config).await {
                    draft.set_status(format!("Failed to save: {}", err));
                } else {
                    api::refresh_overlay_settings().await;
                    // Use the merged config (draft appearance + backend positions/enabled/
                    // behavioral flags) rather than the stale draft, so we don't overwrite
                    // global settings like hide_when_not_live or hide_during_conversations.
                    settings.set(config.overlay_settings.clone());
                    draft.replace(config.overlay_settings);
                    // Save to the active profile so it stays in sync
                    if let Some(ref profile_name) = active_profile() {
                        let _ = api::save_profile(profile_name).await;
                    }
                    profile_dirty.set(false);
                    draft.set_status("Settings saved!".to_string());
                    on_settings_saved.call(());
                }
            }
        }
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
                            profile_dirty.set(api::is_profile_dirty().await);
                            toast.show(format!("{reset_label} position reset"), ToastSeverity::Normal);
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
                draft,
                settings,
                profile_names,
                active_profile,
                profile_dirty,
                metric_overlays_enabled,
                personal_enabled,
                raid_enabled,
                overlays_visible,
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
        section { class: "drawer settings-panel",
            div { class: "settings-header",
                h3 { "Overlay Settings" }
            }

            div { class: "settings-content",
                div { class: "settings-top-tabs",
                    TabButton { label: "Overlays", active: section() == SettingsSection::Overlays, onclick: move |_| section.set(SettingsSection::Overlays) }
                    TabButton { label: "Profiles", active: section() == SettingsSection::Profiles, onclick: move |_| section.set(SettingsSection::Profiles) }
                    TabButton { label: "Global", active: section() == SettingsSection::Global, onclick: move |_| section.set(SettingsSection::Global) }
                }
                if section() == SettingsSection::Overlays {
                    div { class: "setting-row overlay-select-row",
                        label { "Overlay" }
                        select {
                            value: "{tab}",
                            onchange: move |e: Event<FormData>| {
                                selected_tab.set(e.value());
                                metrics_global_open.set(false);
                            },
                            for (group, items) in overlay_groups.iter() {
                                optgroup { label: "{group}",
                                    for (label, key) in items.iter() {
                                        option { key: "{key}", value: "{key}", selected: *key == tab, "{label}" }
                                    }
                                }
                            }
                        }
                    }
                    h4 { class: "overlay-options-title", "{overlay_label(&tab)} Options" }
                }
                // Shared by every metric overlay, so it sits above whichever one is selected
                if showing_overlays && is_metric_tab {
                    MetricsGlobalSection { draft, open: metrics_global_open }
                }
                {tab_content}
            }

            div { class: "settings-footer",
                button {
                    class: if draft.has_changes() || profile_dirty() { "btn btn-save btn-unsaved" } else { "btn btn-save" },
                    disabled: !draft.has_changes() && !profile_dirty(),
                    onclick: save_to_backend,
                    if draft.has_changes() || profile_dirty() { "Save *" } else { "Save" }
                }
                if !draft.status().is_empty() {
                    span { class: "save-status", "{draft.status()}" }
                }
                if profile_dirty() {
                    div { class: "profile-unsaved-indicator",
                        i { class: "fa-solid fa-circle-exclamation" }
                        span { " Changes not saved to profile" }
                    }
                }
            }
        }
    }
}
