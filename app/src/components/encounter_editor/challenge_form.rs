//! Challenge edit form, shown in the editor drawer

use dioxus::prelude::*;

use crate::components::editor_common::{FormCard, ItemActions, Origin};
use crate::components::form_fields::{CheckField, Field, SubSection, TextField};
use crate::types::{ChallengeColumns, ChallengeCondition, ChallengeDefinition, ChallengeMetric, ComparisonOp, EntityFilter};
use crate::utils::parse_hex_color;

use super::DifficultiesEditor;
use super::challenges::IdChipEditor;
use super::tabs::EncounterData;
use super::timers::PhaseSelector;
use super::triggers::EntityFilterDropdown;

/// (value, label) per column layout, in menu order
const COLUMNS: [(ChallengeColumns, &str, &str); 6] = [
    (ChallengeColumns::TotalPercent, "total_percent", "Total + Percent"),
    (ChallengeColumns::TotalPerSecond, "total_per_second", "Total + Per Second"),
    (ChallengeColumns::PerSecondPercent, "per_second_percent", "Per Second + Percent"),
    (ChallengeColumns::TotalOnly, "total_only", "Total Only"),
    (ChallengeColumns::PerSecondOnly, "per_second_only", "Per Second Only"),
    (ChallengeColumns::PercentOnly, "percent_only", "Percent Only"),
];

