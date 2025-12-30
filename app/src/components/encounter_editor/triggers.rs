//! Shared trigger editors
//!
//! Used by timers, phases, and counters for editing trigger conditions.

use dioxus::prelude::*;

use crate::types::{
    AbilitySelector, CounterTrigger, EffectSelector, EntityMatcher, EntitySelector, PhaseTrigger,
    TimerTrigger,
};

use super::tabs::EncounterData;

// ─────────────────────────────────────────────────────────────────────────────
// Reusable ID Selector
// ─────────────────────────────────────────────────────────────────────────────

/// Generic dropdown selector for IDs (timers, phases, counters)
#[component]
fn IdSelector(
    label: &'static str,
    value: String,
    available: Vec<String>,
    on_change: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "flex items-center gap-xs",
            label { class: "text-sm text-secondary", "{label}" }
            select {
                class: "select",
                style: "width: 180px;",
                value: "{value}",
                onchange: move |e| on_change.call(e.value()),
                if value.is_empty() {
                    option { value: "", selected: true, "(select)" }
                }
                for id in &available {
                    option {
                        value: "{id}",
                        selected: *id == value,
                        "{id}"
                    }
                }
                // Allow current value even if not in list (backwards compat)
                if !value.is_empty() && !available.contains(&value) {
                    option {
                        value: "{value}",
                        selected: true,
                        "{value} (not found)"
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Boss Entity Selector (for HP threshold triggers)
// ─────────────────────────────────────────────────────────────────────────────

/// Dropdown selector for boss entities with chip display
#[component]
fn BossSelector(
    selected: Vec<EntitySelector>,
    available_bosses: Vec<String>,
    on_change: EventHandler<Vec<EntitySelector>>,
) -> Element {
    rsx! {
        div { class: "flex-col gap-xs",
            // Selected boss chips
            if !selected.is_empty() {
                div { class: "flex flex-wrap gap-xs",
                    for (idx, sel) in selected.iter().enumerate() {
                        {
                            let selected_clone = selected.clone();
                            let display = sel.display();
                            rsx! {
                                span { class: "chip",
                                    "{display}"
                                    button {
                                        class: "chip-remove",
                                        onclick: move |_| {
                                            let mut new_sels = selected_clone.clone();
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
            }

            // Dropdown to add boss
            div { class: "flex items-center gap-xs",
                label { class: "text-sm text-secondary", "Boss" }
                select {
                    class: "select",
                    style: "width: 180px;",
                    onchange: move |e| {
                        let val = e.value();
                        if !val.is_empty() {
                            let selector = EntitySelector::Name(val);
                            let mut new_sels = selected.clone();
                            if !new_sels.iter().any(|s| s.display() == selector.display()) {
                                new_sels.push(selector);
                                on_change.call(new_sels);
                            }
                        }
                    },
                    option { value: "", "(add boss...)" }
                    for boss in &available_bosses {
                        {
                            let already_selected = selected.iter().any(|s| s.display() == *boss);
                            rsx! {
                                option {
                                    value: "{boss}",
                                    disabled: already_selected,
                                    "{boss}"
                                    if already_selected { " ✓" }
                                }
                            }
                        }
                    }
                }
            }

            if selected.is_empty() {
                span { class: "hint", "No boss selected (triggers for any boss)" }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Timer Trigger Editor
// ─────────────────────────────────────────────────────────────────────────────

/// Composable trigger editor for timer triggers
#[component]
pub fn ComposableTriggerEditor(
    trigger: TimerTrigger,
    encounter_data: EncounterData,
    on_change: EventHandler<TimerTrigger>,
) -> Element {
    rsx! {
        div { class: "composable-trigger-editor",
            TriggerNode {
                trigger: trigger,
                encounter_data: encounter_data,
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
    encounter_data: EncounterData,
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
                    encounter_data: encounter_data.clone(),
                    on_change: on_change,
                    depth: depth,
                }
            } else {
                SimpleTriggerEditor {
                    trigger: trigger.clone(),
                    encounter_data: encounter_data,
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
    encounter_data: EncounterData,
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
                        let encounter_data_for_node = encounter_data.clone();

                        rsx! {
                            div { class: "condition-item",
                                TriggerNode {
                                    trigger: condition_clone,
                                    encounter_data: encounter_data_for_node,
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
    encounter_data: EncounterData,
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
                        "ability_cast" => TimerTrigger::AbilityCast { abilities: vec![], source: EntityMatcher::default() },
                        "effect_applied" => TimerTrigger::EffectApplied { effects: vec![], source: EntityMatcher::default(), target: EntityMatcher::default() },
                        "effect_removed" => TimerTrigger::EffectRemoved { effects: vec![], source: EntityMatcher::default(), target: EntityMatcher::default() },
                        "damage_taken" => TimerTrigger::DamageTaken { abilities: vec![], source: EntityMatcher::default(), target: EntityMatcher::default() },
                        "timer_expires" => TimerTrigger::TimerExpires { timer_id: String::new() },
                        "timer_started" => TimerTrigger::TimerStarted { timer_id: String::new() },
                        "phase_entered" => TimerTrigger::PhaseEntered { phase_id: String::new() },
                        "phase_ended" => TimerTrigger::PhaseEnded { phase_id: String::new() },
                        "boss_hp_below" => TimerTrigger::BossHpBelow { hp_percent: 50.0, entities: vec![] },
                        "counter_reaches" => TimerTrigger::CounterReaches { counter_id: String::new(), value: 1 },
                        "npc_appears" => TimerTrigger::NpcAppears { entities: vec![] },
                        "entity_death" => TimerTrigger::EntityDeath { entities: vec![] },
                        "target_set" => TimerTrigger::TargetSet { entities: vec![] },
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
                option { value: "damage_taken", "Damage Taken" }
                option { value: "timer_expires", "Timer Expires" }
                option { value: "timer_started", "Timer Started" }
                option { value: "phase_entered", "Phase Entered" }
                option { value: "phase_ended", "Phase Ended" }
                option { value: "boss_hp_below", "Boss HP Below" }
                option { value: "counter_reaches", "Counter Reaches" }
                option { value: "npc_appears", "NPC Appears" }
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
                    TimerTrigger::AbilityCast { abilities, .. } => rsx! {
                        AbilitySelectorEditor {
                            label: "Abilities",
                            selectors: abilities,
                            on_change: move |sels| on_change.call(TimerTrigger::AbilityCast { abilities: sels, source: EntityMatcher::default() })
                        }
                    },
                    TimerTrigger::EffectApplied { effects, .. } => rsx! {
                        EffectSelectorEditor {
                            label: "Effects",
                            selectors: effects,
                            on_change: move |sels| on_change.call(TimerTrigger::EffectApplied { effects: sels, source: EntityMatcher::default(), target: EntityMatcher::default() })
                        }
                    },
                    TimerTrigger::EffectRemoved { effects, .. } => rsx! {
                        EffectSelectorEditor {
                            label: "Effects",
                            selectors: effects,
                            on_change: move |sels| on_change.call(TimerTrigger::EffectRemoved { effects: sels, source: EntityMatcher::default(), target: EntityMatcher::default() })
                        }
                    },
                    TimerTrigger::DamageTaken { abilities, .. } => rsx! {
                        AbilitySelectorEditor {
                            label: "Abilities",
                            selectors: abilities,
                            on_change: move |sels| on_change.call(TimerTrigger::DamageTaken { abilities: sels, source: EntityMatcher::default(), target: EntityMatcher::default() })
                        }
                    },
                    TimerTrigger::TimerExpires { timer_id } => {
                        let available_timers = encounter_data.timer_ids();
                        rsx! {
                            IdSelector {
                                label: "Timer",
                                value: timer_id,
                                available: available_timers,
                                on_change: move |id| on_change.call(TimerTrigger::TimerExpires { timer_id: id })
                            }
                        }
                    },
                    TimerTrigger::TimerStarted { timer_id } => {
                        let available_timers = encounter_data.timer_ids();
                        rsx! {
                            IdSelector {
                                label: "Timer",
                                value: timer_id,
                                available: available_timers,
                                on_change: move |id| on_change.call(TimerTrigger::TimerStarted { timer_id: id })
                            }
                        }
                    },
                    TimerTrigger::PhaseEntered { phase_id } => {
                        let available_phases = encounter_data.phase_ids();
                        rsx! {
                            IdSelector {
                                label: "Phase",
                                value: phase_id,
                                available: available_phases,
                                on_change: move |id| on_change.call(TimerTrigger::PhaseEntered { phase_id: id })
                            }
                        }
                    },
                    TimerTrigger::PhaseEnded { phase_id } => {
                        let available_phases = encounter_data.phase_ids();
                        rsx! {
                            IdSelector {
                                label: "Phase",
                                value: phase_id,
                                available: available_phases,
                                on_change: move |id| on_change.call(TimerTrigger::PhaseEnded { phase_id: id })
                            }
                        }
                    },
                    TimerTrigger::BossHpBelow { hp_percent, entities } => {
                        let available_bosses = encounter_data.boss_entity_names();
                        rsx! {
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
                                        oninput: {
                                            let entities = entities.clone();
                                            move |e| {
                                                if let Ok(val) = e.value().parse::<f32>() {
                                                    on_change.call(TimerTrigger::BossHpBelow {
                                                        hp_percent: val,
                                                        entities: entities.clone(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                                BossSelector {
                                    selected: entities.clone(),
                                    available_bosses: available_bosses,
                                    on_change: move |sels| on_change.call(TimerTrigger::BossHpBelow {
                                        hp_percent,
                                        entities: sels,
                                    })
                                }
                            }
                        }
                    },
                    TimerTrigger::CounterReaches { counter_id, value } => {
                        let available_counters = encounter_data.counter_ids();
                        rsx! {
                            div { class: "flex-col gap-xs",
                                IdSelector {
                                    label: "Counter",
                                    value: counter_id.clone(),
                                    available: available_counters,
                                    on_change: move |id| on_change.call(TimerTrigger::CounterReaches {
                                        counter_id: id,
                                        value
                                    })
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
                        }
                    },
                    TimerTrigger::NpcAppears { entities } => rsx! {
                        EntitySelectorEditor {
                            label: "Entity (Spawned)",
                            selectors: entities.clone(),
                            on_change: move |sels| on_change.call(TimerTrigger::NpcAppears {
                                entities: sels
                            })
                        }
                    },
                    TimerTrigger::EntityDeath { entities } => rsx! {
                        EntitySelectorEditor {
                            label: "Entity (Death)",
                            selectors: entities.clone(),
                            on_change: move |sels| on_change.call(TimerTrigger::EntityDeath {
                                entities: sels
                            })
                        }
                    },
                    TimerTrigger::TargetSet { entities } => rsx! {
                        EntitySelectorEditor {
                            label: "NPC (Setter)",
                            selectors: entities.clone(),
                            on_change: move |sels| on_change.call(TimerTrigger::TargetSet {
                                entities: sels
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

/// Editor for a list of entity selectors (NPC ID, roster alias, or name)
#[component]
pub fn EntitySelectorEditor(
    label: &'static str,
    selectors: Vec<EntitySelector>,
    on_change: EventHandler<Vec<EntitySelector>>,
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
                            let selector = EntitySelector::from_input(&new_input());
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
                            let selector = EntitySelector::from_input(&new_input());
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
    encounter_data: EncounterData,
    on_change: EventHandler<PhaseTrigger>,
) -> Element {
    rsx! {
        div { class: "composable-trigger-editor",
            PhaseTriggerNode {
                trigger: trigger,
                encounter_data: encounter_data,
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
    encounter_data: EncounterData,
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
                    encounter_data: encounter_data.clone(),
                    on_change: on_change,
                    depth: depth,
                }
            } else {
                SimplePhaseTriggerEditor {
                    trigger: trigger.clone(),
                    encounter_data: encounter_data,
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
    encounter_data: EncounterData,
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
                        let encounter_data_for_node = encounter_data.clone();

                        rsx! {
                            div { class: "condition-item",
                                PhaseTriggerNode {
                                    trigger: condition_clone,
                                    encounter_data: encounter_data_for_node,
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
    encounter_data: EncounterData,
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
                            entities: vec![],
                        },
                        "boss_hp_above" => PhaseTrigger::BossHpAbove {
                            hp_percent: 50.0,
                            entities: vec![],
                        },
                        "ability_cast" => PhaseTrigger::AbilityCast { abilities: vec![], source: EntityMatcher::default() },
                        "effect_applied" => PhaseTrigger::EffectApplied { effects: vec![], source: EntityMatcher::default(), target: EntityMatcher::default() },
                        "effect_removed" => PhaseTrigger::EffectRemoved { effects: vec![], source: EntityMatcher::default(), target: EntityMatcher::default() },
                        "damage_taken" => PhaseTrigger::DamageTaken { abilities: vec![], source: EntityMatcher::default(), target: EntityMatcher::default() },
                        "counter_reaches" => PhaseTrigger::CounterReaches {
                            counter_id: String::new(),
                            value: 1,
                        },
                        "time_elapsed" => PhaseTrigger::TimeElapsed { secs: 30.0 },
                        "npc_appears" => PhaseTrigger::NpcAppears {
                            entities: vec![],
                        },
                        "entity_death" => PhaseTrigger::EntityDeath {
                            entities: vec![],
                        },
                        "phase_ended" => PhaseTrigger::PhaseEnded {
                            phase_id: String::new(),
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
                option { value: "damage_taken", "Damage Taken" }
                option { value: "counter_reaches", "Counter Reaches" }
                option { value: "time_elapsed", "Time Elapsed" }
                option { value: "npc_appears", "NPC Appears" }
                option { value: "entity_death", "Entity Death" }
                option { value: "phase_ended", "Phase Ended" }
            }

            // Type-specific fields
            {
                match trigger.clone() {
                    PhaseTrigger::CombatStart => rsx! {},
                    PhaseTrigger::BossHpBelow { hp_percent, entities } => {
                        let available_bosses = encounter_data.boss_entity_names();
                        rsx! {
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
                                            let entities = entities.clone();
                                            move |e| {
                                                if let Ok(val) = e.value().parse::<f32>() {
                                                    on_change.call(PhaseTrigger::BossHpBelow {
                                                        hp_percent: val,
                                                        entities: entities.clone(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                                BossSelector {
                                    selected: entities.clone(),
                                    available_bosses: available_bosses,
                                    on_change: move |sels| on_change.call(PhaseTrigger::BossHpBelow {
                                        hp_percent,
                                        entities: sels,
                                    })
                                }
                            }
                        }
                    },
                    PhaseTrigger::BossHpAbove { hp_percent, entities } => {
                        let available_bosses = encounter_data.boss_entity_names();
                        rsx! {
                            div { class: "flex-col gap-xs",
                                div { class: "flex items-center gap-xs",
                                    label { class: "text-sm text-secondary", "HP % Above" }
                                    input {
                                        r#type: "number",
                                        step: "0.1",
                                        min: "0",
                                        max: "100",
                                        class: "input-inline",
                                        style: "width: 70px;",
                                        value: "{hp_percent}",
                                        oninput: {
                                            let entities = entities.clone();
                                            move |e| {
                                                if let Ok(val) = e.value().parse::<f32>() {
                                                    on_change.call(PhaseTrigger::BossHpAbove {
                                                        hp_percent: val,
                                                        entities: entities.clone(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                                BossSelector {
                                    selected: entities.clone(),
                                    available_bosses: available_bosses,
                                    on_change: move |sels| on_change.call(PhaseTrigger::BossHpAbove {
                                        hp_percent,
                                        entities: sels,
                                    })
                                }
                            }
                        }
                    },
                    PhaseTrigger::AbilityCast { abilities, .. } => rsx! {
                        AbilitySelectorEditor {
                            label: "Abilities",
                            selectors: abilities,
                            on_change: move |sels| on_change.call(PhaseTrigger::AbilityCast { abilities: sels, source: EntityMatcher::default() })
                        }
                    },
                    PhaseTrigger::EffectApplied { effects, .. } => rsx! {
                        EffectSelectorEditor {
                            label: "Effects",
                            selectors: effects,
                            on_change: move |sels| on_change.call(PhaseTrigger::EffectApplied { effects: sels, source: EntityMatcher::default(), target: EntityMatcher::default() })
                        }
                    },
                    PhaseTrigger::EffectRemoved { effects, .. } => rsx! {
                        EffectSelectorEditor {
                            label: "Effects",
                            selectors: effects,
                            on_change: move |sels| on_change.call(PhaseTrigger::EffectRemoved { effects: sels, source: EntityMatcher::default(), target: EntityMatcher::default() })
                        }
                    },
                    PhaseTrigger::DamageTaken { abilities, .. } => rsx! {
                        AbilitySelectorEditor {
                            label: "Abilities",
                            selectors: abilities,
                            on_change: move |sels| on_change.call(PhaseTrigger::DamageTaken { abilities: sels, source: EntityMatcher::default(), target: EntityMatcher::default() })
                        }
                    },
                    PhaseTrigger::CounterReaches { counter_id, value } => {
                        let available_counters = encounter_data.counter_ids();
                        rsx! {
                            div { class: "flex-col gap-xs",
                                IdSelector {
                                    label: "Counter",
                                    value: counter_id.clone(),
                                    available: available_counters,
                                    on_change: move |id| on_change.call(PhaseTrigger::CounterReaches {
                                        counter_id: id,
                                        value
                                    })
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
                    PhaseTrigger::NpcAppears { entities } => rsx! {
                        EntitySelectorEditor {
                            label: "Entity (Spawned)",
                            selectors: entities.clone(),
                            on_change: move |sels| on_change.call(PhaseTrigger::NpcAppears {
                                entities: sels
                            })
                        }
                    },
                    PhaseTrigger::EntityDeath { entities } => rsx! {
                        EntitySelectorEditor {
                            label: "Entity (Death)",
                            selectors: entities.clone(),
                            on_change: move |sels| on_change.call(PhaseTrigger::EntityDeath {
                                entities: sels
                            })
                        }
                    },
                    PhaseTrigger::PhaseEnded { phase_id } => {
                        let available_phases = encounter_data.phase_ids();
                        rsx! {
                            IdSelector {
                                label: "Phase",
                                value: phase_id.clone(),
                                available: available_phases,
                                on_change: move |id: String| on_change.call(PhaseTrigger::PhaseEnded {
                                    phase_id: id,
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

// ─────────────────────────────────────────────────────────────────────────────
// Counter Trigger Editor
// ─────────────────────────────────────────────────────────────────────────────

/// Simple counter trigger editor (no composite support for counters)
#[component]
pub fn CounterTriggerEditor(
    trigger: CounterTrigger,
    encounter_data: EncounterData,
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
                            source: EntityMatcher::default(),
                        },
                        "effect_applied" => CounterTrigger::EffectApplied {
                            effects: vec![],
                            source: EntityMatcher::default(),
                            target: EntityMatcher::default(),
                        },
                        "effect_removed" => CounterTrigger::EffectRemoved {
                            effects: vec![],
                            source: EntityMatcher::default(),
                            target: EntityMatcher::default(),
                        },
                        "damage_taken" => CounterTrigger::DamageTaken {
                            abilities: vec![],
                            source: EntityMatcher::default(),
                            target: EntityMatcher::default(),
                        },
                        "timer_expires" => CounterTrigger::TimerExpires {
                            timer_id: String::new(),
                        },
                        "timer_started" => CounterTrigger::TimerStarted {
                            timer_id: String::new(),
                        },
                        "phase_entered" => CounterTrigger::PhaseEntered {
                            phase_id: String::new(),
                        },
                        "phase_ended" => CounterTrigger::PhaseEnded {
                            phase_id: String::new(),
                        },
                        "any_phase_change" => CounterTrigger::AnyPhaseChange,
                        "npc_appears" => CounterTrigger::NpcAppears {
                            entities: vec![],
                        },
                        "entity_death" => CounterTrigger::EntityDeath {
                            entities: vec![],
                        },
                        "counter_reaches" => CounterTrigger::CounterReaches {
                            counter_id: String::new(),
                            value: 1,
                        },
                        "boss_hp_below" => CounterTrigger::BossHpBelow {
                            hp_percent: 50.0,
                            entities: vec![],
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
                option { value: "damage_taken", "Damage Taken" }
                option { value: "timer_expires", "Timer Expires" }
                option { value: "timer_started", "Timer Started" }
                option { value: "phase_entered", "Phase Entered" }
                option { value: "phase_ended", "Phase Ended" }
                option { value: "any_phase_change", "Any Phase Change" }
                option { value: "npc_appears", "NPC Appears" }
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

                    CounterTrigger::AbilityCast { abilities, source, .. } => {
                        let source_for_sels = source.clone();
                        rsx! {
                            AbilitySelectorEditor {
                                label: "Abilities",
                                selectors: abilities.clone(),
                                on_change: move |sels| on_change.call(CounterTrigger::AbilityCast {
                                    abilities: sels,
                                    source: source_for_sels.clone(),
                                })
                            }
                            EntitySelectorEditor {
                                label: "Source Filter",
                                selectors: source.entities.clone(),
                                on_change: {
                                    let abilities = abilities.clone();
                                    move |sels| on_change.call(CounterTrigger::AbilityCast {
                                        abilities: abilities.clone(),
                                        source: EntityMatcher::new(sels),
                                    })
                                }
                            }
                        }
                    },

                    CounterTrigger::EffectApplied { effects, target, .. } => {
                        let target_for_sels = target.clone();
                        rsx! {
                            EffectSelectorEditor {
                                label: "Effects",
                                selectors: effects.clone(),
                                on_change: move |sels| on_change.call(CounterTrigger::EffectApplied {
                                    effects: sels,
                                    source: EntityMatcher::default(),
                                    target: target_for_sels.clone(),
                                })
                            }
                            EntitySelectorEditor {
                                label: "Target Filter",
                                selectors: target.entities.clone(),
                                on_change: {
                                    let effects = effects.clone();
                                    move |sels| on_change.call(CounterTrigger::EffectApplied {
                                        effects: effects.clone(),
                                        source: EntityMatcher::default(),
                                        target: EntityMatcher::new(sels),
                                    })
                                }
                            }
                        }
                    },

                    CounterTrigger::EffectRemoved { effects, target, .. } => {
                        let target_for_sels = target.clone();
                        rsx! {
                            EffectSelectorEditor {
                                label: "Effects",
                                selectors: effects.clone(),
                                on_change: move |sels| on_change.call(CounterTrigger::EffectRemoved {
                                    effects: sels,
                                    source: EntityMatcher::default(),
                                    target: target_for_sels.clone(),
                                })
                            }
                            EntitySelectorEditor {
                                label: "Target Filter",
                                selectors: target.entities.clone(),
                                on_change: {
                                    let effects = effects.clone();
                                    move |sels| on_change.call(CounterTrigger::EffectRemoved {
                                        effects: effects.clone(),
                                        source: EntityMatcher::default(),
                                        target: EntityMatcher::new(sels),
                                    })
                                }
                            }
                        }
                    },

                    CounterTrigger::DamageTaken { abilities, target, .. } => {
                        let target_for_sels = target.clone();
                        rsx! {
                            AbilitySelectorEditor {
                                label: "Abilities",
                                selectors: abilities.clone(),
                                on_change: move |sels| on_change.call(CounterTrigger::DamageTaken {
                                    abilities: sels,
                                    source: EntityMatcher::default(),
                                    target: target_for_sels.clone(),
                                })
                            }
                            EntitySelectorEditor {
                                label: "Target Filter",
                                selectors: target.entities.clone(),
                                on_change: {
                                    let abilities = abilities.clone();
                                    move |sels| on_change.call(CounterTrigger::DamageTaken {
                                        abilities: abilities.clone(),
                                        source: EntityMatcher::default(),
                                        target: EntityMatcher::new(sels),
                                    })
                                }
                            }
                        }
                    },

                    CounterTrigger::TimerExpires { timer_id } => {
                        let available_timers = encounter_data.timer_ids();
                        rsx! {
                            IdSelector {
                                label: "Timer",
                                value: timer_id.clone(),
                                available: available_timers,
                                on_change: move |id: String| on_change.call(CounterTrigger::TimerExpires { timer_id: id })
                            }
                        }
                    },

                    CounterTrigger::TimerStarted { timer_id } => {
                        let available_timers = encounter_data.timer_ids();
                        rsx! {
                            IdSelector {
                                label: "Timer",
                                value: timer_id.clone(),
                                available: available_timers,
                                on_change: move |id: String| on_change.call(CounterTrigger::TimerStarted { timer_id: id })
                            }
                        }
                    },

                    CounterTrigger::PhaseEntered { phase_id } => {
                        let available_phases = encounter_data.phase_ids();
                        rsx! {
                            IdSelector {
                                label: "Phase",
                                value: phase_id.clone(),
                                available: available_phases,
                                on_change: move |id: String| on_change.call(CounterTrigger::PhaseEntered { phase_id: id })
                            }
                        }
                    },

                    CounterTrigger::PhaseEnded { phase_id } => {
                        let available_phases = encounter_data.phase_ids();
                        rsx! {
                            IdSelector {
                                label: "Phase",
                                value: phase_id.clone(),
                                available: available_phases,
                                on_change: move |id: String| on_change.call(CounterTrigger::PhaseEnded { phase_id: id })
                            }
                        }
                    },

                    CounterTrigger::NpcAppears { entities } => rsx! {
                        EntitySelectorEditor {
                            label: "Entity (Spawned)",
                            selectors: entities.clone(),
                            on_change: move |sels| on_change.call(CounterTrigger::NpcAppears {
                                entities: sels
                            })
                        }
                    },

                    CounterTrigger::EntityDeath { entities } => rsx! {
                        EntitySelectorEditor {
                            label: "Entity (Death)",
                            selectors: entities.clone(),
                            on_change: move |sels| on_change.call(CounterTrigger::EntityDeath {
                                entities: sels
                            })
                        }
                    },

                    CounterTrigger::CounterReaches { counter_id, value } => {
                        let available_counters = encounter_data.counter_ids();
                        rsx! {
                            div { class: "flex-col gap-xs",
                                IdSelector {
                                    label: "Counter",
                                    value: counter_id.clone(),
                                    available: available_counters,
                                    on_change: move |id: String| on_change.call(CounterTrigger::CounterReaches {
                                        counter_id: id,
                                        value
                                    })
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
                        }
                    },

                    CounterTrigger::BossHpBelow { hp_percent, entities } => {
                        let available_bosses = encounter_data.boss_entity_names();
                        rsx! {
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
                                            let entities = entities.clone();
                                            move |e| {
                                                if let Ok(val) = e.value().parse::<f32>() {
                                                    on_change.call(CounterTrigger::BossHpBelow {
                                                        hp_percent: val,
                                                        entities: entities.clone(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                                BossSelector {
                                    selected: entities.clone(),
                                    available_bosses: available_bosses,
                                    on_change: move |sels| on_change.call(CounterTrigger::BossHpBelow {
                                        hp_percent,
                                        entities: sels,
                                    })
                                }
                            }
                        }
                    },

                    // Catch-all for trigger types not commonly used in counters
                    _ => rsx! {},
                }
            }
        }
    }
}
