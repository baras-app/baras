//! Effect form cards: what triggers the effect and what refreshes it

use dioxus::prelude::*;

use super::discipline_selector::DisciplineSelector;
use super::effect_trigger::{
    RefreshAbilitiesEditor, TriggerAbilitiesEditor, get_trigger_abilities, get_trigger_effects, get_trigger_filters,
    set_trigger_abilities, set_trigger_effects, set_trigger_source, set_trigger_target,
};
use super::encounter_editor::selectors::{EffectSelectorEditor, EntityFilterDropdown};
use crate::components::editor_common::FormCard;
use crate::components::form_fields::{CheckField, Field, SubSection, TogglePill};
use crate::types::{EffectListItem, EntityFilter, RefreshAbility, RefreshScope, Trigger};

const REFRESH_HELP: &str = "Abilities that refresh this effect's duration. Per ability: 'Min Stacks' only refreshes when the effect has at least that many stacks. Trigger 'On Cast' refreshes the moment the ability is used; 'On Heal' waits for a cast-time heal to land; 'On Damage' refreshes on any damage dealt by the ability. For 'On Cast', the 'Refresh on' control chooses Cast vs First Damage timing — 'Default' defers DoT-tracker effects to the ability's first damaging hit and refreshes everything else immediately.";
const DISCIPLINE_FILTER_HELP: &str =
    "If empty, no restriction. When set, NPCs, companions, and players whose discipline isn't known yet never match.";

/// What an effect's trigger watches for
#[derive(Clone, Copy, PartialEq)]
enum TriggerKind {
    EffectApplied,
    EffectRemoved,
    /// An ability cast (procs, cooldowns)
    AbilityCast,
    DamageTaken,
    HealingTaken,
}

impl TriggerKind {
    const ALL: [Self; 5] = [
        Self::EffectApplied,
        Self::EffectRemoved,
        Self::AbilityCast,
        Self::DamageTaken,
        Self::HealingTaken,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::EffectApplied => "Effect Applied",
            Self::EffectRemoved => "Effect Removed",
            Self::AbilityCast => "Ability Cast",
            Self::DamageTaken => "Damage Taken",
            Self::HealingTaken => "Healing Taken",
        }
    }

    fn of(trigger: &Trigger) -> Self {
        match trigger {
            Trigger::EffectRemoved { .. } => Self::EffectRemoved,
            Trigger::AbilityCast { .. } => Self::AbilityCast,
            Trigger::DamageTaken { .. } => Self::DamageTaken,
            Trigger::HealingTaken { .. } => Self::HealingTaken,
            _ => Self::EffectApplied,
        }
    }

    fn is_effect(self) -> bool {
        matches!(self, Self::EffectApplied | Self::EffectRemoved)
    }

    /// `old` as this kind: source and target carry over, and so do the
    /// effects or abilities when the new kind takes the same list
    fn convert(self, old: &Trigger) -> Trigger {
        let (source, target) = get_trigger_filters(old);
        let effects = get_trigger_effects(old).1;
        let abilities = get_trigger_abilities(old);
        let position = vec![];
        match self {
            Self::EffectApplied => Trigger::EffectApplied {
                effects,
                source,
                target,
                position,
            },
            Self::EffectRemoved => Trigger::EffectRemoved {
                effects,
                source,
                target,
                position,
            },
            Self::AbilityCast => Trigger::AbilityCast {
                abilities,
                source,
                target,
                position,
            },
            Self::DamageTaken => Trigger::DamageTaken {
                abilities,
                source,
                target,
                mitigation: vec![],
                position,
            },
            Self::HealingTaken => Trigger::HealingTaken {
                abilities,
                source,
                target,
                position,
            },
        }
    }
}

