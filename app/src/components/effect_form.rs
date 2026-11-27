//! Effect edit form, shown in the editor drawer: how the effect presents
//! (basics, display, alert and sound) beside its mechanics (trigger, refresh,
//! duration, modifiers)

use baras_types::preview::{OverlayPreviewRequest, PreviewAlertTiming, PreviewTarget};
use dioxus::prelude::*;

use super::effect_editor::effect_origin;
use super::effect_form_sections::{AlertSoundCard, BasicsCard, DisplayCard, DurationCard};
use super::effect_form_trigger::{RefreshCard, TriggerCard};
use super::effect_trigger::effect_icon_id;
use crate::components::editor_common::{ItemActions, PreviewButton, preview_audio, use_item_draft};
use crate::components::editor_drawer::EditorDrawer;
use crate::types::{AlertTrigger, DisplayTarget, EffectListItem};

#[component]
pub(crate) fn EffectEditForm(
    effect: EffectListItem,
    #[props(default = false)] is_draft: bool,
    #[props(default = false)] is_bundled: bool,
    #[props(default = false)] is_user_override: bool,
    on_save: EventHandler<EffectListItem>,
    on_delete: EventHandler<()>,
    on_duplicate: EventHandler<()>,
    #[props(default)] on_export: EventHandler<()>,
    on_close: EventHandler<()>,
    #[props(default)] on_dirty: EventHandler<bool>,
) -> Element {
    let (mut draft, edited) = use_item_draft(effect.clone());

    // The row's enable and audio toggles save straight to the stored effect: follow
    // them, or the next Save here would write the draft's stale values back
    let (row_enabled, row_audio) = (effect.enabled, effect.audio.enabled);
    use_effect(use_reactive!(|(row_enabled, row_audio)| {
        if draft.peek().enabled != row_enabled || draft.peek().audio.enabled != row_audio {
            let mut d = draft.write();
            d.enabled = row_enabled;
            d.audio.enabled = row_audio;
        }
    }));

    // For drafts, always enable save; for existing effects, only when changed
    let has_changes = use_memo(move || is_draft || edited());

    // Notify parent when dirty state changes
    use_effect(move || {
        on_dirty.call(has_changes());
    });

    let tracked = !draft.read().is_alert;
    let body = rsx! {
        div { class: "list-item-body effect-edit-form",
            div { class: "edit-grid effect-form-grid",
                // Left: what it is, how it shows and announces itself
                div { class: "edit-grid-left",
                    BasicsCard { draft, is_draft, effect_id: effect.id.clone() }
                    DisplayCard { draft }
                    AlertSoundCard { draft }
                }
                // Right: the mechanics, trigger first with what follows from it
                div { class: "edit-grid-right",
                    TriggerCard { draft }
                    if tracked {
                        RefreshCard { draft }
                        DurationCard { draft }
                    }
                    super::modifier_editor::ModifierListEditor {
                        modifiers: draft().modifiers.clone(),
                        on_change: move |new_mods: Vec<baras_types::EffectModifier>| draft.write().modifiers = new_mods,
                    }
                }
            }
        }
    };

    rsx! {
        EditorDrawer {
            title: if effect.name.is_empty() { "New effect".to_string() } else { effect.name.clone() },
            dirty: has_changes(),
            on_close: on_close,
            actions: rsx! {
                ItemActions {
                    has_changes: has_changes(),
                    origin: effect_origin(is_draft, is_bundled, is_user_override),
                    on_save: move |_| {
                        on_save.call(draft());
                    },
                    on_delete: move |_| on_delete.call(()),
                    on_duplicate: (!is_draft).then_some(on_duplicate),
                    menu: (!is_draft && is_user_override).then(|| rsx! {
                        button { class: "nav-menu-item", onclick: move |_| on_export.call(()),
                            i { class: "fa-solid fa-file-export" }
                            "Export"
                        }
                    }),
                    PreviewButton {
                        request: preview_request(&draft()),
                    }
                }
            },
            {body}
        }
    }
}

fn preview_request(effect: &EffectListItem) -> OverlayPreviewRequest {
    let mut targets = Vec::new();
    if !effect.is_alert {
        targets.extend(effect.display_targets.iter().filter_map(|t| match t {
            DisplayTarget::RaidFrames => Some(PreviewTarget::RaidFrames),
            DisplayTarget::EffectsA => Some(PreviewTarget::EffectsA),
            DisplayTarget::EffectsB => Some(PreviewTarget::EffectsB),
            DisplayTarget::EffectsC => Some(PreviewTarget::EffectsC),
            DisplayTarget::Cooldowns => Some(PreviewTarget::Cooldowns),
            DisplayTarget::CooldownsB => Some(PreviewTarget::CooldownsB),
            DisplayTarget::DotTracker => Some(PreviewTarget::DotTracker),
            DisplayTarget::BossHealth => Some(PreviewTarget::BossHealth),
            DisplayTarget::EffectsOverlay | DisplayTarget::None => None,
        }));
    }
    // Effects only show alert text when one is set
    if effect.alert_text.is_some() && (effect.is_alert || effect.alert_on != AlertTrigger::None) {
        targets.push(PreviewTarget::Alerts);
    }
    OverlayPreviewRequest {
        text: effect.display_text.clone().unwrap_or_else(|| effect.name.clone()),
        alert_text: effect.alert_text.clone(),
        alert_timing: match effect.alert_on {
            _ if effect.is_alert => PreviewAlertTiming::Start,
            AlertTrigger::OnExpire => PreviewAlertTiming::Expire,
            AlertTrigger::Countdown => PreviewAlertTiming::Countdown(effect.alert_countdown_secs.unwrap_or(0.0)),
            _ => PreviewAlertTiming::Start,
        },
        duration_secs: effect.duration_secs.unwrap_or(0.0),
        color: effect.color.unwrap_or([128, 128, 128, 255]),
        icon_ability_id: effect_icon_id(effect.icon_ability_id, &effect.trigger),
        show_icon: effect.show_icon,
        display_source: effect.display_source,
        show_target: effect.show_target,
        show_single_stack: effect.show_single_stack,
        show_countdown: effect.show_countdown,
        stack_priority: effect.stack_priority,
        stacks: Some(effect.default_charges.unwrap_or(1)),
        targets,
        audio: preview_audio(&effect.audio, effect.is_alert),
    }
}
