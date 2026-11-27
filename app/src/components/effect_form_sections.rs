//! Effect form cards: what the effect is, how it shows and announces itself,
//! and how long it is tracked

use dioxus::prelude::*;

use super::discipline_selector::DisciplineSelector;
use super::effect_editor::slug_id;
use super::effect_trigger::effect_icon_id;
use super::sound_picker::SoundPicker;
use crate::components::ability_icon::use_icon_url;
use crate::components::editor_common::FormCard;
use crate::components::form_fields::{CheckField, Field, NumberField, PickField, SubSection, TextField, TogglePill};
use crate::types::{AlertTrigger, DisplayTarget, EffectListItem, effect_alert_label};
use crate::utils::parse_hex_color;

const DISPLAY_TEXT_HELP: &str = "Text shown on the overlay. Defaults to effect name. Templates: {source}, {target}, {stack}, {ability}, {effect}. Templating does not appear in previews.";
const ALERT_TEXT_HELP: &str = "Text shown in the alert notification. Leave blank for no alert. Templates: {source}, {target}, {stack}, {ability}, {effect}. Templating does not appear in previews.";

/// Overlays drawn with personal icons (Effects A/B/C)
fn on_effects_overlay(targets: &[DisplayTarget]) -> bool {
    targets.iter().any(|t| {
        matches!(
            t,
            DisplayTarget::EffectsA | DisplayTarget::EffectsB | DisplayTarget::EffectsC
        )
    })
}

fn on_cooldowns(targets: &[DisplayTarget]) -> bool {
    targets
        .iter()
        .any(|t| matches!(t, DisplayTarget::Cooldowns | DisplayTarget::CooldownsB))
}

/// Overlays an effect can be shown on
fn effect_targets() -> impl Iterator<Item = DisplayTarget> {
    DisplayTarget::all()
        .iter()
        .copied()
        .filter(|t| !matches!(t, DisplayTarget::None))
}

// ─────────────────────────────────────────────────────────────────────────────
// Basics: what it is and who it is for
// ─────────────────────────────────────────────────────────────────────────────

