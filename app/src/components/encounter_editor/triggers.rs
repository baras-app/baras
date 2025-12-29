//! Shared trigger editors
//!
//! Used by timers, phases, and counters for editing trigger conditions.

use dioxus::prelude::*;

use crate::types::{AbilitySelector, CounterTrigger, EffectSelector, PhaseTrigger, TimerTrigger};

// ─────────────────────────────────────────────────────────────────────────────
// Timer Trigger Editor
// ─────────────────────────────────────────────────────────────────────────────

/// Composable trigger editor for timer triggers
#[component]
pub fn ComposableTriggerEditor(
    trigger: TimerTrigger,
    on_change: EventHandler<TimerTrigger>,
) -> Element {
    rsx! {
        div { class: "composable-trigger-editor",
            TriggerNode {
                trigger: trigger,
                on_change: on_change,
                depth: 0,
            }
        }
    }
}

/// Recursive trigger node
#[component]
fn TriggerNode(
    trigger: TimerTrigger,
    on_change: EventHandler<TimerTrigger>,
    depth: u8,
) -> Element {
    let is_composite = matches!(trigger, TimerTrigger::AnyOf { .. });

    let trigger_for_or = trigger.clone();
    let indent = format!("padding-left: {}px;", depth as u32 * 12);

    rsx! {
        div {
            class: "trigger-node",
            style: "{indent}",

            if is_composite {
                CompositeEditor {
                    trigger: trigger.clone(),
                    on_change: on_change,
                    depth: depth,
                }
            } else {
                SimpleTriggerEditor {
                    trigger: trigger.clone(),
                    on_change: on_change,
                }
            }

            if depth == 0 && !is_composite {
                div { class: "flex gap-xs mt-sm",
                    button {
                        class: "btn-compose",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_change.call(TimerTrigger::AnyOf {
                                conditions: vec![trigger_for_or.clone()]
                            });
                        },
                        "+ OR"
                    }
                }
            }
        }
    }
}

