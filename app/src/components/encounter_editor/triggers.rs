//! Shared trigger editors
//!
//! Used by timers, phases, and counters for editing trigger conditions.

use dioxus::prelude::*;

use crate::components::form_fields::{Field, NumberField};
use crate::types::{
    AbilitySelector, EffectSelector, EntityFilter, PositionAxis, PositionConstraint, PositionEntity, PositionOp,
    TimerTrigger,
};

use super::selectors::{
    AbilitySelectorEditor, BossSelector, EffectSelectorEditor, EntityFilterDropdown, EntitySelectorEditor, IdSelector,
    MitigationTypeEditor,
};
use super::tabs::EncounterData;

// ─────────────────────────────────────────────────────────────────────────────
// Timer Trigger Editor
// ─────────────────────────────────────────────────────────────────────────────

/// Composable trigger editor for timer triggers
#[component]
pub fn ComposableTriggerEditor(
    trigger: TimerTrigger,
    encounter_data: EncounterData,
    on_change: EventHandler<TimerTrigger>,
    /// Hide trigger types that only work in the timer system (TargetSet, TimeElapsed, Manual, Never)
    #[props(default = false)]
    hide_timer_only: bool,
) -> Element {
    rsx! {
        div { class: "composable-trigger-editor",
            TriggerNode {
                trigger: trigger,
                encounter_data: encounter_data,
                on_change: on_change,
                depth: 0,
                hide_timer_only: hide_timer_only,
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
    #[props(default = false)] hide_timer_only: bool,
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
                    hide_timer_only: hide_timer_only,
                }
            } else {
                SimpleTriggerEditor {
                    trigger: trigger.clone(),
                    encounter_data: encounter_data,
                    on_change: on_change,
                    hide_timer_only: hide_timer_only,
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
    #[props(default = false)] hide_timer_only: bool,
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
                                    hide_timer_only: hide_timer_only,
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
                "+ Add Trigger"
            }
        }
    }
}

/// A handler that applies one field change to a copy of `trigger` and sends
/// it on, leaving every other field as it was.
fn edit<V: 'static>(
    trigger: &TimerTrigger,
    on_change: EventHandler<TimerTrigger>,
    set: fn(&mut TimerTrigger, V),
) -> impl FnMut(V) + 'static {
    let trigger = trigger.clone();
    move |value| {
        let mut edited = trigger.clone();
        set(&mut edited, value);
        on_change.call(edited);
    }
}

fn set_abilities(trigger: &mut TimerTrigger, value: Vec<AbilitySelector>) {
    if let TimerTrigger::AbilityCast { abilities, .. }
    | TimerTrigger::DamageTaken { abilities, .. }
    | TimerTrigger::HealingTaken { abilities, .. }
    | TimerTrigger::ThreatModified { abilities, .. } = trigger
    {
        *abilities = value;
    }
}

fn set_effects(trigger: &mut TimerTrigger, value: Vec<EffectSelector>) {
    if let TimerTrigger::EffectApplied { effects, .. } | TimerTrigger::EffectRemoved { effects, .. } = trigger {
        *effects = value;
    }
}

fn set_source(trigger: &mut TimerTrigger, value: EntityFilter) {
    if let TimerTrigger::AbilityCast { source, .. }
    | TimerTrigger::EffectApplied { source, .. }
    | TimerTrigger::EffectRemoved { source, .. }
    | TimerTrigger::DamageTaken { source, .. }
    | TimerTrigger::HealingTaken { source, .. }
    | TimerTrigger::ThreatModified { source, .. } = trigger
    {
        *source = value;
    }
}

fn set_target(trigger: &mut TimerTrigger, value: EntityFilter) {
    if let TimerTrigger::AbilityCast { target, .. }
    | TimerTrigger::EffectApplied { target, .. }
    | TimerTrigger::EffectRemoved { target, .. }
    | TimerTrigger::DamageTaken { target, .. }
    | TimerTrigger::HealingTaken { target, .. }
    | TimerTrigger::ThreatModified { target, .. } = trigger
    {
        *target = value;
    }
}

