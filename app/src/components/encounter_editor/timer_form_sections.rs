//! Timer form cards: what the timer is, how it shows and announces itself,
//! and how long it runs

use dioxus::prelude::*;

use crate::components::SoundPicker;
use crate::components::ability_icon::use_icon_url;
use crate::components::editor_common::FormCard;
use crate::components::form_fields::{
    CheckField, Deprecated, Field, NoteField, NumberField, PickField, SubSection, TextField, TogglePill,
};
use crate::types::{AlertTrigger, BossTimerDefinition, TimerDisplayTarget, timer_alert_label};
use crate::utils::parse_hex_color;

const DISPLAY_TEXT_HELP: &str = "Text shown on the overlay timer bar. Defaults to timer name. Templates: {source}, {target}. Templating does not appear in previews.";
const ALERT_TEXT_HELP: &str = "Text shown in the alert notification. Defaults to timer name. Templates: {source}, {target}. Templating does not appear in previews.";

/// Overlays a timer can be shown on
fn timer_targets() -> impl Iterator<Item = TimerDisplayTarget> {
    TimerDisplayTarget::all()
        .iter()
        .copied()
        .filter(|t| !matches!(t, TimerDisplayTarget::None))
}

// ─────────────────────────────────────────────────────────────────────────────
// Basics: what it is and when it applies
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub(super) fn BasicsCard(mut draft: Signal<BossTimerDefinition>, timer_id: String) -> Element {
    let d = draft.read().clone();
    rsx! {
        FormCard { icon: "fa-tag", title: "Basics",
            Field { label: "Type", help: "Countdown shows a timer bar; Instant alert only shows a brief notification",
                for (label, alert) in [("Countdown", false), ("Instant alert", true)] {
                    TogglePill {
                        label,
                        active: d.is_alert == alert,
                        on_click: move |_| draft.write().is_alert = alert,
                    }
                }
            }
            TextField { label: "Name", value: d.name.clone(), on_input: move |v| draft.write().name = v }
            Field { label: "Timer ID",
                code { class: "id-display", "{timer_id}" }
            }
            Field { label: "Difficulties",
                for diff in ["story", "veteran", "master"] {
                    TogglePill {
                        label: diff,
                        active: d.difficulties.iter().any(|x| x == diff),
                        on_click: move |_| {
                            let mut d = draft.write();
                            if d.difficulties.iter().any(|x| x == diff) {
                                d.difficulties.retain(|x| x != diff);
                            } else {
                                d.difficulties.push(diff.to_string());
                            }
                        },
                    }
                }
            }
            Field { label: "Group Size",
                for (label, size) in [("All", None), ("8", Some(8u8)), ("16", Some(16u8))] {
                    TogglePill {
                        label,
                        active: d.group_size == size,
                        on_click: move |_| draft.write().group_size = size,
                    }
                }
            }
            CheckField { label: "Enabled", checked: d.enabled, on_change: move |v| draft.write().enabled = v }
            if !d.enabled {
                div { class: "text-xs text-warning mt-xs",
                    i { class: "fa-solid fa-triangle-exclamation" }
                    " Disabling a timer may break phases, counters, or other timers that depend on it."
                }
            }
            NoteField { value: d.note.clone(), on_input: move |v| draft.write().note = v }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Display: where and how it shows
// ─────────────────────────────────────────────────────────────────────────────

/// An instant alert shows on no overlay: only its color and icon apply
#[component]
pub(super) fn DisplayCard(mut draft: Signal<BossTimerDefinition>) -> Element {
    let d = draft.read().clone();
    let (icon_preview_url, icon_loaded) = use_icon_url(d.icon_ability_id);
    let color_hex = format!("#{:02x}{:02x}{:02x}", d.color[0], d.color[1], d.color[2]);

    rsx! {
        FormCard { icon: "fa-display", title: "Display",
            if !d.is_alert {
                PickField {
                    label: "Overlays",
                    help: "Overlays this timer will appear on. Pick several to show the same timer on more than one overlay.",
                    none_label: "No overlay",
                    noun: "overlays",
                    options: timer_targets().map(|t| t.label().to_string()).collect::<Vec<_>>(),
                    picked: d.display_targets.iter().map(|t| t.label().to_string()).collect::<Vec<_>>(),
                    on_change: move |picked: Vec<String>| {
                        let mut d = draft.write();
                        d.display_targets = timer_targets().filter(|t| picked.iter().any(|p| p == t.label())).collect();
                        // Clear queue-only fields when AbilityQueue is removed
                        if !d.display_targets.contains(&TimerDisplayTarget::AbilityQueue) {
                            d.queue_on_expire = false;
                            d.gcd_secs = None;
                            d.queue_priority = 0;
                            d.queue_remove_trigger = None;
                            d.queue_countdown_bar = false;
                            d.queue_hide_from_next = false;
                        }
                    },
                }
                TextField {
                    label: "Display Text",
                    help: DISPLAY_TEXT_HELP,
                    value: d.display_text.clone().unwrap_or_default(),
                    placeholder: "(defaults to name)",
                    on_input: move |v: String| draft.write().display_text = (!v.is_empty()).then_some(v),
                }
            }
            Field { label: "Color",
                input {
                    class: "color-picker",
                    r#type: "color",
                    value: "{color_hex}",
                    oninput: move |e| {
                        if let Some(color) = parse_hex_color(&e.value()) {
                            draft.write().color = color;
                        }
                    }
                }
            }
            Field { label: "Icon ID", help: "Ability ID to use for the icon on the timer bar or alert. Leave blank for no icon.",
                if let Some(ref url) = icon_preview_url() {
                    img { src: "{url}", class: "icon-preview", width: "24", height: "24", alt: "Icon preview" }
                } else if !icon_loaded() {
                    span { class: "icon-preview pending" }
                }
                input {
                    r#type: "text",
                    placeholder: "(none)",
                    value: "{d.icon_ability_id.map(|id| id.to_string()).unwrap_or_default()}",
                    oninput: move |e| draft.write().icon_ability_id = e.value().parse::<u64>().ok(),
                }
                if icon_loaded() && icon_preview_url().is_none() && d.icon_ability_id.is_some() {
                    span { class: "field-unit", "(not found)" }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Alert & sound: how it announces itself
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub(super) fn AlertSoundCard(mut draft: Signal<BossTimerDefinition>) -> Element {
    let d = draft.read().clone();
    rsx! {
        FormCard { icon: "fa-bell", title: "Alert & Sound",
            TextField {
                label: "Alert Text",
                help: ALERT_TEXT_HELP,
                value: d.alert_text.clone().unwrap_or_default(),
                placeholder: "(timer name)",
                on_input: move |v: String| draft.write().alert_text = (!v.is_empty()).then_some(v),
            }
            if d.is_alert {
                // Instant alerts always fire on trigger: no choice needed
                Field { label: "Alert On", help: "Instant alerts always fire when triggered",
                    span { class: "field-unit", "On trigger (instant)" }
                }
            } else {
                Field { label: "Alert On", help: "When to show the alert text: on timer start, on timer end, or never",
                    select {
                        value: "{timer_alert_label(&d.alert_on)}",
                        onchange: move |e| {
                            let mut d = draft.write();
                            d.alert_on = match e.value().as_str() {
                                "Timer Start" => AlertTrigger::OnApply,
                                "Timer End" => AlertTrigger::OnExpire,
                                "Countdown" => AlertTrigger::Countdown,
                                _ => AlertTrigger::None,
                            };
                            // A sensible default window the first time Countdown is picked
                            if d.alert_on == AlertTrigger::Countdown && d.alert_countdown_secs.is_none() {
                                d.alert_countdown_secs = Some(3.0);
                            }
                        },
                        for trigger in AlertTrigger::all() {
                            option {
                                value: "{timer_alert_label(trigger)}",
                                selected: *trigger == d.alert_on,
                                "{timer_alert_label(trigger)}"
                            }
                        }
                    }
                }
                if d.alert_on == AlertTrigger::Countdown {
                    NumberField {
                        label: "Countdown Window",
                        help: "Show a live-updating alert during the last N seconds of the timer (0-10). Alert text shows remaining time to one decimal.",
                        value: d.alert_countdown_secs.unwrap_or(3.0).to_string(),
                        unit: "sec",
                        min: "0",
                        max: "10",
                        step: "0.5",
                        on_input: move |v: String| if let Ok(v) = v.parse::<f32>() {
                            draft.write().alert_countdown_secs = Some(v.clamp(0.0, 10.0));
                        },
                    }
                }
            }

            SubSection { title: "Sound",
                CheckField { label: "Play a sound", checked: d.audio.enabled, on_change: move |v| draft.write().audio.enabled = v }
                if d.audio.enabled {
                    SoundPicker { value: d.audio.file.clone(), on_change: move |v| draft.write().audio.file = v }
                    // Audio timing options (only for countdown timers)
                    if !d.is_alert {
                        Field { label: "Audio Offset", help: "When to play the sound relative to timer expiration",
                            select {
                                value: "{d.audio.offset}",
                                onchange: move |e| if let Ok(v) = e.value().parse::<u8>() { draft.write().audio.offset = v },
                                option { value: "0", "On expiration" }
                                for s in 1..=10u8 { option { value: "{s}", "{s}s before" } }
                            }
                        }
                        Field { label: "Countdown", help: "Voice countdown starting at the specified seconds remaining",
                            select {
                                class: "field-short",
                                value: "{d.audio.countdown_start}",
                                onchange: move |e| if let Ok(v) = e.value().parse::<u8>() { draft.write().audio.countdown_start = v },
                                option { value: "0", "Off" }
                                for s in 1..=10u8 { option { value: "{s}", "{s}s" } }
                            }
                            select {
                                value: "{d.audio.countdown_voice.clone().unwrap_or_else(|| \"Amy\".to_string())}",
                                onchange: move |e| {
                                    let v = e.value();
                                    draft.write().audio.countdown_voice = (v != "Amy").then_some(v);
                                },
                                for voice in ["Amy", "Jim", "Yolo", "Nerevar"] { option { value: voice, "{voice}" } }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Timing (countdowns only)
// ─────────────────────────────────────────────────────────────────────────────

/// `had_chain`: the stored timer chains, so the deprecated option stays visible
#[component]
pub(super) fn TimingCard(
    mut draft: Signal<BossTimerDefinition>,
    all_timers: Vec<BossTimerDefinition>,
    had_chain: bool,
) -> Element {
    let d = draft.read().clone();
    let chains_to = d.chains_to.clone().unwrap_or_default();
    rsx! {
        FormCard { icon: "fa-clock", title: "Timing",
            NumberField {
                label: "Duration",
                help: "How long the countdown timer runs in seconds",
                value: d.duration_secs.to_string(),
                unit: "sec",
                min: "0",
                on_input: move |v: String| if let Ok(v) = v.parse::<f32>() { draft.write().duration_secs = v },
            }
            CheckField {
                label: "Can refresh",
                help: "Resets the timer duration if triggered again while already running",
                checked: d.can_be_refreshed,
                on_change: move |v| draft.write().can_be_refreshed = v,
            }
            NumberField {
                label: "Show at",
                help: "Only show the timer when this many seconds remain. 0 = always visible",
                value: d.show_at_secs.to_string(),
                unit: "sec remaining",
                min: "0",
                max: d.duration_secs.to_string(),
                on_input: move |v: String| if let Ok(v) = v.parse::<f32>() {
                    let mut d = draft.write();
                    d.show_at_secs = v.min(d.duration_secs).max(0.0);
                },
            }
            NumberField {
                label: "Repeats",
                help: "Number of times this timer auto-restarts after expiring. 0 = no repeat",
                value: d.repeats.to_string(),
                min: "0",
                max: "255",
                step: "1",
                on_input: move |v: String| if let Ok(v) = v.parse::<u8>() { draft.write().repeats = v },
            }
            // Deprecated: kept visible only for timers that already chain
            if had_chain || d.chains_to.is_some() {
                Field { label: "Chains To", help: "Starts another timer when this one expires",
                    select {
                        onchange: move |e| {
                            let v = e.value();
                            draft.write().chains_to = (!v.is_empty()).then_some(v);
                        },
                        option { value: "", selected: chains_to.is_empty(), "(none)" }
                        for t in all_timers.iter().filter(|t| t.id != d.id) {
                            option { value: "{t.id}", selected: t.id == chains_to, "{t.id}" }
                        }
                    }
                    Deprecated { note: "Deprecated: start the other timer with a Timer Expires trigger instead" }
                }
            }
        }
    }
}
