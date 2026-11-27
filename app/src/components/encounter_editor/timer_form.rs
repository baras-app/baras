//! Timer edit form, shown in the editor drawer: how the timer presents
//! (basics, display, alert and sound) beside its mechanics (trigger, timing,
//! ability queue)

use baras_types::preview::{OverlayPreviewRequest, PreviewAlertTiming, PreviewTarget};
use dioxus::prelude::*;

use crate::api;
use crate::components::editor_common::{ItemActions, Origin, PreviewButton, preview_audio, use_item_draft};
use crate::components::editor_drawer::EditorDrawer;
use crate::types::{AlertTrigger, BossTimerDefinition, BossWithPath, EncounterItem, TimerDisplayTarget};

use super::tabs::EncounterData;
use super::timer_form_queue::AbilityQueueCard;
use super::timer_form_sections::{AlertSoundCard, BasicsCard, DisplayCard, TimingCard};
use super::timer_form_trigger::TriggerCard;

#[component]
pub(super) fn TimerEditForm(
    timer: BossTimerDefinition,
    all_timers: Vec<BossTimerDefinition>,
    #[props(default)] is_builtin: bool,
    #[props(default)] is_modified: bool,
    boss_with_path: BossWithPath,
    encounter_data: EncounterData,
    on_change: EventHandler<Vec<BossTimerDefinition>>,
    on_refetch: EventHandler<()>,
    on_status: EventHandler<(String, bool)>,
    on_collapse: EventHandler<()>,
    #[props(default)] on_dirty: EventHandler<bool>,
) -> Element {
    let timer_display = timer.clone();
    let timer_for_delete = timer.clone();
    let (mut draft, has_changes) = use_item_draft(timer.clone());

    // The row's role and audio toggles save straight to the stored timer: follow
    // them, or the next Save here would write the draft's stale values back
    let (row_roles, row_audio) = (timer.roles.clone(), timer.audio.enabled);
    use_effect(use_reactive!(|(row_roles, row_audio)| {
        if draft.peek().roles != row_roles || draft.peek().audio.enabled != row_audio {
            let mut d = draft.write();
            d.roles = row_roles;
            d.audio.enabled = row_audio;
        }
    }));

    // Notify parent when dirty state changes
    use_effect(move || {
        on_dirty.call(has_changes());
    });
    // Save handler
    let handle_save = {
        let timers = all_timers.clone();
        let bwp = boss_with_path.clone();
        move |_: ()| {
            let updated = draft();
            // A countdown with no duration is stored fine and then never shows
            let invalid = if updated.name.trim().is_empty() {
                Some("A timer needs a name")
            } else if !updated.is_alert && updated.duration_secs <= 0.0 {
                Some("A countdown needs a duration above 0 seconds")
            } else {
                None
            };
            if let Some(reason) = invalid {
                on_status.call((reason.to_string(), true));
                return;
            }
            let mut current = timers.clone();
            let boss_id = bwp.boss.id.clone();
            let file_path = bwp.file_path.clone();
            let item = EncounterItem::Timer(updated.clone());
            spawn(async move {
                match api::update_encounter_item(&boss_id, &file_path, &item, None).await {
                    Ok(_) => {
                        // Only a stored save reaches the list: the form stays unsaved on failure
                        if let Some(idx) = current.iter().position(|t| t.id == updated.id) {
                            current[idx] = updated;
                            on_change.call(current);
                        }
                        on_status.call(("Saved".to_string(), false));
                        on_refetch.call(());
                    }
                    Err(e) => on_status.call((format!("Failed to save: {e}"), true)),
                }
            });
        }
    };

    // Delete/Reset handler
    let handle_delete = {
        let timer_del = timer_for_delete.clone();
        let timers = all_timers.clone();
        let bwp = boss_with_path.clone();
        let is_reset = is_builtin || is_modified; // Built-in items get "reset", not "delete"
        move |_: ()| {
            let t = timer_del.clone();
            let timers_clone = timers.clone();
            let boss_id = bwp.boss.id.clone();
            let file_path = bwp.file_path.clone();
            spawn(async move {
                match api::delete_encounter_item("timer", &t.id, &boss_id, &file_path).await {
                    Ok(_) => {
                        if is_reset {
                            // For built-in items, refetch to get the original definition back
                            on_status.call(("Reset to built-in".to_string(), false));
                            on_collapse.call(());
                            on_refetch.call(());
                        } else {
                            let filtered: Vec<_> = timers_clone.into_iter().filter(|timer| timer.id != t.id).collect();
                            on_change.call(filtered);
                            on_collapse.call(());
                            on_status.call(("Deleted".to_string(), false));
                        }
                    }
                    Err(err) => {
                        on_status.call((err, true));
                    }
                }
            });
        }
    };

    // Duplicate handler
    let handle_duplicate = {
        let timer_dup = timer_display.clone();
        let timers = all_timers.clone();
        let bwp = boss_with_path.clone();
        move |_: ()| {
            let t = timer_dup.clone();
            let ts = timers.clone();
            let boss_id = bwp.boss.id.clone();
            let file_path = bwp.file_path.clone();
            spawn(async move {
                match api::duplicate_encounter_timer(&t.id, &boss_id, &file_path).await {
                    Ok(new_timer) => {
                        let mut current = ts;
                        current.push(new_timer);
                        on_change.call(current);
                        on_status.call(("Duplicated".to_string(), false));
                    }
                    Err(e) => {
                        on_status.call((e, true));
                    }
                }
            });
        }
    };

    let d = draft.read().clone();
    let body = rsx! {
        div { class: "list-item-body",
            div { class: "edit-grid form-grid-2",
                // Left: what it is, how it shows and announces itself
                div { class: "edit-grid-left",
                    BasicsCard { draft, timer_id: timer.id.clone() }
                    DisplayCard { draft }
                    AlertSoundCard { draft }
                }
                // Right: the mechanics, trigger first with what follows from it
                div { class: "edit-grid-right",
                    TriggerCard { draft, encounter_data: encounter_data.clone() }
                    if !d.is_alert {
                        TimingCard { draft, all_timers: all_timers.clone(), had_chain: timer.chains_to.is_some() }
                        // Only when the Ability Queue overlay is a target
                        if d.display_targets.contains(&TimerDisplayTarget::AbilityQueue) {
                            AbilityQueueCard { draft, all_timers: all_timers.clone(), encounter_data: encounter_data.clone() }
                        }
                    }
                }
            }
        }
    };

    rsx! {
        EditorDrawer {
            title: timer.name.clone(),
            dirty: has_changes(),
            on_close: on_collapse,
            actions: rsx! {
                ItemActions {
                    has_changes: has_changes(),
                    origin: Origin::of(is_builtin, is_modified),
                    on_save: handle_save,
                    on_delete: handle_delete,
                    on_duplicate: handle_duplicate,
                    PreviewButton {
                        request: preview_request(&draft()),
                    }
                }
            },
            {body}
        }
    }
}