/// Editor for simple (non-composite) triggers
#[component]
pub fn SimpleTriggerEditor(
    trigger: TimerTrigger,
    encounter_data: EncounterData,
    on_change: EventHandler<TimerTrigger>,
    /// Hide trigger types that only work in the timer system (TargetSet, TimeElapsed, Manual, Never)
    #[props(default = false)]
    hide_timer_only: bool,
) -> Element {
    let trigger_type = trigger.type_name();

    // The per-field closures below rebuild trigger variants with `position: vec![]`.
    // Re-attach the current constraints so editing other fields doesn't wipe them;
    // the position editor itself calls `raw_on_change` to allow clearing.
    let raw_on_change = on_change;
    let preserved_position = trigger.position_constraints().to_vec();
    let trigger_for_position = trigger.clone();
    let on_change = EventHandler::new(move |t: TimerTrigger| {
        let t = if t.supports_position() && t.position_constraints().is_empty() && !preserved_position.is_empty() {
            t.with_position(preserved_position.clone())
        } else {
            t
        };
        raw_on_change.call(t)
    });

    let trigger_for_type = trigger.clone();
    let source_target = |source: EntityFilter, target: EntityFilter| {
        rsx! {
            EntityFilterDropdown {
                label: "Source",
                value: source,
                options: EntityFilter::source_options(),
                on_change: edit(&trigger, on_change, set_source),
            }
            EntityFilterDropdown {
                label: "Target",
                value: target,
                options: EntityFilter::target_options(),
                on_change: edit(&trigger, on_change, set_target),
            }
        }
    };

    rsx! {
        div { class: "trigger-fields",
            Field { label: "When",
            select {
                value: "{trigger_type}",
                onchange: move |e| {
                    let new_trigger = match e.value().as_str() {
                        "combat_start" => TimerTrigger::CombatStart,
                        "combat_end" => TimerTrigger::CombatEnd,
                        "ability_cast" => TimerTrigger::AbilityCast { abilities: vec![], source: EntityFilter::default(), target: EntityFilter::default(), position: vec![] },
                        "effect_applied" => TimerTrigger::EffectApplied { effects: vec![], source: EntityFilter::default(), target: EntityFilter::default(), position: vec![] },
                        "effect_removed" => TimerTrigger::EffectRemoved { effects: vec![], source: EntityFilter::default(), target: EntityFilter::default(), position: vec![] },
                        "damage_taken" => TimerTrigger::DamageTaken { abilities: vec![], source: EntityFilter::default(), target: EntityFilter::default(), mitigation: vec![], position: vec![] },
                        "healing_taken" => TimerTrigger::HealingTaken { abilities: vec![], source: EntityFilter::default(), target: EntityFilter::default(), position: vec![] },
                        "threat_modified" => TimerTrigger::ThreatModified { abilities: vec![], source: EntityFilter::default(), target: EntityFilter::default(), threat_value: None },
                        "timer_expires" => TimerTrigger::TimerExpires { timer_id: String::new() },
                        "timer_started" => TimerTrigger::TimerStarted { timer_id: String::new() },
                        "timer_canceled" => TimerTrigger::TimerCanceled { timer_id: String::new() },
                        "phase_entered" => TimerTrigger::PhaseEntered { phase_id: String::new() },
                        "phase_ended" => TimerTrigger::PhaseEnded { phase_id: String::new() },
                        "any_phase_change" => TimerTrigger::AnyPhaseChange,
                        "boss_hp_below" => TimerTrigger::BossHpBelow { hp_percent: 50.0, selector: vec![] },
                        "counter_reaches" => TimerTrigger::CounterReaches { counter_id: String::new(), value: 1 },
                        "counter_changes" => TimerTrigger::CounterChanges { counter_id: String::new() },
                        "npc_appears" => TimerTrigger::NpcAppears { selector: vec![], position: vec![] },
                        "entity_death" => TimerTrigger::EntityDeath { selector: vec![], position: vec![] },
                        "target_set" => TimerTrigger::TargetSet { selector: vec![], target: EntityFilter::default() },
                        "time_elapsed" => TimerTrigger::TimeElapsed { secs: 30.0 },
                        "manual" => TimerTrigger::Manual,
                        "never" => TimerTrigger::Never,
                        _ => trigger_for_type.clone(),
                    };
                    on_change.call(new_trigger);
                },
                option { value: "combat_start", selected: trigger_type == "combat_start", "Combat Start" }
                option { value: "combat_end", selected: trigger_type == "combat_end", "Combat End" }
                option { value: "ability_cast", selected: trigger_type == "ability_cast", "Ability Cast" }
                option { value: "effect_applied", selected: trigger_type == "effect_applied", "Effect Applied" }
                option { value: "effect_removed", selected: trigger_type == "effect_removed", "Effect Removed" }
                option { value: "damage_taken", selected: trigger_type == "damage_taken", "Damage Taken" }
                option { value: "healing_taken", selected: trigger_type == "healing_taken", "Healing Taken" }
                option { value: "threat_modified", selected: trigger_type == "threat_modified", "Threat Modified" }
                option { value: "timer_expires", selected: trigger_type == "timer_expires", "Timer Expires" }
                option { value: "timer_started", selected: trigger_type == "timer_started", "Timer Started" }
                option { value: "timer_canceled", selected: trigger_type == "timer_canceled", "Timer Canceled" }
                option { value: "phase_entered", selected: trigger_type == "phase_entered", "Phase Entered" }
                option { value: "phase_ended", selected: trigger_type == "phase_ended", "Phase Ended" }
                option { value: "any_phase_change", selected: trigger_type == "any_phase_change", "Any Phase Change" }
                option { value: "boss_hp_below", selected: trigger_type == "boss_hp_below", "Boss HP Below" }
                option { value: "counter_reaches", selected: trigger_type == "counter_reaches", "Counter Reaches" }
                option { value: "counter_changes", selected: trigger_type == "counter_changes", "Counter Changes" }
                option { value: "npc_appears", selected: trigger_type == "npc_appears", "NPC Appears" }
                option { value: "entity_death", selected: trigger_type == "entity_death", "Entity Death" }
                // Timer-only options (hidden for phases/counters)
                if !hide_timer_only {
                    option { value: "target_set", selected: trigger_type == "target_set", "Target Set" }
                    option { value: "time_elapsed", selected: trigger_type == "time_elapsed", "Time Elapsed" }
                    option { value: "manual", selected: trigger_type == "manual", "Manual" }
                    option { value: "never", selected: trigger_type == "never", "Never" }
                }
            }
            }

            // What the picked event needs, nested under it
            div { class: "trigger-details",
            {
                match trigger.clone() {
                    TimerTrigger::CombatStart
                    | TimerTrigger::CombatEnd
                    | TimerTrigger::AnyPhaseChange
                    | TimerTrigger::Never
                    | TimerTrigger::Manual => rsx! {},
                    TimerTrigger::AbilityCast { abilities, source, target, .. }
                    | TimerTrigger::HealingTaken { abilities, source, target, .. } => rsx! {
                        AbilitySelectorEditor {
                            label: "Abilities",
                            selectors: abilities,
                            on_change: edit(&trigger, on_change, set_abilities),
                        }
                        {source_target(source, target)}
                    },
                    TimerTrigger::EffectApplied { effects, source, target, .. }
                    | TimerTrigger::EffectRemoved { effects, source, target, .. } => rsx! {
                        EffectSelectorEditor {
                            label: "Effects",
                            selectors: effects,
                            on_change: edit(&trigger, on_change, set_effects),
                        }
                        {source_target(source, target)}
                    },
                    TimerTrigger::DamageTaken { abilities, source, target, mitigation, .. } => rsx! {
                        AbilitySelectorEditor {
                            label: "Abilities",
                            selectors: abilities,
                            on_change: edit(&trigger, on_change, set_abilities),
                        }
                        {source_target(source, target)}
                        MitigationTypeEditor {
                            selected: mitigation,
                            on_change: edit(&trigger, on_change, |t, m| {
                                if let TimerTrigger::DamageTaken { mitigation, .. } = t {
                                    *mitigation = m;
                                }
                            }),
                        }
                    },
                    TimerTrigger::ThreatModified { abilities, source, target, .. } => rsx! {
                        AbilitySelectorEditor {
                            label: "Abilities",
                            selectors: abilities,
                            on_change: edit(&trigger, on_change, set_abilities),
                        }
                        {source_target(source, target)}
                    },
                    TimerTrigger::TimerExpires { timer_id } => {
                        let available_timers = encounter_data.timer_options();
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
                        let available_timers = encounter_data.timer_options();
                        rsx! {
                            IdSelector {
                                label: "Timer",
                                value: timer_id,
                                available: available_timers,
                                on_change: move |id| on_change.call(TimerTrigger::TimerStarted { timer_id: id })
                            }
                        }
                    },
                    TimerTrigger::TimerCanceled { timer_id } => {
                        let available_timers = encounter_data.timer_options();
                        rsx! {
                            IdSelector {
                                label: "Timer",
                                value: timer_id,
                                available: available_timers,
                                on_change: move |id| on_change.call(TimerTrigger::TimerCanceled { timer_id: id })
                            }
                        }
                    },
                    TimerTrigger::PhaseEntered { phase_id } => {
                        let available_phases = encounter_data.phase_options();
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
                        let available_phases = encounter_data.phase_options();
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
                            Fragment {
                                NumberField {
                                    label: "HP below",
                                    unit: "%",
                                    step: "0.1",
                                    min: "0",
                                    max: "100",
                                    value: "{hp_percent}",
                                    on_input: {
                                        let selector = selector.clone();
                                        move |v: String| {
                                            if let Ok(val) = v.parse::<f32>() {
                                                on_change.call(TimerTrigger::BossHpBelow {
                                                    hp_percent: val,
                                                    selector: selector.clone(),
                                                });
                                            }
                                        }
                                    },
                                }
                                BossSelector {
                                    selected: selector.clone(),
                                    available_bosses,
                                    on_change: move |sels| on_change.call(TimerTrigger::BossHpBelow {
                                        hp_percent,
                                        selector: sels,
                                    })
                                }
                            }
                        }
                    },
                    TimerTrigger::CounterReaches { counter_id, value } => {
                        let available_counters = encounter_data.counter_options();
                        rsx! {
                            Fragment {
                                IdSelector {
                                    label: "Counter",
                                    value: counter_id.clone(),
                                    available: available_counters,
                                    on_change: move |id| on_change.call(TimerTrigger::CounterReaches {
                                        counter_id: id,
                                        value
                                    })
                                }
                                NumberField {
                                    label: "Value",
                                    min: "0",
                                    step: "1",
                                    value: "{value}",
                                    on_input: {
                                        let counter_id = counter_id.clone();
                                        move |v: String| {
                                            if let Ok(val) = v.parse::<u32>() {
                                                on_change.call(TimerTrigger::CounterReaches {
                                                    counter_id: counter_id.clone(),
                                                    value: val
                                                });
                                            }
                                        }
                                    },
                                }
                            }
                        }
                    },
                    TimerTrigger::CounterChanges { counter_id } => {
                        let available_counters = encounter_data.counter_options();
                        rsx! {
                            IdSelector {
                                label: "Counter",
                                value: counter_id,
                                available: available_counters,
                                on_change: move |id| on_change.call(TimerTrigger::CounterChanges {
                                    counter_id: id,
                                })
                            }
                        }
                    },
                    TimerTrigger::NpcAppears { selector, .. } => rsx! {
                        EntitySelectorEditor {
                            label: "Entity",
                            selectors: selector.clone(),
                            on_change: move |sels| on_change.call(TimerTrigger::NpcAppears {
                                selector: sels,
                                position: vec![],
                            })
                        }
                    },
                    TimerTrigger::EntityDeath { selector, .. } => rsx! {
                        EntitySelectorEditor {
                            label: "Entity",
                            selectors: selector.clone(),
                            on_change: move |sels| on_change.call(TimerTrigger::EntityDeath {
                                selector: sels,
                                position: vec![],
                            })
                        }
                    },
                    TimerTrigger::TargetSet { selector, target } => {
                        let target_for_selector = target.clone();
                        let selector_for_target = selector.clone();
                        rsx! {
                            EntitySelectorEditor {
                                label: "Set by",
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
                        NumberField {
                            label: "After",
                            unit: "s into combat",
                            step: "0.1",
                            min: "0",
                            value: "{secs}",
                            on_input: move |v: String| {
                                if let Ok(val) = v.parse::<f32>() {
                                    on_change.call(TimerTrigger::TimeElapsed { secs: val });
                                }
                            },
                        }
                    },
                    _ => rsx! {
                        span { class: "hint", "Composite trigger" }
                    },
                }
            }

            // Position constraints (shared across all event-driven trigger types)
            if trigger.supports_position() {
                if !trigger.position_constraints().is_empty() {
                    span { class: "text-sm font-bold text-secondary mt-sm",
                        "Position Constraints"
                        span {
                            class: "help-icon",
                            title: "Coordinate gates on the event's source/target entities. ALL must pass for the trigger to fire. Hover entities in the Combat Log view to find coordinates.",
                            "?"
                        }
                    }
                }
                PositionConstraintList {
                    constraints: trigger.position_constraints().to_vec(),
                    // npc_appears/entity_death always check the subject NPC — no entity choice
                    show_entity: !matches!(
                        trigger,
                        TimerTrigger::NpcAppears { .. } | TimerTrigger::EntityDeath { .. }
                    ),
                    on_change: move |p: Vec<PositionConstraint>| {
                        raw_on_change.call(trigger_for_position.clone().with_position(p));
                    }
                }
            }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Position Constraint Editor
// ─────────────────────────────────────────────────────────────────────────────

/// List editor for a trigger's position constraints (AND semantics).
/// Mirrors the Conditions subsection layout (condition-card rows).
#[component]
fn PositionConstraintList(
    constraints: Vec<PositionConstraint>,
    on_change: EventHandler<Vec<PositionConstraint>>,
    /// Hide the source/target dropdown for triggers where the subject entity
    /// is implicit (e.g. npc_appears)
    #[props(default = true)]
    show_entity: bool,
) -> Element {
    let constraints_for_add = constraints.clone();
    let is_empty = constraints.is_empty();
    rsx! {
        div { class: "conditions-editor",
            for (idx, constraint) in constraints.iter().enumerate() {
                div { class: "condition-card", key: "{idx}",
                    div { class: "condition-card-content",
                        PositionConstraintRow {
                            constraint: constraint.clone(),
                            show_entity,
                            on_change: {
                                let constraints = constraints.clone();
                                move |c: PositionConstraint| {
                                    let mut updated = constraints.clone();
                                    updated[idx] = c;
                                    on_change.call(updated);
                                }
                            }
                        }
                    }
                    button {
                        class: "btn btn-danger btn-sm",
                        title: "Remove constraint",
                        onclick: {
                            let constraints = constraints.clone();
                            move |_| {
                                let mut updated = constraints.clone();
                                updated.remove(idx);
                                on_change.call(updated);
                            }
                        },
                        "×"
                    }
                }
            }

            button {
                class: if is_empty { "btn-compose" } else { "btn-dashed text-sm" },
                style: if is_empty { "align-self: flex-start;" } else { "" },
                onclick: move |_| {
                    let mut updated = constraints_for_add.clone();
                    updated.push(PositionConstraint {
                        entity: PositionEntity::Source,
                        axis: PositionAxis::X,
                        op: PositionOp::Between { min: 0.0, max: 0.0 },
                    });
                    on_change.call(updated);
                },
                if is_empty { "+ Position Constraint" } else { "+ Add Position Constraint" }
            }
        }
    }
}

/// One position constraint: entity + axis + operator + value(s).
#[component]
fn PositionConstraintRow(
    constraint: PositionConstraint,
    on_change: EventHandler<PositionConstraint>,
    #[props(default = true)] show_entity: bool,
) -> Element {
    let c_entity = constraint.clone();
    let c_axis = constraint.clone();
    let c_op = constraint.clone();
    let c_val = constraint.clone();
    let c_min = constraint.clone();
    let c_max = constraint.clone();

    let (op_name, value, min, max) = match constraint.op {
        PositionOp::Gt { value } => ("gt", value, 0.0, 0.0),
        PositionOp::Gte { value } => ("gte", value, 0.0, 0.0),
        PositionOp::Lt { value } => ("lt", value, 0.0, 0.0),
        PositionOp::Lte { value } => ("lte", value, 0.0, 0.0),
        PositionOp::Between { min, max } => ("between", 0.0, min, max),
    };
    let is_between = op_name == "between";

    let entity_name = match constraint.entity {
        PositionEntity::Source => "source",
        PositionEntity::Target => "target",
    };
    let axis_name = match constraint.axis {
        PositionAxis::X => "x",
        PositionAxis::Y => "y",
        PositionAxis::Z => "z",
        PositionAxis::Facing => "facing",
        PositionAxis::XMinusY => "x_minus_y",
        PositionAxis::XPlusY => "x_plus_y",
    };

    rsx! {
        div { class: "flex items-center gap-xs",
            if show_entity {
                select {
                    class: "select w-sm",
                    value: "{entity_name}",
                    onchange: move |e| {
                        let mut c = c_entity.clone();
                        c.entity = if e.value() == "target" {
                            PositionEntity::Target
                        } else {
                            PositionEntity::Source
                        };
                        on_change.call(c);
                    },
                    option { value: "source", selected: entity_name == "source", "Source" }
                    option { value: "target", selected: entity_name == "target", "Target" }
                }
            }
            select {
                class: "select w-sm",
                value: "{axis_name}",
                onchange: move |e| {
                    let mut c = c_axis.clone();
                    c.axis = match e.value().as_str() {
                        "y" => PositionAxis::Y,
                        "z" => PositionAxis::Z,
                        "facing" => PositionAxis::Facing,
                        "x_minus_y" => PositionAxis::XMinusY,
                        "x_plus_y" => PositionAxis::XPlusY,
                        _ => PositionAxis::X,
                    };
                    on_change.call(c);
                },
                option { value: "x", selected: axis_name == "x", "X" }
                option { value: "y", selected: axis_name == "y", "Y" }
                option { value: "z", selected: axis_name == "z", "Z" }
                option { value: "facing", selected: axis_name == "facing", "Facing" }
                option { value: "x_minus_y", selected: axis_name == "x_minus_y", "X − Y" }
                option { value: "x_plus_y", selected: axis_name == "x_plus_y", "X + Y" }
            }
            select {
                class: "select w-sm",
                value: "{op_name}",
                onchange: move |e| {
                    let mut c = c_op.clone();
                    // Carry the current scalar across op changes where possible
                    let current = match c.op {
                        PositionOp::Gt { value }
                        | PositionOp::Gte { value }
                        | PositionOp::Lt { value }
                        | PositionOp::Lte { value } => value,
                        PositionOp::Between { min, .. } => min,
                    };
                    c.op = match e.value().as_str() {
                        "gt" => PositionOp::Gt { value: current },
                        "gte" => PositionOp::Gte { value: current },
                        "lt" => PositionOp::Lt { value: current },
                        "lte" => PositionOp::Lte { value: current },
                        _ => PositionOp::Between { min: current, max: current },
                    };
                    on_change.call(c);
                },
                option { value: "between", selected: op_name == "between", "Between" }
                option { value: "gt", selected: op_name == "gt", ">" }
                option { value: "gte", selected: op_name == "gte", ">=" }
                option { value: "lt", selected: op_name == "lt", "<" }
                option { value: "lte", selected: op_name == "lte", "<=" }
            }
            if is_between {
                input {
                    r#type: "number",
                    step: "0.1",
                    class: "input-inline w-sm",
                    value: "{min}",
                    oninput: move |e| {
                        if let Ok(v) = e.value().parse::<f32>()
                            && let PositionOp::Between { max, .. } = c_min.op
                        {
                            let mut c = c_min.clone();
                            c.op = PositionOp::Between { min: v, max };
                            on_change.call(c);
                        }
                    }
                }
                span { class: "text-sm text-secondary", "to" }
                input {
                    r#type: "number",
                    step: "0.1",
                    class: "input-inline w-sm",
                    value: "{max}",
                    oninput: move |e| {
                        if let Ok(v) = e.value().parse::<f32>()
                            && let PositionOp::Between { min, .. } = c_max.op
                        {
                            let mut c = c_max.clone();
                            c.op = PositionOp::Between { min, max: v };
                            on_change.call(c);
                        }
                    }
                }
            } else {
                input {
                    r#type: "number",
                    step: "0.1",
                    class: "input-inline w-sm",
                    value: "{value}",
                    oninput: move |e| {
                        if let Ok(v) = e.value().parse::<f32>() {
                            let mut c = c_val.clone();
                            c.op = match c.op {
                                PositionOp::Gt { .. } => PositionOp::Gt { value: v },
                                PositionOp::Gte { .. } => PositionOp::Gte { value: v },
                                PositionOp::Lt { .. } => PositionOp::Lt { value: v },
                                PositionOp::Lte { .. } => PositionOp::Lte { value: v },
                                between => between,
                            };
                            on_change.call(c);
                        }
                    }
                }
            }
        }
    }
}