fn refresh_scope_label(scope: RefreshScope) -> &'static str {
    match scope {
        RefreshScope::Both => "Both",
        RefreshScope::Source => "Source",
        RefreshScope::Target => "Target",
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Trigger
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub(super) fn TriggerCard(mut draft: Signal<EffectListItem>) -> Element {
    let d = draft.read().clone();
    let kind = TriggerKind::of(&d.trigger);
    let (source, target) = get_trigger_filters(&d.trigger);

    rsx! {
        FormCard { icon: "fa-bolt", title: "Trigger",
            Field { label: "When", help: "What starts this effect: a game buff / debuff being applied or removed, an ability being cast, or damage / healing taken from an ability",
                select {
                    onchange: move |e| {
                        let Some(new_kind) = TriggerKind::ALL.into_iter().find(|k| k.label() == e.value()) else { return };
                        let mut d = draft.write();
                        d.trigger = new_kind.convert(&d.trigger);
                    },
                    for k in TriggerKind::ALL {
                        option { value: "{k.label()}", selected: k == kind, "{k.label()}" }
                    }
                }
            }
            // What the picked trigger needs, nested under it
            div { class: "trigger-details",
                if kind.is_effect() {
                    EffectSelectorEditor {
                        label: "Effects",
                        selectors: get_trigger_effects(&d.trigger).1,
                        on_change: move |selectors| {
                            let mut d = draft.write();
                            d.trigger = set_trigger_effects(d.trigger.clone(), selectors);
                        }
                    }
                } else {
                    Field { label: "Abilities", top: true,
                        TriggerAbilitiesEditor {
                            abilities: get_trigger_abilities(&d.trigger),
                            on_change: move |abilities| {
                                let mut d = draft.write();
                                d.trigger = set_trigger_abilities(d.trigger.clone(), abilities);
                            }
                        }
                    }
                }
                Field { label: "Source", help: "Who must cast/apply for this effect to trigger (e.g., Local Player = you, Any = anyone)",
                    EntityFilterDropdown {
                        label: "",
                        value: source,
                        options: EntityFilter::source_options(),
                        on_change: move |f| {
                            let mut d = draft.write();
                            d.trigger = set_trigger_source(d.trigger.clone(), f);
                        }
                    }
                }
                Field { label: "Target", help: "Who must receive this effect for it to trigger (e.g., Any = track on anyone, Local Player = only on you)",
                    EntityFilterDropdown {
                        label: "",
                        value: target,
                        options: EntityFilter::target_options(),
                        on_change: move |f| {
                            let mut d = draft.write();
                            d.trigger = set_trigger_target(d.trigger.clone(), f);
                        }
                    }
                }
            }

            SubSection { title: "Only from / on these disciplines", help: DISCIPLINE_FILTER_HELP,
                Field { label: "Source", help: "Restrict the source to players of these disciplines/roles.",
                    DisciplineSelector {
                        selected: d.source_disciplines.clone(),
                        include_roles: true,
                        empty_label: "Any discipline",
                        on_change: move |v: Vec<String>| draft.write().source_disciplines = v,
                    }
                }
                Field { label: "Target", help: "Restrict the target to players of these disciplines/roles.",
                    DisciplineSelector {
                        selected: d.target_disciplines.clone(),
                        include_roles: true,
                        empty_label: "Any discipline",
                        on_change: move |v: Vec<String>| draft.write().target_disciplines = v,
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Refresh (tracked effects only)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub(super) fn RefreshCard(mut draft: Signal<EffectListItem>) -> Element {
    let d = draft.read().clone();
    rsx! {
        FormCard { icon: "fa-rotate", title: "Refresh",
            SubSection { title: "Refreshed by", help: REFRESH_HELP,
                RefreshAbilitiesEditor {
                    abilities: d.refresh_abilities.clone(),
                    on_change: move |abilities: Vec<RefreshAbility>| draft.write().refresh_abilities = abilities,
                }
            }
            Field {
                label: "Refresh Scope",
                help: "Which (source, target) axis identifies the same effect for refresh / Ignore Refreshes. Both = per (source, target) (default). Source = collapse across targets (e.g. one HealingTaken timer regardless of who was healed). Target = collapse across sources (e.g. one debuff entry regardless of which player applied it).",
                for scope in RefreshScope::all().iter().copied() {
                    TogglePill {
                        label: refresh_scope_label(scope),
                        active: scope == d.refresh_scope,
                        on_click: move |_| draft.write().refresh_scope = scope,
                    }
                }
            }
            SubSection { title: "Rules",
                CheckField {
                    label: "AoE refresh",
                    help: "Use damage correlation to detect multi-target refreshes instead of requiring individual ApplyEffect signals per target",
                    checked: d.is_aoe_refresh,
                    on_change: move |v: bool| {
                        let mut d = draft.write();
                        d.is_aoe_refresh = v;
                        if !v {
                            d.aoe_refresh_immediate = false;
                        }
                    },
                }
                if d.is_aoe_refresh {
                    CheckField { nested: true,
                        label: "Immediate",
                        help: "Off: only refreshes targets hit at the same time as the primary target (use for DOTs like Corrosive Grenade where ongoing ticks would cause false refreshes). On: any damage hit from the ability refreshes that target.",
                        checked: d.aoe_refresh_immediate,
                        on_change: move |v| draft.write().aoe_refresh_immediate = v,
                    }
                }
                CheckField {
                    label: "Refresh duration when charges modified",
                    help: "Reset timer when effect stacks change",
                    checked: d.is_refreshed_on_modify,
                    on_change: move |v| draft.write().is_refreshed_on_modify = v,
                }
                CheckField {
                    label: "Ignore refreshes",
                    help: "When the effect is already active, do not reset its duration if the trigger fires again.",
                    checked: d.ignore_refreshes,
                    on_change: move |v| draft.write().ignore_refreshes = v,
                }
            }
        }
    }
}