#[component]
pub(super) fn ChallengeEditForm(
    challenge: ChallengeDefinition,
    encounter_data: EncounterData,
    #[props(default)] is_builtin: bool,
    on_save: EventHandler<ChallengeDefinition>,
    on_delete: EventHandler<ChallengeDefinition>,
    #[props(default)] on_dirty: EventHandler<bool>,
) -> Element {
    let challenge_id_display = challenge.id.clone();
    let challenge_for_delete = challenge.clone();
    let challenge_for_draft = challenge.clone();
    let original = challenge.clone();

    let mut draft = use_signal(|| challenge_for_draft);
    let mut just_saved = use_signal(|| false);

    // Reset just_saved when user makes new changes after saving
    let original_for_effect = original.clone();
    use_effect(move || {
        if draft() != original_for_effect && just_saved() {
            just_saved.set(false);
        }
    });

    let has_changes = use_memo(move || !just_saved() && draft() != original);

    // Notify parent when dirty state changes
    use_effect(move || {
        on_dirty.call(has_changes());
    });

    let handle_save = move |_: ()| {
        just_saved.set(true);
        on_save.call(draft());
    };

    let handle_delete = move |_: ()| {
        on_delete.call(challenge_for_delete.clone());
    };

    let current_color = draft().color;
    let color_hex = current_color
        .map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]))
        .unwrap_or_else(|| "#4a90d9".to_string());
    let columns_value = COLUMNS.iter().find(|(c, ..)| *c == draft().columns).map(|(_, v, _)| *v).unwrap_or("total_percent");

    rsx! {
        div { class: "challenge-edit-form",
            div { class: "edit-grid",
                // ═══ LEFT: Identity Card ═════════════════════════════════════
                FormCard { icon: "fa-tag", title: "Identity",
                    Field { label: "Challenge ID",
                        code { class: "id-display", "{challenge_id_display}" }
                    }
                    TextField { label: "Name", value: draft().name, on_input: move |v| draft.write().name = v }
                    TextField {
                        label: "Display Text",
                        value: draft().display_text.unwrap_or_default(),
                        placeholder: "(defaults to name)",
                        on_input: move |v: String| draft.write().display_text = (!v.is_empty()).then_some(v),
                    }
                    TextField {
                        label: "Description",
                        value: draft().description.unwrap_or_default(),
                        placeholder: "(optional)",
                        on_input: move |v: String| draft.write().description = (!v.is_empty()).then_some(v),
                    }
                    Field {
                        label: "Difficulties",
                        help: "Only track this challenge on selected difficulties. Empty = all difficulties.",
                        DifficultiesEditor {
                            difficulties: draft().difficulties.clone(),
                            on_change: move |updated: Vec<String>| draft.write().difficulties = updated,
                        }
                    }
                    CheckField { label: "Enabled", checked: draft().enabled, on_change: move |v| draft.write().enabled = v }

                    SubSection { title: "Display",
                        Field { label: "Metric", help: "What combat metric to track for this challenge",
                            select {
                                value: "{draft().metric:?}",
                                onchange: move |e| {
                                    draft.write().metric = match e.value().as_str() {
                                        "Damage" => ChallengeMetric::Damage,
                                        "Healing" => ChallengeMetric::Healing,
                                        "EffectiveHealing" => ChallengeMetric::EffectiveHealing,
                                        "DamageTaken" => ChallengeMetric::DamageTaken,
                                        "HealingTaken" => ChallengeMetric::HealingTaken,
                                        "AbilityCount" => ChallengeMetric::AbilityCount,
                                        "EffectCount" => ChallengeMetric::EffectCount,
                                        "EffectStacks" => ChallengeMetric::EffectStacks,
                                        "DamageAbsorbed" => ChallengeMetric::DamageAbsorbed,
                                        "InterruptCount" => ChallengeMetric::InterruptCount,
                                        _ => ChallengeMetric::Damage,
                                    };
                                },
                                for metric in ChallengeMetric::all() {
                                    option { value: "{metric:?}", selected: draft().metric == *metric, "{metric.label()}" }
                                }
                            }
                        }
                        Field { label: "Columns", help: "Which data columns to show in the challenge overlay",
                            select {
                                value: columns_value,
                                onchange: move |e| {
                                    let v = e.value();
                                    draft.write().columns = COLUMNS
                                        .iter()
                                        .find(|(_, key, _)| *key == v)
                                        .map(|(c, ..)| *c)
                                        .unwrap_or(ChallengeColumns::TotalPercent);
                                },
                                for (_, key, label) in COLUMNS {
                                    option { value: key, selected: key == columns_value, "{label}" }
                                }
                            }
                        }
                        Field { label: "Bar Color",
                            input {
                                r#type: "color",
                                class: "color-picker",
                                value: "{color_hex}",
                                oninput: move |e| {
                                    if let Some(color) = parse_hex_color(&e.value()) {
                                        draft.write().color = Some([color[0], color[1], color[2], color[3]]);
                                    }
                                }
                            }
                            if current_color.is_some() {
                                button {
                                    class: "btn btn-sm",
                                    title: "Use default color",
                                    onclick: move |_| draft.write().color = None,
                                    i { class: "fa-solid fa-rotate-left" }
                                }
                            } else {
                                span { class: "field-unit", "(using default)" }
                            }
                        }
                    }
                }

                // ═══ RIGHT: Conditions Card ══════════════════════════════════
                FormCard { icon: "fa-filter", title: "Conditions",
                    SubSection { title: "Match", help: "Events count toward the challenge only when every condition holds",
                        div { class: "flex-col gap-xs",
                            if draft().conditions.is_empty() {
                                span { class: "conditions-editor-empty", "No conditions (matches all events)" }
                            } else {
                                for (idx, condition) in draft().conditions.iter().enumerate() {
                                    ChallengeConditionRow {
                                        condition: condition.clone(),
                                        encounter_data: encounter_data.clone(),
                                        on_change: move |updated| draft.write().conditions[idx] = updated,
                                        on_remove: move |_| { draft.write().conditions.remove(idx); },
                                    }
                                }
                            }
                            button {
                                class: "btn-dashed text-sm",
                                onclick: move |_| draft.write().conditions.push(ChallengeCondition::Phase { phase_ids: vec![] }),
                                "+ Add Condition"
                            }
                        }
                    }
                }
            }

            ItemActions {
                has_changes: has_changes(),
                origin: Origin::of(is_builtin, false),
                on_save: handle_save,
                on_delete: handle_delete,
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Challenge Condition Row
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn ChallengeConditionRow(
    condition: ChallengeCondition,
    encounter_data: EncounterData,
    on_change: EventHandler<ChallengeCondition>,
    on_remove: EventHandler<()>,
) -> Element {
    let condition_type = condition.label();

    rsx! {
        div { class: "condition-card",
            div { class: "condition-card-content",
                div { class: "condition-simple",
                    // Row 1: Type selector
                    div { class: "condition-simple-fields",
                        span { class: "condition-label", "Type" }
                        select {
                            class: "select",
                            value: "{condition_type}",
                            onchange: move |e| {
                                let new_condition = match e.value().as_str() {
                                    "Phase" => ChallengeCondition::Phase { phase_ids: vec![] },
                                    "Source" => ChallengeCondition::Source { matcher: EntityFilter::Boss },
                                    "Target" => ChallengeCondition::Target { matcher: EntityFilter::Boss },
                                    "Ability" => ChallengeCondition::Ability { ability_ids: vec![] },
                                    "Effect" => ChallengeCondition::Effect { effect_ids: vec![] },
                                    "Counter" => ChallengeCondition::Counter {
                                        counter_id: String::new(),
                                        operator: ComparisonOp::Eq,
                                        value: 0,
                                    },
                                    "Boss HP Range" => ChallengeCondition::BossHpRange {
                                        min_hp: None,
                                        max_hp: None,
                                        npc_id: None,
                                    },
                                    _ => condition.clone(),
                                };
                                on_change.call(new_condition);
                            },
                            option { value: "Phase", "Phase" }
                            option { value: "Source", "Source" }
                            option { value: "Target", "Target" }
                            option { value: "Ability", "Ability" }
                            option { value: "Effect", "Effect" }
                            option { value: "Counter", "Counter" }
                            option { value: "Boss HP Range", "Boss HP Range" }
                        }
                    }

                    // Row 2: Condition-specific fields
                    div { class: "condition-simple-fields",
                        {
                    match &condition {
                        ChallengeCondition::Phase { phase_ids } => rsx! {
                            span { class: "condition-label", "Phase" }
                            PhaseSelector {
                                selected: phase_ids.clone(),
                                available: encounter_data.phase_ids(),
                                on_change: move |ids| {
                                    on_change.call(ChallengeCondition::Phase { phase_ids: ids });
                                }
                            }
                        },
                        ChallengeCondition::Source { matcher } => rsx! {
                            span { class: "condition-label", "Entity" }
                            EntityFilterDropdown {
                                label: "",
                                value: matcher.clone(),
                                options: EntityFilter::common_options(),
                                on_change: move |m| {
                                    on_change.call(ChallengeCondition::Source { matcher: m });
                                }
                            }
                        },
                        ChallengeCondition::Target { matcher } => rsx! {
                            span { class: "condition-label", "Entity" }
                            EntityFilterDropdown {
                                label: "",
                                value: matcher.clone(),
                                options: EntityFilter::common_options(),
                                on_change: move |m| {
                                    on_change.call(ChallengeCondition::Target { matcher: m });
                                }
                            }
                        },
                        ChallengeCondition::Ability { ability_ids } => rsx! {
                            span { class: "condition-label", "IDs" }
                            IdChipEditor {
                                ids: ability_ids.clone(),
                                placeholder: "Ability ID (Enter)",
                                on_change: move |ids| on_change.call(ChallengeCondition::Ability { ability_ids: ids })
                            }
                        },
                        ChallengeCondition::Effect { effect_ids } => rsx! {
                            span { class: "condition-label", "IDs" }
                            IdChipEditor {
                                ids: effect_ids.clone(),
                                placeholder: "Effect ID (Enter)",
                                on_change: move |ids| on_change.call(ChallengeCondition::Effect { effect_ids: ids })
                            }
                        },
                        ChallengeCondition::Counter { counter_id, operator, value } => {
                            let selected_counter = if counter_id.is_empty() {
                                "__none__".to_string()
                            } else {
                                counter_id.clone()
                            };
                            let counters = encounter_data.counter_ids();
                            let counter_id_for_op = counter_id.clone();
                            let counter_id_for_val = counter_id.clone();
                            let has_counter = !counter_id.is_empty();
                            let current_op = *operator;
                            let current_val = *value;
                            let op_value = operator.as_str();
                            rsx! {
                                span { class: "condition-label", "Counter" }
                                select {
                                    class: "select",
                                    value: "{selected_counter}",
                                    onchange: move |e| {
                                        let new_id = if e.value() == "__none__" {
                                            String::new()
                                        } else {
                                            e.value()
                                        };
                                        on_change.call(ChallengeCondition::Counter {
                                            counter_id: new_id,
                                            operator: current_op,
                                            value: current_val,
                                        });
                                    },
                                    option { value: "__none__", selected: selected_counter == "__none__", "(select counter)" }
                                    for cid in &counters {
                                        option { value: "{cid}", selected: *cid == selected_counter, "{cid}" }
                                    }
                                }
                                if has_counter {
                                    select {
                                        class: "select w-xs shrink-0",
                                        value: "{op_value}",
                                        onchange: {
                                            let cid = counter_id_for_op.clone();
                                            move |e| {
                                                on_change.call(ChallengeCondition::Counter {
                                                    counter_id: cid.clone(),
                                                    operator: ComparisonOp::from_str_or(&e.value(), ComparisonOp::Eq),
                                                    value: current_val,
                                                });
                                            }
                                        },
                                        for op in ComparisonOp::all() {
                                            option { value: "{op.as_str()}", selected: op_value == op.as_str(), "{op.label()}" }
                                        }
                                    }
                                    input {
                                        r#type: "number",
                                        class: "input-inline w-xs shrink-0",
                                        min: "0",
                                        value: "{current_val}",
                                        oninput: {
                                            let cid = counter_id_for_val.clone();
                                            move |e| {
                                                if let Ok(v) = e.value().parse::<u32>() {
                                                    on_change.call(ChallengeCondition::Counter {
                                                        counter_id: cid.clone(),
                                                        operator: current_op,
                                                        value: v,
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        ChallengeCondition::BossHpRange { min_hp, max_hp, npc_id } => {
                            let current_min = *min_hp;
                            let current_max = *max_hp;
                            let current_npc = *npc_id;
                            rsx! {
                                span { class: "condition-label", "HP %" }
                                input {
                                    r#type: "number",
                                    min: "0",
                                    max: "100",
                                    class: "input-inline w-sm",
                                    placeholder: "min",
                                    value: "{current_min.map(|v| v.to_string()).unwrap_or_default()}",
                                    oninput: move |e| {
                                        let min = e.value().parse().ok();
                                        on_change.call(ChallengeCondition::BossHpRange {
                                            min_hp: min,
                                            max_hp: current_max,
                                            npc_id: current_npc,
                                        });
                                    }
                                }
                                span { class: "text-sm text-muted", "to" }
                                input {
                                    r#type: "number",
                                    min: "0",
                                    max: "100",
                                    class: "input-inline w-sm",
                                    placeholder: "max",
                                    value: "{current_max.map(|v| v.to_string()).unwrap_or_default()}",
                                    oninput: move |e| {
                                        let max = e.value().parse().ok();
                                        on_change.call(ChallengeCondition::BossHpRange {
                                            min_hp: current_min,
                                            max_hp: max,
                                            npc_id: current_npc,
                                        });
                                    }
                                }
                                span { class: "text-sm text-muted", "%" }
                            }
                        }
                    }
                }
                    }
                }
            }

            // Remove button
            button {
                class: "btn btn-danger btn-xs",
                onclick: move |_| on_remove.call(()),
                "×"
            }
        }
    }
}
