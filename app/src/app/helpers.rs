//! Free helper functions used by the App shell.

use dioxus::prelude::*;
use std::collections::HashMap;

use crate::api;
use crate::components::{ToastSeverity, use_toast};
use crate::types::{MetricType, OverlayStatus};

/// WASM cannot cfg on the host OS, so ask the webview instead.
pub(super) fn is_macos() -> bool {
    web_sys::window()
        .map(|w| w.navigator().user_agent().unwrap_or_default().contains("Macintosh"))
        .unwrap_or(false)
}

pub(super) fn is_linux() -> bool {
    web_sys::window()
        .map(|w| w.navigator().user_agent().unwrap_or_default().contains("Linux"))
        .unwrap_or(false)
}

// ─────────────────────────────────────────────────────────────────────────────
// Parsely Guild Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Apply the theme now and persist it (header toggle and Settings checkbox)
pub(super) fn set_light_theme(mut light_theme: Signal<bool>, on: bool) {
    light_theme.set(on);
    crate::utils::apply_theme(on);
    let mut toast = use_toast();
    spawn(async move {
        if let Some(mut cfg) = api::get_config().await {
            cfg.light_theme = on;
            if let Err(err) = api::update_config(&cfg).await {
                toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
            }
        }
    });
}

/// Persist guild fields (`guilds` + `selected_guild`) to the config without
/// touching unrelated parsely settings. Used by add/remove handlers so the
/// chip list stays in sync with what the upload modal sees.
pub(super) fn save_parsely_guild_fields(
    guilds: Vec<String>,
    selected: Option<String>,
    mut save_status: Signal<String>,
) {
    let mut toast = use_toast();
    spawn(async move {
        if let Some(mut cfg) = api::get_config().await {
            cfg.parsely.guilds = guilds;
            cfg.parsely.selected_guild = selected;
            match api::update_config(&cfg).await {
                Ok(_) => save_status.set("Saved!".to_string()),
                Err(err) => toast.show(format!("Failed to save guilds: {}", err), ToastSeverity::Normal),
            }
        }
    });
}

/// Add the trimmed guild input to the list and persist guild fields to config.
/// No-op if the input is empty or already present.
pub(super) fn add_parsely_guild(
    mut input: Signal<String>,
    mut guilds: Signal<Vec<String>>,
    mut selected: Signal<Option<String>>,
    save_status: Signal<String>,
) {
    let trimmed = input.read().trim().to_string();
    if trimmed.is_empty() || guilds.read().iter().any(|g| g == &trimmed) {
        return;
    }
    guilds.write().push(trimmed.clone());
    if selected.read().is_none() {
        selected.set(Some(trimmed));
    }
    input.set(String::new());

    save_parsely_guild_fields(guilds.read().clone(), selected.read().clone(), save_status);
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_status(
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
    effects_c_enabled: &mut Signal<bool>,
    cooldowns_enabled: &mut Signal<bool>,
    cooldowns_b_enabled: &mut Signal<bool>,
    dot_tracker_enabled: &mut Signal<bool>,
    notes_enabled: &mut Signal<bool>,
    combat_time_enabled: &mut Signal<bool>,
    operation_timer_enabled: &mut Signal<bool>,
    ability_queue_enabled: &mut Signal<bool>,
    enemy_frames_enabled: &mut Signal<bool>,
    ability_cast_enabled: &mut Signal<bool>,
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
    effects_c_enabled.set(status.effects_c_enabled);
    cooldowns_enabled.set(status.cooldowns_enabled);
    cooldowns_b_enabled.set(status.cooldowns_b_enabled);
    dot_tracker_enabled.set(status.dot_tracker_enabled);
    notes_enabled.set(status.notes_enabled);
    combat_time_enabled.set(status.combat_time_enabled);
    operation_timer_enabled.set(status.operation_timer_enabled);
    ability_queue_enabled.set(status.ability_queue_enabled);
    enemy_frames_enabled.set(status.enemy_frames_enabled);
    ability_cast_enabled.set(status.ability_cast_enabled);
    overlays_visible.set(status.overlays_visible);
    move_mode.set(status.move_mode);
    rearrange_mode.set(status.rearrange_mode);
    auto_hidden.set(status.auto_hidden);
}
