//! Effect trigger plumbing: reading and rewriting the trigger's parts, and the
//! trigger / refresh ability list editors

use dioxus::prelude::*;

use crate::types::{AbilitySelector, EffectSelector, EntityFilter, RefreshAbility, RefreshTrigger, Trigger};

/// Get the source and target filters from a trigger
pub(crate) fn get_trigger_filters(trigger: &Trigger) -> (EntityFilter, EntityFilter) {
    match trigger {
        Trigger::EffectApplied { source, target, .. }
        | Trigger::EffectRemoved { source, target, .. }
        | Trigger::AbilityCast { source, target, .. }
        | Trigger::DamageTaken { source, target, .. }
        | Trigger::HealingTaken { source, target, .. } => (source.clone(), target.clone()),
        _ => (EntityFilter::Any, EntityFilter::Any),
    }
}

/// Get the "when" label for effect-based triggers
pub(crate) fn get_effect_when_label(trigger: &Trigger) -> &'static str {
    match trigger {
        Trigger::EffectApplied { .. } => "Effect Applied",
        Trigger::EffectRemoved { .. } => "Effect Removed",
        _ => "Effect Applied",
    }
}

/// Icon the overlay shows: the explicit icon, else the trigger's first effect/ability id
pub(crate) fn effect_icon_id(icon: Option<u64>, trigger: &Trigger) -> Option<u64> {
    icon.or_else(|| {
        let (is_effect_trigger, effects) = get_trigger_effects(trigger);
        if is_effect_trigger {
            effects.first().and_then(|sel| match sel {
                EffectSelector::Id(id) => Some(*id),
                EffectSelector::Name(_) => None,
            })
        } else {
            get_trigger_abilities(trigger).first().and_then(|sel| match sel {
                AbilitySelector::Id(id) => Some(*id),
                AbilitySelector::Name(_) => None,
            })
        }
    })
}

/// Get the effects from an effect-based trigger (returns is_effect_trigger, effects)
pub(crate) fn get_trigger_effects(trigger: &Trigger) -> (bool, Vec<EffectSelector>) {
    match trigger {
        Trigger::EffectApplied { effects, .. } | Trigger::EffectRemoved { effects, .. } => {
            (true, effects.clone())
        }
        _ => (false, vec![]),
    }
}

/// Get abilities from an ability-based trigger
pub(crate) fn get_trigger_abilities(trigger: &Trigger) -> Vec<AbilitySelector> {
    match trigger {
        Trigger::AbilityCast { abilities, .. }
        | Trigger::DamageTaken { abilities, .. }
        | Trigger::HealingTaken { abilities, .. } => abilities.clone(),
        _ => vec![],
    }
}

/// Set the source filter on a trigger
pub(crate) fn set_trigger_source(trigger: Trigger, source: EntityFilter) -> Trigger {
    match trigger {
        Trigger::EffectApplied {
            effects, target, ..
        } => Trigger::EffectApplied {
            effects,
            source,
            target,
            position: vec![],
        },
        Trigger::EffectRemoved {
            effects, target, ..
        } => Trigger::EffectRemoved {
            effects,
            source,
            target,
            position: vec![],
        },
        Trigger::AbilityCast {
            abilities, target, ..
        } => Trigger::AbilityCast {
            abilities,
            source,
            target,
            position: vec![],
        },
        Trigger::DamageTaken {
            abilities, target, mitigation, ..
        } => Trigger::DamageTaken {
            abilities,
            source,
            target,
            mitigation,
            position: vec![],
        },
        Trigger::HealingTaken {
            abilities, target, ..
        } => Trigger::HealingTaken {
            abilities,
            source,
            target,
            position: vec![],
        },
        other => other,
    }
}

/// Set the target filter on a trigger
pub(crate) fn set_trigger_target(trigger: Trigger, target: EntityFilter) -> Trigger {
    match trigger {
        Trigger::EffectApplied {
            effects, source, ..
        } => Trigger::EffectApplied {
            effects,
            source,
            target,
            position: vec![],
        },
        Trigger::EffectRemoved {
            effects, source, ..
        } => Trigger::EffectRemoved {
            effects,
            source,
            target,
            position: vec![],
        },
        Trigger::AbilityCast {
            abilities, source, ..
        } => Trigger::AbilityCast {
            abilities,
            source,
            target,
            position: vec![],
        },
        Trigger::DamageTaken {
            abilities, source, mitigation, ..
        } => Trigger::DamageTaken {
            abilities,
            source,
            target,
            mitigation,
            position: vec![],
        },
        Trigger::HealingTaken {
            abilities, source, ..
        } => Trigger::HealingTaken {
            abilities,
            source,
            target,
            position: vec![],
        },
        other => other,
    }
}

