//! Shared trigger editors
//!
//! Used by timers, phases, and counters for editing trigger conditions.

use dioxus::prelude::*;

use crate::types::{
    AbilitySelector, EffectSelector, EntityFilter, EntitySelector, MitigationType, PositionAxis,
    PositionConstraint, PositionEntity, PositionOp, TimerTrigger,
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
pub fn BossSelector(
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
        let t = if t.supports_position()
            && t.position_constraints().is_empty()
            && !preserved_position.is_empty()
        {
            t.with_position(preserved_position.clone())
        } else {
            t
        };
        raw_on_change.call(t)
    });

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
                        "ability_cast" => TimerTrigger::AbilityCast { abilities: vec![], source: EntityFilter::default(), target: EntityFilter::default(), position: vec![] },
                        "effect_applied" => TimerTrigger::EffectApplied { effects: vec![], source: EntityFilter::default(), target: EntityFilter::default(), position: vec![] },
                        "effect_removed" => TimerTrigger::EffectRemoved { effects: vec![], source: EntityFilter::default(), target: EntityFilter::default(), position: vec![] },
                        "damage_taken" => TimerTrigger::DamageTaken { abilities: vec![], source: EntityFilter::default(), target: EntityFilter::default(), mitigation: vec![], position: vec![] },
                        "healing_taken" => TimerTrigger::HealingTaken { abilities: vec![], source: EntityFilter::default(), target: EntityFilter::default(), position: vec![] },
                        "threat_modified" => TimerTrigger::ThreatModified { abilities: vec![], source: EntityFilter::default(), target: EntityFilter::default() },
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
                        _ => trigger.clone(),
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

            // Type-specific fields
            {
                match trigger.clone() {
                    TimerTrigger::CombatStart
                    | TimerTrigger::CombatEnd
                    | TimerTrigger::AnyPhaseChange
                    | TimerTrigger::Never
                    | TimerTrigger::Manual => rsx! {},
                    TimerTrigger::AbilityCast { abilities, source, target, .. } => {
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
                                on_change: move |sels| on_change.call(TimerTrigger::AbilityCast {
                                    abilities: sels,
                                    source: source_for_abilities.clone(),
                                    target: target_for_abilities.clone(),
                                    position: vec![],
                                })
                            }
                            EntityFilterDropdown {
                                label: "Source",
                                value: source,
                                options: EntityFilter::source_options(),
                                on_change: move |f| on_change.call(TimerTrigger::AbilityCast {
                                    abilities: abilities_for_source.clone(),
                                    source: f,
                                    target: target_for_source.clone(),
                                    position: vec![],
                                })
                            }
                            EntityFilterDropdown {
                                label: "Target",
                                value: target,
                                options: EntityFilter::target_options(),
                                on_change: move |f| on_change.call(TimerTrigger::AbilityCast {
                                    abilities: abilities_for_target.clone(),
                                    source: source_for_target.clone(),
                                    target: f,
                                    position: vec![],
                                })
                            }
                        }
                    },
                    TimerTrigger::EffectApplied { effects, source, target, .. } => {
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
                                    position: vec![],
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
                                    position: vec![],
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
                                    position: vec![],
                                })
                            }
                        }
                    },
                    TimerTrigger::EffectRemoved { effects, source, target, .. } => {
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
                                    position: vec![],
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
                                    position: vec![],
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
                                    position: vec![],
                                })
                            }
                        }
                    },
                    TimerTrigger::DamageTaken { abilities, source, target, mitigation, .. } => {
                        let source_for_abilities = source.clone();
                        let target_for_abilities = target.clone();
                        let mitigation_for_abilities = mitigation.clone();
                        let abilities_for_source = abilities.clone();
                        let target_for_source = target.clone();
                        let mitigation_for_source = mitigation.clone();
                        let abilities_for_target = abilities.clone();
                        let source_for_target = source.clone();
                        let mitigation_for_target = mitigation.clone();
                        let abilities_for_mitigation = abilities.clone();
                        let source_for_mitigation = source.clone();
                        let target_for_mitigation = target.clone();
                        rsx! {
                            AbilitySelectorEditor {
                                label: "Abilities (empty = any)",
                                selectors: abilities,
                                on_change: move |sels| on_change.call(TimerTrigger::DamageTaken {
                                    abilities: sels,
                                    source: source_for_abilities.clone(),
                                    target: target_for_abilities.clone(),
                                    mitigation: mitigation_for_abilities.clone(),
                                    position: vec![],
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
                                    mitigation: mitigation_for_source.clone(),
                                    position: vec![],
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
                                    mitigation: mitigation_for_target.clone(),
                                    position: vec![],
                                })
                            }
                            MitigationTypeEditor {
                                selected: mitigation,
                                on_change: move |m| on_change.call(TimerTrigger::DamageTaken {
                                    abilities: abilities_for_mitigation.clone(),
                                    source: source_for_mitigation.clone(),
                                    target: target_for_mitigation.clone(),
                                    mitigation: m,
                                    position: vec![],
                                })
                            }
                        }
                    },
                    TimerTrigger::HealingTaken { abilities, source, target, .. } => {
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
                                on_change: move |sels| on_change.call(TimerTrigger::HealingTaken {
                                    abilities: sels,
                                    source: source_for_abilities.clone(),
                                    target: target_for_abilities.clone(),
                                    position: vec![],
                                })
                            }
                            EntityFilterDropdown {
                                label: "Source",
                                value: source,
                                options: EntityFilter::source_options(),
                                on_change: move |f| on_change.call(TimerTrigger::HealingTaken {
                                    abilities: abilities_for_source.clone(),
                                    source: f,
                                    target: target_for_source.clone(),
                                    position: vec![],
                                })
                            }
                            EntityFilterDropdown {
                                label: "Target",
                                value: target,
                                options: EntityFilter::target_options(),
                                on_change: move |f| on_change.call(TimerTrigger::HealingTaken {
                                    abilities: abilities_for_target.clone(),
                                    source: source_for_target.clone(),
                                    target: f,
                                    position: vec![],
                                })
                            }
                        }
                    },
                    TimerTrigger::ThreatModified { abilities, source, target } => {
                        let source_for_abilities = source.clone();
                        let target_for_abilities = target.clone();
                        let abilities_for_source = abilities.clone();
                        let target_for_source = target.clone();
                        let abilities_for_target = abilities.clone();
                        let source_for_target = source.clone();
                        rsx! {
                            AbilitySelectorEditor {
                                label: "Abilities (empty = any)",
                                selectors: abilities,
                                on_change: move |sels| on_change.call(TimerTrigger::ThreatModified {
                                    abilities: sels,
                                    source: source_for_abilities.clone(),
                                    target: target_for_abilities.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Source",
                                value: source,
                                options: EntityFilter::source_options(),
                                on_change: move |f| on_change.call(TimerTrigger::ThreatModified {
                                    abilities: abilities_for_source.clone(),
                                    source: f,
                                    target: target_for_source.clone(),
                                })
                            }
                            EntityFilterDropdown {
                                label: "Target",
                                value: target,
                                options: EntityFilter::target_options(),
                                on_change: move |f| on_change.call(TimerTrigger::ThreatModified {
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
                    TimerTrigger::TimerCanceled { timer_id } => {
                        let available_timers = encounter_data.timer_ids();
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
                    TimerTrigger::CounterChanges { counter_id } => {
                        let available_counters = encounter_data.counter_ids();
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
                            label: "Entity (Spawned)",
                            selectors: selector.clone(),
                            on_change: move |sels| on_change.call(TimerTrigger::NpcAppears {
                                selector: sels,
                                position: vec![],
                            })
                        }
                    },
                    TimerTrigger::EntityDeath { selector, .. } => rsx! {
                        EntitySelectorEditor {
                            label: "Entity (Death)",
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
    #[props(default = true)]
    show_entity: bool,
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
                    class: "select",
                    style: "width: 90px;",
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
                class: "select",
                style: "width: 80px;",
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
                class: "select",
                style: "width: 90px;",
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
                    class: "input-inline",
                    style: "width: 90px;",
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
                    class: "input-inline",
                    style: "width: 90px;",
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
                    class: "input-inline",
                    style: "width: 90px;",
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
// Mitigation Type Editor
// ─────────────────────────────────────────────────────────────────────────────

/// Editor for an optional list of mitigation type filters on DamageTaken triggers.
/// Renders chips for selected types and a dropdown to add more.
/// An empty list means "any hit result" (no filtering).
#[component]
pub fn MitigationTypeEditor(
    selected: Vec<MitigationType>,
    on_change: EventHandler<Vec<MitigationType>>,
) -> Element {
    let mut pending = use_signal(|| MitigationType::Miss);

    let selected_for_add = selected.clone();

    rsx! {
        div { class: "flex-col gap-xs items-start",
            span { class: "text-sm text-secondary text-left", "Mitigation (optional):" }

            // Selected type chips
            if !selected.is_empty() {
                div { class: "flex flex-wrap gap-xs",
                    for (idx, m) in selected.iter().enumerate() {
                        {
                            let selected_clone = selected.clone();
                            let label = m.display_name();
                            rsx! {
                                span { class: "chip",
                                    "{label}"
                                    button {
                                        class: "chip-remove",
                                        onclick: move |_| {
                                            let mut next = selected_clone.clone();
                                            next.remove(idx);
                                            on_change.call(next);
                                        },
                                        "×"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Add dropdown + button
            div { class: "flex gap-xs items-center",
                select {
                    class: "select",
                    style: "width: 130px;",
                    onchange: move |e| {
                        let m = match e.value().as_str() {
                            "Miss" => MitigationType::Miss,
                            "Parry" => MitigationType::Parry,
                            "Dodge" => MitigationType::Dodge,
                            "Immune" => MitigationType::Immune,
                            "Resist" => MitigationType::Resist,
                            "Deflect" => MitigationType::Deflect,
                            "Shield" => MitigationType::Shield,
                            "Absorbed" => MitigationType::Absorbed,
                            "Cover" => MitigationType::Cover,
                            "Reflected" => MitigationType::Reflected,
                            _ => MitigationType::Miss,
                        };
                        pending.set(m);
                    },
                    for m in MitigationType::ALL {
                        option { value: "{m:?}", selected: *m == *pending.read(), "{m.display_name()}" }
                    }
                }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| {
                        let m = *pending.read();
                        if !selected_for_add.contains(&m) {
                            let mut next = selected_for_add.clone();
                            next.push(m);
                            on_change.call(next);
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
