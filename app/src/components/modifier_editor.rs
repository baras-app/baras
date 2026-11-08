//! Modifier editor component for the effect editor
//!
//! Provides inline editing of `EffectModifier` entries on an `EffectDefinition`.
//! Each modifier has a trigger type, duration adjustment, and optional constraints.

use dioxus::prelude::*;

use crate::components::editor_common::FormCard;

use super::encounter_editor::triggers::{
    AbilitySelectorEditor, EffectSelectorEditor,
};
use crate::types::{
    AbilitySelector, ChargeDirection, EffectModifier, EffectSelector, MitigationType,
    Trigger,
};

// ─────────────────────────────────────────────────────────────────────────────
// Modifier Trigger Type (UI discriminant)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Default)]
enum ModifierTriggerType {
    #[default]
    AbilityCast,
    DamageTaken,
    DamageDealt,
    HealingTaken,
    HealingDealt,
    EffectApplied,
    EffectRemoved,
    ChargesChanged,
    SelfChargesChanged,
    ResourceSpent,
    KillingBlow,
    AnyOf,
}

impl ModifierTriggerType {
    fn label(self) -> &'static str {
        match self {
            Self::AbilityCast => "Ability Cast",
            Self::DamageTaken => "Damage Taken",
            Self::DamageDealt => "Damage Dealt",
            Self::HealingTaken => "Healing Taken",
            Self::HealingDealt => "Healing Dealt",
            Self::EffectApplied => "Effect Applied",
            Self::EffectRemoved => "Effect Removed",
            Self::ChargesChanged => "Charges Changed",
            Self::SelfChargesChanged => "Self Charges Changed",
            Self::ResourceSpent => "Resource Spent",
            Self::KillingBlow => "Killing Blow",
            Self::AnyOf => "Any Of (OR)",
        }
    }

    fn from_label(label: &str) -> Option<Self> {
        Self::all().iter().copied().find(|t| t.label() == label)
    }

    fn all() -> &'static [Self] {
        &[
            Self::AbilityCast,
            Self::DamageTaken,
            Self::DamageDealt,
            Self::HealingTaken,
            Self::HealingDealt,
            Self::EffectApplied,
            Self::EffectRemoved,
            Self::ChargesChanged,
            Self::SelfChargesChanged,
            Self::ResourceSpent,
            Self::KillingBlow,
            Self::AnyOf,
        ]
    }

    /// Trigger types allowed as `AnyOf` children (no nesting).
    fn leaf() -> &'static [Self] {
        let all = Self::all();
        &all[..all.len() - 1]
    }

    fn from_trigger(trigger: &Trigger) -> Self {
        match trigger {
            Trigger::AbilityCast { .. } => Self::AbilityCast,
            Trigger::DamageTaken { .. } => Self::DamageTaken,
            Trigger::DamageDealt { .. } => Self::DamageDealt,
            Trigger::HealingTaken { .. } => Self::HealingTaken,
            Trigger::HealingDealt { .. } => Self::HealingDealt,
            Trigger::EffectApplied { .. } => Self::EffectApplied,
            Trigger::EffectRemoved { .. } => Self::EffectRemoved,
            Trigger::ChargesChanged { .. } => Self::ChargesChanged,
            Trigger::SelfChargesChanged { .. } => Self::SelfChargesChanged,
            Trigger::ResourceSpent { .. } => Self::ResourceSpent,
            Trigger::KillingBlow => Self::KillingBlow,
            Trigger::AnyOf { .. } => Self::AnyOf,
            _ => Self::AbilityCast,
        }
    }

    fn default_trigger(self) -> Trigger {
        match self {
            Self::AbilityCast => Trigger::AbilityCast {
                abilities: vec![],
                source: Default::default(),
                target: Default::default(),
                position: vec![],
            },
            Self::DamageTaken => Trigger::DamageTaken {
                abilities: vec![],
                source: Default::default(),
                target: Default::default(),
                mitigation: vec![],
                position: vec![],
            },
            Self::DamageDealt => Trigger::DamageDealt {
                abilities: vec![],
                source: Default::default(),
                target: Default::default(),
                mitigation: vec![],
                position: vec![],
            },
            Self::HealingTaken => Trigger::HealingTaken {
                abilities: vec![],
                source: Default::default(),
                target: Default::default(),
                position: vec![],
            },
            Self::HealingDealt => Trigger::HealingDealt {
                abilities: vec![],
                source: Default::default(),
                target: Default::default(),
                position: vec![],
            },
            Self::EffectApplied => Trigger::EffectApplied {
                effects: vec![],
                source: Default::default(),
                target: Default::default(),
                position: vec![],
            },
            Self::EffectRemoved => Trigger::EffectRemoved {
                effects: vec![],
                source: Default::default(),
                target: Default::default(),
                position: vec![],
            },
            Self::ChargesChanged => Trigger::ChargesChanged {
                effects: vec![],
                direction: None,
            },
            Self::SelfChargesChanged => Trigger::SelfChargesChanged { direction: None },
            Self::ResourceSpent => Trigger::ResourceSpent { per_amount: 0.0 },
            Self::KillingBlow => Trigger::KillingBlow,
            Self::AnyOf => Trigger::AnyOf { conditions: vec![] },
        }
    }
}