/// `effect_id` is the saved id; a draft types its own
#[component]
pub(super) fn BasicsCard(mut draft: Signal<EffectListItem>, is_draft: bool, effect_id: String) -> Element {
    let d = draft.read().clone();
    rsx! {
        FormCard { icon: "fa-tag", title: "Basics",
            Field { label: "Type", help: "Tracked shows the effect on its overlays for its duration; Instant alert only fires alert text and audio on trigger",
                for (label, alert) in [("Tracked", false), ("Instant alert", true)] {
                    TogglePill {
                        label,
                        active: d.is_alert == alert,
                        on_click: move |_| draft.write().is_alert = alert,
                    }
                }
            }
            TextField { label: "Name", value: d.name.clone(), on_input: move |v| draft.write().name = v }
            // Drafts pick their ID (blank = made from the name); saved IDs are fixed
            if is_draft {
                Field { label: "Effect ID", help: "Unique ID, lowercase snake_case. Leave blank to make one from the name.",
                    input {
                        r#type: "text",
                        placeholder: "{slug_id(&d.name)}",
                        value: "{d.id}",
                        oninput: move |e| draft.write().id = e.value(),
                    }
                }
            } else {
                Field { label: "Effect ID",
                    code { class: "id-display", "{effect_id}" }
                }
            }
            Field {
                label: "Your Disciplines",
                help: "Only activate this effect when your local player is one of the selected disciplines. If empty, the effect applies to all disciplines.",
                DisciplineSelector {
                    selected: d.disciplines.clone(),
                    on_change: move |v: Vec<String>| draft.write().disciplines = v,
                }
            }
            CheckField { label: "Enabled", checked: d.enabled, on_change: move |v| draft.write().enabled = v }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Display: where and how it shows
// ─────────────────────────────────────────────────────────────────────────────

/// An instant alert shows on no overlay: only its color and icon apply
#[component]
pub(super) fn DisplayCard(mut draft: Signal<EffectListItem>) -> Element {
    let d = draft.read().clone();
    // Explicit icon_ability_id, or the trigger's id when it is blank
    let (icon_preview_url, icon_loaded) = use_icon_url(effect_icon_id(d.icon_ability_id, &d.trigger));
    let color = d.color.unwrap_or([128, 128, 128, 255]);
    let color_hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);
    let targets = d.display_targets.clone();

    rsx! {
        FormCard { icon: "fa-display", title: "Display",
            if !d.is_alert {
                PickField {
                    label: "Overlays",
                    help: "Overlays this effect will appear on. Pick several to show the same effect on more than one overlay.",
                    none_label: "No overlay",
                    noun: "overlays",
                    options: effect_targets().map(|t| t.label().to_string()).collect::<Vec<_>>(),
                    picked: targets.iter().map(|t| t.label().to_string()).collect::<Vec<_>>(),
                    on_change: move |picked: Vec<String>| {
                        draft.write().display_targets =
                            effect_targets().filter(|t| picked.iter().any(|p| p == t.label())).collect();
                    },
                }
                Field { label: "Display Text", help: DISPLAY_TEXT_HELP,
                    input {
                        r#type: "text",
                        placeholder: "{d.name}",
                        value: "{d.display_text.clone().unwrap_or_default()}",
                        oninput: move |e| {
                            let v = e.value();
                            draft.write().display_text = (!v.is_empty()).then_some(v);
                        }
                    }
                }
            }
            Field { label: "Color",
                input {
                    r#type: "color",
                    value: "{color_hex}",
                    class: "color-picker",
                    oninput: move |e| {
                        if let Some(c) = parse_hex_color(&e.value()) {
                            draft.write().color = Some(c);
                        }
                    }
                }
            }
            Field { label: "Icon ID", help: "Ability ID to use for the icon. Leave blank to auto-detect from trigger.",
                if let Some(ref url) = icon_preview_url() {
                    img { src: "{url}", class: "icon-preview", width: "24", height: "24", alt: "Icon preview" }
                } else if !icon_loaded() {
                    span { class: "icon-preview pending" }
                }
                input {
                    r#type: "text",
                    placeholder: "(auto)",
                    value: "{d.icon_ability_id.map(|id| id.to_string()).unwrap_or_default()}",
                    oninput: move |e| draft.write().icon_ability_id = e.value().parse::<u64>().ok(),
                }
                if icon_loaded() && icon_preview_url().is_none() && d.icon_ability_id.is_some() {
                    span { class: "field-unit", "(not found)" }
                }
            }

            SubSection { title: "Options",
                CheckField { label: "Show icon", checked: d.show_icon, on_change: move |v| draft.write().show_icon = v }
                // Source / target names: personal overlays only
                if !d.is_alert && (on_effects_overlay(&targets) || on_cooldowns(&targets)) {
                    CheckField {
                        label: "Display source",
                        help: "Show who applied this effect on the overlay",
                        checked: d.display_source,
                        on_change: move |v| draft.write().display_source = v,
                    }
                    CheckField {
                        label: "Display target",
                        help: "Show who this effect targets (hidden when the target is the source)",
                        checked: d.show_target,
                        on_change: move |v| draft.write().show_target = v,
                    }
                }
                if !d.is_alert && (on_effects_overlay(&targets) || targets.contains(&DisplayTarget::RaidFrames)) {
                    CheckField {
                        label: "Show stack count at 1",
                        help: "Always display a stack count for this effect, starting at 1. Non-stacking effects show \"1\"; real counts show as they climb. Raid frames normally hide counts below 2.",
                        checked: d.show_single_stack,
                        on_change: move |v| draft.write().show_single_stack = v,
                    }
                }
                if !d.is_alert && on_effects_overlay(&targets) {
                    CheckField {
                        label: "Show countdown",
                        help: "Show the remaining-time text on this effect's icon. Uncheck to hide the countdown for just this effect. The overlay's own 'Show countdown' setting must also be on.",
                        checked: d.show_countdown,
                        on_change: move |v| draft.write().show_countdown = v,
                    }
                    CheckField {
                        label: "Emphasize stacks",
                        help: "Draw this effect's stack count large and centered with the countdown in the corner. Works even when the overlay's own 'Emphasize stacked effects' setting is off.",
                        checked: d.stack_priority,
                        on_change: move |v| draft.write().stack_priority = v,
                    }
                    CheckField {
                        label: "Show when absent",
                        help: "Always show this effect's icon when playing a matching discipline: grayed out while the effect is inactive, colored while active (e.g. tank guard, sniper cover). Requires the discipline to be detected.",
                        checked: d.track_uptime,
                        on_change: move |v: bool| {
                            let mut d = draft.write();
                            d.track_uptime = v;
                            if !v {
                                d.track_uptime_after_first = false;
                            }
                        },
                    }
                    if d.track_uptime {
                        CheckField { nested: true,
                            label: "Only show after first appearance",
                            help: "Hide the grayed-out icon until this effect has been applied at least once this session. After that, it persists as usual while the effect is inactive.",
                            checked: d.track_uptime_after_first,
                            on_change: move |v| draft.write().track_uptime_after_first = v,
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Duration (tracked effects only)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub(super) fn DurationCard(mut draft: Signal<EffectListItem>) -> Element {
    let d = draft.read().clone();
    let cooldown = on_cooldowns(&d.display_targets);
    rsx! {
        FormCard { icon: "fa-clock", title: "Duration",
            NumberField {
                label: "Duration",
                help: "How long the effect displays (seconds). Set to 0 for effects that track via game events",
                value: d.duration_secs.unwrap_or(0.0).to_string(),
                unit: "sec",
                min: "0",
                on_input: move |v: String| draft.write().duration_secs = v.parse::<f32>().ok().filter(|&v| v > 0.0),
            }
            CheckField {
                label: "Duration affected by alacrity",
                help: "Adjusts the effect duration based on the player's alacrity stat",
                checked: d.is_affected_by_alacrity,
                on_change: move |v| draft.write().is_affected_by_alacrity = v,
            }
            // Cooldowns always ignore effect removed, so they don't get the choice
            if !cooldown {
                CheckField {
                    label: "Fixed duration",
                    help: "Use the duration timer instead of tracking when the game removes the effect",
                    checked: d.ignore_effect_removed,
                    on_change: move |v| draft.write().ignore_effect_removed = v,
                }
            }
            NumberField {
                label: "Show at",
                help: "Only display the effect when this many seconds remain. 0 = always visible",
                value: d.show_at_secs.to_string(),
                unit: "sec remaining",
                min: "0",
                max: d.duration_secs.unwrap_or(999.0).to_string(),
                on_input: move |v: String| if let Ok(v) = v.parse::<f32>() {
                    let mut d = draft.write();
                    d.show_at_secs = v.min(d.duration_secs.unwrap_or(f32::MAX)).max(0.0);
                },
            }
            if cooldown {
                NumberField {
                    label: "Ready State",
                    help: "Seconds before expiration to show the cooldown as ready",
                    value: d.cooldown_ready_secs.to_string(),
                    unit: "sec",
                    min: "0",
                    step: "0.1",
                    on_input: move |v: String| if let Ok(v) = v.parse::<f32>() { draft.write().cooldown_ready_secs = v.max(0.0) },
                }
            }
            SubSection { title: "Tracking",
                CheckField {
                    label: "Persist past death",
                    help: "Keep showing effect after player dies",
                    checked: d.persist_past_death,
                    on_change: move |v| draft.write().persist_past_death = v,
                }
                CheckField {
                    label: "Track outside combat",
                    help: "Continue tracking this effect between combat encounters",
                    checked: d.track_outside_combat,
                    on_change: move |v| draft.write().track_outside_combat = v,
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Alert & sound: how it announces itself
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub(super) fn AlertSoundCard(mut draft: Signal<EffectListItem>) -> Element {
    let d = draft.read().clone();
    rsx! {
        FormCard { icon: "fa-bell", title: "Alert & Sound",
            TextField {
                label: "Alert Text",
                help: ALERT_TEXT_HELP,
                value: d.alert_text.clone().unwrap_or_default(),
                placeholder: "(none)",
                on_input: move |v: String| draft.write().alert_text = (!v.is_empty()).then_some(v),
            }
            if d.is_alert {
                Field { label: "Alert On", help: "Instant alerts always fire when triggered",
                    span { class: "field-unit", "On trigger (instant)" }
                }
            } else {
                Field { label: "Alert On", help: "When to show alert text: on effect start, on effect end, or never",
                    select {
                        value: "{effect_alert_label(&d.alert_on)}",
                        onchange: move |e| {
                            let mut d = draft.write();
                            d.alert_on = match e.value().as_str() {
                                "Effect Start" => AlertTrigger::OnApply,
                                "Effect End" => AlertTrigger::OnExpire,
                                "Countdown" => AlertTrigger::Countdown,
                                _ => AlertTrigger::None,
                            };
                            // Seed a sensible default the first time Countdown is selected
                            if d.alert_on == AlertTrigger::Countdown && d.alert_countdown_secs.is_none() {
                                d.alert_countdown_secs = Some(3.0);
                            }
                        },
                        for trigger in AlertTrigger::all() {
                            option {
                                value: "{effect_alert_label(trigger)}",
                                selected: *trigger == d.alert_on,
                                "{effect_alert_label(trigger)}"
                            }
                        }
                    }
                }
                if d.alert_on == AlertTrigger::Countdown {
                    NumberField {
                        label: "Countdown Window",
                        help: "Show a live-updating alert during the last N seconds before the effect expires (0-10). Alert text shows remaining time to one decimal.",
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
                    if !d.is_alert {
                        Field { label: "Audio Offset", help: "When to play the sound relative to effect expiration",
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
