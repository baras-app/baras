//! Timer edit form, shown in the editor drawer

use dioxus::prelude::*;

use crate::api;
use crate::components::SoundPicker;
use crate::components::editor_common::{FormCard, ItemActions, Origin};
use crate::components::form_fields::{CheckField, Deprecated, Field, NumberField, SubSection, TextField, TogglePill};
use crate::types::{AlertTrigger, BossTimerDefinition, BossWithPath, Condition, EncounterItem, TimerDisplayTarget, Trigger, timer_alert_label};
use crate::utils::parse_hex_color;

use super::optional_trigger::OptionalTrigger;
use super::conditions::{ConditionsEditor, CounterConditionEditor};
use super::tabs::EncounterData;
use super::timers::PhaseSelector;
use super::triggers::ComposableTriggerEditor;

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
    let timer_for_draft = timer.clone();
    let timer_for_delete = timer.clone();
    let timer_id = timer.id.clone();
    let timer_original = timer.clone();
    let mut draft = use_signal(|| timer_for_draft);
    let mut just_saved = use_signal(|| false);

    // Reset just_saved when user makes new changes after saving
    let timer_original_for_effect = timer_original.clone();
    use_effect(move || {
        if draft() != timer_original_for_effect && just_saved() {
            just_saved.set(false);
        }
    });

    let has_changes = use_memo(move || !just_saved() && draft() != timer_original);

    // Notify parent when dirty state changes
    use_effect(move || {
        on_dirty.call(has_changes());
    });
    let color_hex = format!(
        "#{:02x}{:02x}{:02x}",
        draft().color[0],
        draft().color[1],
        draft().color[2]
    );

    // Icon preview for timer bar icon
    let mut icon_preview_url = use_signal(|| None::<String>);
    use_effect(move || {
        let current_draft = draft();
        if let Some(ability_id) = current_draft.icon_ability_id {
            spawn(async move {
                if let Some(url) = api::get_icon_preview(ability_id).await {
                    icon_preview_url.set(Some(url));
                } else {
                    icon_preview_url.set(None);
                }
            });
        } else {
            icon_preview_url.set(None);
        }
    });

    // Save handler
    let handle_save = {
        let timers = all_timers.clone();
        let bwp = boss_with_path.clone();
        move |_: ()| {
            just_saved.set(true);
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
                    Ok(_) => {
                        on_status.call(("Saved".to_string(), false));
                        on_refetch.call(());
                    }
                    Err(_) => on_status.call(("Failed to save".to_string(), true)),
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
                            let filtered: Vec<_> = timers_clone
                                .into_iter()
                                .filter(|timer| timer.id != t.id)
                                .collect();
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

    // Get other timer IDs for chains_to dropdown
    let other_timer_ids: Vec<String> = all_timers
        .iter()
        .filter(|t| t.id != timer_id)
        .map(|t| t.id.clone())
        .collect();

    rsx! {
        div { class: "list-item-body",
            div { class: "edit-grid left-55",
                // ═══ LEFT COLUMN: Identity, Trigger ════════════════════════════
                div { class: "edit-grid-left",
                    FormCard { icon: "fa-tag", title: "Identity",
                        Field { label: "Timer ID",
                            code { class: "id-display", "{timer_display.id}" }
                        }
                        TextField { label: "Name", value: draft().name, on_input: move |v| draft.write().name = v }
                        if !draft().is_alert {
                            TextField {
                                label: "Display Text",
                                help: "Text shown on the overlay timer bar. Defaults to timer name.",
                                value: draft().display_text.unwrap_or_default(),
                                placeholder: "(defaults to name)",
                                on_input: move |v: String| draft.write().display_text = (!v.is_empty()).then_some(v),
                            }
                        }
                        Field { label: "Difficulties",
                            for diff in ["story", "veteran", "master"] {
                                TogglePill {
                                    label: diff,
                                    active: draft().difficulties.iter().any(|d| d == diff),
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
                                    active: draft().group_size == size,
                                    on_click: move |_| draft.write().group_size = size,
                                }
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
                        Field { label: "Icon ID", help: "Ability ID to use for the icon on the timer bar. Leave blank for no icon.",
                            if let Some(ref url) = icon_preview_url() {
                                img { src: "{url}", class: "icon-preview", width: "24", height: "24", alt: "Icon preview" }
                            }
                            input {
                                r#type: "text",
                                placeholder: "(none)",
                                value: "{draft().icon_ability_id.map(|id| id.to_string()).unwrap_or_default()}",
                                oninput: move |e| draft.write().icon_ability_id = e.value().parse::<u64>().ok(),
                            }
                            if icon_preview_url().is_none() && draft().icon_ability_id.is_some() {
                                span { class: "field-unit", "(not found)" }
                            }
                        }
                        if !draft().is_alert {
                            Field {
                                label: "Display Overlays",
                                help: "Overlays this timer will appear on. Select multiple to show the same timer on more than one overlay.",
                                for target in TimerDisplayTarget::all().iter().filter(|t| !matches!(t, TimerDisplayTarget::None)).copied() {
                                    label { class: "field-check",
                                        input {
                                            r#type: "checkbox",
                                            checked: draft().display_targets.contains(&target),
                                            onchange: move |e| {
                                                let mut d = draft.write();
                                                d.display_targets.retain(|t| *t != target);
                                                if e.checked() {
                                                    d.display_targets.push(target);
                                                }
                                                // Clear queue-only fields when AbilityQueue is removed
                                                if !d.display_targets.contains(&TimerDisplayTarget::AbilityQueue) {
                                                    d.queue_on_expire = false;
                                                    d.gcd_secs = None;
                                                    d.queue_priority = 0;
                                                    d.queue_remove_trigger = None;
                                                    d.queue_countdown_bar = false;
                                                    d.queue_hide_from_next = false;
                                                }
                                            }
                                        }
                                        "{target.label()}"
                                    }
                                }
                            }
                        }
                    }

                    FormCard { icon: "fa-bolt", title: "Trigger",
                        SubSection { title: "Start", help: "The game event that starts this timer",
                            ComposableTriggerEditor {
                                trigger: draft().trigger.clone(),
                                encounter_data: encounter_data.clone(),
                                on_change: move |t| draft.write().trigger = t,
                            }
                            // Deprecated: kept visible only for timers that already chain
                            if !draft().is_alert && (timer_display.chains_to.is_some() || draft().chains_to.is_some()) {
                                Field { label: "Chains To", help: "Starts another timer when this one expires",
                                    {
                                        let selected_timer = draft().chains_to.unwrap_or_default();
                                        rsx! {
                                            select {
                                                onchange: move |e| {
                                                    let v = e.value();
                                                    draft.write().chains_to = (!v.is_empty()).then_some(v);
                                                },
                                                option { value: "", selected: selected_timer.is_empty(), "(none)" }
                                                for tid in &other_timer_ids {
                                                    option { value: "{tid}", selected: tid == &selected_timer, "{tid}" }
                                                }
                                            }
                                        }
                                    }
                                    Deprecated { note: "Deprecated: start the other timer with a Timer Expires trigger instead" }
                                }
                            }
                        }
                        if !draft().is_alert {
                            OptionalTrigger {
                                title: "Cancel On",
                                help: "Cancels this timer when the trigger fires. Default: combat end",
                                noun: "Cancel",
                                empty: "Default: combat end",
                                initial: Trigger::CombatStart,
                                trigger: draft().cancel_trigger,
                                encounter_data: encounter_data.clone(),
                                on_change: move |t| draft.write().cancel_trigger = t,
                            }
                        }
                        SubSection {
                            title: "Conditions",
                            help: "State conditions that must ALL be true for this timer to be active. Use for phase, counter, HP, or entity state guards.",
                            ConditionsEditor {
                                conditions: draft().conditions.clone(),
                                encounter_data: encounter_data.clone(),
                                on_change: move |c| draft.write().conditions = c,
                            }

                            // Legacy fields: show migration banner if old fields have values
                            if !draft().phases.is_empty() || draft().counter_condition.is_some() {
                                div { class: "legacy-conditions-banner",
                                    i { class: "fa-solid fa-circle-info" }
                                    span { "Legacy phase/counter fields detected." }
                                    button {
                                        class: "btn btn-sm",
                                        title: "Move legacy phase and counter conditions into the new Conditions system",
                                        onclick: move |_| {
                                            let mut d = draft.write();
                                            if !d.phases.is_empty() {
                                                let phase_ids = std::mem::take(&mut d.phases);
                                                d.conditions.push(Condition::PhaseActive { phase_ids });
                                            }
                                            if let Some(cc) = d.counter_condition.take() {
                                                d.conditions.push(Condition::CounterCompare {
                                                    counter_id: cc.counter_id,
                                                    operator: cc.operator,
                                                    value: cc.value,
                                                });
                                            }
                                        },
                                        "Migrate"
                                    }
                                }
                                Field { label: "Phases (legacy)", muted: true,
                                    PhaseSelector {
                                        selected: draft().phases.clone(),
                                        available: encounter_data.phase_ids(),
                                        on_change: move |p| draft.write().phases = p,
                                    }
                                }
                                Field { label: "Counter (legacy)", muted: true,
                                    CounterConditionEditor {
                                        condition: draft().counter_condition.clone(),
                                        counters: encounter_data.counter_ids(),
                                        on_change: move |c| draft.write().counter_condition = c,
                                    }
                                }
                            }
                        }
                    }
                }

                // ═══ RIGHT COLUMN: Timing, Alerts, Audio ═══════════════════════
                div { class: "edit-grid-right",
                    FormCard { icon: "fa-clock", title: "Timing",
                        CheckField {
                            label: "Instant alert only",
                            help: "Shows a brief alert notification instead of a countdown timer bar",
                            checked: draft().is_alert,
                            on_change: move |v| draft.write().is_alert = v,
                        }
                        if !draft().is_alert {
                            CheckField {
                                label: "Can refresh",
                                help: "Resets the timer duration if triggered again while already running",
                                checked: draft().can_be_refreshed,
                                on_change: move |v| draft.write().can_be_refreshed = v,
                            }
                            NumberField {
                                label: "Duration",
                                help: "How long the countdown timer runs in seconds",
                                value: draft().duration_secs.to_string(),
                                unit: "sec",
                                min: "0",
                                on_input: move |v: String| if let Ok(v) = v.parse::<f32>() { draft.write().duration_secs = v },
                            }
                            NumberField {
                                label: "Show at",
                                help: "Only show the timer when this many seconds remain. 0 = always visible",
                                value: draft().show_at_secs.to_string(),
                                unit: "sec remaining",
                                min: "0",
                                max: draft().duration_secs.to_string(),
                                on_input: move |v: String| if let Ok(v) = v.parse::<f32>() {
                                    let mut d = draft.write();
                                    d.show_at_secs = v.min(d.duration_secs).max(0.0);
                                },
                            }
                            NumberField {
                                label: "Repeats",
                                help: "Number of times this timer auto-restarts after expiring. 0 = no repeat",
                                value: draft().repeats.to_string(),
                                min: "0",
                                max: "255",
                                step: "1",
                                on_input: move |v: String| if let Ok(v) = v.parse::<u8>() { draft.write().repeats = v },
                            }
                        }
                    }

                    FormCard { icon: "fa-bell", title: "Alerts",
                        TextField {
                            label: "Alert Text",
                            help: ALERT_TEXT_HELP,
                            value: draft().alert_text.unwrap_or_default(),
                            placeholder: "(timer name)",
                            on_input: move |v: String| draft.write().alert_text = (!v.is_empty()).then_some(v),
                        }
                        if draft().is_alert {
                            // Instant alerts always fire on trigger — no choice needed
                            Field { label: "Alert On", help: "Instant alerts always fire when triggered",
                                span { class: "field-unit", "On trigger (instant)" }
                            }
                        } else {
                            Field { label: "Alert On", help: "When to show the alert text: on timer start, on timer end, or never",
                                select {
                                    value: "{timer_alert_label(&draft().alert_on)}",
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
                                            selected: *trigger == draft().alert_on,
                                            "{timer_alert_label(trigger)}"
                                        }
                                    }
                                }
                            }
                            if draft().alert_on == AlertTrigger::Countdown {
                                NumberField {
                                    label: "Countdown Window",
                                    help: "Show a live-updating alert during the last N seconds of the timer (0-10). Alert text shows remaining time to one decimal.",
                                    value: draft().alert_countdown_secs.unwrap_or(3.0).to_string(),
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
                    }

                    FormCard { icon: "fa-volume-up", title: "Audio",
                        CheckField { label: "Enable audio", checked: draft().audio.enabled, on_change: move |v| draft.write().audio.enabled = v }
                        if draft().audio.enabled {
                            SoundPicker { value: draft().audio.file.clone(), on_change: move |v| draft.write().audio.file = v }
                            // Audio timing options (only for countdown timers)
                            if !draft().is_alert {
                                Field { label: "Audio Offset", help: "When to play the sound relative to timer expiration",
                                    select {
                                        value: "{draft().audio.offset}",
                                        onchange: move |e| if let Ok(v) = e.value().parse::<u8>() { draft.write().audio.offset = v },
                                        option { value: "0", "On expiration" }
                                        for s in 1..=10u8 { option { value: "{s}", "{s}s before" } }
                                    }
                                }
                                Field { label: "Voice", help: "Voice countdown starting at the specified seconds remaining",
                                    select {
                                        class: "field-short",
                                        value: "{draft().audio.countdown_start}",
                                        onchange: move |e| if let Ok(v) = e.value().parse::<u8>() { draft.write().audio.countdown_start = v },
                                        option { value: "0", "Off" }
                                        for s in 1..=10u8 { option { value: "{s}", "{s}s" } }
                                    }
                                    select {
                                        value: "{draft().audio.countdown_voice.unwrap_or_else(|| \"Amy\".to_string())}",
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

                    FormCard { icon: "fa-triangle-exclamation", title: "Advanced",
                        CheckField { label: "Timer enabled", checked: draft().enabled, on_change: move |v| draft.write().enabled = v }
                        if !draft().enabled {
                            div { class: "text-xs text-warning mt-xs",
                                i { class: "fa-solid fa-triangle-exclamation" }
                                " Disabling a timer may break phases, counters, or other timers that depend on it."
                            }
                        }
                    }

                    // Only when the Ability Queue overlay is a target
                    if !draft().is_alert && draft().display_targets.contains(&TimerDisplayTarget::AbilityQueue) {
                        AbilityQueueCard { draft, all_timers: all_timers.clone(), encounter_data: encounter_data.clone() }
                    }
                }
            }

            ItemActions {
                has_changes: has_changes(),
                origin: Origin::of(is_builtin, is_modified),
                on_save: handle_save,
                on_delete: handle_delete,
                on_duplicate: handle_duplicate,
            }
        }
    }
}

const ALERT_TEXT_HELP: &str = "Text shown in the alert notification. Defaults to timer name. {source} and {target} will show the source/target names if available";

#[component]
fn AbilityQueueCard(mut draft: Signal<BossTimerDefinition>, all_timers: Vec<BossTimerDefinition>, encounter_data: EncounterData) -> Element {
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
