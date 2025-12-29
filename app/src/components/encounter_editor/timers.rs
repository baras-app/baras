//! Timer editing tab
//!
//! Full CRUD for boss timers with all fields exposed.

use dioxus::prelude::*;

use crate::api;
use crate::types::{BossListItem, EntityFilter, TimerListItem, TimerTrigger};

use super::conditions::CounterConditionEditor;
use super::tabs::EncounterData;
use super::triggers::ComposableTriggerEditor;

// ─────────────────────────────────────────────────────────────────────────────
// Timers Tab
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn TimersTab(
    boss: BossListItem,
    timers: Vec<TimerListItem>,
    encounter_data: EncounterData,
    on_change: EventHandler<Vec<TimerListItem>>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let mut expanded_timer = use_signal(|| None::<String>);
    let mut show_new_timer = use_signal(|| false);

    rsx! {
        div { class: "timers-tab",
            // Header
            div { class: "flex items-center justify-between mb-sm",
                span { class: "text-sm text-secondary", "{timers.len()} timers" }
                button {
                    class: "btn btn-success btn-sm",
                    onclick: move |_| show_new_timer.set(true),
                    "+ New Timer"
                }
            }

            // New timer form
            if show_new_timer() {
                {
                    let timers_for_create = timers.clone();
                    rsx! {
                        NewTimerForm {
                            boss: boss.clone(),
                            encounter_data: encounter_data.clone(),
                            on_create: move |new_timer: TimerListItem| {
                                let timers_clone = timers_for_create.clone();
                                spawn(async move {
                                    if let Some(created) = api::create_encounter_timer(&new_timer).await {
                                        let mut current = timers_clone;
                                        current.push(created);
                                        on_change.call(current);
                                        on_status.call(("Created".to_string(), false));
                                    } else {
                                        on_status.call(("Failed to create".to_string(), true));
                                    }
                                });
                                show_new_timer.set(false);
                            },
                            on_cancel: move |_| show_new_timer.set(false),
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
                        let timer_key = timer.timer_id.clone();
                        let is_expanded = expanded_timer() == Some(timer_key.clone());
                        let timers_for_row = timers.clone();

                        rsx! {
                            TimerRow {
                                key: "{timer_key}",
                                timer: timer.clone(),
                                all_timers: timers_for_row,
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
    timer: TimerListItem,
    all_timers: Vec<TimerListItem>,
    encounter_data: EncounterData,
    expanded: bool,
    on_toggle: EventHandler<()>,
    on_change: EventHandler<Vec<TimerListItem>>,
    on_status: EventHandler<(String, bool)>,
    on_collapse: EventHandler<()>,
) -> Element {
    let color_hex = format!("#{:02x}{:02x}{:02x}", timer.color[0], timer.color[1], timer.color[2]);
    let timer_for_enable = timer.clone();
    let timer_for_audio = timer.clone();
    let timers_for_enable = all_timers.clone();
    let timers_for_audio = all_timers.clone();

    rsx! {
        div { class: "list-item",
            // Header row
            div {
                class: "list-item-header",
                onclick: move |_| on_toggle.call(()),

                span { class: "list-item-expand", if expanded { "▼" } else { "▶" } }
                span {
                    class: "color-swatch",
                    style: "background: {color_hex};"
                }
                span { class: "font-medium text-primary", "{timer.name}" }
                span { class: "text-xs text-mono text-muted", "{timer.timer_id}" }
                span { class: "tag", "{timer.trigger.label()}" }
                span { class: "text-sm text-secondary", "{timer.duration_secs:.1}s" }

                // Enabled toggle (clickable without expanding)
                span {
                    class: "row-toggle",
                    title: if timer.enabled { "Disable timer" } else { "Enable timer" },
                    onclick: move |e| {
                        e.stop_propagation();
                        let mut updated = timer_for_enable.clone();
                        updated.enabled = !updated.enabled;
                        let mut current = timers_for_enable.clone();
                        if let Some(idx) = current.iter().position(|t| t.timer_id == updated.timer_id) {
                            current[idx] = updated.clone();
                            on_change.call(current);
                        }
                        spawn(async move {
                            api::update_encounter_timer(&updated).await;
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
                    title: if timer.audio_file.is_some() { "Disable audio" } else { "No audio configured" },
                    onclick: move |e| {
                        e.stop_propagation();
                        // Only toggle if audio was configured
                        if timer_for_audio.audio_file.is_some() {
                            let mut updated = timer_for_audio.clone();
                            updated.audio_file = None;
                            let mut current = timers_for_audio.clone();
                            if let Some(idx) = current.iter().position(|t| t.timer_id == updated.timer_id) {
                                current[idx] = updated.clone();
                                on_change.call(current);
                            }
                            spawn(async move {
                                api::update_encounter_timer(&updated).await;
                            });
                        }
                    },
                    span {
                        class: if timer.audio_file.is_some() { "text-primary" } else { "text-muted" },
                        if timer.audio_file.is_some() { "🔊" } else { "🔇" }
                    }
                }
            }

            // Edit form
            if expanded {
                TimerEditForm {
                    timer: timer.clone(),
                    all_timers: all_timers,
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
    timer: TimerListItem,
    all_timers: Vec<TimerListItem>,
    encounter_data: EncounterData,
    on_change: EventHandler<Vec<TimerListItem>>,
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
        move |_| {
            let updated = draft();
            let mut current = timers.clone();
            if let Some(idx) = current.iter().position(|t| t.timer_id == updated.timer_id) {
                current[idx] = updated.clone();
                on_change.call(current);
            }
            spawn(async move {
                if api::update_encounter_timer(&updated).await {
                    on_status.call(("Saved".to_string(), false));
                } else {
                    on_status.call(("Failed to save".to_string(), true));
                }
            });
        }
    };

    // Delete handler
    let handle_delete = {
        let timer_del = timer.clone();
        let timers = all_timers.clone();
        move |_| {
            let filtered: Vec<_> = timers.clone().into_iter()
                .filter(|t| t.timer_id != timer_del.timer_id)
                .collect();
            on_change.call(filtered);
            on_collapse.call(());
            let t = timer_del.clone();
            spawn(async move {
                if api::delete_encounter_timer(&t.timer_id, &t.boss_id, &t.file_path).await {
                    on_status.call(("Deleted".to_string(), false));
                } else {
                    on_status.call(("Failed to delete".to_string(), true));
                }
            });
        }
    };

    // Duplicate handler
    let handle_duplicate = {
        let timer_dup = timer.clone();
        let timers = all_timers.clone();
        move |_| {
            let t = timer_dup.clone();
            let ts = timers.clone();
            spawn(async move {
                if let Some(new_timer) = api::duplicate_encounter_timer(&t.timer_id, &t.boss_id, &t.file_path).await {
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
        .filter(|t| t.timer_id != timer.timer_id)
        .map(|t| t.timer_id.clone())
        .collect();

    rsx! {
        div { class: "list-item-body",
            // ─── Two Column Layout ─────────────────────────────────────────────
            div { class: "timer-edit-grid",
                // ═══ LEFT COLUMN: Main Timer Settings ═══════════════════════════
                div { class: "timer-edit-left",
                    div { class: "form-row-hz",
                        label { "Timer ID" }
                        code { class: "tag-muted text-mono text-xs", "{timer_display.timer_id}" }
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
                            step: "0.1",
                            min: "0",
                            style: "width: 70px;",
                            value: "{draft().duration_secs}",
                            oninput: move |e| {
                                if let Ok(val) = e.value().parse::<f32>() {
                                    let mut d = draft();
                                    d.duration_secs = val;
                                    draft.set(d);
                                }
                            }
                        }
                        span { class: "text-muted", "sec" }
                        span { class: "ml-md" }
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
                        span { class: "ml-md" }
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
                        div { class: "flex items-center gap-xs",
                            input {
                                r#type: "checkbox",
                                checked: draft().cancel_trigger.is_some(),
                                onchange: move |e| {
                                    let mut d = draft();
                                    d.cancel_trigger = if e.checked() {
                                        Some(TimerTrigger::CombatStart)
                                    } else {
                                        None
                                    };
                                    draft.set(d);
                                }
                            }
                            if draft().cancel_trigger.is_some() {
                                ComposableTriggerEditor {
                                    trigger: draft().cancel_trigger.clone().unwrap(),
                                    encounter_data: encounter_data.clone(),
                                    on_change: move |t| {
                                        let mut d = draft();
                                        d.cancel_trigger = Some(t);
                                        draft.set(d);
                                    }
                                }
                            } else {
                                span { class: "text-muted text-sm", "(disabled)" }
                            }
                        }
                    }
                }

                // ═══ RIGHT COLUMN: Conditions & Audio ═══════════════════════════
                div { class: "timer-edit-right",
                    // ─── Conditions ──────────────────────────────────────────────
                    span { class: "text-sm font-bold text-secondary", "Conditions" }

                    div { class: "form-row-hz mt-xs",
                        label { "Source" }
                        EntityFilterSelector {
                            value: draft().source.clone(),
                            on_change: move |f| {
                                let mut d = draft();
                                d.source = f;
                                draft.set(d);
                            }
                        }
                    }

                    div { class: "form-row-hz",
                        label { "Target" }
                        EntityFilterSelector {
                            value: draft().target.clone(),
                            on_change: move |f| {
                                let mut d = draft();
                                d.target = f;
                                draft.set(d);
                            }
                        }
                    }

                    div { class: "form-row-hz",
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
                            step: "0.1",
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

                    div { class: "form-row-hz",
                        label { "Audio" }
                        div { class: "flex items-center gap-xs",
                            input {
                                class: "input-inline",
                                r#type: "text",
                                style: "width: 140px;",
                                placeholder: "(none)",
                                value: "{draft().audio_file.clone().unwrap_or_default()}",
                                oninput: move |e| {
                                    let mut d = draft();
                                    d.audio_file = if e.value().is_empty() { None } else { Some(e.value()) };
                                    draft.set(d);
                                }
                            }
                            button {
                                class: "btn btn-sm",
                                r#type: "button",
                                onclick: move |_| {
                                    spawn(async move {
                                        if let Some(path) = api::pick_audio_file().await {
                                            let mut d = draft();
                                            d.audio_file = Some(path);
                                            draft.set(d);
                                        }
                                    });
                                },
                                "Browse"
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

            // File info
            div { class: "mt-sm text-xs text-muted", "File: {timer_display.file_path}" }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// New Timer Form
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn NewTimerForm(
    boss: BossListItem,
    encounter_data: EncounterData,
    on_create: EventHandler<TimerListItem>,
    on_cancel: EventHandler<()>,
) -> Element {
    let mut name = use_signal(String::new);
    let mut duration = use_signal(|| 30.0f32);
    let mut color = use_signal(|| [255u8, 128, 0, 255]);
    let mut trigger = use_signal(|| TimerTrigger::CombatStart);
    let mut difficulties = use_signal(|| vec!["story".to_string(), "veteran".to_string(), "master".to_string()]);

    let color_hex = format!("#{:02x}{:02x}{:02x}", color()[0], color()[1], color()[2]);

    rsx! {
        div { class: "new-timer-form mb-md",
            div { class: "flex items-center justify-between mb-sm",
                h4 { class: "text-primary", "New Timer" }
                button {
                    class: "btn btn-ghost btn-sm",
                    onclick: move |_| on_cancel.call(()),
                    "×"
                }
            }

            // Name
            div { class: "form-row-hz",
                label { "Name" }
                input {
                    class: "input-inline",
                    r#type: "text",
                    style: "width: 250px;",
                    placeholder: "e.g., Rocket Salvo",
                    value: "{name}",
                    oninput: move |e| name.set(e.value())
                }
            }

            // Difficulties
            div { class: "form-row-hz",
                label { "Difficulties" }
                div { class: "flex gap-xs",
                    for diff in ["story", "veteran", "master"] {
                        {
                            let diff_str = diff.to_string();
                            let is_active = difficulties().contains(&diff_str);
                            let diff_clone = diff_str.clone();

                            rsx! {
                                button {
                                    class: if is_active { "toggle-btn active" } else { "toggle-btn" },
                                    onclick: move |_| {
                                        let mut d = difficulties();
                                        if d.contains(&diff_clone) {
                                            d.retain(|x| x != &diff_clone);
                                        } else {
                                            d.push(diff_clone.clone());
                                        }
                                        difficulties.set(d);
                                    },
                                    "{diff}"
                                }
                            }
                        }
                    }
                }
            }

            // Duration and Color
            div { class: "form-row-hz",
                label { "Duration" }
                input {
                    class: "input-inline",
                    r#type: "number",
                    step: "0.1",
                    min: "0",
                    style: "width: 70px;",
                    value: "{duration}",
                    oninput: move |e| {
                        if let Ok(val) = e.value().parse::<f32>() {
                            duration.set(val);
                        }
                    }
                }
                span { class: "text-muted", "sec" }

                span { class: "ml-md" }
                label { class: "text-sm text-secondary", "Color" }
                input {
                    class: "color-picker",
                    r#type: "color",
                    value: "{color_hex}",
                    oninput: move |e| {
                        if let Some(c) = parse_hex_color(&e.value()) {
                            color.set(c);
                        }
                    }
                }
            }

            // Trigger
            div { class: "form-row-hz",
                label { "Trigger" }
                ComposableTriggerEditor {
                    trigger: trigger(),
                    encounter_data: encounter_data.clone(),
                    on_change: move |t| trigger.set(t)
                }
            }

            // Actions
            div { class: "flex gap-sm",
                button {
                    class: if name().is_empty() { "btn btn-sm" } else { "btn btn-success btn-sm" },
                    disabled: name().is_empty(),
                    onclick: move |_| {
                        on_create.call(TimerListItem {
                            timer_id: String::new(),
                            boss_id: boss.id.clone(),
                            boss_name: boss.name.clone(),
                            area_name: boss.area_name.clone(),
                            category: boss.category.clone(),
                            file_path: boss.file_path.clone(),
                            name: name(),
                            display_text: None,
                            enabled: true,
                            duration_secs: duration(),
                            color: color(),
                            phases: vec![],
                            difficulties: difficulties(),
                            trigger: trigger(),
                            source: EntityFilter::Any,
                            target: EntityFilter::Any,
                            counter_condition: None,
                            cancel_trigger: None,
                            can_be_refreshed: false,
                            repeats: 0,
                            chains_to: None,
                            alert_at_secs: None,
                            is_alert: false,
                            alert_text: None,
                            show_on_raid_frames: false,
                            audio_file: None,
                        });
                    },
                    "Create Timer"
                }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| on_cancel.call(()),
                    "Cancel"
                }
            }
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
    let current_value = value.type_name();
    let specific_value = if let EntityFilter::Specific(s) = &value { s.clone() } else { String::new() };

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
                        "specific" => EntityFilter::Specific(String::new()),
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
                option { value: "specific", "Specific Entity" }
            }

            if matches!(value, EntityFilter::Specific(_)) {
                input {
                    class: "input-inline",
                    r#type: "text",
                    style: "width: 100%;",
                    placeholder: "Entity name",
                    value: "{specific_value}",
                    oninput: move |e| on_change.call(EntityFilter::Specific(e.value()))
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase Selector (multi-select dropdown)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn PhaseSelector(
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
