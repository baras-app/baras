//! Timer editing tab
//!
//! Full CRUD for boss timers with all fields exposed.
//! Uses BossTimerDefinition DSL type directly.

use dioxus::prelude::*;

use crate::api;
use crate::types::{
    AudioConfig, BossWithPath, BossTimerDefinition, EncounterItem, EntityFilter, Trigger,
};

use super::conditions::CounterConditionEditor;
use super::tabs::EncounterData;
use super::triggers::ComposableTriggerEditor;
use super::InlineNameCreator;

/// Check if a trigger type supports source filtering
/// Only event-based triggers with source actors make sense to filter
fn trigger_supports_source(trigger: &Trigger) -> bool {
    match trigger {
        Trigger::AbilityCast { .. }
        | Trigger::EffectApplied { .. }
        | Trigger::EffectRemoved { .. } => true,
        // For composite triggers, check if any sub-condition supports source
        Trigger::AnyOf { conditions } => conditions.iter().any(trigger_supports_source),
        _ => false,
    }
}

/// Check if a trigger type supports target filtering
/// Only event-based triggers with target actors make sense to filter
fn trigger_supports_target(trigger: &Trigger) -> bool {
    match trigger {
        Trigger::EffectApplied { .. }
        | Trigger::EffectRemoved { .. }
        | Trigger::TargetSet { .. } => true,
        // For composite triggers, check if any sub-condition supports target
        Trigger::AnyOf { conditions } => conditions.iter().any(trigger_supports_target),
        _ => false,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Timers Tab
// ─────────────────────────────────────────────────────────────────────────────

/// Create a default timer definition with sensible defaults
fn default_timer(name: String) -> BossTimerDefinition {
    BossTimerDefinition {
        id: String::new(), // Backend generates from name
        name,
        display_text: None,
        trigger: Trigger::CombatStart,
        duration_secs: 30.0,
        is_alert: false,
        alert_text: None,
        color: [255, 128, 0, 255], // Orange
        phases: vec![],
        counter_condition: None,
        difficulties: vec!["story".to_string(), "veteran".to_string(), "master".to_string()],
        enabled: true,
        can_be_refreshed: false,
        repeats: 0,
        chains_to: None,
        cancel_trigger: None,
        alert_at_secs: None,
        show_on_raid_frames: false,
        show_at_secs: 0.0,
        audio: AudioConfig::default(),
    }
}

#[component]
pub fn TimersTab(
    boss_with_path: BossWithPath,
    encounter_data: EncounterData,
    on_change: EventHandler<Vec<BossTimerDefinition>>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let mut expanded_timer = use_signal(|| None::<String>);

    // Extract timers from BossWithPath
    let timers = boss_with_path.boss.timers.clone();

    rsx! {
        div { class: "timers-tab",
            // Header
            div { class: "flex items-center justify-between mb-sm",
                span { class: "text-sm text-secondary", "{timers.len()} timers" }
                {
                    let bwp = boss_with_path.clone();
                    let timers_for_create = timers.clone();
                    rsx! {
                        InlineNameCreator {
                            button_label: "+ New Timer",
                            placeholder: "Timer name...",
                            on_create: move |name: String| {
                                let timers_clone = timers_for_create.clone();
                                let boss_id = bwp.boss.id.clone();
                                let file_path = bwp.file_path.clone();
                                let timer = default_timer(name);
                                let item = EncounterItem::Timer(timer);
                                spawn(async move {
                                    match api::create_encounter_item(&boss_id, &file_path, &item).await {
                                        Ok(EncounterItem::Timer(created)) => {
                                            let created_id = created.id.clone();
                                            let mut current = timers_clone;
                                            current.push(created);
                                            on_change.call(current);
                                            expanded_timer.set(Some(created_id));
                                            on_status.call(("Created".to_string(), false));
                                        }
                                        Ok(_) => on_status.call(("Unexpected response type".to_string(), true)),
                                        Err(e) => on_status.call((e, true)),
                                    }
                                });
                            }
                        }
                    }
                }
            }

            // Timer list
            if timers.is_empty() {
                div { class: "empty-state text-sm", "No timers defined" }
            } else {
                for timer in timers.clone() {
                    {
                        let timer_key = timer.id.clone();
                        let is_expanded = expanded_timer() == Some(timer_key.clone());
                        let timers_for_row = timers.clone();

                        rsx! {
                            TimerRow {
                                key: "{timer_key}",
                                timer: timer.clone(),
                                all_timers: timers_for_row,
                                boss_with_path: boss_with_path.clone(),
                                encounter_data: encounter_data.clone(),
                                expanded: is_expanded,
                                on_toggle: move |_| {
                                    expanded_timer.set(if is_expanded { None } else { Some(timer_key.clone()) });
                                },
                                on_change: on_change,
                                on_status: on_status,
                                on_collapse: move |_| expanded_timer.set(None),
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Timer Row
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn TimerRow(
    timer: BossTimerDefinition,
    all_timers: Vec<BossTimerDefinition>,
    boss_with_path: BossWithPath,
    encounter_data: EncounterData,
    expanded: bool,
    on_toggle: EventHandler<()>,
    on_change: EventHandler<Vec<BossTimerDefinition>>,
    on_status: EventHandler<(String, bool)>,
    on_collapse: EventHandler<()>,
) -> Element {
    let color_hex = format!("#{:02x}{:02x}{:02x}", timer.color[0], timer.color[1], timer.color[2]);
    let timer_for_enable = timer.clone();
    let timer_for_audio = timer.clone();
    let timers_for_enable = all_timers.clone();
    let timers_for_audio = all_timers.clone();
    let bwp_for_enable = boss_with_path.clone();
    let bwp_for_audio = boss_with_path.clone();

    rsx! {
        div { class: "list-item",
            // Header row
            div {
                class: "list-item-header",
                onclick: move |_| on_toggle.call(()),

                // Left side - expandable content
                div { class: "flex items-center gap-xs flex-1 min-w-0",
                    span { class: "list-item-expand", if expanded { "▼" } else { "▶" } }
                    span {
                        class: "color-swatch",
                        style: "background: {color_hex};"
                    }
                    span { class: "font-medium text-primary truncate", "{timer.name}" }
                    span { class: "text-xs text-mono text-muted truncate", "{timer.id}" }
                    span { class: "tag", "{timer.trigger.label()}" }
                    span { class: "text-sm text-secondary", "{timer.duration_secs:.1}s" }
                }

                // Right side - fixed toggle buttons
                div { class: "flex items-center gap-xs", style: "flex-shrink: 0;",
                    // Enabled toggle (clickable without expanding)
                    span {
                        class: "row-toggle",
                        title: if timer.enabled { "Disable timer" } else { "Enable timer" },
                        onclick: move |e| {
                            e.stop_propagation();
                            let mut updated = timer_for_enable.clone();
                            updated.enabled = !updated.enabled;
                            let mut current = timers_for_enable.clone();
                            if let Some(idx) = current.iter().position(|t| t.id == updated.id) {
                                current[idx] = updated.clone();
                                on_change.call(current);
                            }
                            let boss_id = bwp_for_enable.boss.id.clone();
                            let file_path = bwp_for_enable.file_path.clone();
                            let item = EncounterItem::Timer(updated);
                            spawn(async move {
                                let _ = api::update_encounter_item(&boss_id, &file_path, &item, None).await;
                            });
                        },
                        span {
                            class: if timer.enabled { "text-success" } else { "text-muted" },
                            if timer.enabled { "✓" } else { "○" }
                        }
                    }

                    // Audio toggle (clickable without expanding)
                    span {
                        class: "row-toggle",
                        title: if timer.audio.enabled { "Disable audio" } else { "Enable audio" },
                        onclick: move |e| {
                            e.stop_propagation();
                            let mut updated = timer_for_audio.clone();
                            updated.audio.enabled = !updated.audio.enabled;
                            let mut current = timers_for_audio.clone();
                            if let Some(idx) = current.iter().position(|t| t.id == updated.id) {
                                current[idx] = updated.clone();
                                on_change.call(current);
                            }
                            let boss_id = bwp_for_audio.boss.id.clone();
                            let file_path = bwp_for_audio.file_path.clone();
                            let item = EncounterItem::Timer(updated);
                            spawn(async move {
                                let _ = api::update_encounter_item(&boss_id, &file_path, &item, None).await;
                            });
                        },
                        span {
                            class: if timer.audio.enabled { "text-primary" } else { "text-muted" },
                            if timer.audio.enabled { "🔊" } else { "🔇" }
                        }
                    }
                }
            }

            // Edit form
            if expanded {
                TimerEditForm {
                    timer: timer.clone(),
                    all_timers: all_timers,
                    boss_with_path: boss_with_path,
                    encounter_data: encounter_data,
                    on_change: on_change,
                    on_status: on_status,
                    on_collapse: on_collapse,
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Timer Edit Form (Full)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn TimerEditForm(
    timer: BossTimerDefinition,
    all_timers: Vec<BossTimerDefinition>,
    boss_with_path: BossWithPath,
    encounter_data: EncounterData,
    on_change: EventHandler<Vec<BossTimerDefinition>>,
    on_status: EventHandler<(String, bool)>,
    on_collapse: EventHandler<()>,
) -> Element {
    let timer_original = timer.clone();
    let timer_display = timer.clone();
    let mut draft = use_signal(|| timer.clone());
    let mut confirm_delete = use_signal(|| false);

    let has_changes = use_memo(move || draft() != timer_original);
    let color_hex = format!("#{:02x}{:02x}{:02x}", draft().color[0], draft().color[1], draft().color[2]);

    // Save handler
    let handle_save = {
        let timers = all_timers.clone();
        let bwp = boss_with_path.clone();
        move |_| {
            let updated = draft();
            let mut current = timers.clone();
            if let Some(idx) = current.iter().position(|t| t.id == updated.id) {
                current[idx] = updated.clone();
                on_change.call(current);
            }
            let boss_id = bwp.boss.id.clone();
            let file_path = bwp.file_path.clone();
            let item = EncounterItem::Timer(updated);
            spawn(async move {
                match api::update_encounter_item(&boss_id, &file_path, &item, None).await {
                    Ok(_) => on_status.call(("Saved".to_string(), false)),
                    Err(_) => on_status.call(("Failed to save".to_string(), true)),
                }
            });
        }
    };

    // Delete handler
    let handle_delete = {
        let timer_del = timer.clone();
        let timers = all_timers.clone();
        let bwp = boss_with_path.clone();
        move |_| {
            let t = timer_del.clone();
            let timers_clone = timers.clone();
            let boss_id = bwp.boss.id.clone();
            let file_path = bwp.file_path.clone();
            spawn(async move {
                match api::delete_encounter_item("timer", &t.id, &boss_id, &file_path).await {
                    Ok(_) => {
                        let filtered: Vec<_> = timers_clone.into_iter()
                            .filter(|timer| timer.id != t.id)
                            .collect();
                        on_change.call(filtered);
                        on_collapse.call(());
                        on_status.call(("Deleted".to_string(), false));
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
        let timer_dup = timer.clone();
        let timers = all_timers.clone();
        let bwp = boss_with_path.clone();
        move |_| {
            let t = timer_dup.clone();
            let ts = timers.clone();
            let boss_id = bwp.boss.id.clone();
            let file_path = bwp.file_path.clone();
            spawn(async move {
                if let Some(new_timer) = api::duplicate_encounter_timer(&t.id, &boss_id, &file_path).await {
                    let mut current = ts;
                    current.push(new_timer);
                    on_change.call(current);
                    on_status.call(("Duplicated".to_string(), false));
                } else {
                    on_status.call(("Failed to duplicate".to_string(), true));
                }
            });
        }
    };

    // Get other timer IDs for chains_to dropdown
    let other_timer_ids: Vec<String> = all_timers.iter()
        .filter(|t| t.id != timer.id)
        .map(|t| t.id.clone())
        .collect();

    rsx! {
        div { class: "list-item-body",
            // ─── Two Column Layout ─────────────────────────────────────────────
            div { class: "timer-edit-grid",
                // ═══ LEFT COLUMN: Main Timer Settings ═══════════════════════════
                div { class: "timer-edit-left",
                    div { class: "form-row-hz",
                        label { "Timer ID" }
                        code { class: "tag-muted text-mono text-xs", "{timer_display.id}" }
                    }

                    div { class: "form-row-hz",
                        label { "Name" }
                        input {
                            class: "input-inline",
                            r#type: "text",
                            style: "width: 200px;",
                            value: "{draft().name}",
                            oninput: move |e| {
                                let mut d = draft();
                                d.name = e.value();
                                draft.set(d);
                            }
                        }
                    }

                    div { class: "form-row-hz",
                        label { "Display Text" }
                        input {
                            class: "input-inline",
                            r#type: "text",
                            style: "width: 200px;",
                            placeholder: "(defaults to name)",
                            value: "{draft().display_text.clone().unwrap_or_default()}",
                            oninput: move |e| {
                                let mut d = draft();
                                d.display_text = if e.value().is_empty() { None } else { Some(e.value()) };
                                draft.set(d);
                            }
                        }
                    }

                    div { class: "form-row-hz",
                        label { "Difficulties" }
                        div { class: "flex gap-xs",
                            for diff in ["story", "veteran", "master"] {
                                {
                                    let diff_str = diff.to_string();
                                    let is_active = draft().difficulties.contains(&diff_str);
                                    let diff_clone = diff_str.clone();

                                    rsx! {
                                        button {
                                            class: if is_active { "toggle-btn active" } else { "toggle-btn" },
                                            onclick: move |_| {
                                                let mut d = draft();
                                                if d.difficulties.contains(&diff_clone) {
                                                    d.difficulties.retain(|x| x != &diff_clone);
                                                } else {
                                                    d.difficulties.push(diff_clone.clone());
                                                }
                                                draft.set(d);
                                            },
                                            "{diff}"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div { class: "form-row-hz",
                        label { "Duration" }
                        input {
                            class: "input-inline",
                            r#type: "number",
                            step: "1",
                            min: "0",
                            style: "width: 70px;",
                            disabled: draft().is_alert,
                            value: "{draft().duration_secs}",
                            oninput: move |e| {
                                if let Ok(val) = e.value().parse::<f32>() {
                                    let mut d = draft();
                                    d.duration_secs = val;
                                    draft.set(d);
                                }
                            }
                        }
                        span { class: if draft().is_alert { "text-muted opacity-50" } else { "text-muted" }, "sec" }
                        span { class: "ml-md" }
                        label { class: "flex items-center gap-xs text-sm",
                            input {
                                r#type: "checkbox",
                                checked: draft().is_alert,
                                onchange: move |e| {
                                    let mut d = draft();
                                    d.is_alert = e.checked();
                                    draft.set(d);
                                }
                            }
                            "Is Alert"
                        }
                    }

                    div { class: "form-row-hz", style: "align-items: flex-start;",
                        label { style: "padding-top: 6px;", "Trigger" }
                        ComposableTriggerEditor {
                            trigger: draft().trigger.clone(),
                            encounter_data: encounter_data.clone(),
                            on_change: move |t| {
                                let mut d = draft();
                                d.trigger = t;
                                draft.set(d);
                            }
                        }
                    }

                    // Note: Source/Target filtering is now handled within the trigger conditions
                    // via the ComposableTriggerEditor component

                    div { class: "form-row-hz",
                        label { "Options" }
                        div { class: "flex gap-md flex-wrap",
                            label { class: "flex items-center gap-xs text-sm",
                                input {
                                    r#type: "checkbox",
                                    checked: draft().can_be_refreshed,
                                    onchange: move |e| {
                                        let mut d = draft();
                                        d.can_be_refreshed = e.checked();
                                        draft.set(d);
                                    }
                                }
                                "Can Refresh"
                            }
                            div { class: "flex items-center gap-xs",
                                span { class: "text-sm text-secondary", "Repeats" }
                                input {
                                    class: "input-inline",
                                    r#type: "number",
                                    min: "0",
                                    max: "255",
                                    style: "width: 50px;",
                                    value: "{draft().repeats}",
                                    oninput: move |e| {
                                        if let Ok(val) = e.value().parse::<u8>() {
                                            let mut d = draft();
                                            d.repeats = val;
                                            draft.set(d);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div { class: "form-row-hz",
                        label { "Chains To" }
                        select {
                            class: "select",
                            style: "width: 160px;",
                            value: "{draft().chains_to.clone().unwrap_or_default()}",
                            onchange: move |e| {
                                let mut d = draft();
                                d.chains_to = if e.value().is_empty() { None } else { Some(e.value()) };
                                draft.set(d);
                            },
                            option { value: "", "(none)" }
                            for tid in &other_timer_ids {
                                option { value: "{tid}", "{tid}" }
                            }
                        }
                    }

                    div { class: "form-row-hz", style: "align-items: flex-start;",
                        label { style: "padding-top: 6px;", "Cancel On" }
                        if let Some(cancel) = draft().cancel_trigger.clone() {
                            div { class: "flex-col gap-xs",
                                ComposableTriggerEditor {
                                    trigger: cancel.clone(),
                                    encounter_data: encounter_data.clone(),
                                    on_change: move |t| {
                                        let mut d = draft();
                                        d.cancel_trigger = Some(t);
                                        draft.set(d);
                                    }
                                }
                                button {
                                    class: "btn btn-sm",
                                    style: "width: fit-content;",
                                    onclick: move |_| {
                                        let mut d = draft();
                                        d.cancel_trigger = None;
                                        draft.set(d);
                                    },
                                    "Remove Cancel Trigger"
                                }
                            }
                        } else {
                            div { class: "flex-col gap-xs",
                                span { class: "text-muted text-sm", "(default: combat end)" }
                                button {
                                    class: "btn btn-sm",
                                    onclick: move |_| {
                                        let mut d = draft();
                                        d.cancel_trigger = Some(Trigger::CombatStart);
                                        draft.set(d);
                                    },
                                    "+ Add Cancel Trigger"
                                }
                            }
                        }
                    }
                }

                // ═══ RIGHT COLUMN: Conditions & Audio ═══════════════════════════
                div { class: "timer-edit-right",
                    // ─── Color & Enabled ────────────────────────────────────────
                    div { class: "flex items-center gap-md mb-md",
                        div { class: "flex items-center gap-xs",
                            label { class: "text-sm text-secondary", "Color" }
                            input {
                                class: "color-picker",
                                r#type: "color",
                                value: "{color_hex}",
                                oninput: move |e| {
                                    if let Some(color) = parse_hex_color(&e.value()) {
                                        let mut d = draft();
                                        d.color = color;
                                        draft.set(d);
                                    }
                                }
                            }
                        }
                        div { class: "flex items-center gap-xs",
                            label { class: "text-sm text-secondary", "Enabled" }
                            input {
                                r#type: "checkbox",
                                checked: draft().enabled,
                                onchange: move |e| {
                                    let mut d = draft();
                                    d.enabled = e.checked();
                                    draft.set(d);
                                }
                            }
                        }
                    }

                    // ─── Show At ─────────────────────────────────────────────────
                    div { class: "form-row-hz",
                        label { "Show at" }
                        input {
                            r#type: "number",
                            class: "input-inline",
                            style: "width: 60px;",
                            min: "0",
                            max: "{draft().duration_secs as u32}",
                            value: "{draft().show_at_secs as u32}",
                            oninput: move |e| {
                                if let Ok(val) = e.value().parse::<f32>() {
                                    let mut d = draft();
                                    // Clamp to duration
                                    d.show_at_secs = val.min(d.duration_secs).max(0.0);
                                    draft.set(d);
                                }
                            }
                        }
                        span { class: "text-sm text-secondary", "sec remaining (0 = always)" }
                    }

                    // ─── Conditions ──────────────────────────────────────────────
                    span { class: "text-sm font-bold text-secondary", "Conditions" }

                    div { class: "form-row-hz mt-xs",
                        label { "Phases" }
                        PhaseSelector {
                            selected: draft().phases.clone(),
                            available: encounter_data.phase_ids(),
                            on_change: move |p| {
                                let mut d = draft();
                                d.phases = p;
                                draft.set(d);
                            }
                        }
                    }

                    div { class: "form-row-hz",
                        label { "Counter" }
                        CounterConditionEditor {
                            condition: draft().counter_condition.clone(),
                            counters: encounter_data.counter_ids(),
                            on_change: move |c| {
                                let mut d = draft();
                                d.counter_condition = c;
                                draft.set(d);
                            }
                        }
                    }

                    // ─── Audio & Alerts ──────────────────────────────────────────
                    span { class: "text-sm font-bold text-secondary mt-md", "Audio & Alerts" }

                    div { class: "form-row-hz mt-xs",
                        label { "Countdown" }
                        input {
                            class: "input-inline",
                            r#type: "number",
                            step: "1",
                            min: "0",
                            style: "width: 60px;",
                            placeholder: "-",
                            value: "{draft().alert_at_secs.map(|v| v.to_string()).unwrap_or_default()}",
                            oninput: move |e| {
                                let mut d = draft();
                                d.alert_at_secs = e.value().parse::<f32>().ok();
                                draft.set(d);
                            }
                        }
                        span { class: "text-muted", "sec" }
                    }

                    if draft().is_alert {
                        div { class: "form-row-hz",
                            label { "Alert Text" }
                            input {
                                class: "input-inline",
                                r#type: "text",
                                style: "width: 140px;",
                                placeholder: "(timer name)",
                                value: "{draft().alert_text.clone().unwrap_or_default()}",
                                oninput: move |e| {
                                    let mut d = draft();
                                    d.alert_text = if e.value().is_empty() { None } else { Some(e.value()) };
                                    draft.set(d);
                                }
                            }
                        }
                    }

                    // ─── Audio Section ───────────────────────────────────────────────
                    div { class: "form-row-hz",
                        label { "Enable Audio" }
                        input {
                            r#type: "checkbox",
                            checked: draft().audio.enabled,
                            onchange: move |e| {
                                let mut d = draft();
                                d.audio.enabled = e.checked();
                                draft.set(d);
                            }
                        }
                    }

                    div { class: "form-row-hz",
                        label { "Alert Sound" }
                        div { class: "flex items-center gap-xs",
                            select {
                                class: "select-inline",
                                style: "width: 140px;",
                                value: "{draft().audio.file.clone().unwrap_or_default()}",
                                onchange: move |e| {
                                    let mut d = draft();
                                    d.audio.file = if e.value().is_empty() { None } else { Some(e.value()) };
                                    draft.set(d);
                                },
                                option { value: "", "(none)" }
                                option { value: "Alarm.mp3", "Alarm.mp3" }
                                option { value: "Alert.mp3", "Alert.mp3" }
                                // Show custom path if set and not a bundled sound
                                if let Some(ref path) = draft().audio.file {
                                    if !path.is_empty() && path != "Alarm.mp3" && path != "Alert.mp3" {
                                        option { value: "{path}", selected: true, "{path} (custom)" }
                                    }
                                }
                            }
                            button {
                                class: "btn btn-sm",
                                r#type: "button",
                                onclick: move |_| {
                                    spawn(async move {
                                        if let Some(path) = api::pick_audio_file().await {
                                            // Validate extension
                                            let lower = path.to_lowercase();
                                            if lower.ends_with(".mp3") || lower.ends_with(".wav") {
                                                let mut d = draft();
                                                d.audio.file = Some(path);
                                                draft.set(d);
                                            }
                                        }
                                    });
                                },
                                "Browse"
                            }
                        }
                    }

                    // Audio offset - when to play the sound before timer expires
                    div { class: "form-row-hz",
                        label { "Audio Offset" }
                        div { class: "flex items-center gap-md",
                            select {
                                class: "select-inline",
                                style: "width: 120px;",
                                value: "{draft().audio.offset}",
                                onchange: move |e| {
                                    if let Ok(val) = e.value().parse::<u8>() {
                                        let mut d = draft();
                                        d.audio.offset = val;
                                        draft.set(d);
                                    }
                                },
                                option { value: "0", "On expiration" }
                                option { value: "1", "1s before" }
                                option { value: "2", "2s before" }
                                option { value: "3", "3s before" }
                                option { value: "4", "4s before" }
                                option { value: "5", "5s before" }
                                option { value: "6", "6s before" }
                                option { value: "7", "7s before" }
                                option { value: "8", "8s before" }
                                option { value: "9", "9s before" }
                                option { value: "10", "10s before" }
                            }
                        }
                    }

                    div { class: "form-row-hz",
                        label { "Countdown" }
                        div { class: "flex items-center gap-md",
                            select {
                                class: "select-inline",
                                style: "width: 80px;",
                                value: "{draft().audio.countdown_start}",
                                onchange: move |e| {
                                    if let Ok(val) = e.value().parse::<u8>() {
                                        let mut d = draft();
                                        d.audio.countdown_start = val;
                                        draft.set(d);
                                    }
                                },
                                option { value: "0", "Off" }
                                option { value: "1", "1s" }
                                option { value: "2", "2s" }
                                option { value: "3", "3s" }
                                option { value: "4", "4s" }
                                option { value: "5", "5s" }
                                option { value: "6", "6s" }
                                option { value: "7", "7s" }
                                option { value: "8", "8s" }
                                option { value: "9", "9s" }
                                option { value: "10", "10s" }
                            }
                            span { class: "text-sm text-secondary", "Voice" }
                            select {
                                class: "select-inline",
                                style: "width: 100px;",
                                value: "{draft().audio.countdown_voice.clone().unwrap_or_else(|| \"Amy\".to_string())}",
                                onchange: move |e| {
                                    let mut d = draft();
                                    d.audio.countdown_voice = if e.value() == "Amy" { None } else { Some(e.value()) };
                                    draft.set(d);
                                },
                                option { value: "Amy", "Amy" }
                                option { value: "Jim", "Jim" }
                                option { value: "Yolo", "Yolo" }
                                option { value: "Nerevar", "Nerevar" }
                            }
                        }
                    }
                }
            }

            // ─── Actions ────────────────────────────────────────────────────────
            div { class: "form-actions",
                button {
                    class: if has_changes() { "btn btn-success btn-sm" } else { "btn btn-sm" },
                    disabled: !has_changes(),
                    onclick: handle_save,
                    "Save"
                }
                button {
                    class: "btn btn-primary btn-sm",
                    onclick: handle_duplicate,
                    "Duplicate"
                }

                if confirm_delete() {
                    span { class: "flex items-center gap-xs ml-auto",
                        "Delete?"
                        button {
                            class: "btn btn-danger btn-sm",
                            onclick: handle_delete,
                            "Yes"
                        }
                        button {
                            class: "btn btn-sm",
                            onclick: move |_| confirm_delete.set(false),
                            "No"
                        }
                    }
                } else {
                    button {
                        class: "btn btn-danger btn-sm ml-auto",
                        onclick: move |_| confirm_delete.set(true),
                        "Delete"
                    }
                }
            }

            // File info (from context)
            div { class: "mt-sm text-xs text-muted", "File: {boss_with_path.file_path}" }
        }
    }
}


// ─────────────────────────────────────────────────────────────────────────────
// Entity Filter Selector
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn EntityFilterSelector(
    value: EntityFilter,
    on_change: EventHandler<EntityFilter>,
) -> Element {
    use baras_types::EntitySelector;

    let current_value = value.type_name();
    let selector_value = if let EntityFilter::Selector(selectors) = &value {
        selectors.first().map(|s| s.display()).unwrap_or_default()
    } else {
        String::new()
    };

    rsx! {
        div { class: "flex-col gap-xs",
            select {
                class: "select",
                style: "width: 160px;",
                value: "{current_value}",
                onchange: move |e| {
                    let new_filter = match e.value().as_str() {
                        "any" => EntityFilter::Any,
                        "local_player" => EntityFilter::LocalPlayer,
                        "other_players" => EntityFilter::OtherPlayers,
                        "any_player" => EntityFilter::AnyPlayer,
                        "any_companion" => EntityFilter::AnyCompanion,
                        "group_members" => EntityFilter::GroupMembers,
                        "group_members_except_local" => EntityFilter::GroupMembersExceptLocal,
                        "boss" => EntityFilter::Boss,
                        "npc_except_boss" => EntityFilter::NpcExceptBoss,
                        "any_npc" => EntityFilter::AnyNpc,
                        "selector" => EntityFilter::Selector(vec![]),
                        _ => EntityFilter::Any,
                    };
                    on_change.call(new_filter);
                },
                option { value: "any", "Any" }
                option { value: "local_player", "Local Player" }
                option { value: "local_companion", "Local Companion" }
                option { value: "local_player_or_companion", "Local + Companion" }
                option { value: "other_players", "Other Players" }
                option { value: "any_player", "Any Player" }
                option { value: "any_companion", "Any Companion" }
                option { value: "any_player_or_companion", "Any Player/Companion" }
                option { value: "group_members", "Group Members" }
                option { value: "group_members_except_local", "Group (Except Local)" }
                option { value: "boss", "Boss" }
                option { value: "npc_except_boss", "NPC (Except Boss)" }
                option { value: "any_npc", "Any NPC" }
                option { value: "selector", "Specific (ID or Name)" }
            }

            if matches!(value, EntityFilter::Selector(_)) {
                input {
                    class: "input-inline",
                    r#type: "text",
                    style: "width: 100%;",
                    placeholder: "NPC ID or entity name",
                    value: "{selector_value}",
                    oninput: move |e| {
                        let selector = EntitySelector::from_input(&e.value());
                        on_change.call(EntityFilter::Selector(vec![selector]))
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase Selector (multi-select dropdown)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn PhaseSelector(
    selected: Vec<String>,
    available: Vec<String>,
    on_change: EventHandler<Vec<String>>,
) -> Element {
    let mut dropdown_open = use_signal(|| false);

    // Display text
    let display = if selected.is_empty() {
        "(all phases)".to_string()
    } else if selected.len() == 1 {
        selected[0].clone()
    } else {
        format!("{} phases", selected.len())
    };

    rsx! {
        div {
            class: "phase-selector",
            style: "position: relative;",
            // Dropdown trigger
            button {
                class: "select",
                style: "width: 160px; text-align: left;",
                onclick: move |_| dropdown_open.set(!dropdown_open()),
                "{display}"
                span { class: "ml-auto", "▾" }
            }

            // Dropdown menu
            if dropdown_open() {
                div {
                    class: "phase-dropdown",
                    style: "position: absolute; top: 100%; left: 0; z-index: 1000; background: #1e1e2e; border: 1px solid var(--border-medium); border-radius: var(--radius-sm); padding: var(--space-xs); min-width: 160px; max-height: 200px; overflow-y: auto; box-shadow: 0 4px 12px rgba(0,0,0,0.5);",

                    if available.is_empty() {
                        span { class: "text-muted text-sm", "No phases defined" }
                    } else {
                        // "All" option (clears selection)
                        label { class: "flex items-center gap-xs text-sm p-xs cursor-pointer",
                            input {
                                r#type: "checkbox",
                                checked: selected.is_empty(),
                                onchange: move |_| {
                                    on_change.call(vec![]);
                                    dropdown_open.set(false);
                                }
                            }
                            "(all phases)"
                        }

                        // Individual phases
                        for phase in available.iter() {
                            {
                                let phase_id = phase.clone();
                                let is_selected = selected.contains(&phase_id);
                                let selected_clone = selected.clone();

                                rsx! {
                                    label { class: "flex items-center gap-xs text-sm p-xs cursor-pointer",
                                        input {
                                            r#type: "checkbox",
                                            checked: is_selected,
                                            onchange: move |_| {
                                                let mut new_selected = selected_clone.clone();
                                                if is_selected {
                                                    new_selected.retain(|p| p != &phase_id);
                                                } else {
                                                    new_selected.push(phase_id.clone());
                                                }
                                                on_change.call(new_selected);
                                            }
                                        }
                                        "{phase}"
                                    }
                                }
                            }
                        }
                    }

                    // Close button
                    button {
                        class: "btn btn-sm w-full mt-xs",
                        onclick: move |_| dropdown_open.set(false),
                        "Done"
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn parse_hex_color(hex: &str) -> Option<[u8; 4]> {
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6 {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some([r, g, b, 255])
    } else {
        None
    }
}
