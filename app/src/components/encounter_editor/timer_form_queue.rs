//! Timer form card for the Ability Queue overlay

use dioxus::prelude::*;

use crate::components::SoundPicker;
use crate::components::editor_common::FormCard;
use crate::components::form_fields::{CheckField, Field, NumberField, SubSection};
use crate::types::{BossTimerDefinition, Condition, Trigger};

use super::conditions::ConditionsEditor;
use super::optional_trigger::OptionalTrigger;
use super::tabs::EncounterData;

#[component]
pub(super) fn AbilityQueueCard(
    mut draft: Signal<BossTimerDefinition>,
    all_timers: Vec<BossTimerDefinition>,
    encounter_data: EncounterData,
) -> Element {
    let d = draft();
    let next_audio = d.queue_next_audio.clone().unwrap_or_default();
    let next_audio_on = d.queue_next_audio.as_ref().is_some_and(|a| a.enabled);

    rsx! {
        FormCard { icon: "fa-list-ol", title: "Ability Queue",
            CheckField {
                label: "Hold as ready",
                help: "When enabled, the timer stays visible as 'READY' after expiring instead of being removed.",
                checked: d.queue_on_expire,
                on_change: move |v| draft.write().queue_on_expire = v,
            }
            CheckField {
                label: "Countdown mode",
                help: "Render this timer's bar as a trickling-down bar (full → empty) instead of the default filling-up progress bar used by other entries.",
                checked: d.queue_countdown_bar,
                on_change: move |v| draft.write().queue_countdown_bar = v,
            }
            CheckField {
                label: "Don't show as next",
                help: "When enabled, this timer is never highlighted as the 'next cast' — use for display-only entries (incoming debuffs, lockout/mechanic windows) that aren't castable abilities.",
                checked: d.queue_hide_from_next,
                on_change: move |v| draft.write().queue_hide_from_next = v,
            }
            NumberField {
                label: "GCD",
                help: "Creates a GCD countdown bar when this timer fires. Leave empty for no GCD bar.",
                value: d.gcd_secs.map(|v| v.to_string()).unwrap_or_default(),
                unit: "sec",
                min: "0",
                max: "10",
                step: "0.1",
                placeholder: "none",
                on_input: move |v: String| draft.write().gcd_secs = v.parse::<f32>().ok().filter(|&v| v > 0.0),
            }
            NumberField {
                label: "Priority",
                help: "Sort order for ability queue entries (0–255, higher = shown first). Applies whether the timer holds as ready or expires normally.",
                value: d.queue_priority.to_string(),
                min: "0",
                max: "255",
                step: "1",
                on_input: move |v: String| draft.write().queue_priority = v.parse::<u8>().unwrap_or(0),
            }
            SubSection { title: "Blocking",
                Field {
                    label: "Blocked By",
                    help: "Timers from this encounter that prevent this entry from appearing as 'next cast' while they're active. OR semantics — any one active blocker blocks the entry.",
                    top: true,
                    div { class: "flex-col gap-xs",
                        // Stored as definition ids; chips show the name (the raw id for a stale reference)
                        if !d.queue_blocking_timers.is_empty() {
                            div { class: "flex flex-wrap gap-xs",
                                for (idx, blocker_id) in d.queue_blocking_timers.iter().cloned().enumerate() {
                                    span { class: "chip",
                                        {all_timers.iter().find(|t| t.id == blocker_id).map(|t| t.name.clone()).unwrap_or(blocker_id)}
                                        button {
                                            title: "Remove",
                                            class: "chip-remove",
                                            onclick: move |_| { draft.write().queue_blocking_timers.remove(idx); },
                                            "×"
                                        }
                                    }
                                }
                            }
                        }
                        select {
                            onchange: move |e| {
                                let v = e.value();
                                let mut d = draft.write();
                                if !v.is_empty() && !d.queue_blocking_timers.contains(&v) {
                                    d.queue_blocking_timers.push(v);
                                }
                            },
                            option { value: "", "(add blocker...)" }
                            for t in all_timers.iter().filter(|t| t.id != d.id) {
                                {
                                    let already = d.queue_blocking_timers.contains(&t.id);
                                    rsx! {
                                        option { value: "{t.id}", disabled: already, "{t.name}" if already { " ✓" } }
                                    }
                                }
                            }
                        }
                    }
                }
                Field {
                    label: "Blocked By Condition",
                    help: "State condition that blocks this entry from appearing as 'next cast' when satisfied. OR'd with the Blocked By timer list — either signal blocks the entry. Multiple conditions are AND'd implicitly (use 'Any Of' to OR).",
                    top: true,
                    ConditionsEditor {
                        conditions: match d.queue_blocking_condition.clone() {
                            None => Vec::new(),
                            Some(Condition::AllOf { conditions }) => conditions,
                            Some(c) => vec![c],
                        },
                        encounter_data: encounter_data.clone(),
                        on_change: move |v: Vec<Condition>| {
                            draft.write().queue_blocking_condition = match v.len() {
                                0 => None,
                                1 => v.into_iter().next(),
                                _ => Some(Condition::AllOf { conditions: v }),
                            };
                        },
                    }
                }
            }
            SubSection {
                title: "Next-Cast Audio",
                help: "Plays once when this timer becomes the unique highest-priority 'next cast'. Suppressed during ties and rate-limited to avoid jitter near the GCD edge.",
                CheckField {
                    label: "Enable",
                    checked: next_audio_on,
                    on_change: move |on: bool| {
                        let mut d = draft.write();
                        let mut a = d.queue_next_audio.clone().unwrap_or_default();
                        a.enabled = on;
                        // Drop entirely when nothing else is set, so the saved TOML stays clean
                        d.queue_next_audio = (on || a.file.is_some()).then_some(a);
                    },
                }
                if next_audio_on {
                    SoundPicker {
                        value: next_audio.file.clone(),
                        on_change: move |v| {
                            let mut d = draft.write();
                            let mut a = d.queue_next_audio.clone().unwrap_or_default();
                            a.enabled = true;
                            a.file = v;
                            d.queue_next_audio = Some(a);
                        },
                    }
                    NumberField {
                        label: "GCD Remaining",
                        help: "Defer the cue until this many seconds remain on the GCD. Leave blank (or 0) to play immediately when the timer becomes the next cast.",
                        value: next_audio.at_gcd_remaining.map(|v| v.to_string()).unwrap_or_default(),
                        unit: "sec",
                        min: "0",
                        step: "0.1",
                        placeholder: "immediate",
                        on_input: move |v: String| {
                            let mut d = draft.write();
                            let mut a = d.queue_next_audio.clone().unwrap_or_default();
                            a.enabled = true;
                            a.at_gcd_remaining = v.trim().parse::<f32>().ok().filter(|v| *v > 0.0);
                            d.queue_next_audio = Some(a);
                        },
                    }
                }
            }
            if d.queue_on_expire {
                OptionalTrigger {
                    title: "Clear On",
                    help: "Removes the ready entry when this trigger fires (e.g. ability cast).",
                    noun: "Clear",
                    empty: "Default: combat end",
                    initial: Trigger::CombatStart,
                    trigger: d.queue_remove_trigger.clone(),
                    encounter_data: encounter_data.clone(),
                    on_change: move |t| draft.write().queue_remove_trigger = t,
                }
            }
        }
    }
}
