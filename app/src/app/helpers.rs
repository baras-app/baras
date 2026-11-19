//! Free helper functions used by the App shell.

use dioxus::prelude::*;
use std::collections::HashSet;

use crate::api;
use crate::components::{ToastSeverity, use_toast};
use crate::types::{OverlayStatus, OverlayType};

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

pub(super) fn apply_status(
    status: OverlayStatus,
    overlays_enabled: &mut Signal<HashSet<String>>,
    overlays_visible: &mut Signal<bool>,
    move_mode: &mut Signal<bool>,
    rearrange_mode: &mut Signal<bool>,
    auto_hidden: &mut Signal<bool>,
) {
    overlays_visible.set(status.overlays_visible);
    move_mode.set(status.move_mode);
    rearrange_mode.set(status.rearrange_mode);
    auto_hidden.set(status.auto_hidden);
    overlays_enabled.set(status.enabled);
}

pub(super) fn set_overlay_enabled(overlays_enabled: &mut Signal<HashSet<String>>, key: &str, on: bool) {
    let mut set = overlays_enabled.write();
    if on {
        set.insert(key.to_string());
    } else {
        set.remove(key);
    }
}

/// One toggle on the Overlays page
pub(super) struct OverlayDef {
    pub kind: OverlayType,
    /// Config key; also the settings tab unless `has_settings` is false
    pub key: &'static str,
    pub label: &'static str,
    pub icon: &'static str,
    pub title: &'static str,
    pub has_settings: bool,
}

const fn overlay(
    kind: OverlayType,
    key: &'static str,
    label: &'static str,
    icon: &'static str,
    title: &'static str,
) -> OverlayDef {
    OverlayDef {
        kind,
        key,
        label,
        icon,
        title,
        has_settings: true,
    }
}

/// The hand-arranged overlay categories; Metrics and PvP are built from `MetricType`
#[rustfmt::skip]
pub(super) const OVERLAY_CATEGORIES: [(&str, &[OverlayDef]); 3] = [
    ("General", &[
        overlay(OverlayType::Personal, "personal", "Personal Stats", "fa-solid fa-chart-bar", "Shows your personal combat statistics"),
        overlay(OverlayType::Raid, "raid", "Raid Frames", "fa-solid fa-users", "Displays party/raid member health bars with effect tracking"),
        overlay(OverlayType::Alerts, "alerts", "Alerts", "fa-solid fa-bell", "Shows combat alerts and notifications"),
        overlay(OverlayType::CombatTime, "combat_time", "Combat Time", "fa-solid fa-stopwatch", "Displays the current encounter combat time"),
        overlay(OverlayType::OperationTimer, "operation_timer", "Op Timer", "fa-solid fa-hourglass-half", "Displays a persistent timer for the entire operation run"),
        overlay(OverlayType::AbilityCast, "ability_cast", "Abilities Cast", "fa-solid fa-list-ol", "Lists the abilities you recently cast"),
    ]),
    ("Encounter", &[
        overlay(OverlayType::BossHealth, "boss_health", "Boss Health", "fa-solid fa-heart-pulse", "Shows boss health bars and cast timers"),
        overlay(OverlayType::Challenges, "challenges", "Challenges", "fa-solid fa-trophy", "Tracks raid challenge objectives and progress"),
        overlay(OverlayType::TimersA, "timers_a", "Timers A", "fa-solid fa-clock", "Displays encounter-specific timers and phase markers (Group A)"),
        overlay(OverlayType::TimersB, "timers_b", "Timers B", "fa-solid fa-clock", "Displays encounter-specific timers and phase markers (Group B)"),
        overlay(OverlayType::Notes, "notes", "Notes", "fa-solid fa-note-sticky", "Displays encounter notes written in the Encounter Editor"),
        OverlayDef {
            has_settings: false,
            ..overlay(OverlayType::AbilityQueue, "ability_queue", "Ability Queue", "fa-solid fa-layer-group", "Displays GCD bar, queued/ready abilities, and active ability countdowns")
        },
    ]),
    ("Effects", &[
        overlay(OverlayType::EffectsA, "effects_a", "Effects A", "fa-solid fa-bolt", "Displays tracked buffs and effects (Group A)"),
        overlay(OverlayType::EffectsB, "effects_b", "Effects B", "fa-solid fa-bolt", "Displays tracked buffs and effects (Group B)"),
        overlay(OverlayType::EffectsC, "effects_c", "Effects C", "fa-solid fa-bolt", "Displays tracked buffs and effects (Group C)"),
        overlay(OverlayType::Cooldowns, "cooldowns", "Cooldowns A", "fa-solid fa-rotate", "Tracks ability cooldowns (Group A)"),
        overlay(OverlayType::CooldownsB, "cooldowns_b", "Cooldowns B", "fa-solid fa-rotate", "Tracks ability cooldowns (Group B)"),
        overlay(OverlayType::DotTracker, "dot_tracker", "DOT Tracker", "fa-solid fa-fire", "Tracks damage-over-time effects on targets"),
    ]),
];