/// Does this trigger (or any `AnyOf` child) carry crit info?
fn supports_crit(trigger: &Trigger) -> bool {
    match trigger {
        Trigger::DamageTaken { .. }
        | Trigger::DamageDealt { .. }
        | Trigger::HealingTaken { .. }
        | Trigger::HealingDealt { .. } => true,
        Trigger::AnyOf { conditions } => conditions.iter().any(supports_crit),
        _ => false,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Trigger type select (shared by the modifier header and AnyOf children)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
struct TriggerTypeSelectProps {
    current: ModifierTriggerType,
    options: &'static [ModifierTriggerType],
    on_change: EventHandler<Trigger>,
}

#[component]
fn TriggerTypeSelect(props: TriggerTypeSelectProps) -> Element {
    rsx! {
        select {
            class: "select-inline",
            style: "flex: 1;",
            value: "{props.current.label()}",
            onchange: move |e: Event<FormData>| {
                if let Some(t) = ModifierTriggerType::from_label(&e.value()) {
                    props.on_change.call(t.default_trigger());
                }
            },
            for tt in props.options {
                option { value: "{tt.label()}", selected: *tt == props.current, "{tt.label()}" }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Modifier List Editor
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct ModifierListEditorProps {
    pub modifiers: Vec<EffectModifier>,
    pub on_change: EventHandler<Vec<EffectModifier>>,
}

#[component]
pub fn ModifierListEditor(props: ModifierListEditorProps) -> Element {
    let modifiers = props.modifiers.clone();

    rsx! {
        FormCard {
            icon: "fa-sliders",
            title: "Modifiers",
            action: rsx! {
                button {
                    class: "btn btn-xs",
                    title: "Add modifier",
                    onclick: {
                        let modifiers = modifiers.clone();
                        let on_change = props.on_change.clone();
                        move |_| {
                            let mut mods = modifiers.clone();
                            mods.push(EffectModifier {
                                trigger: Trigger::DamageTaken {
                                    abilities: vec![],
                                    source: Default::default(),
                                    target: Default::default(),
                                    mitigation: vec![],
                                    position: vec![],
                                },
                                adjust_duration_secs: 0.0,
                                requires_crit: false,
                                refill_duration: false,
                                icd_secs: None,
                                icd_from_application: false,
                                icd_affected_by_alacrity: false,
                                max_duration_secs: None,
                                cancel: false,
                            });
                            on_change.call(mods);
                        }
                    },
                    i { class: "fa-solid fa-plus" }
                    " Add"
                }
            },
            if modifiers.is_empty() {
                div { class: "text-muted text-sm", style: "padding: 4px 0;",
                    "No modifiers configured. Add one to reactively adjust this effect's duration when events occur."
                }
            }
            for (idx, modifier) in modifiers.iter().enumerate() {
                {
                    let on_change = props.on_change.clone();
                    let all_modifiers = modifiers.clone();
                    rsx! {
                        SingleModifierEditor {
                            key: "{idx}",
                            modifier: modifier.clone(),
                            index: idx,
                            on_update: {
                                let all_modifiers = all_modifiers.clone();
                                let on_change = on_change.clone();
                                move |updated: EffectModifier| {
                                    let mut mods = all_modifiers.clone();
                                    mods[idx] = updated;
                                    on_change.call(mods);
                                }
                            },
                            on_remove: {
                                let all_modifiers = all_modifiers.clone();
                                let on_change = on_change.clone();
                                move |_| {
                                    let mut mods = all_modifiers.clone();
                                    mods.remove(idx);
                                    on_change.call(mods);
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Single Modifier Editor
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
struct SingleModifierEditorProps {
    modifier: EffectModifier,
    index: usize,
    on_update: EventHandler<EffectModifier>,
    on_remove: EventHandler<()>,
}

#[component]
fn SingleModifierEditor(props: SingleModifierEditorProps) -> Element {
    let trigger_type = ModifierTriggerType::from_trigger(&props.modifier.trigger);
    let modifier = props.modifier.clone();

    rsx! {
        div { class: "modifier-entry",
            // Header row: trigger type + remove button
            div { class: "modifier-entry-header",
                TriggerTypeSelect {
                    current: trigger_type,
                    options: ModifierTriggerType::all(),
                    on_change: {
                        let modifier = modifier.clone();
                        let on_update = props.on_update.clone();
                        move |t: Trigger| {
                            let mut m = modifier.clone();
                            m.trigger = t;
                            on_update.call(m);
                        }
                    }
                }
                button {
                    class: "btn btn-xs btn-danger",
                    title: "Remove modifier",
                    onclick: {
                        let on_remove = props.on_remove.clone();
                        move |_| on_remove.call(())
                    },
                    i { class: "fa-solid fa-trash" }
                }
            }

            // ── Trigger conditions ──
            div { class: "modifier-section",
                div { class: "modifier-section-title", "Trigger" }
                {render_trigger_fields(&modifier.trigger, EventHandler::new({
                    let modifier = modifier.clone();
                    let on_update = props.on_update.clone();
                    move |t: Trigger| {
                        let mut m = modifier.clone();
                        m.trigger = t;
                        on_update.call(m);
                    }
                }))}
                {render_trigger_conditions(&modifier, &props.on_update)}
            }

            // ── Modifier effect ──
            div { class: "modifier-section",
            div { class: "modifier-section-title", "Modifier" }

            // Cancel (remove effect instead of adjusting duration)
            div { class: "form-row-hz",
                label {
                    "Cancel Effect"
                    span { class: "help-icon", title: "Remove the effect when this modifier fires instead of adjusting its duration", "?" }
                }
                input {
                    r#type: "checkbox",
                    checked: modifier.cancel,
                    onchange: {
                        let modifier = modifier.clone();
                        let on_update = props.on_update.clone();
                        move |e: Event<FormData>| {
                            let mut m = modifier.clone();
                            m.cancel = e.checked();
                            on_update.call(m);
                        }
                    }
                }
            }

            // Common modifier fields
            if !modifier.cancel {
            div { class: "form-row-hz",
                label { "Duration Adjust (s)" }
                input {
                    r#type: "number",
                    class: "input-number",
                    step: "0.1",
                    value: "{modifier.adjust_duration_secs}",
                    onchange: {
                        let modifier = modifier.clone();
                        let on_update = props.on_update.clone();
                        move |e: Event<FormData>| {
                            let mut m = modifier.clone();
                            m.adjust_duration_secs = e.value().parse().unwrap_or(0.0);
                            on_update.call(m);
                        }
                    }
                }
            }

            // Refill Duration
            div { class: "form-row-hz",
                label {
                    "Refill Duration"
                    span { class: "help-icon", title: "Refill the effect to its maximum duration, including adjustment (if applicable).", "?" }
                }
                input {
                    r#type: "checkbox",
                    checked: modifier.refill_duration,
                    onchange: {
                        let modifier = modifier.clone();
                        let on_update = props.on_update.clone();
                        move |e: Event<FormData>| {
                            let mut m = modifier.clone();
                            m.refill_duration = e.checked();
                            on_update.call(m);
                        }
                    }
                }
            }

            // Max Duration
            div { class: "form-row-hz",
                label {
                    "Max Duration (s)"
                    span { class: "help-icon", title: "The maximum duration this effect can obtain through this modifier.", "?" }
                }
                input {
                    r#type: "number",
                    class: "input-number",
                    step: "0.5",
                    min: "0",
                    placeholder: "None",
                    value: "{modifier.max_duration_secs.map(|v| v.to_string()).unwrap_or_default()}",
                    onchange: {
                        let modifier = modifier.clone();
                        let on_update = props.on_update.clone();
                        move |e: Event<FormData>| {
                            let mut m = modifier.clone();
                            m.max_duration_secs = e.value().parse::<f32>().ok().filter(|v| *v > 0.0);
                            on_update.call(m);
                        }
                    }
                }
            }
            }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Trigger conditions (gating shared across trigger types)
// ─────────────────────────────────────────────────────────────────────────────

fn render_trigger_conditions(
    modifier: &EffectModifier,
    on_update: &EventHandler<EffectModifier>,
) -> Element {
    let modifier = modifier.clone();
    let on_update = on_update.clone();
    let has_crit = supports_crit(&modifier.trigger);
    rsx! {
            // Requires Crit (only for damage/healing triggers)
            if has_crit {
                div { class: "form-row-hz",
                    label { "Requires Critical Hit" }
                    input {
                        r#type: "checkbox",
                        checked: modifier.requires_crit,
                        onchange: {
                            let modifier = modifier.clone();
                            let on_update = on_update.clone();
                            move |e: Event<FormData>| {
                                let mut m = modifier.clone();
                                m.requires_crit = e.checked();
                                on_update.call(m);
                            }
                        }
                    }
                }
            }

            // ICD
            div { class: "form-row-hz",
                label {
                    "ICD (s)"
                    span { class: "help-icon", title: "Internal Cooldown. Minimum time between trigger events that must elapse before this modifier can be applied again", "?" }
                }
                input {
                    r#type: "number",
                    class: "input-number",
                    step: "0.1",
                    min: "0",
                    placeholder: "None",
                    value: "{modifier.icd_secs.map(|v| v.to_string()).unwrap_or_default()}",
                    onchange: {
                        let modifier = modifier.clone();
                        let on_update = on_update.clone();
                        move |e: Event<FormData>| {
                            let mut m = modifier.clone();
                            m.icd_secs = e.value().parse::<f32>().ok().filter(|v| *v > 0.0);
                            on_update.call(m);
                        }
                    }
                }
            }

            div { class: "form-row-hz",
                label {
                    "ICD From Application"
                    span { class: "help-icon", title: "Count the effect's initial application as an ICD proc, so the modifier cannot fire until the ICD has elapsed since the effect was applied", "?" }
                }
                input {
                    r#type: "checkbox",
                    checked: modifier.icd_from_application,
                    disabled: modifier.icd_secs.is_none(),
                    onchange: {
                        let modifier = modifier.clone();
                        let on_update = on_update.clone();
                        move |e: Event<FormData>| {
                            let mut m = modifier.clone();
                            m.icd_from_application = e.checked();
                            on_update.call(m);
                        }
                    }
                }
            }
            div { class: "form-row-hz",
                label {
                    "ICD Affected by Alacrity"
                    span { class: "help-icon", title: "Scale the ICD by the effect holder's alacrity, using the same formula as effect duration", "?" }
                }
                input {
                    r#type: "checkbox",
                    checked: modifier.icd_affected_by_alacrity,
                    disabled: modifier.icd_secs.is_none(),
                    onchange: {
                        let modifier = modifier.clone();
                        let on_update = on_update.clone();
                        move |e: Event<FormData>| {
                            let mut m = modifier.clone();
                            m.icd_affected_by_alacrity = e.checked();
                            on_update.call(m);
                        }
                    }
                }
            }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Trigger-specific field rendering
// ─────────────────────────────────────────────────────────────────────────────

fn render_trigger_fields(trigger: &Trigger, on_change: EventHandler<Trigger>) -> Element {
    match trigger {
        Trigger::AbilityCast { abilities, .. } => rsx! {
            AbilitySelectorEditor {
                label: "Abilities",
                selectors: abilities.clone(),
                on_change: move |abilities: Vec<AbilitySelector>| on_change.call(Trigger::AbilityCast {
                    abilities,
                    source: Default::default(),
                    target: Default::default(),
                    position: vec![],
                })
            }
        },
        Trigger::DamageTaken { abilities, mitigation, .. } => {
            let (abs, mits) = (abilities.clone(), mitigation.clone());
            let build = move |abilities: Vec<AbilitySelector>, mitigation: Vec<MitigationType>| Trigger::DamageTaken {
                abilities,
                source: Default::default(),
                target: Default::default(),
                mitigation,
                position: vec![],
            };
            rsx! {
                AbilitySelectorEditor {
                    label: "Abilities",
                    selectors: abs.clone(),
                    on_change: {
                        let mits = mits.clone();
                        move |a: Vec<AbilitySelector>| on_change.call(build(a, mits.clone()))
                    }
                }
                {render_mitigation(&mits, EventHandler::new(move |m: Vec<MitigationType>| on_change.call(build(abs.clone(), m))))}
            }
        }
        Trigger::DamageDealt { abilities, mitigation, .. } => {
            let (abs, mits) = (abilities.clone(), mitigation.clone());
            let build = move |abilities: Vec<AbilitySelector>, mitigation: Vec<MitigationType>| Trigger::DamageDealt {
                abilities,
                source: Default::default(),
                target: Default::default(),
                mitigation,
                position: vec![],
            };
            rsx! {
                AbilitySelectorEditor {
                    label: "Abilities",
                    selectors: abs.clone(),
                    on_change: {
                        let mits = mits.clone();
                        move |a: Vec<AbilitySelector>| on_change.call(build(a, mits.clone()))
                    }
                }
                {render_mitigation(&mits, EventHandler::new(move |m: Vec<MitigationType>| on_change.call(build(abs.clone(), m))))}
            }
        }
        Trigger::HealingTaken { abilities, .. } => rsx! {
            AbilitySelectorEditor {
                label: "Abilities",
                selectors: abilities.clone(),
                on_change: move |abilities: Vec<AbilitySelector>| on_change.call(Trigger::HealingTaken {
                    abilities,
                    source: Default::default(),
                    target: Default::default(),
                    position: vec![],
                })
            }
        },
        Trigger::HealingDealt { abilities, .. } => rsx! {
            AbilitySelectorEditor {
                label: "Abilities",
                selectors: abilities.clone(),
                on_change: move |abilities: Vec<AbilitySelector>| on_change.call(Trigger::HealingDealt {
                    abilities,
                    source: Default::default(),
                    target: Default::default(),
                    position: vec![],
                })
            }
        },
        Trigger::EffectApplied { effects, .. } => rsx! {
            EffectSelectorEditor {
                label: "Effects",
                selectors: effects.clone(),
                on_change: move |effects: Vec<EffectSelector>| on_change.call(Trigger::EffectApplied {
                    effects,
                    source: Default::default(),
                    target: Default::default(),
                    position: vec![],
                })
            }
        },
        Trigger::EffectRemoved { effects, .. } => rsx! {
            EffectSelectorEditor {
                label: "Effects",
                selectors: effects.clone(),
                on_change: move |effects: Vec<EffectSelector>| on_change.call(Trigger::EffectRemoved {
                    effects,
                    source: Default::default(),
                    target: Default::default(),
                    position: vec![],
                })
            }
        },
        Trigger::ChargesChanged { effects, direction } => {
            let (effs, dir) = (effects.clone(), *direction);
            rsx! {
                EffectSelectorEditor {
                    label: "Effects",
                    selectors: effs.clone(),
                    on_change: move |effects: Vec<EffectSelector>| on_change.call(Trigger::ChargesChanged { effects, direction: dir })
                }
                {render_direction_select(dir, EventHandler::new(move |direction| on_change.call(Trigger::ChargesChanged {
                    effects: effs.clone(),
                    direction,
                })))}
            }
        }
        Trigger::SelfChargesChanged { direction } => {
            render_direction_select(*direction, EventHandler::new(move |direction| on_change.call(Trigger::SelfChargesChanged { direction })))
        }
        Trigger::ResourceSpent { per_amount } => rsx! {
            div { class: "form-row-hz",
                label {
                    "Per Amount"
                    span { class: "help-icon", title: "Scale the duration adjust by (amount spent / this value). 0 = flat adjust per spend event", "?" }
                }
                input {
                    r#type: "number",
                    class: "input-number",
                    step: "1",
                    min: "0",
                    placeholder: "0 (flat)",
                    value: "{per_amount}",
                    onchange: move |e: Event<FormData>| on_change.call(Trigger::ResourceSpent {
                        per_amount: e.value().parse::<f32>().unwrap_or(0.0).max(0.0),
                    }),
                }
            }
        },
        Trigger::AnyOf { conditions } => {
            let conds = conditions.clone();
            rsx! {
                div { class: "modifier-anyof",
                    for (i, child) in conds.iter().enumerate() {
                        div { class: "modifier-anyof-child", key: "{i}",
                            div { class: "modifier-entry-header",
                                TriggerTypeSelect {
                                    current: ModifierTriggerType::from_trigger(child),
                                    options: ModifierTriggerType::leaf(),
                                    on_change: {
                                        let conds = conds.clone();
                                        move |t: Trigger| {
                                            let mut c = conds.clone();
                                            c[i] = t;
                                            on_change.call(Trigger::AnyOf { conditions: c });
                                        }
                                    }
                                }
                                button {
                                    class: "btn btn-xs btn-danger",
                                    title: "Remove condition",
                                    onclick: {
                                        let conds = conds.clone();
                                        move |_| {
                                            let mut c = conds.clone();
                                            c.remove(i);
                                            on_change.call(Trigger::AnyOf { conditions: c });
                                        }
                                    },
                                    i { class: "fa-solid fa-xmark" }
                                }
                            }
                            {render_trigger_fields(child, EventHandler::new({
                                let conds = conds.clone();
                                move |t: Trigger| {
                                    let mut c = conds.clone();
                                    c[i] = t;
                                    on_change.call(Trigger::AnyOf { conditions: c });
                                }
                            }))}
                        }
                    }
                    button {
                        class: "btn-sm",
                        onclick: {
                            let conds = conds.clone();
                            move |_| {
                                let mut c = conds.clone();
                                c.push(ModifierTriggerType::default().default_trigger());
                                on_change.call(Trigger::AnyOf { conditions: c });
                            }
                        },
                        i { class: "fa-solid fa-plus" }
                        " Add Condition"
                    }
                }
            }
        }
        _ => rsx! {},
    }
}

fn render_mitigation(mitigation: &[MitigationType], on_change: EventHandler<Vec<MitigationType>>) -> Element {
    let mits = mitigation.to_vec();
    rsx! {
        div { class: "form-row-hz",
            label { "Mitigation Filter" }
            div { class: "flex flex-wrap gap-xs",
                for mit_type in MitigationType::ALL {
                    label { class: "flex items-center gap-xs text-sm",
                        input {
                            r#type: "checkbox",
                            checked: mits.contains(mit_type),
                            onchange: {
                                let mits = mits.clone();
                                let mit = *mit_type;
                                move |e: Event<FormData>| {
                                    let mut m = mits.clone();
                                    if e.checked() {
                                        if !m.contains(&mit) { m.push(mit); }
                                    } else {
                                        m.retain(|x| *x != mit);
                                    }
                                    on_change.call(m);
                                }
                            }
                        }
                        span { "{mit_type.display_name()}" }
                    }
                }
            }
        }
    }
}

fn render_direction_select(direction: Option<ChargeDirection>, on_change: EventHandler<Option<ChargeDirection>>) -> Element {
    rsx! {
        div { class: "form-row-hz",
            label { "Direction" }
            select {
                class: "select-inline",
                value: "{direction_label(direction)}",
                onchange: move |e: Event<FormData>| {
                    on_change.call(match e.value().as_str() {
                        "Increased" => Some(ChargeDirection::Increased),
                        "Decreased" => Some(ChargeDirection::Decreased),
                        "Neutral" => Some(ChargeDirection::Neutral),
                        _ => None,
                    });
                },
                option { value: "Any", selected: direction.is_none(), "Any" }
                option { value: "Increased", selected: direction == Some(ChargeDirection::Increased), "Increased" }
                option { value: "Decreased", selected: direction == Some(ChargeDirection::Decreased), "Decreased" }
                option { value: "Neutral", selected: direction == Some(ChargeDirection::Neutral), "Neutral" }
            }
        }
    }
}

fn direction_label(direction: Option<ChargeDirection>) -> &'static str {
    match direction {
        Some(ChargeDirection::Increased) => "Increased",
        Some(ChargeDirection::Decreased) => "Decreased",
        Some(ChargeDirection::Neutral) => "Neutral",
        None => "Any",
    }
}