/// Editor for composite triggers (AnyOf only)
#[component]
fn CompositeEditor(
    trigger: TimerTrigger,
    on_change: EventHandler<TimerTrigger>,
    depth: u8,
) -> Element {
    let conditions = match &trigger {
        TimerTrigger::AnyOf { conditions } => conditions.clone(),
        _ => return rsx! { span { "Invalid composite" } },
    };

    let conditions_for_unwrap = conditions.clone();
    let conditions_for_add = conditions.clone();
    let conditions_len = conditions.len();

    rsx! {
        div { class: "composite-trigger",
            div { class: "composite-header",
                span { class: "composite-label", "ANY OF (OR)" }
                if conditions_len == 1 {
                    button {
                        class: "btn-compose",
                        onclick: move |_| {
                            if let Some(first) = conditions_for_unwrap.first() {
                                on_change.call(first.clone());
                            }
                        },
                        "Unwrap"
                    }
                }
            }

            div { class: "composite-conditions",
                for (idx, condition) in conditions.iter().enumerate() {
                    {
                        let conditions_for_update = conditions.clone();
                        let conditions_for_remove = conditions.clone();
                        let condition_clone = condition.clone();

                        rsx! {
                            div { class: "condition-item",
                                TriggerNode {
                                    trigger: condition_clone,
                                    on_change: move |new_cond| {
                                        let mut new_conditions = conditions_for_update.clone();
                                        new_conditions[idx] = new_cond;
                                        on_change.call(TimerTrigger::AnyOf { conditions: new_conditions });
                                    },
                                    depth: depth + 1,
                                }
                                if conditions_len > 1 {
                                    button {
                                        class: "btn btn-danger btn-sm",
                                        onclick: move |_| {
                                            let mut new_conditions = conditions_for_remove.clone();
                                            new_conditions.remove(idx);
                                            on_change.call(TimerTrigger::AnyOf { conditions: new_conditions });
                                        },
                                        "×"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            button {
                class: "btn-dashed text-sm",
                onclick: move |_| {
                    let mut new_conditions = conditions_for_add.clone();
                    new_conditions.push(TimerTrigger::CombatStart);
                    on_change.call(TimerTrigger::AnyOf { conditions: new_conditions });
                },
                "+ Add Condition"
            }
        }
    }
}

/// Editor for simple (non-composite) triggers
#[component]
pub fn SimpleTriggerEditor(
    trigger: TimerTrigger,
    on_change: EventHandler<TimerTrigger>,
) -> Element {
    let trigger_type = trigger.type_name();

    rsx! {
        div { class: "flex-col gap-xs",
            select {
                class: "select",
                style: "width: 180px;",
                value: "{trigger_type}",
                onchange: move |e| {
                    let new_trigger = match e.value().as_str() {
                        "combat_start" => TimerTrigger::CombatStart,
                        "ability_cast" => TimerTrigger::AbilityCast { abilities: vec![] },
                        "effect_applied" => TimerTrigger::EffectApplied { effects: vec![] },
                        "effect_removed" => TimerTrigger::EffectRemoved { effects: vec![] },
                        "timer_expires" => TimerTrigger::TimerExpires { timer_id: String::new() },
                        "timer_started" => TimerTrigger::TimerStarted { timer_id: String::new() },
                        "phase_entered" => TimerTrigger::PhaseEntered { phase_id: String::new() },
                        "phase_ended" => TimerTrigger::PhaseEnded { phase_id: String::new() },
                        "boss_hp_threshold" => TimerTrigger::BossHpThreshold { hp_percent: 50.0, npc_id: None, boss_name: None },
                        "counter_reaches" => TimerTrigger::CounterReaches { counter_id: String::new(), value: 1 },
                        "entity_first_seen" => TimerTrigger::EntityFirstSeen { entity: None, npc_id: None, entity_name: None },
                        "entity_death" => TimerTrigger::EntityDeath { entity: None, npc_id: None, entity_name: None },
                        "target_set" => TimerTrigger::TargetSet { entity: None, npc_id: None, entity_name: None },
                        "time_elapsed" => TimerTrigger::TimeElapsed { secs: 30.0 },
                        "manual" => TimerTrigger::Manual,
                        _ => trigger.clone(),
                    };
                    on_change.call(new_trigger);
                },
                option { value: "combat_start", "Combat Start" }
                option { value: "ability_cast", "Ability Cast" }
                option { value: "effect_applied", "Effect Applied" }
                option { value: "effect_removed", "Effect Removed" }
                option { value: "timer_expires", "Timer Expires" }
                option { value: "timer_started", "Timer Started" }
                option { value: "phase_entered", "Phase Entered" }
                option { value: "phase_ended", "Phase Ended" }
                option { value: "boss_hp_threshold", "Boss HP Threshold" }
                option { value: "counter_reaches", "Counter Reaches" }
                option { value: "entity_first_seen", "Entity First Seen" }
                option { value: "entity_death", "Entity Death" }
                option { value: "target_set", "Target Set" }
                option { value: "time_elapsed", "Time Elapsed" }
                option { value: "manual", "Manual" }
            }

            // Type-specific fields
            {
                match trigger.clone() {
                    TimerTrigger::CombatStart => rsx! {},
                    TimerTrigger::Manual => rsx! {},
                    TimerTrigger::AbilityCast { abilities } => rsx! {
                        AbilitySelectorEditor {
                            label: "Abilities",
                            selectors: abilities,
                            on_change: move |sels| on_change.call(TimerTrigger::AbilityCast { abilities: sels })
                        }
                    },
                    TimerTrigger::EffectApplied { effects } => rsx! {
                        EffectSelectorEditor {
                            label: "Effects",
                            selectors: effects,
                            on_change: move |sels| on_change.call(TimerTrigger::EffectApplied { effects: sels })
                        }
                    },
                    TimerTrigger::EffectRemoved { effects } => rsx! {
                        EffectSelectorEditor {
                            label: "Effects",
                            selectors: effects,
                            on_change: move |sels| on_change.call(TimerTrigger::EffectRemoved { effects: sels })
                        }
                    },
                    TimerTrigger::TimerExpires { timer_id } => rsx! {
                        div { class: "flex items-center gap-xs",
                            label { class: "text-sm text-secondary", "Timer ID" }
                            input {
                                r#type: "text",
                                class: "input-inline flex-1",
                                value: "{timer_id}",
                                oninput: move |e| on_change.call(TimerTrigger::TimerExpires { timer_id: e.value() })
                            }
                        }
                    },
                    TimerTrigger::TimerStarted { timer_id } => rsx! {
                        div { class: "flex items-center gap-xs",
                            label { class: "text-sm text-secondary", "Timer ID" }
                            input {
                                r#type: "text",
                                class: "input-inline flex-1",
                                value: "{timer_id}",
                                oninput: move |e| on_change.call(TimerTrigger::TimerStarted { timer_id: e.value() })
                            }
                        }
                    },
                    TimerTrigger::PhaseEntered { phase_id } => rsx! {
                        div { class: "flex items-center gap-xs",
                            label { class: "text-sm text-secondary", "Phase ID" }
                            input {
                                r#type: "text",
                                class: "input-inline flex-1",
                                value: "{phase_id}",
                                oninput: move |e| on_change.call(TimerTrigger::PhaseEntered { phase_id: e.value() })
                            }
                        }
                    },
                    TimerTrigger::PhaseEnded { phase_id } => rsx! {
                        div { class: "flex items-center gap-xs",
                            label { class: "text-sm text-secondary", "Phase ID" }
                            input {
                                r#type: "text",
                                class: "input-inline flex-1",
                                value: "{phase_id}",
                                oninput: move |e| on_change.call(TimerTrigger::PhaseEnded { phase_id: e.value() })
                            }
                        }
                    },
                    TimerTrigger::BossHpThreshold { hp_percent, npc_id, boss_name } => rsx! {
                        div { class: "flex-col gap-xs",
                            div { class: "flex items-center gap-xs",
                                label { class: "text-sm text-secondary", "HP %" }
                                input {
                                    r#type: "number",
                                    step: "0.1",
                                    min: "0",
                                    max: "100",
                                    class: "input-inline",
                                    style: "width: 70px;",
                                    value: "{hp_percent}",
                                    oninput: move |e| {
                                        if let Ok(val) = e.value().parse::<f32>() {
                                            on_change.call(TimerTrigger::BossHpThreshold {
                                                hp_percent: val,
                                                npc_id,
                                                boss_name: boss_name.clone(),
                                            });
                                        }
                                    }
                                }
                            }
                            span { class: "hint", "Triggers when HP drops below threshold" }
                        }
                    },
                    TimerTrigger::CounterReaches { counter_id, value } => rsx! {
                        div { class: "flex-col gap-xs",
                            div { class: "flex items-center gap-xs",
                                label { class: "text-sm text-secondary", "Counter" }
                                input {
                                    r#type: "text",
                                    class: "input-inline flex-1",
                                    placeholder: "counter_id",
                                    value: "{counter_id}",
                                    oninput: move |e| on_change.call(TimerTrigger::CounterReaches {
                                        counter_id: e.value(),
                                        value
                                    })
                                }
                            }
                            div { class: "flex items-center gap-xs",
                                label { class: "text-sm text-secondary", "Value" }
                                input {
                                    r#type: "number",
                                    min: "0",
                                    class: "input-inline",
                                    style: "width: 70px;",
                                    value: "{value}",
                                    oninput: {
                                        let counter_id = counter_id.clone();
                                        move |e| {
                                            if let Ok(val) = e.value().parse::<u32>() {
                                                on_change.call(TimerTrigger::CounterReaches {
                                                    counter_id: counter_id.clone(),
                                                    value: val
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    TimerTrigger::EntityFirstSeen { entity, npc_id, entity_name } => rsx! {
                        EntityTriggerFields {
                            hint: "First Seen",
                            entity: entity,
                            npc_id: npc_id,
                            entity_name: entity_name,
                            on_change: move |(e, n, en)| on_change.call(TimerTrigger::EntityFirstSeen {
                                entity: e, npc_id: n, entity_name: en
                            })
                        }
                    },
                    TimerTrigger::EntityDeath { entity, npc_id, entity_name } => rsx! {
                        EntityTriggerFields {
                            hint: "Death",
                            entity: entity,
                            npc_id: npc_id,
                            entity_name: entity_name,
                            on_change: move |(e, n, en)| on_change.call(TimerTrigger::EntityDeath {
                                entity: e, npc_id: n, entity_name: en
                            })
                        }
                    },
                    TimerTrigger::TargetSet { entity, npc_id, entity_name } => rsx! {
                        EntityTriggerFields {
                            hint: "Target Set",
                            entity: entity,
                            npc_id: npc_id,
                            entity_name: entity_name,
                            on_change: move |(e, n, en)| on_change.call(TimerTrigger::TargetSet {
                                entity: e, npc_id: n, entity_name: en
                            })
                        }
                    },
                    TimerTrigger::TimeElapsed { secs } => rsx! {
                        div { class: "flex items-center gap-xs",
                            label { class: "text-sm text-secondary", "Seconds" }
                            input {
                                r#type: "number",
                                step: "0.1",
                                min: "0",
                                class: "input-inline",
                                style: "width: 80px;",
                                value: "{secs}",
                                oninput: move |e| {
                                    if let Ok(val) = e.value().parse::<f32>() {
                                        on_change.call(TimerTrigger::TimeElapsed { secs: val });
                                    }
                                }
                            }
                            span { class: "hint", "into combat" }
                        }
                    },
                    _ => rsx! {
                        span { class: "hint", "Composite trigger" }
                    },
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Entity Trigger Fields (shared by EntityFirstSeen, EntityDeath, TargetSet)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn EntityTriggerFields(
    hint: &'static str,
    entity: Option<String>,
    npc_id: Option<i64>,
    entity_name: Option<String>,
    on_change: EventHandler<(Option<String>, Option<i64>, Option<String>)>,
) -> Element {
    rsx! {
        div { class: "flex-col gap-xs",
            span { class: "hint", "Entity {hint}" }

            div { class: "flex items-center gap-xs",
                label { class: "text-sm text-secondary", "Entity" }
                input {
                    r#type: "text",
                    class: "input-inline flex-1",
                    placeholder: "From roster",
                    value: "{entity.clone().unwrap_or_default()}",
                    oninput: {
                        let npc_id = npc_id;
                        let entity_name = entity_name.clone();
                        move |e| {
                            let val = if e.value().is_empty() { None } else { Some(e.value()) };
                            on_change.call((val, npc_id, entity_name.clone()));
                        }
                    }
                }
            }

            div { class: "flex items-center gap-xs",
                label { class: "text-sm text-secondary", "NPC ID" }
                input {
                    r#type: "text",
                    class: "input-inline flex-1",
                    placeholder: "Optional",
                    value: "{npc_id.map(|v| v.to_string()).unwrap_or_default()}",
                    oninput: {
                        let entity = entity.clone();
                        let entity_name = entity_name.clone();
                        move |e| {
                            let val = e.value().parse::<i64>().ok();
                            on_change.call((entity.clone(), val, entity_name.clone()));
                        }
                    }
                }
            }

            div { class: "flex items-center gap-xs",
                label { class: "text-sm text-secondary", "Name" }
                input {
                    r#type: "text",
                    class: "input-inline flex-1",
                    placeholder: "Fallback",
                    value: "{entity_name.clone().unwrap_or_default()}",
                    oninput: {
                        let entity = entity.clone();
                        let npc_id = npc_id;
                        move |e| {
                            let val = if e.value().is_empty() { None } else { Some(e.value()) };
                            on_change.call((entity.clone(), npc_id, val));
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Selector List Editors (for ability/effect IDs or names)
// ─────────────────────────────────────────────────────────────────────────────

/// Editor for a list of effect selectors (ID or name)
#[component]
pub fn EffectSelectorEditor(
    label: &'static str,
    selectors: Vec<EffectSelector>,
    on_change: EventHandler<Vec<EffectSelector>>,
) -> Element {
    let mut new_input = use_signal(String::new);

    let selectors_for_keydown = selectors.clone();
    let selectors_for_click = selectors.clone();

    rsx! {
        div { class: "flex-col gap-xs items-start",
            span { class: "text-sm text-secondary text-left", "{label}:" }

            // Selector chips
            div { class: "flex flex-wrap gap-xs",
                for (idx, sel) in selectors.iter().enumerate() {
                    {
                        let selectors_clone = selectors.clone();
                        let display = sel.display();
                        rsx! {
                            span { class: "chip",
                                "{display}"
                                button {
                                    class: "chip-remove",
                                    onclick: move |_| {
                                        let mut new_sels = selectors_clone.clone();
                                        new_sels.remove(idx);
                                        on_change.call(new_sels);
                                    },
                                    "×"
                                }
                            }
                        }
                    }
                }
            }

            // Add new selector
            div { class: "flex gap-xs",
                input {
                    r#type: "text",
                    class: "input-inline",
                    style: "width: 180px;",
                    placeholder: "ID or Name (Enter)",
                    value: "{new_input}",
                    oninput: move |e| new_input.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter && !new_input().trim().is_empty() {
                            let selector = EffectSelector::from_input(&new_input());
                            let mut new_sels = selectors_for_keydown.clone();
                            if !new_sels.contains(&selector) {
                                new_sels.push(selector);
                                on_change.call(new_sels);
                            }
                            new_input.set(String::new());
                        }
                    }
                }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| {
                        if !new_input().trim().is_empty() {
                            let selector = EffectSelector::from_input(&new_input());
                            let mut new_sels = selectors_for_click.clone();
                            if !new_sels.contains(&selector) {
                                new_sels.push(selector);
                                on_change.call(new_sels);
                            }
                            new_input.set(String::new());
                        }
                    },
                    "Add"
                }
            }
        }
    }
}

/// Editor for a list of ability selectors (ID or name)
#[component]
pub fn AbilitySelectorEditor(
    label: &'static str,
    selectors: Vec<AbilitySelector>,
    on_change: EventHandler<Vec<AbilitySelector>>,
) -> Element {
    let mut new_input = use_signal(String::new);

    let selectors_for_keydown = selectors.clone();
    let selectors_for_click = selectors.clone();

    rsx! {
        div { class: "flex-col gap-xs items-start",
            span { class: "text-sm text-secondary text-left", "{label}:" }

            // Selector chips
            div { class: "flex flex-wrap gap-xs",
                for (idx, sel) in selectors.iter().enumerate() {
                    {
                        let selectors_clone = selectors.clone();
                        let display = sel.display();
                        rsx! {
                            span { class: "chip",
                                "{display}"
                                button {
                                    class: "chip-remove",
                                    onclick: move |_| {
                                        let mut new_sels = selectors_clone.clone();
                                        new_sels.remove(idx);
                                        on_change.call(new_sels);
                                    },
                                    "×"
                                }
                            }
                        }
                    }
                }
            }

            // Add new selector
            div { class: "flex gap-xs",
                input {
                    r#type: "text",
                    class: "input-inline",
                    style: "width: 180px;",
                    placeholder: "ID or Name (Enter)",
                    value: "{new_input}",
                    oninput: move |e| new_input.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter && !new_input().trim().is_empty() {
                            let selector = AbilitySelector::from_input(&new_input());
                            let mut new_sels = selectors_for_keydown.clone();
                            if !new_sels.contains(&selector) {
                                new_sels.push(selector);
                                on_change.call(new_sels);
                            }
                            new_input.set(String::new());
                        }
                    }
                }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| {
                        if !new_input().trim().is_empty() {
                            let selector = AbilitySelector::from_input(&new_input());
                            let mut new_sels = selectors_for_click.clone();
                            if !new_sels.contains(&selector) {
                                new_sels.push(selector);
                                on_change.call(new_sels);
                            }
                            new_input.set(String::new());
                        }
                    },
                    "Add"
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Phase Trigger Editor
// ─────────────────────────────────────────────────────────────────────────────

/// Composable trigger editor for phase triggers
#[component]
pub fn PhaseTriggerEditor(
    trigger: PhaseTrigger,
    on_change: EventHandler<PhaseTrigger>,
) -> Element {
    rsx! {
        div { class: "composable-trigger-editor",
            PhaseTriggerNode {
                trigger: trigger,
                on_change: on_change,
                depth: 0,
            }
        }
    }
}

/// Recursive phase trigger node
#[component]
fn PhaseTriggerNode(
    trigger: PhaseTrigger,
    on_change: EventHandler<PhaseTrigger>,
    depth: u8,
) -> Element {
    let is_composite = matches!(trigger, PhaseTrigger::AnyOf { .. });
    let trigger_for_or = trigger.clone();
    let indent = format!("padding-left: {}px;", depth as u32 * 12);

    rsx! {
        div {
            class: "trigger-node",
            style: "{indent}",

            if is_composite {
                PhaseCompositeEditor {
                    trigger: trigger.clone(),
                    on_change: on_change,
                    depth: depth,
                }
            } else {
                SimplePhaseTriggerEditor {
                    trigger: trigger.clone(),
                    on_change: on_change,
                }
            }

            if depth == 0 && !is_composite {
                div { class: "flex gap-xs mt-sm",
                    button {
                        class: "btn-compose",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_change.call(PhaseTrigger::AnyOf {
                                conditions: vec![trigger_for_or.clone()]
                            });
                        },
                        "+ OR"
                    }
                }
            }
        }
    }
}

/// Editor for composite phase triggers (AnyOf)
#[component]
fn PhaseCompositeEditor(
    trigger: PhaseTrigger,
    on_change: EventHandler<PhaseTrigger>,
    depth: u8,
) -> Element {
    let conditions = match &trigger {
        PhaseTrigger::AnyOf { conditions } => conditions.clone(),
        _ => return rsx! { span { "Invalid composite" } },
    };

    let conditions_for_unwrap = conditions.clone();
    let conditions_for_add = conditions.clone();
    let conditions_len = conditions.len();

    rsx! {
        div { class: "composite-trigger",
            div { class: "composite-header",
                span { class: "composite-label", "ANY OF (OR)" }
                if conditions_len == 1 {
                    button {
                        class: "btn-compose",
                        onclick: move |_| {
                            if let Some(first) = conditions_for_unwrap.first() {
                                on_change.call(first.clone());
                            }
                        },
                        "Unwrap"
                    }
                }
            }

            div { class: "composite-conditions",
                for (idx, condition) in conditions.iter().enumerate() {
                    {
                        let conditions_for_update = conditions.clone();
                        let conditions_for_remove = conditions.clone();
                        let condition_clone = condition.clone();

                        rsx! {
                            div { class: "condition-item",
                                PhaseTriggerNode {
                                    trigger: condition_clone,
                                    on_change: move |new_cond| {
                                        let mut new_conditions = conditions_for_update.clone();
                                        new_conditions[idx] = new_cond;
                                        on_change.call(PhaseTrigger::AnyOf { conditions: new_conditions });
                                    },
                                    depth: depth + 1,
                                }
                                if conditions_len > 1 {
                                    button {
                                        class: "btn btn-danger btn-sm",
                                        onclick: move |_| {
                                            let mut new_conditions = conditions_for_remove.clone();
                                            new_conditions.remove(idx);
                                            on_change.call(PhaseTrigger::AnyOf { conditions: new_conditions });
                                        },
                                        "×"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            button {
                class: "btn-dashed text-sm",
                onclick: move |_| {
                    let mut new_conditions = conditions_for_add.clone();
                    new_conditions.push(PhaseTrigger::CombatStart);
                    on_change.call(PhaseTrigger::AnyOf { conditions: new_conditions });
                },
                "+ Add Condition"
            }
        }
    }
}

/// Editor for simple phase triggers
#[component]
fn SimplePhaseTriggerEditor(
    trigger: PhaseTrigger,
    on_change: EventHandler<PhaseTrigger>,
) -> Element {
    let trigger_type = trigger.type_name();

    rsx! {
        div { class: "flex-col gap-xs",
            select {
                class: "select",
                style: "width: 180px;",
                value: "{trigger_type}",
                onchange: move |e| {
                    let new_trigger = match e.value().as_str() {
                        "combat_start" => PhaseTrigger::CombatStart,
                        "boss_hp_below" => PhaseTrigger::BossHpBelow {
                            hp_percent: 50.0,
                            entity: None,
                            npc_id: None,
                            boss_name: None,
                        },
                        "boss_hp_above" => PhaseTrigger::BossHpAbove {
                            hp_percent: 50.0,
                            entity: None,
                            npc_id: None,
                            boss_name: None,
                        },
                        "ability_cast" => PhaseTrigger::AbilityCast { abilities: vec![] },
                        "effect_applied" => PhaseTrigger::EffectApplied { effects: vec![] },
                        "effect_removed" => PhaseTrigger::EffectRemoved { effects: vec![] },
                        "counter_reaches" => PhaseTrigger::CounterReaches {
                            counter_id: String::new(),
                            value: 1,
                        },
                        "time_elapsed" => PhaseTrigger::TimeElapsed { secs: 30.0 },
                        "entity_first_seen" => PhaseTrigger::EntityFirstSeen {
                            entity: None,
                            npc_id: None,
                            entity_name: None,
                        },
                        "entity_death" => PhaseTrigger::EntityDeath {
                            entity: None,
                            npc_id: None,
                            entity_name: None,
                        },
                        "phase_ended" => PhaseTrigger::PhaseEnded {
                            phase_id: None,
                            phase_ids: vec![],
                        },
                        _ => trigger.clone(),
                    };
                    on_change.call(new_trigger);
                },
                option { value: "combat_start", "Combat Start" }
                option { value: "boss_hp_below", "Boss HP Below" }
                option { value: "boss_hp_above", "Boss HP Above" }
                option { value: "ability_cast", "Ability Cast" }
                option { value: "effect_applied", "Effect Applied" }
                option { value: "effect_removed", "Effect Removed" }
                option { value: "counter_reaches", "Counter Reaches" }
                option { value: "time_elapsed", "Time Elapsed" }
                option { value: "entity_first_seen", "Entity First Seen" }
                option { value: "entity_death", "Entity Death" }
                option { value: "phase_ended", "Phase Ended" }
            }

            // Type-specific fields
            {
                match trigger.clone() {
                    PhaseTrigger::CombatStart => rsx! {},
                    PhaseTrigger::BossHpBelow { hp_percent, entity, npc_id, boss_name } => rsx! {
                        BossHpFields {
                            label: "Below",
                            hp_percent: hp_percent,
                            entity: entity,
                            npc_id: npc_id,
                            boss_name: boss_name,
                            on_change: move |(hp, e, n, bn)| on_change.call(PhaseTrigger::BossHpBelow {
                                hp_percent: hp, entity: e, npc_id: n, boss_name: bn,
                            })
                        }
                    },
                    PhaseTrigger::BossHpAbove { hp_percent, entity, npc_id, boss_name } => rsx! {
                        BossHpFields {
                            label: "Above",
                            hp_percent: hp_percent,
                            entity: entity,
                            npc_id: npc_id,
                            boss_name: boss_name,
                            on_change: move |(hp, e, n, bn)| on_change.call(PhaseTrigger::BossHpAbove {
                                hp_percent: hp, entity: e, npc_id: n, boss_name: bn,
                            })
                        }
                    },
                    PhaseTrigger::AbilityCast { abilities } => rsx! {
                        AbilitySelectorEditor {
                            label: "Abilities",
                            selectors: abilities,
                            on_change: move |sels| on_change.call(PhaseTrigger::AbilityCast { abilities: sels })
                        }
                    },
                    PhaseTrigger::EffectApplied { effects } => rsx! {
                        EffectSelectorEditor {
                            label: "Effects",
                            selectors: effects,
                            on_change: move |sels| on_change.call(PhaseTrigger::EffectApplied { effects: sels })
                        }
                    },
                    PhaseTrigger::EffectRemoved { effects } => rsx! {
                        EffectSelectorEditor {
                            label: "Effects",
                            selectors: effects,
                            on_change: move |sels| on_change.call(PhaseTrigger::EffectRemoved { effects: sels })
                        }
                    },
                    PhaseTrigger::CounterReaches { counter_id, value } => rsx! {
                        div { class: "flex-col gap-xs",
                            div { class: "flex items-center gap-xs",
                                label { class: "text-sm text-secondary", "Counter" }
                                input {
                                    r#type: "text",
                                    class: "input-inline flex-1",
                                    placeholder: "counter_id",
                                    value: "{counter_id}",
                                    oninput: move |e| on_change.call(PhaseTrigger::CounterReaches {
                                        counter_id: e.value(),
                                        value
                                    })
                                }
                            }
                            div { class: "flex items-center gap-xs",
                                label { class: "text-sm text-secondary", "Value" }
                                input {
                                    r#type: "number",
                                    min: "0",
                                    class: "input-inline",
                                    style: "width: 70px;",
                                    value: "{value}",
                                    oninput: {
                                        let counter_id = counter_id.clone();
                                        move |e| {
                                            if let Ok(val) = e.value().parse::<u32>() {
                                                on_change.call(PhaseTrigger::CounterReaches {
                                                    counter_id: counter_id.clone(),
                                                    value: val
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },
                    PhaseTrigger::TimeElapsed { secs } => rsx! {
                        div { class: "flex items-center gap-xs",
                            label { class: "text-sm text-secondary", "Seconds" }
                            input {
                                r#type: "number",
                                step: "0.1",
                                min: "0",
                                class: "input-inline",
                                style: "width: 80px;",
                                value: "{secs}",
                                oninput: move |e| {
                                    if let Ok(val) = e.value().parse::<f32>() {
                                        on_change.call(PhaseTrigger::TimeElapsed { secs: val });
                                    }
                                }
                            }
                            span { class: "hint", "into combat" }
                        }
                    },
                    PhaseTrigger::EntityFirstSeen { entity, npc_id, entity_name } => rsx! {
                        EntityTriggerFields {
                            hint: "First Seen",
                            entity: entity,
                            npc_id: npc_id,
                            entity_name: entity_name,
                            on_change: move |(e, n, en)| on_change.call(PhaseTrigger::EntityFirstSeen {
                                entity: e, npc_id: n, entity_name: en
                            })
                        }
                    },
                    PhaseTrigger::EntityDeath { entity, npc_id, entity_name } => rsx! {
                        EntityTriggerFields {
                            hint: "Death",
                            entity: entity,
                            npc_id: npc_id,
                            entity_name: entity_name,
                            on_change: move |(e, n, en)| on_change.call(PhaseTrigger::EntityDeath {
                                entity: e, npc_id: n, entity_name: en
                            })
                        }
                    },
                    PhaseTrigger::PhaseEnded { phase_id, phase_ids } => rsx! {
                        div { class: "flex items-center gap-xs",
                            label { class: "text-sm text-secondary", "Phase ID" }
                            input {
                                r#type: "text",
                                class: "input-inline flex-1",
                                value: "{phase_id.clone().unwrap_or_default()}",
                                oninput: move |e| on_change.call(PhaseTrigger::PhaseEnded {
                                    phase_id: if e.value().is_empty() { None } else { Some(e.value()) },
                                    phase_ids: phase_ids.clone()
                                })
                            }
                        }
                    },
                    _ => rsx! {
                        span { class: "hint", "Composite trigger" }
                    },
                }
            }
        }
    }
}

/// Boss HP threshold fields (shared by BossHpBelow/BossHpAbove)
#[component]
fn BossHpFields(
    label: &'static str,
    hp_percent: f32,
    entity: Option<String>,
    npc_id: Option<i64>,
    boss_name: Option<String>,
    on_change: EventHandler<(f32, Option<String>, Option<i64>, Option<String>)>,
) -> Element {
    rsx! {
        div { class: "flex-col gap-xs",
            div { class: "flex items-center gap-xs",
                label { class: "text-sm text-secondary", "HP % {label}" }
                input {
                    r#type: "number",
                    step: "0.1",
                    min: "0",
                    max: "100",
                    class: "input-inline",
                    style: "width: 70px;",
                    value: "{hp_percent}",
                    oninput: {
                        let entity = entity.clone();
                        let boss_name = boss_name.clone();
                        move |e| {
                            if let Ok(val) = e.value().parse::<f32>() {
                                on_change.call((val, entity.clone(), npc_id, boss_name.clone()));
                            }
                        }
                    }
                }
            }

            div { class: "flex items-center gap-xs",
                label { class: "text-sm text-secondary", "Entity" }
                input {
                    r#type: "text",
                    class: "input-inline flex-1",
                    placeholder: "From roster",
                    value: "{entity.clone().unwrap_or_default()}",
                    oninput: {
                        let npc_id = npc_id;
                        let boss_name = boss_name.clone();
                        move |e| {
                            let val = if e.value().is_empty() { None } else { Some(e.value()) };
                            on_change.call((hp_percent, val, npc_id, boss_name.clone()));
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Counter Trigger Editor
// ─────────────────────────────────────────────────────────────────────────────

/// Simple counter trigger editor (no composite support for counters)
#[component]
pub fn CounterTriggerEditor(
    trigger: CounterTrigger,
    on_change: EventHandler<CounterTrigger>,
) -> Element {
    let trigger_type = trigger.type_name();

    rsx! {
        div { class: "flex-col gap-xs",
            select {
                class: "select",
                style: "width: 180px;",
                value: "{trigger_type}",
                onchange: move |e| {
                    let new_trigger = match e.value().as_str() {
                        "combat_start" => CounterTrigger::CombatStart,
                        "combat_end" => CounterTrigger::CombatEnd,
                        "ability_cast" => CounterTrigger::AbilityCast {
                            abilities: vec![],
                            source: None,
                        },
                        "effect_applied" => CounterTrigger::EffectApplied {
                            effects: vec![],
                            target: None,
                        },
                        "effect_removed" => CounterTrigger::EffectRemoved {
                            effects: vec![],
                            target: None,
                        },
                        "timer_expires" => CounterTrigger::TimerExpires {
                            timer_id: String::new(),
                        },
                        "timer_starts" => CounterTrigger::TimerStarts {
                            timer_id: String::new(),
                        },
                        "phase_entered" => CounterTrigger::PhaseEntered {
                            phase_id: String::new(),
                        },
                        "phase_ended" => CounterTrigger::PhaseEnded {
                            phase_id: String::new(),
                        },
                        "any_phase_change" => CounterTrigger::AnyPhaseChange,
                        "entity_first_seen" => CounterTrigger::EntityFirstSeen {
                            entity: None,
                            npc_id: None,
                            entity_name: None,
                        },
                        "entity_death" => CounterTrigger::EntityDeath {
                            entity: None,
                            npc_id: None,
                            entity_name: None,
                        },
                        "counter_reaches" => CounterTrigger::CounterReaches {
                            counter_id: String::new(),
                            value: 1,
                        },
                        "boss_hp_below" => CounterTrigger::BossHpBelow {
                            hp_percent: 50.0,
                            entity: None,
                            boss_name: None,
                        },
                        "never" => CounterTrigger::Never,
                        _ => trigger.clone(),
                    };
                    on_change.call(new_trigger);
                },
                option { value: "combat_start", "Combat Start" }
                option { value: "combat_end", "Combat End" }
                option { value: "ability_cast", "Ability Cast" }
                option { value: "effect_applied", "Effect Applied" }
                option { value: "effect_removed", "Effect Removed" }
                option { value: "timer_expires", "Timer Expires" }
                option { value: "timer_starts", "Timer Starts" }
                option { value: "phase_entered", "Phase Entered" }
                option { value: "phase_ended", "Phase Ended" }
                option { value: "any_phase_change", "Any Phase Change" }
                option { value: "entity_first_seen", "Entity First Seen" }
                option { value: "entity_death", "Entity Death" }
                option { value: "counter_reaches", "Counter Reaches" }
                option { value: "boss_hp_below", "Boss HP Below" }
                option { value: "never", "Never" }
            }

            // Type-specific fields
            {
                match trigger.clone() {
                    CounterTrigger::CombatStart | CounterTrigger::CombatEnd
                    | CounterTrigger::AnyPhaseChange | CounterTrigger::Never => rsx! {},

                    CounterTrigger::AbilityCast { abilities, source } => {
                        let source_for_sels = source.clone();
                        let source_display = source.clone().unwrap_or_default();
                        rsx! {
                            AbilitySelectorEditor {
                                label: "Abilities",
                                selectors: abilities.clone(),
                                on_change: move |sels| on_change.call(CounterTrigger::AbilityCast {
                                    abilities: sels,
                                    source: source_for_sels.clone(),
                                })
                            }
                            div { class: "flex items-center gap-xs",
                                label { class: "text-sm text-secondary", "Source" }
                                input {
                                    r#type: "text",
                                    class: "input-inline flex-1",
                                    placeholder: "Entity (optional)",
                                    value: "{source_display}",
                                    oninput: {
                                        let abilities = abilities.clone();
                                        move |e| on_change.call(CounterTrigger::AbilityCast {
                                            abilities: abilities.clone(),
                                            source: if e.value().is_empty() { None } else { Some(e.value()) },
                                        })
                                    }
                                }
                            }
                        }
                    },

                    CounterTrigger::EffectApplied { effects, target } => {
                        let target_for_sels = target.clone();
                        let target_display = target.clone().unwrap_or_default();
                        rsx! {
                            EffectSelectorEditor {
                                label: "Effects",
                                selectors: effects.clone(),
                                on_change: move |sels| on_change.call(CounterTrigger::EffectApplied {
                                    effects: sels,
                                    target: target_for_sels.clone(),
                                })
                            }
                            div { class: "flex items-center gap-xs",
                                label { class: "text-sm text-secondary", "Target" }
                                input {
                                    r#type: "text",
                                    class: "input-inline flex-1",
                                    placeholder: "Entity (optional)",
                                    value: "{target_display}",
                                    oninput: {
                                        let effects = effects.clone();
                                        move |e| on_change.call(CounterTrigger::EffectApplied {
                                            effects: effects.clone(),
                                            target: if e.value().is_empty() { None } else { Some(e.value()) },
                                        })
                                    }
                                }
                            }
                        }
                    },

                    CounterTrigger::EffectRemoved { effects, target } => {
                        let target_for_sels = target.clone();
                        let target_display = target.clone().unwrap_or_default();
                        rsx! {
                            EffectSelectorEditor {
                                label: "Effects",
                                selectors: effects.clone(),
                                on_change: move |sels| on_change.call(CounterTrigger::EffectRemoved {
                                    effects: sels,
                                    target: target_for_sels.clone(),
                                })
                            }
                            div { class: "flex items-center gap-xs",
                                label { class: "text-sm text-secondary", "Target" }
                                input {
                                    r#type: "text",
                                    class: "input-inline flex-1",
                                    placeholder: "Entity (optional)",
                                    value: "{target_display}",
                                    oninput: {
                                        let effects = effects.clone();
                                        move |e| on_change.call(CounterTrigger::EffectRemoved {
                                            effects: effects.clone(),
                                            target: if e.value().is_empty() { None } else { Some(e.value()) },
                                        })
                                    }
                                }
                            }
                        }
                    },

                    CounterTrigger::TimerExpires { timer_id } => rsx! {
                        div { class: "flex items-center gap-xs",
                            label { class: "text-sm text-secondary", "Timer ID" }
                            input {
                                r#type: "text",
                                class: "input-inline flex-1",
                                value: "{timer_id}",
                                oninput: move |e| on_change.call(CounterTrigger::TimerExpires { timer_id: e.value() })
                            }
                        }
                    },

                    CounterTrigger::TimerStarts { timer_id } => rsx! {
                        div { class: "flex items-center gap-xs",
                            label { class: "text-sm text-secondary", "Timer ID" }
                            input {
                                r#type: "text",
                                class: "input-inline flex-1",
                                value: "{timer_id}",
                                oninput: move |e| on_change.call(CounterTrigger::TimerStarts { timer_id: e.value() })
                            }
                        }
                    },

                    CounterTrigger::PhaseEntered { phase_id } => rsx! {
                        div { class: "flex items-center gap-xs",
                            label { class: "text-sm text-secondary", "Phase ID" }
                            input {
                                r#type: "text",
                                class: "input-inline flex-1",
                                value: "{phase_id}",
                                oninput: move |e| on_change.call(CounterTrigger::PhaseEntered { phase_id: e.value() })
                            }
                        }
                    },

                    CounterTrigger::PhaseEnded { phase_id } => rsx! {
                        div { class: "flex items-center gap-xs",
                            label { class: "text-sm text-secondary", "Phase ID" }
                            input {
                                r#type: "text",
                                class: "input-inline flex-1",
                                value: "{phase_id}",
                                oninput: move |e| on_change.call(CounterTrigger::PhaseEnded { phase_id: e.value() })
                            }
                        }
                    },

                    CounterTrigger::EntityFirstSeen { entity, npc_id, entity_name } => rsx! {
                        EntityTriggerFields {
                            hint: "First Seen",
                            entity: entity,
                            npc_id: npc_id,
                            entity_name: entity_name,
                            on_change: move |(e, n, en)| on_change.call(CounterTrigger::EntityFirstSeen {
                                entity: e, npc_id: n, entity_name: en
                            })
                        }
                    },

                    CounterTrigger::EntityDeath { entity, npc_id, entity_name } => rsx! {
                        EntityTriggerFields {
                            hint: "Death",
                            entity: entity,
                            npc_id: npc_id,
                            entity_name: entity_name,
                            on_change: move |(e, n, en)| on_change.call(CounterTrigger::EntityDeath {
                                entity: e, npc_id: n, entity_name: en
                            })
                        }
                    },

                    CounterTrigger::CounterReaches { counter_id, value } => rsx! {
                        div { class: "flex-col gap-xs",
                            div { class: "flex items-center gap-xs",
                                label { class: "text-sm text-secondary", "Counter" }
                                input {
                                    r#type: "text",
                                    class: "input-inline flex-1",
                                    placeholder: "counter_id",
                                    value: "{counter_id}",
                                    oninput: move |e| on_change.call(CounterTrigger::CounterReaches {
                                        counter_id: e.value(),
                                        value
                                    })
                                }
                            }
                            div { class: "flex items-center gap-xs",
                                label { class: "text-sm text-secondary", "Value" }
                                input {
                                    r#type: "number",
                                    min: "0",
                                    class: "input-inline",
                                    style: "width: 70px;",
                                    value: "{value}",
                                    oninput: {
                                        let counter_id = counter_id.clone();
                                        move |e| {
                                            if let Ok(val) = e.value().parse::<u32>() {
                                                on_change.call(CounterTrigger::CounterReaches {
                                                    counter_id: counter_id.clone(),
                                                    value: val
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    },

                    CounterTrigger::BossHpBelow { hp_percent, entity, boss_name } => rsx! {
                        div { class: "flex-col gap-xs",
                            div { class: "flex items-center gap-xs",
                                label { class: "text-sm text-secondary", "HP % Below" }
                                input {
                                    r#type: "number",
                                    step: "0.1",
                                    min: "0",
                                    max: "100",
                                    class: "input-inline",
                                    style: "width: 70px;",
                                    value: "{hp_percent}",
                                    oninput: {
                                        let entity = entity.clone();
                                        let boss_name = boss_name.clone();
                                        move |e| {
                                            if let Ok(val) = e.value().parse::<f32>() {
                                                on_change.call(CounterTrigger::BossHpBelow {
                                                    hp_percent: val,
                                                    entity: entity.clone(),
                                                    boss_name: boss_name.clone(),
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                            div { class: "flex items-center gap-xs",
                                label { class: "text-sm text-secondary", "Entity" }
                                input {
                                    r#type: "text",
                                    class: "input-inline flex-1",
                                    placeholder: "From roster",
                                    value: "{entity.clone().unwrap_or_default()}",
                                    oninput: {
                                        let boss_name = boss_name.clone();
                                        move |e| on_change.call(CounterTrigger::BossHpBelow {
                                            hp_percent,
                                            entity: if e.value().is_empty() { None } else { Some(e.value()) },
                                            boss_name: boss_name.clone(),
                                        })
                                    }
                                }
                            }
                        }
                    },
                }
            }
        }
    }
}