/// Set the effects on an effect-based trigger
pub(crate) fn set_trigger_effects(trigger: Trigger, effects: Vec<EffectSelector>) -> Trigger {
    match trigger {
        Trigger::EffectApplied { source, target, .. } => Trigger::EffectApplied {
            effects,
            source,
            target,
            position: vec![],
        },
        Trigger::EffectRemoved { source, target, .. } => Trigger::EffectRemoved {
            effects,
            source,
            target,
            position: vec![],
        },
        other => other,
    }
}

/// Set the abilities on an ability-based trigger
pub(crate) fn set_trigger_abilities(trigger: Trigger, abilities: Vec<AbilitySelector>) -> Trigger {
    match trigger {
        Trigger::AbilityCast { source, target, .. } => Trigger::AbilityCast {
            abilities,
            source,
            target,
            position: vec![],
        },
        Trigger::DamageTaken { source, target, mitigation, .. } => Trigger::DamageTaken {
            abilities,
            source,
            target,
            mitigation,
            position: vec![],
        },
        Trigger::HealingTaken { source, target, .. } => Trigger::HealingTaken {
            abilities,
            source,
            target,
            position: vec![],
        },
        other => other,
    }
}

#[component]
pub(crate) fn TriggerAbilitiesEditor(
    abilities: Vec<AbilitySelector>,
    on_change: EventHandler<Vec<AbilitySelector>>,
) -> Element {
    let mut new_input = use_signal(String::new);

    let abilities_for_keydown = abilities.clone();
    let abilities_for_click = abilities.clone();

    rsx! {
        div { class: "flex-col gap-xs items-start",

            // Ability chips
            div { class: "flex flex-wrap gap-xs",
                for (idx, sel) in abilities.iter().enumerate() {
                    {
                        let abilities_clone = abilities.clone();
                        let display = sel.display();
                        rsx! {
                            span { class: "chip",
                                "{display}"
                                button {
                                    class: "chip-remove",
                                    onclick: move |_| {
                                        let mut new_abs = abilities_clone.clone();
                                        new_abs.remove(idx);
                                        on_change.call(new_abs);
                                    },
                                    "×"
                                }
                            }
                        }
                    }
                }
            }

            // Add new ability
            div { class: "flex gap-xs",
                input {
                    r#type: "text",
                    class: "input-inline flex-fill",
                    placeholder: "Ability ID or Name (Enter)",
                    value: "{new_input}",
                    oninput: move |e| new_input.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter && !new_input().trim().is_empty() {
                            let selector = AbilitySelector::from_input(&new_input());
                            let mut new_abs = abilities_for_keydown.clone();
                            if !new_abs.iter().any(|s| s.display() == selector.display()) {
                                new_abs.push(selector);
                                on_change.call(new_abs);
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
                            let mut new_abs = abilities_for_click.clone();
                            if !new_abs.iter().any(|s| s.display() == selector.display()) {
                                new_abs.push(selector);
                                on_change.call(new_abs);
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

/// Build a `RefreshAbility`, collapsing to the simple form when no conditions
/// are set so unmodified definitions keep serializing as a bare selector.
fn make_refresh_ability(
    ability: AbilitySelector,
    min_stacks: Option<u8>,
    trigger: RefreshTrigger,
    ignore_immune_resist: bool,
    on_first_damage: Option<bool>,
) -> RefreshAbility {
    if min_stacks.is_none()
        && trigger == RefreshTrigger::Activation
        && !ignore_immune_resist
        && on_first_damage.is_none()
    {
        RefreshAbility::Simple(ability)
    } else {
        RefreshAbility::Conditional {
            ability,
            min_stacks,
            trigger,
            ignore_immune_resist,
            on_first_damage,
        }
    }
}

#[component]
pub(crate) fn RefreshAbilitiesEditor(
    abilities: Vec<RefreshAbility>,
    on_change: EventHandler<Vec<RefreshAbility>>,
) -> Element {
    let mut new_input = use_signal(String::new);

    let abilities_for_keydown = abilities.clone();
    let abilities_for_click = abilities.clone();

    rsx! {
        div { class: "flex-col gap-xs items-start",
            // One row per refresh ability
            for (idx, ra) in abilities.iter().enumerate() {
                {
                    let display = ra.ability().display();
                    let min_stacks = ra.min_stacks();
                    let trigger = ra.trigger();
                    let ignore_immune = ra.ignore_immune_resist();
                    let on_first_damage = ra.on_first_damage();
                    let abilities_rm = abilities.clone();
                    let abilities_ms = abilities.clone();
                    let abilities_tr = abilities.clone();
                    let abilities_ir = abilities.clone();
                    let abilities_fd = abilities.clone();
                    rsx! {
                        div { class: "flex gap-xs items-center flex-wrap",
                            span { class: "chip",
                                "{display}"
                                button {
                                    class: "chip-remove",
                                    onclick: move |_| {
                                        let mut next = abilities_rm.clone();
                                        next.remove(idx);
                                        on_change.call(next);
                                    },
                                    "×"
                                }
                            }
                            select {
                                class: "select-inline",
                                onchange: move |e| {
                                    let trig = match e.value().as_str() {
                                        "heal" => RefreshTrigger::Heal,
                                        "damage" => RefreshTrigger::Damage,
                                        _ => RefreshTrigger::Activation,
                                    };
                                    let mut next = abilities_tr.clone();
                                    let ability = next[idx].ability().clone();
                                    next[idx] = make_refresh_ability(ability, min_stacks, trig, ignore_immune, on_first_damage);
                                    on_change.call(next);
                                },
                                option {
                                    value: "activation",
                                    selected: trigger == RefreshTrigger::Activation,
                                    "On Cast"
                                }
                                option {
                                    value: "heal",
                                    selected: trigger == RefreshTrigger::Heal,
                                    "On Heal"
                                }
                                option {
                                    value: "damage",
                                    selected: trigger == RefreshTrigger::Damage,
                                    "On Damage"
                                }
                            }
                            label { class: "flex items-center gap-xs text-sm text-secondary",
                                "Min Stacks"
                                input {
                                    r#type: "number",
                                    class: "input-inline",
                                    style: "width: 4rem;",
                                    min: "0",
                                    placeholder: "any",
                                    value: min_stacks.map(|m| m.to_string()).unwrap_or_default(),
                                    onchange: move |e| {
                                        let parsed = e.value().trim().parse::<u8>().ok().filter(|n| *n > 0);
                                        let mut next = abilities_ms.clone();
                                        let ability = next[idx].ability().clone();
                                        next[idx] = make_refresh_ability(ability, parsed, trigger, ignore_immune, on_first_damage);
                                        on_change.call(next);
                                    }
                                }
                            }
                            if trigger == RefreshTrigger::Activation {
                                label { class: "flex items-center gap-xs text-sm text-secondary",
                                    "Refresh on"
                                    select {
                                        class: "select-inline",
                                        onchange: move |e| {
                                            let fd = match e.value().as_str() {
                                                "cast" => Some(false),
                                                "first_damage" => Some(true),
                                                _ => None,
                                            };
                                            let mut next = abilities_fd.clone();
                                            let ability = next[idx].ability().clone();
                                            next[idx] = make_refresh_ability(ability, min_stacks, trigger, ignore_immune, fd);
                                            on_change.call(next);
                                        },
                                        option {
                                            value: "default",
                                            selected: on_first_damage.is_none(),
                                            "Default"
                                        }
                                        option {
                                            value: "cast",
                                            selected: on_first_damage == Some(false),
                                            "Cast"
                                        }
                                        option {
                                            value: "first_damage",
                                            selected: on_first_damage == Some(true),
                                            "First Damage"
                                        }
                                    }
                                    span {
                                        class: "help-icon",
                                        title: "When the refresh fires for an On Cast ability. 'Default' uses the effect's display type — DoT-tracker effects wait for the ability's damage to land, all others refresh the instant the ability is cast. 'Cast' forces an immediate refresh; 'First Damage' waits for the ability's first damaging hit (so an interrupted cast won't refresh).",
                                        "?"
                                    }
                                }
                            }
                            if trigger == RefreshTrigger::Damage {
                                label { class: "flex items-center gap-xs text-sm text-secondary",
                                    input {
                                        r#type: "checkbox",
                                        checked: ignore_immune,
                                        onchange: move |e| {
                                            let mut next = abilities_ir.clone();
                                            let ability = next[idx].ability().clone();
                                            next[idx] = make_refresh_ability(ability, min_stacks, trigger, e.checked(), on_first_damage);
                                            on_change.call(next);
                                        }
                                    }
                                    span { class: "flex items-center",
                                        "Ignore Immune/Resist"
                                        span {
                                            class: "help-icon",
                                            title: "When refreshing on damage, skip damage events that were immune or resisted — they won't refresh the effect.",
                                            "?"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Add new ability (defaults to On Cast, any stacks)
            div { class: "flex gap-xs",
                input {
                    r#type: "text",
                    class: "input-inline flex-fill",
                    placeholder: "Ability ID or Name (Enter)",
                    value: "{new_input}",
                    oninput: move |e| new_input.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter && !new_input().trim().is_empty() {
                            let selector = AbilitySelector::from_input(&new_input());
                            let mut next = abilities_for_keydown.clone();
                            if !next.iter().any(|r| r.ability().display() == selector.display()) {
                                next.push(RefreshAbility::Simple(selector));
                                on_change.call(next);
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
                            let mut next = abilities_for_click.clone();
                            if !next.iter().any(|r| r.ability().display() == selector.display()) {
                                next.push(RefreshAbility::Simple(selector));
                                on_change.call(next);
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
