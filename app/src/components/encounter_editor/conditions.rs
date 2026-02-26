//! Condition editors
//!
//! State-based conditions for gating timers, phases, and victory triggers.
//! Supports recursive composition via AllOf, AnyOf, and Not.

use dioxus::prelude::*;

use super::tabs::EncounterData;
use super::timers::PhaseSelector;
use crate::types::{ComparisonOp, Condition, CounterCondition};

// ═══════════════════════════════════════════════════════════════════════════
// Legacy Counter Condition Editor (kept for backward compat display)
// ═══════════════════════════════════════════════════════════════════════════

/// Editor for counter conditions (legacy)
/// Shows empty by default, selecting a counter enables the condition
#[component]
pub fn CounterConditionEditor(
    condition: Option<CounterCondition>,
    counters: Vec<String>,
    on_change: EventHandler<Option<CounterCondition>>,
) -> Element {
    let effective_condition = condition.clone().filter(|c| !c.counter_id.is_empty());

    let cond = effective_condition.clone().unwrap_or(CounterCondition {
        counter_id: String::new(),
        operator: ComparisonOp::Eq,
        value: 1,
    });

    let op_value = match cond.operator {
        ComparisonOp::Eq => "eq",
        ComparisonOp::Lt => "lt",
        ComparisonOp::Gt => "gt",
        ComparisonOp::Lte => "lte",
        ComparisonOp::Gte => "gte",
        ComparisonOp::Ne => "ne",
    };

    let selected_value = if cond.counter_id.is_empty() {
        "__none__".to_string()
    } else {
        cond.counter_id.clone()
    };

    rsx! {
        div { class: "flex items-center gap-xs",
            select {
                class: "select",
                style: "width: 140px;",
                value: "{selected_value}",
                onchange: {
                    let cond_clone = cond.clone();
                    move |e| {
                        if e.value() == "__none__" {
                            on_change.call(None);
                        } else {
                            on_change.call(Some(CounterCondition {
                                counter_id: e.value(),
                                operator: cond_clone.operator,
                                value: cond_clone.value,
                            }));
                        }
                    }
                },
                option { value: "__none__", selected: selected_value == "__none__", "(none)" }
                if counters.is_empty() {
                    option { value: "__none__", disabled: true, "No counters defined" }
                }
                for counter_id in &counters {
                    option {
                        value: "{counter_id}",
                        selected: *counter_id == selected_value,
                        "{counter_id}"
                    }
                }
            }

            if effective_condition.is_some() {
                select {
                    class: "select",
                    style: "width: 55px;",
                    value: "{op_value}",
                    onchange: {
                        let cond_clone = cond.clone();
                        move |e| {
                            let op = match e.value().as_str() {
                                "eq" => ComparisonOp::Eq,
                                "lt" => ComparisonOp::Lt,
                                "gt" => ComparisonOp::Gt,
                                "lte" => ComparisonOp::Lte,
                                "gte" => ComparisonOp::Gte,
                                "ne" => ComparisonOp::Ne,
                                _ => ComparisonOp::Eq,
                            };
                            on_change.call(Some(CounterCondition {
                                counter_id: cond_clone.counter_id.clone(),
                                operator: op,
                                value: cond_clone.value,
                            }));
                        }
                    },
                    option { value: "eq", selected: op_value == "eq", "=" }
                    option { value: "lt", selected: op_value == "lt", "<" }
                    option { value: "gt", selected: op_value == "gt", ">" }
                    option { value: "lte", selected: op_value == "lte", "≤" }
                    option { value: "gte", selected: op_value == "gte", "≥" }
                    option { value: "ne", selected: op_value == "ne", "≠" }
                }

                input {
                    r#type: "number",
                    class: "input-inline",
                    style: "width: 55px;",
                    min: "0",
                    value: "{cond.value}",
                    oninput: {
                        let cond_clone = cond.clone();
                        move |e| {
                            if let Ok(val) = e.value().parse::<u32>() {
                                on_change.call(Some(CounterCondition {
                                    counter_id: cond_clone.counter_id.clone(),
                                    operator: cond_clone.operator,
                                    value: val,
                                }));
                            }
                        }
                    }
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// New Unified Conditions Editor
// ═══════════════════════════════════════════════════════════════════════════

/// Top-level conditions editor. Manages a `Vec<Condition>` (implicitly AND'd).
#[component]
pub fn ConditionsEditor(
    conditions: Vec<Condition>,
    encounter_data: EncounterData,
    on_change: EventHandler<Vec<Condition>>,
) -> Element {
    let conditions_for_add = conditions.clone();

    rsx! {
        div { class: "conditions-editor",
            if conditions.is_empty() {
                span { class: "conditions-editor-empty", "No conditions (always active)" }
            }

            for (idx, condition) in conditions.iter().enumerate() {
                {
                    let conditions_for_update = conditions.clone();
                    let conditions_for_remove = conditions.clone();
                    let condition_clone = condition.clone();
                    let encounter_data_clone = encounter_data.clone();

                    rsx! {
                        div { class: "condition-card",
                            div { class: "condition-card-content",
                                ConditionNode {
                                    condition: condition_clone,
                                    encounter_data: encounter_data_clone,
                                    on_change: move |new_cond| {
                                        let mut new_conditions = conditions_for_update.clone();
                                        new_conditions[idx] = new_cond;
                                        on_change.call(new_conditions);
                                    },
                                    depth: 0,
                                }
                            }
                            button {
                                class: "btn btn-danger btn-sm",
                                title: "Remove condition",
                                onclick: move |_| {
                                    let mut new_conditions = conditions_for_remove.clone();
                                    new_conditions.remove(idx);
                                    on_change.call(new_conditions);
                                },
                                "×"
                            }
                        }
                    }
                }
            }

            button {
                class: "btn-dashed text-sm",
                onclick: move |_| {
                    let mut new_conditions = conditions_for_add.clone();
                    new_conditions.push(Condition::PhaseActive { phase_ids: vec![] });
                    on_change.call(new_conditions);
                },
                "+ Add Condition"
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Recursive Condition Node
// ═══════════════════════════════════════════════════════════════════════════

/// Recursive condition dispatcher. Routes to composite, not, or simple editor.
#[component]
fn ConditionNode(
    condition: Condition,
    encounter_data: EncounterData,
    on_change: EventHandler<Condition>,
    depth: u8,
) -> Element {
    let indent = format!("padding-left: {}px;", depth as u32 * 12);

    rsx! {
        div {
            class: "condition-node",
            style: "{indent}",

            match &condition {
                Condition::AllOf { .. } | Condition::AnyOf { .. } => rsx! {
                    CompositeConditionEditor {
                        condition: condition.clone(),
                        encounter_data: encounter_data.clone(),
                        on_change: on_change,
                        depth: depth,
                    }
                },
                Condition::Not { .. } => rsx! {
                    NotConditionEditor {
                        condition: condition.clone(),
                        encounter_data: encounter_data.clone(),
                        on_change: on_change,
                        depth: depth,
                    }
                },
                _ => rsx! {
                    SimpleConditionEditor {
                        condition: condition.clone(),
                        encounter_data: encounter_data.clone(),
                        on_change: on_change,
                    }
                },
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Composite Condition Editor (AllOf / AnyOf)
// ═══════════════════════════════════════════════════════════════════════════

#[component]
fn CompositeConditionEditor(
    condition: Condition,
    encounter_data: EncounterData,
    on_change: EventHandler<Condition>,
    depth: u8,
) -> Element {
    let (is_all_of, children) = match &condition {
        Condition::AllOf { conditions } => (true, conditions.clone()),
        Condition::AnyOf { conditions } => (false, conditions.clone()),
        _ => return rsx! { span { "Invalid composite" } },
    };

    let label = if is_all_of {
        "ALL OF (AND)"
    } else {
        "ANY OF (OR)"
    };
    let css_class = if is_all_of {
        "composite-condition--all-of"
    } else {
        "composite-condition--any-of"
    };

    let children_for_unwrap = children.clone();
    let children_for_add = children.clone();
    let children_len = children.len();

    rsx! {
        div { class: "{css_class}",
            // Header
            div { class: "composite-header",
                span { class: "composite-label", "{label}" }

                // Toggle between AllOf/AnyOf
                button {
                    class: "btn-compose",
                    title: if is_all_of { "Switch to OR" } else { "Switch to AND" },
                    onclick: {
                        let children_toggle = children.clone();
                        move |_| {
                            if is_all_of {
                                on_change.call(Condition::AnyOf { conditions: children_toggle.clone() });
                            } else {
                                on_change.call(Condition::AllOf { conditions: children_toggle.clone() });
                            }
                        }
                    },
                    if is_all_of { "-> OR" } else { "-> AND" }
                }

                // Unwrap if single child
                if children_len == 1 {
                    button {
                        class: "btn-compose",
                        onclick: move |_| {
                            if let Some(first) = children_for_unwrap.first() {
                                on_change.call(first.clone());
                            }
                        },
                        "Unwrap"
                    }
                }
            }

            // Children
            div { class: "composite-conditions",
                for (idx, child) in children.iter().enumerate() {
                    {
                        let children_for_update = children.clone();
                        let children_for_remove = children.clone();
                        let child_clone = child.clone();
                        let encounter_data_clone = encounter_data.clone();

                        rsx! {
                            div { class: "condition-item",
                                ConditionNode {
                                    condition: child_clone,
                                    encounter_data: encounter_data_clone,
                                    on_change: {
                                        let is_all = is_all_of;
                                        move |new_cond| {
                                            let mut new_children = children_for_update.clone();
                                            new_children[idx] = new_cond;
                                            if is_all {
                                                on_change.call(Condition::AllOf { conditions: new_children });
                                            } else {
                                                on_change.call(Condition::AnyOf { conditions: new_children });
                                            }
                                        }
                                    },
                                    depth: depth + 1,
                                }
                                if children_len > 1 {
                                    button {
                                        class: "btn btn-danger btn-sm",
                                        onclick: {
                                            let is_all = is_all_of;
                                            move |_| {
                                                let mut new_children = children_for_remove.clone();
                                                new_children.remove(idx);
                                                if is_all {
                                                    on_change.call(Condition::AllOf { conditions: new_children });
                                                } else {
                                                    on_change.call(Condition::AnyOf { conditions: new_children });
                                                }
                                            }
                                        },
                                        "×"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Add child button
            button {
                class: "btn-dashed text-sm",
                onclick: {
                    move |_| {
                        let mut new_children = children_for_add.clone();
                        new_children.push(Condition::PhaseActive { phase_ids: vec![] });
                        if is_all_of {
                            on_change.call(Condition::AllOf { conditions: new_children });
                        } else {
                            on_change.call(Condition::AnyOf { conditions: new_children });
                        }
                    }
                },
                "+ Add Condition"
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Not Condition Editor
// ═══════════════════════════════════════════════════════════════════════════

#[component]
fn NotConditionEditor(
    condition: Condition,
    encounter_data: EncounterData,
    on_change: EventHandler<Condition>,
    depth: u8,
) -> Element {
    let inner = match &condition {
        Condition::Not { condition: inner } => *inner.clone(),
        _ => return rsx! { span { "Invalid not" } },
    };

    let inner_for_unwrap = inner.clone();

    rsx! {
        div { class: "condition-not",
            div { class: "composite-header",
                span { class: "composite-label", "NOT" }
                button {
                    class: "btn-compose",
                    title: "Remove negation (unwrap)",
                    onclick: move |_| {
                        on_change.call(inner_for_unwrap.clone());
                    },
                    "Unwrap"
                }
            }
            div { class: "composite-conditions",
                ConditionNode {
                    condition: inner.clone(),
                    encounter_data: encounter_data.clone(),
                    on_change: move |new_inner| {
                        on_change.call(Condition::Not {
                            condition: Box::new(new_inner),
                        });
                    },
                    depth: depth + 1,
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Simple Condition Editor (leaf nodes)
// ═══════════════════════════════════════════════════════════════════════════

/// Returns the type name string for a condition (for the dropdown value).
fn condition_type_name(condition: &Condition) -> &'static str {
    match condition {
        Condition::PhaseActive { .. } => "phase_active",
        Condition::CounterCompare { .. } => "counter_compare",
        Condition::TimerTimeRemaining { .. } => "timer_time_remaining",
        Condition::AllOf { .. } => "all_of",
        Condition::AnyOf { .. } => "any_of",
        Condition::Not { .. } => "not",
    }
}

/// Create a default condition for a given type name.
fn default_condition_for_type(type_name: &str, current: &Condition) -> Condition {
    match type_name {
        "phase_active" => Condition::PhaseActive { phase_ids: vec![] },
        "counter_compare" => Condition::CounterCompare {
            counter_id: String::new(),
            operator: ComparisonOp::Gte,
            value: 1,
        },
        "timer_time_remaining" => Condition::TimerTimeRemaining {
            timer_id: String::new(),
            operator: ComparisonOp::Gte,
            value: 0.0,
        },
        // Composite: wrap current condition
        "all_of" => Condition::AllOf {
            conditions: vec![current.clone()],
        },
        "any_of" => Condition::AnyOf {
            conditions: vec![current.clone()],
        },
        "not" => Condition::Not {
            condition: Box::new(current.clone()),
        },
        _ => Condition::PhaseActive { phase_ids: vec![] },
    }
}

#[component]
fn SimpleConditionEditor(
    condition: Condition,
    encounter_data: EncounterData,
    on_change: EventHandler<Condition>,
) -> Element {
    let type_name = condition_type_name(&condition);

    rsx! {
        div { class: "condition-simple",
            // Type selector dropdown
            div { class: "flex items-center gap-xs",
                select {
                    class: "select",
                    style: "width: 160px;",
                    value: "{type_name}",
                    onchange: {
                        let current = condition.clone();
                        move |e| {
                            on_change.call(default_condition_for_type(&e.value(), &current));
                        }
                    },
                    option { value: "phase_active", selected: type_name == "phase_active", "Phase Active" }
                    option { value: "counter_compare", selected: type_name == "counter_compare", "Counter Compare" }
                    option { value: "timer_time_remaining", selected: type_name == "timer_time_remaining", "Timer Time Remaining" }
                    // Composite wrappers
                    option { value: "all_of", selected: type_name == "all_of", "All Of (AND)" }
                    option { value: "any_of", selected: type_name == "any_of", "Any Of (OR)" }
                    option { value: "not", selected: type_name == "not", "Not" }
                }
            }

            // Type-specific fields
            div { class: "condition-simple-fields",
                match &condition {
                    Condition::PhaseActive { phase_ids } => rsx! {
                        PhaseActiveEditor {
                            phase_ids: phase_ids.clone(),
                            encounter_data: encounter_data.clone(),
                            on_change: on_change,
                        }
                    },
                    Condition::CounterCompare { counter_id, operator, value } => rsx! {
                        CounterCompareEditor {
                            counter_id: counter_id.clone(),
                            operator: *operator,
                            value: *value,
                            encounter_data: encounter_data.clone(),
                            on_change: on_change,
                        }
                    },
                    Condition::TimerTimeRemaining { timer_id, operator, value } => rsx! {
                        TimerTimeRemainingEditor {
                            timer_id: timer_id.clone(),
                            operator: *operator,
                            value: *value,
                            encounter_data: encounter_data.clone(),
                            on_change: on_change,
                        }
                    },
                    // AllOf/AnyOf/Not shouldn't render here (handled by ConditionNode),
                    // but if they do, show nothing
                    _ => rsx! {},
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Type-Specific Sub-Editors
// ═══════════════════════════════════════════════════════════════════════════

/// Phase Active condition: select which phases
#[component]
fn PhaseActiveEditor(
    phase_ids: Vec<String>,
    encounter_data: EncounterData,
    on_change: EventHandler<Condition>,
) -> Element {
    rsx! {
        PhaseSelector {
            selected: phase_ids,
            available: encounter_data.phase_ids(),
            on_change: move |new_phases| {
                on_change.call(Condition::PhaseActive { phase_ids: new_phases });
            }
        }
    }
}

/// Counter Compare condition: counter dropdown + operator + value
#[component]
fn CounterCompareEditor(
    counter_id: String,
    operator: ComparisonOp,
    value: u32,
    encounter_data: EncounterData,
    on_change: EventHandler<Condition>,
) -> Element {
    let op_value = match operator {
        ComparisonOp::Eq => "eq",
        ComparisonOp::Lt => "lt",
        ComparisonOp::Gt => "gt",
        ComparisonOp::Lte => "lte",
        ComparisonOp::Gte => "gte",
        ComparisonOp::Ne => "ne",
    };

    let selected_counter = if counter_id.is_empty() {
        "__none__".to_string()
    } else {
        counter_id.clone()
    };

    let counters = encounter_data.counter_ids();
    let counter_id_for_op = counter_id.clone();
    let counter_id_for_val = counter_id.clone();
    let has_counter = !counter_id.is_empty();

    rsx! {
        div { class: "flex items-center gap-xs",
            select {
                class: "select",
                style: "width: 140px;",
                value: "{selected_counter}",
                onchange: move |e| {
                    let new_id = if e.value() == "__none__" {
                        String::new()
                    } else {
                        e.value()
                    };
                    on_change.call(Condition::CounterCompare {
                        counter_id: new_id,
                        operator,
                        value,
                    });
                },
                option { value: "__none__", selected: selected_counter == "__none__", "(select counter)" }
                for cid in &counters {
                    option { value: "{cid}", selected: *cid == selected_counter, "{cid}" }
                }
            }

            if has_counter {
                select {
                    class: "select",
                    style: "width: 55px;",
                    value: "{op_value}",
                    onchange: {
                        let cid = counter_id_for_op.clone();
                        move |e| {
                            let op = match e.value().as_str() {
                                "eq" => ComparisonOp::Eq,
                                "lt" => ComparisonOp::Lt,
                                "gt" => ComparisonOp::Gt,
                                "lte" => ComparisonOp::Lte,
                                "gte" => ComparisonOp::Gte,
                                "ne" => ComparisonOp::Ne,
                                _ => ComparisonOp::Eq,
                            };
                            on_change.call(Condition::CounterCompare {
                                counter_id: cid.clone(),
                                operator: op,
                                value,
                            });
                        }
                    },
                    option { value: "eq", selected: op_value == "eq", "=" }
                    option { value: "lt", selected: op_value == "lt", "<" }
                    option { value: "gt", selected: op_value == "gt", ">" }
                    option { value: "lte", selected: op_value == "lte", "\u{2264}" }
                    option { value: "gte", selected: op_value == "gte", "\u{2265}" }
                    option { value: "ne", selected: op_value == "ne", "\u{2260}" }
                }

                input {
                    r#type: "number",
                    class: "input-inline",
                    style: "width: 55px;",
                    min: "0",
                    value: "{value}",
                    oninput: {
                        let cid = counter_id_for_val.clone();
                        move |e| {
                            if let Ok(val) = e.value().parse::<u32>() {
                                on_change.call(Condition::CounterCompare {
                                    counter_id: cid.clone(),
                                    operator,
                                    value: val,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Timer Time Remaining condition: timer dropdown + operator (gte/lte) + seconds value
#[component]
fn TimerTimeRemainingEditor(
    timer_id: String,
    operator: ComparisonOp,
    value: f32,
    encounter_data: EncounterData,
    on_change: EventHandler<Condition>,
) -> Element {
    let op_value = match operator {
        ComparisonOp::Gte => "gte",
        ComparisonOp::Lte => "lte",
        // Fallback display for hand-edited TOML with other ops
        ComparisonOp::Gt => "gte",
        ComparisonOp::Lt => "lte",
        ComparisonOp::Eq => "gte",
        ComparisonOp::Ne => "gte",
    };

    let selected_timer = if timer_id.is_empty() {
        "__none__".to_string()
    } else {
        timer_id.clone()
    };

    let timers = encounter_data.countdown_timer_ids();
    let timer_id_for_op = timer_id.clone();
    let timer_id_for_val = timer_id.clone();
    let has_timer = !timer_id.is_empty();

    rsx! {
        div { class: "flex items-center gap-xs",
            select {
                class: "select",
                style: "width: 140px;",
                value: "{selected_timer}",
                onchange: move |e| {
                    let new_id = if e.value() == "__none__" {
                        String::new()
                    } else {
                        e.value()
                    };
                    on_change.call(Condition::TimerTimeRemaining {
                        timer_id: new_id,
                        operator,
                        value,
                    });
                },
                option { value: "__none__", selected: selected_timer == "__none__", "(select timer)" }
                for tid in &timers {
                    option { value: "{tid}", selected: *tid == selected_timer, "{tid}" }
                }
            }

            if has_timer {
                select {
                    class: "select",
                    style: "width: 55px;",
                    value: "{op_value}",
                    onchange: {
                        let tid = timer_id_for_op.clone();
                        move |e| {
                            let op = match e.value().as_str() {
                                "gte" => ComparisonOp::Gte,
                                "lte" => ComparisonOp::Lte,
                                _ => ComparisonOp::Gte,
                            };
                            on_change.call(Condition::TimerTimeRemaining {
                                timer_id: tid.clone(),
                                operator: op,
                                value,
                            });
                        }
                    },
                    option { value: "gte", selected: op_value == "gte", "\u{2265}" }
                    option { value: "lte", selected: op_value == "lte", "\u{2264}" }
                }

                input {
                    r#type: "number",
                    class: "input-inline",
                    style: "width: 70px;",
                    min: "0",
                    step: "0.1",
                    value: "{value}",
                    oninput: {
                        let tid = timer_id_for_val.clone();
                        move |e| {
                            if let Ok(val) = e.value().parse::<f32>() {
                                on_change.call(Condition::TimerTimeRemaining {
                                    timer_id: tid.clone(),
                                    operator,
                                    value: val,
                                });
                            }
                        }
                    }
                }

                span { class: "text-muted text-sm", "sec" }
            }
        }
    }
}