fn preview_request(timer: &BossTimerDefinition) -> OverlayPreviewRequest {
    let mut targets = Vec::new();
    if !timer.is_alert {
        targets.extend(timer.display_targets.iter().filter_map(|t| match t {
            TimerDisplayTarget::TimersA => Some(PreviewTarget::TimersA),
            TimerDisplayTarget::TimersB => Some(PreviewTarget::TimersB),
            TimerDisplayTarget::AbilityQueue | TimerDisplayTarget::None => None,
        }));
    }
    if timer.is_alert || timer.alert_on != AlertTrigger::None {
        targets.push(PreviewTarget::Alerts);
    }
    OverlayPreviewRequest {
        text: timer.display_text.clone().unwrap_or_else(|| timer.name.clone()),
        alert_text: timer.alert_text.clone(),
        alert_timing: match timer.alert_on {
            _ if timer.is_alert => PreviewAlertTiming::Start,
            AlertTrigger::OnExpire => PreviewAlertTiming::Expire,
            AlertTrigger::Countdown => PreviewAlertTiming::Countdown(timer.alert_countdown_secs.unwrap_or(0.0)),
            _ => PreviewAlertTiming::Start,
        },
        duration_secs: timer.duration_secs,
        color: timer.color,
        icon_ability_id: timer.icon_ability_id,
        show_icon: timer.icon_ability_id.is_some(),
        display_source: false,
        show_target: false,
        show_single_stack: false,
        show_countdown: true,
        stack_priority: false,
        stacks: None,
        targets,
        audio: preview_audio(&timer.audio, timer.is_alert),
    }
}
