//! Shared trigger editors
//!
//! Used by timers, phases, and counters for editing trigger conditions.

use dioxus::prelude::*;

use crate::types::{
    AbilitySelector, CounterTrigger, EffectSelector, EntityFilter, EntitySelector, PhaseTrigger,
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
// Entity Filter Dropdown (for source/target filters)
// ─────────────────────────────────────────────────────────────────────────────

/// Dropdown selector for EntityFilter values (source/target)
/// Supports all standard options plus Selector for specific entities
#[component]
pub fn EntityFilterDropdown(
    label: &'static str,
    value: EntityFilter,
    options: &'static [EntityFilter],
    on_change: EventHandler<EntityFilter>,
) -> Element {
    let is_selector = matches!(value, EntityFilter::Selector(_));
    let selectors = if let EntityFilter::Selector(s) = &value {
        s.clone()
    } else {
        vec![]
    };

    rsx! {
        div { class: "flex-col gap-xs",
            div { class: "flex items-center gap-xs",
                if !label.is_empty() {
                    span { class: "text-sm text-secondary", "{label}:" }
                }
                select {
                    class: "select",
                    style: "width: 160px;",
                    onchange: move |e| {
                        let selected = e.value();
                        if selected == "Specific (ID or Name)" {
                            on_change.call(EntityFilter::Selector(vec![]));
                        } else {
                            for opt in options {
                                if opt.label() == selected {
                                    on_change.call(opt.clone());
                                    break;
                                }
                            }
                        }
                    },
                    for opt in options.iter() {
                        option {
                            value: "{opt.label()}",
                            selected: *opt == value,
                            "{opt.label()}"
                        }
                    }
                    option {
                        value: "Specific (ID or Name)",
                        selected: is_selector,
                        "Specific (ID or Name)"
                    }
                }
            }
            if is_selector {
                EntitySelectorEditor {
                    label: "",
                    selectors: selectors,
                    on_change: move |sels| on_change.call(EntityFilter::Selector(sels))
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
                        "combat_end" => TimerTrigger::CombatEnd,
                        "ability_cast" => TimerTrigger::AbilityCast { abilities: vec![], source: EntityFilter::default() },
                        "effect_applied" => TimerTrigger::EffectApplied { effects: vec![], source: EntityFilter::default(), target: EntityFilter::default() },
                        "effect_removed" => TimerTrigger::EffectRemoved { effects: vec![], source: EntityFilter::default(), target: EntityFilter::default() },
                        "damage_taken" => TimerTrigger::DamageTaken { abilities: vec![], source: EntityFilter::default(), target: EntityFilter::default() },
                        "timer_expires" => TimerTrigger::TimerExpires { timer_id: String::new() },
                        "timer_started" => TimerTrigger::TimerStarted { timer_id: String::new() },
                        "phase_entered" => TimerTrigger::PhaseEntered { phase_id: String::new() },
                        "phase_ended" => TimerTrigger::PhaseEnded { phase_id: String::new() },
                        "any_phase_change" => TimerTrigger::AnyPhaseChange,
                        "boss_hp_below" => TimerTrigger::BossHpBelow { hp_percent: 50.0, selector: vec![] },
                        "counter_reaches" => TimerTrigger::CounterReaches { counter_id: String::new(), value: 1 },
                        "npc_appears" => TimerTrigger::NpcAppears { selector: vec![] },
                        "entity_death" => TimerTrigger::EntityDeath { selector: vec![] },
                        "target_set" => TimerTrigger::TargetSet { selector: vec![], target: EntityFilter::default() },
                        "time_elapsed" => TimerTrigger::TimeElapsed { secs: 30.0 },
                        "manual" => TimerTrigger::Manual,
                        "never" => TimerTrigger::Never,
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
                option { value: "boss_hp_below", "Boss HP Below" }
                option { value: "counter_reaches", "Counter Reaches" }
                option { value: "npc_appears", "NPC Appears" }
                option { value: "entity_death", "Entity Death" }
                option { value: "target_set", "Target Set" }
                option { value: "time_elapsed", "Time Elapsed" }
                option { value: "manual", "Manual" }
                option { value: "never", "Never" }
            }

            // Type-specific fields
            {
                match trigger.clone() {
                    TimerTrigger::CombatStart
                    | TimerTrigger::CombatEnd
                    | TimerTrigger::AnyPhaseChange
                    | TimerTrigger::Never
                    | TimerTrigger::Manual => rsx! {},
                    TimerTrigger::AbilityCast { abilities, source } => {
                        let source_for_abilities = source.clone();
                        let abilities_for_source = abilities.clone();
                        rsx! {
                            AbilitySelectorEditor {
                                label: "Abilities",
                                selectors: abilities,
                                on_change: move |sels| on_change.call(TimerTrigger::AbilityCast {
                                    abilities: sels,
                                    source: source_for_abilities.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Source",
                                value: source,
                                options: EntityFilter::source_options(),
                                on_change: move |f| on_change.call(TimerTrigger::AbilityCast {
                                    abilities: abilities_for_source.clone(),
                                    source: f,
                                })
                            }
                        }
                    },
                    TimerTrigger::EffectApplied { effects, source, target } => {
                        let source_for_effects = source.clone();
                        let target_for_effects = target.clone();
                        let effects_for_source = effects.clone();
                        let target_for_source = target.clone();
                        let effects_for_target = effects.clone();
                        let source_for_target = source.clone();
                        rsx! {
                            EffectSelectorEditor {
                                label: "Effects",
                                selectors: effects,
                                on_change: move |sels| on_change.call(TimerTrigger::EffectApplied {
                                    effects: sels,
                                    source: source_for_effects.clone(),
                                    target: target_for_effects.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Source",
                                value: source,
                                options: EntityFilter::source_options(),
                                on_change: move |f| on_change.call(TimerTrigger::EffectApplied {
                                    effects: effects_for_source.clone(),
                                    source: f,
                                    target: target_for_source.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Target",
                                value: target,
                                options: EntityFilter::target_options(),
                                on_change: move |f| on_change.call(TimerTrigger::EffectApplied {
                                    effects: effects_for_target.clone(),
                                    source: source_for_target.clone(),
                                    target: f,
                                })
                            }
                        }
                    },
                    TimerTrigger::EffectRemoved { effects, source, target } => {
                        let source_for_effects = source.clone();
                        let target_for_effects = target.clone();
                        let effects_for_source = effects.clone();
                        let target_for_source = target.clone();
                        let effects_for_target = effects.clone();
                        let source_for_target = source.clone();
                        rsx! {
                            EffectSelectorEditor {
                                label: "Effects",
                                selectors: effects,
                                on_change: move |sels| on_change.call(TimerTrigger::EffectRemoved {
                                    effects: sels,
                                    source: source_for_effects.clone(),
                                    target: target_for_effects.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Source",
                                value: source,
                                options: EntityFilter::source_options(),
                                on_change: move |f| on_change.call(TimerTrigger::EffectRemoved {
                                    effects: effects_for_source.clone(),
                                    source: f,
                                    target: target_for_source.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Target",
                                value: target,
                                options: EntityFilter::target_options(),
                                on_change: move |f| on_change.call(TimerTrigger::EffectRemoved {
                                    effects: effects_for_target.clone(),
                                    source: source_for_target.clone(),
                                    target: f,
                                })
                            }
                        }
                    },
                    TimerTrigger::DamageTaken { abilities, source, target } => {
                        let source_for_abilities = source.clone();
                        let target_for_abilities = target.clone();
                        let abilities_for_source = abilities.clone();
                        let target_for_source = target.clone();
                        let abilities_for_target = abilities.clone();
                        let source_for_target = source.clone();
                        rsx! {
                            AbilitySelectorEditor {
                                label: "Abilities",
                                selectors: abilities,
                                on_change: move |sels| on_change.call(TimerTrigger::DamageTaken {
                                    abilities: sels,
                                    source: source_for_abilities.clone(),
                                    target: target_for_abilities.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Source",
                                value: source,
                                options: EntityFilter::source_options(),
                                on_change: move |f| on_change.call(TimerTrigger::DamageTaken {
                                    abilities: abilities_for_source.clone(),
                                    source: f,
                                    target: target_for_source.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Target",
                                value: target,
                                options: EntityFilter::target_options(),
                                on_change: move |f| on_change.call(TimerTrigger::DamageTaken {
                                    abilities: abilities_for_target.clone(),
                                    source: source_for_target.clone(),
                                    target: f,
                                })
                            }
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
                    TimerTrigger::BossHpBelow { hp_percent, selector } => {
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
                                            let selector = selector.clone();
                                            move |e| {
                                                if let Ok(val) = e.value().parse::<f32>() {
                                                    on_change.call(TimerTrigger::BossHpBelow {
                                                        hp_percent: val,
                                                        selector: selector.clone(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                                BossSelector {
                                    selected: selector.clone(),
                                    available_bosses: available_bosses,
                                    on_change: move |sels| on_change.call(TimerTrigger::BossHpBelow {
                                        hp_percent,
                                        selector: sels,
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
                    TimerTrigger::NpcAppears { selector } => rsx! {
                        EntitySelectorEditor {
                            label: "Entity (Spawned)",
                            selectors: selector.clone(),
                            on_change: move |sels| on_change.call(TimerTrigger::NpcAppears {
                                selector: sels
                            })
                        }
                    },
                    TimerTrigger::EntityDeath { selector } => rsx! {
                        EntitySelectorEditor {
                            label: "Entity (Death)",
                            selectors: selector.clone(),
                            on_change: move |sels| on_change.call(TimerTrigger::EntityDeath {
                                selector: sels
                            })
                        }
                    },
                    TimerTrigger::TargetSet { selector, target } => {
                        let target_for_selector = target.clone();
                        let selector_for_target = selector.clone();
                        rsx! {
                            EntitySelectorEditor {
                                label: "NPC (Setter)",
                                selectors: selector.clone(),
                                on_change: move |sels| on_change.call(TimerTrigger::TargetSet {
                                    selector: sels,
                                    target: target_for_selector.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Target",
                                value: target,
                                options: EntityFilter::target_options(),
                                on_change: move |f| on_change.call(TimerTrigger::TargetSet {
                                    selector: selector_for_target.clone(),
                                    target: f,
                                })
                            }
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
            if !label.is_empty() {
                span { class: "text-sm text-secondary text-left", "{label}:" }
            }

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
                            selector: vec![],
                        },
                        "boss_hp_above" => PhaseTrigger::BossHpAbove {
                            hp_percent: 50.0,
                            selector: vec![],
                        },
                        "ability_cast" => PhaseTrigger::AbilityCast { abilities: vec![], source: EntityFilter::default() },
                        "effect_applied" => PhaseTrigger::EffectApplied { effects: vec![], source: EntityFilter::default(), target: EntityFilter::default() },
                        "effect_removed" => PhaseTrigger::EffectRemoved { effects: vec![], source: EntityFilter::default(), target: EntityFilter::default() },
                        "damage_taken" => PhaseTrigger::DamageTaken { abilities: vec![], source: EntityFilter::default(), target: EntityFilter::default() },
                        "counter_reaches" => PhaseTrigger::CounterReaches {
                            counter_id: String::new(),
                            value: 1,
                        },
                        "time_elapsed" => PhaseTrigger::TimeElapsed { secs: 30.0 },
                        "npc_appears" => PhaseTrigger::NpcAppears {
                            selector: vec![],
                        },
                        "entity_death" => PhaseTrigger::EntityDeath {
                            selector: vec![],
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
                    PhaseTrigger::BossHpBelow { hp_percent, selector } => {
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
                                            let selector = selector.clone();
                                            move |e| {
                                                if let Ok(val) = e.value().parse::<f32>() {
                                                    on_change.call(PhaseTrigger::BossHpBelow {
                                                        hp_percent: val,
                                                        selector: selector.clone(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                                BossSelector {
                                    selected: selector.clone(),
                                    available_bosses: available_bosses,
                                    on_change: move |sels| on_change.call(PhaseTrigger::BossHpBelow {
                                        hp_percent,
                                        selector: sels,
                                    })
                                }
                            }
                        }
                    },
                    PhaseTrigger::BossHpAbove { hp_percent, selector } => {
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
                                            let selector = selector.clone();
                                            move |e| {
                                                if let Ok(val) = e.value().parse::<f32>() {
                                                    on_change.call(PhaseTrigger::BossHpAbove {
                                                        hp_percent: val,
                                                        selector: selector.clone(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                                BossSelector {
                                    selected: selector.clone(),
                                    available_bosses: available_bosses,
                                    on_change: move |sels| on_change.call(PhaseTrigger::BossHpAbove {
                                        hp_percent,
                                        selector: sels,
                                    })
                                }
                            }
                        }
                    },
                    PhaseTrigger::AbilityCast { abilities, source } => {
                        let source_for_abilities = source.clone();
                        let abilities_for_source = abilities.clone();
                        rsx! {
                            AbilitySelectorEditor {
                                label: "Abilities",
                                selectors: abilities,
                                on_change: move |sels| on_change.call(PhaseTrigger::AbilityCast {
                                    abilities: sels,
                                    source: source_for_abilities.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Source",
                                value: source,
                                options: EntityFilter::source_options(),
                                on_change: move |f| on_change.call(PhaseTrigger::AbilityCast {
                                    abilities: abilities_for_source.clone(),
                                    source: f,
                                })
                            }
                        }
                    },
                    PhaseTrigger::EffectApplied { effects, source, target } => {
                        let source_for_effects = source.clone();
                        let target_for_effects = target.clone();
                        let effects_for_source = effects.clone();
                        let target_for_source = target.clone();
                        let effects_for_target = effects.clone();
                        let source_for_target = source.clone();
                        rsx! {
                            EffectSelectorEditor {
                                label: "Effects",
                                selectors: effects,
                                on_change: move |sels| on_change.call(PhaseTrigger::EffectApplied {
                                    effects: sels,
                                    source: source_for_effects.clone(),
                                    target: target_for_effects.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Source",
                                value: source,
                                options: EntityFilter::source_options(),
                                on_change: move |f| on_change.call(PhaseTrigger::EffectApplied {
                                    effects: effects_for_source.clone(),
                                    source: f,
                                    target: target_for_source.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Target",
                                value: target,
                                options: EntityFilter::target_options(),
                                on_change: move |f| on_change.call(PhaseTrigger::EffectApplied {
                                    effects: effects_for_target.clone(),
                                    source: source_for_target.clone(),
                                    target: f,
                                })
                            }
                        }
                    },
                    PhaseTrigger::EffectRemoved { effects, source, target } => {
                        let source_for_effects = source.clone();
                        let target_for_effects = target.clone();
                        let effects_for_source = effects.clone();
                        let target_for_source = target.clone();
                        let effects_for_target = effects.clone();
                        let source_for_target = source.clone();
                        rsx! {
                            EffectSelectorEditor {
                                label: "Effects",
                                selectors: effects,
                                on_change: move |sels| on_change.call(PhaseTrigger::EffectRemoved {
                                    effects: sels,
                                    source: source_for_effects.clone(),
                                    target: target_for_effects.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Source",
                                value: source,
                                options: EntityFilter::source_options(),
                                on_change: move |f| on_change.call(PhaseTrigger::EffectRemoved {
                                    effects: effects_for_source.clone(),
                                    source: f,
                                    target: target_for_source.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Target",
                                value: target,
                                options: EntityFilter::target_options(),
                                on_change: move |f| on_change.call(PhaseTrigger::EffectRemoved {
                                    effects: effects_for_target.clone(),
                                    source: source_for_target.clone(),
                                    target: f,
                                })
                            }
                        }
                    },
                    PhaseTrigger::DamageTaken { abilities, source, target } => {
                        let source_for_abilities = source.clone();
                        let target_for_abilities = target.clone();
                        let abilities_for_source = abilities.clone();
                        let target_for_source = target.clone();
                        let abilities_for_target = abilities.clone();
                        let source_for_target = source.clone();
                        rsx! {
                            AbilitySelectorEditor {
                                label: "Abilities",
                                selectors: abilities,
                                on_change: move |sels| on_change.call(PhaseTrigger::DamageTaken {
                                    abilities: sels,
                                    source: source_for_abilities.clone(),
                                    target: target_for_abilities.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Source",
                                value: source,
                                options: EntityFilter::source_options(),
                                on_change: move |f| on_change.call(PhaseTrigger::DamageTaken {
                                    abilities: abilities_for_source.clone(),
                                    source: f,
                                    target: target_for_source.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Target",
                                value: target,
                                options: EntityFilter::target_options(),
                                on_change: move |f| on_change.call(PhaseTrigger::DamageTaken {
                                    abilities: abilities_for_target.clone(),
                                    source: source_for_target.clone(),
                                    target: f,
                                })
                            }
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
                    PhaseTrigger::NpcAppears { selector } => rsx! {
                        EntitySelectorEditor {
                            label: "Entity (Spawned)",
                            selectors: selector.clone(),
                            on_change: move |sels| on_change.call(PhaseTrigger::NpcAppears {
                                selector: sels
                            })
                        }
                    },
                    PhaseTrigger::EntityDeath { selector } => rsx! {
                        EntitySelectorEditor {
                            label: "Entity (Death)",
                            selectors: selector.clone(),
                            on_change: move |sels| on_change.call(PhaseTrigger::EntityDeath {
                                selector: sels
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
                            source: EntityFilter::default(),
                        },
                        "effect_applied" => CounterTrigger::EffectApplied {
                            effects: vec![],
                            source: EntityFilter::default(),
                            target: EntityFilter::default(),
                        },
                        "effect_removed" => CounterTrigger::EffectRemoved {
                            effects: vec![],
                            source: EntityFilter::default(),
                            target: EntityFilter::default(),
                        },
                        "damage_taken" => CounterTrigger::DamageTaken {
                            abilities: vec![],
                            source: EntityFilter::default(),
                            target: EntityFilter::default(),
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
                            selector: vec![],
                        },
                        "entity_death" => CounterTrigger::EntityDeath {
                            selector: vec![],
                        },
                        "counter_reaches" => CounterTrigger::CounterReaches {
                            counter_id: String::new(),
                            value: 1,
                        },
                        "boss_hp_below" => CounterTrigger::BossHpBelow {
                            hp_percent: 50.0,
                            selector: vec![],
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

                    CounterTrigger::AbilityCast { abilities, source } => {
                        let source_for_abilities = source.clone();
                        let abilities_for_source = abilities.clone();
                        rsx! {
                            AbilitySelectorEditor {
                                label: "Abilities",
                                selectors: abilities,
                                on_change: move |sels| on_change.call(CounterTrigger::AbilityCast {
                                    abilities: sels,
                                    source: source_for_abilities.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Source",
                                value: source,
                                options: EntityFilter::source_options(),
                                on_change: move |f| on_change.call(CounterTrigger::AbilityCast {
                                    abilities: abilities_for_source.clone(),
                                    source: f,
                                })
                            }
                        }
                    },

                    CounterTrigger::EffectApplied { effects, source, target } => {
                        let source_for_effects = source.clone();
                        let target_for_effects = target.clone();
                        let effects_for_source = effects.clone();
                        let target_for_source = target.clone();
                        let effects_for_target = effects.clone();
                        let source_for_target = source.clone();
                        rsx! {
                            EffectSelectorEditor {
                                label: "Effects",
                                selectors: effects,
                                on_change: move |sels| on_change.call(CounterTrigger::EffectApplied {
                                    effects: sels,
                                    source: source_for_effects.clone(),
                                    target: target_for_effects.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Source",
                                value: source,
                                options: EntityFilter::source_options(),
                                on_change: move |f| on_change.call(CounterTrigger::EffectApplied {
                                    effects: effects_for_source.clone(),
                                    source: f,
                                    target: target_for_source.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Target",
                                value: target,
                                options: EntityFilter::target_options(),
                                on_change: move |f| on_change.call(CounterTrigger::EffectApplied {
                                    effects: effects_for_target.clone(),
                                    source: source_for_target.clone(),
                                    target: f,
                                })
                            }
                        }
                    },

                    CounterTrigger::EffectRemoved { effects, source, target } => {
                        let source_for_effects = source.clone();
                        let target_for_effects = target.clone();
                        let effects_for_source = effects.clone();
                        let target_for_source = target.clone();
                        let effects_for_target = effects.clone();
                        let source_for_target = source.clone();
                        rsx! {
                            EffectSelectorEditor {
                                label: "Effects",
                                selectors: effects,
                                on_change: move |sels| on_change.call(CounterTrigger::EffectRemoved {
                                    effects: sels,
                                    source: source_for_effects.clone(),
                                    target: target_for_effects.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Source",
                                value: source,
                                options: EntityFilter::source_options(),
                                on_change: move |f| on_change.call(CounterTrigger::EffectRemoved {
                                    effects: effects_for_source.clone(),
                                    source: f,
                                    target: target_for_source.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Target",
                                value: target,
                                options: EntityFilter::target_options(),
                                on_change: move |f| on_change.call(CounterTrigger::EffectRemoved {
                                    effects: effects_for_target.clone(),
                                    source: source_for_target.clone(),
                                    target: f,
                                })
                            }
                        }
                    },

                    CounterTrigger::DamageTaken { abilities, source, target } => {
                        let source_for_abilities = source.clone();
                        let target_for_abilities = target.clone();
                        let abilities_for_source = abilities.clone();
                        let target_for_source = target.clone();
                        let abilities_for_target = abilities.clone();
                        let source_for_target = source.clone();
                        rsx! {
                            AbilitySelectorEditor {
                                label: "Abilities",
                                selectors: abilities,
                                on_change: move |sels| on_change.call(CounterTrigger::DamageTaken {
                                    abilities: sels,
                                    source: source_for_abilities.clone(),
                                    target: target_for_abilities.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Source",
                                value: source,
                                options: EntityFilter::source_options(),
                                on_change: move |f| on_change.call(CounterTrigger::DamageTaken {
                                    abilities: abilities_for_source.clone(),
                                    source: f,
                                    target: target_for_source.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Target",
                                value: target,
                                options: EntityFilter::target_options(),
                                on_change: move |f| on_change.call(CounterTrigger::DamageTaken {
                                    abilities: abilities_for_target.clone(),
                                    source: source_for_target.clone(),
                                    target: f,
                                })
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

                    CounterTrigger::NpcAppears { selector } => rsx! {
                        EntitySelectorEditor {
                            label: "Entity (Spawned)",
                            selectors: selector.clone(),
                            on_change: move |sels| on_change.call(CounterTrigger::NpcAppears {
                                selector: sels
                            })
                        }
                    },

                    CounterTrigger::EntityDeath { selector } => rsx! {
                        EntitySelectorEditor {
                            label: "Entity (Death)",
                            selectors: selector.clone(),
                            on_change: move |sels| on_change.call(CounterTrigger::EntityDeath {
                                selector: sels
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

                    CounterTrigger::BossHpBelow { hp_percent, selector } => {
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
                                            let selector = selector.clone();
                                            move |e| {
                                                if let Ok(val) = e.value().parse::<f32>() {
                                                    on_change.call(CounterTrigger::BossHpBelow {
                                                        hp_percent: val,
                                                        selector: selector.clone(),
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                                BossSelector {
                                    selected: selector.clone(),
                                    available_bosses: available_bosses,
                                    on_change: move |sels| on_change.call(CounterTrigger::BossHpBelow {
                                        hp_percent,
                                        selector: sels,
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
