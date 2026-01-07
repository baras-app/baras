//! Challenge editing tab
//!
//! CRUD for boss challenge definitions.
//! Uses ChallengeDefinition DSL type directly.

use dioxus::prelude::*;

use crate::api;
use crate::types::{
    BossWithPath, ChallengeColumns, ChallengeCondition, ChallengeDefinition, ChallengeMetric,
    ComparisonOp, EncounterItem, EntityFilter,
};
use crate::utils::parse_hex_color;

use super::InlineNameCreator;
use super::tabs::EncounterData;
use super::timers::PhaseSelector;
use super::triggers::EntityFilterDropdown;

// ─────────────────────────────────────────────────────────────────────────────
// Challenges Tab
// ─────────────────────────────────────────────────────────────────────────────

/// Create a default challenge definition
fn default_challenge(name: String) -> ChallengeDefinition {
    ChallengeDefinition {
        id: String::new(), // Backend generates ID
        name,
        display_text: None,
        description: None,
        metric: ChallengeMetric::Damage,
        conditions: vec![],
        enabled: true,
        color: None,
        columns: ChallengeColumns::TotalPercent,
    }
}

#[component]
pub fn ChallengesTab(
    boss_with_path: BossWithPath,
    encounter_data: EncounterData,
    on_change: EventHandler<Vec<ChallengeDefinition>>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let mut expanded_challenge = use_signal(|| None::<String>);

    // Extract challenges from BossWithPath
    let challenges = boss_with_path.boss.challenges.clone();

    rsx! {
        div { class: "challenges-tab",
            // Header
            div { class: "flex items-center justify-between mb-sm",
                span { class: "text-sm text-secondary", "{challenges.len()} challenges" }
                {
                    let bwp = boss_with_path.clone();
                    let challenges_for_create = challenges.clone();
                    rsx! {
                        InlineNameCreator {
                            button_label: "+ New Challenge",
                            placeholder: "Challenge name...",
                            on_create: move |name: String| {
                                let challenges_clone = challenges_for_create.clone();
                                let boss_id = bwp.boss.id.clone();
                                let file_path = bwp.file_path.clone();
                                let challenge = default_challenge(name);
                                let item = EncounterItem::Challenge(challenge);
                                spawn(async move {
                                    match api::create_encounter_item(&boss_id, &file_path, &item).await {
                                        Ok(EncounterItem::Challenge(created)) => {
                                            let created_id = created.id.clone();
                                            let mut current = challenges_clone;
                                            current.push(created);
                                            on_change.call(current);
                                            expanded_challenge.set(Some(created_id));
                                            on_status.call(("Created".to_string(), false));
                                        }
                                        Ok(_) => on_status.call(("Unexpected response type".to_string(), true)),
                                        Err(e) => on_status.call((e, true)),
                                    }
                                });
                            }
                        }
                    }
                }
            }

            // Challenge list
            if challenges.is_empty() {
                div { class: "empty-state text-sm", "No challenges defined" }
            } else {
                for challenge in challenges.clone() {
                    {
                        let challenge_key = challenge.id.clone();
                        let is_expanded = expanded_challenge() == Some(challenge_key.clone());
                        let challenges_for_row = challenges.clone();

                        rsx! {
                            ChallengeRow {
                                key: "{challenge_key}",
                                challenge: challenge.clone(),
                                boss_with_path: boss_with_path.clone(),
                                expanded: is_expanded,
                                encounter_data: encounter_data.clone(),
                                on_toggle: move |_| {
                                    expanded_challenge.set(if is_expanded { None } else { Some(challenge_key.clone()) });
                                },
                                on_change: on_change,
                                on_status: on_status,
                                on_collapse: move |_| expanded_challenge.set(None),
                                all_challenges: challenges_for_row,
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Challenge Row
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn ChallengeRow(
    challenge: ChallengeDefinition,
    boss_with_path: BossWithPath,
    expanded: bool,
    all_challenges: Vec<ChallengeDefinition>,
    encounter_data: EncounterData,
    on_toggle: EventHandler<()>,
    on_change: EventHandler<Vec<ChallengeDefinition>>,
    on_status: EventHandler<(String, bool)>,
    on_collapse: EventHandler<()>,
) -> Element {
    let metric_label = challenge.metric.label();
    let condition_count = challenge.conditions.len();

    // Extract context for API calls
    let boss_id = boss_with_path.boss.id.clone();
    let file_path = boss_with_path.file_path.clone();

    rsx! {
        div { class: "list-item",
            // Header row
            div {
                class: "list-item-header",
                onclick: move |_| on_toggle.call(()),
                span { class: "list-item-expand", if expanded { "▼" } else { "▶" } }
                span { class: "font-medium", "{challenge.name}" }
                span { class: "tag", "{metric_label}" }
                if condition_count > 0 {
                    span { class: "tag tag-secondary", "{condition_count} conditions" }
                }
            }

            // Expanded content
            if expanded {
                {
                    let boss_id_save = boss_id.clone();
                    let file_path_save = file_path.clone();
                    let boss_id_delete = boss_id.clone();
                    let file_path_delete = file_path.clone();

                    rsx! {
                        div { class: "list-item-body",
                            ChallengeEditForm {
                                challenge: challenge.clone(),
                                encounter_data: encounter_data,
                                on_save: move |updated: ChallengeDefinition| {
                                    let boss_id = boss_id_save.clone();
                                    let file_path = file_path_save.clone();
                                    let item = EncounterItem::Challenge(updated);
                                    on_status.call(("Saving...".to_string(), false));
                                    spawn(async move {
                                        match api::update_encounter_item(&boss_id, &file_path, &item, None).await {
                                            Ok(_) => on_status.call(("Saved".to_string(), false)),
                                            Err(_) => on_status.call(("Failed to save".to_string(), true)),
                                        }
                                    });
                                },
                                on_delete: {
                                    let all_challenges = all_challenges.clone();
                                    move |challenge_to_delete: ChallengeDefinition| {
                                        let all_challenges = all_challenges.clone();
                                        let boss_id = boss_id_delete.clone();
                                        let file_path = file_path_delete.clone();
                                        let challenge_id = challenge_to_delete.id.clone();
                                        spawn(async move {
                                            match api::delete_encounter_item("challenge", &challenge_id, &boss_id, &file_path).await {
                                                Ok(_) => {
                                                    let updated: Vec<_> = all_challenges.iter()
                                                        .filter(|c| c.id != challenge_id)
                                                        .cloned()
                                                        .collect();
                                                    on_change.call(updated);
                                                    on_collapse.call(());
                                                    on_status.call(("Deleted".to_string(), false));
                                                }
                                                Err(err) => {
                                                    on_status.call((err, true));
                                                }
                                            }
                                        });
                                    }
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Challenge Edit Form
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn ChallengeEditForm(
    challenge: ChallengeDefinition,
    encounter_data: EncounterData,
    on_save: EventHandler<ChallengeDefinition>,
    on_delete: EventHandler<ChallengeDefinition>,
) -> Element {
    // Clone values needed for closures and display
    let challenge_id_display = challenge.id.clone();
    let challenge_for_delete = challenge.clone();

    let mut draft = use_signal(|| challenge.clone());
    let original = challenge.clone();

    let has_changes = use_memo(move || draft() != original);

    let handle_save = move |_| {
        let updated = draft();
        on_save.call(updated);
    };

    let handle_delete = move |_| {
        on_delete.call(challenge_for_delete.clone());
    };

    rsx! {
        div { class: "challenge-edit-form",
            // ─── ID (read-only) ─────────────────────────────────────────────
            div { class: "form-row-hz",
                label { "Challenge ID" }
                code { class: "tag-muted text-mono text-xs", "{challenge_id_display}" }
            }

            // ─── Name ────────────────────────────────────────────────────────
            div { class: "form-row-hz",
                label { "Name" }
                input {
                    class: "input-inline",
                    style: "width: 300px;",
                    value: "{draft().name}",
                    oninput: move |e| {
                        let mut d = draft();
                        d.name = e.value();
                        draft.set(d);
                    }
                }
            }

            // ─── Display Text ────────────────────────────────────────────────
            div { class: "form-row-hz",
                label { "Display Text" }
                input {
                    class: "input-inline",
                    style: "width: 300px;",
                    placeholder: "(defaults to name)",
                    value: "{draft().display_text.clone().unwrap_or_default()}",
                    oninput: move |e| {
                        let mut d = draft();
                        d.display_text = if e.value().is_empty() { None } else { Some(e.value()) };
                        draft.set(d);
                    }
                }
            }

            // ─── Description ─────────────────────────────────────────────────
            div { class: "form-row-hz",
                label { "Description" }
                input {
                    class: "input-inline",
                    style: "width: 400px;",
                    placeholder: "(optional)",
                    value: "{draft().description.clone().unwrap_or_default()}",
                    oninput: move |e| {
                        let mut d = draft();
                        d.description = if e.value().is_empty() { None } else { Some(e.value()) };
                        draft.set(d);
                    }
                }
            }

            // ─── Metric ──────────────────────────────────────────────────────
            div { class: "form-row-hz",
                label { "Metric" }
                select {
                    class: "input-inline",
                    value: "{draft().metric:?}",
                    onchange: move |e| {
                        let mut d = draft();
                        d.metric = match e.value().as_str() {
                            "Damage" => ChallengeMetric::Damage,
                            "Healing" => ChallengeMetric::Healing,
                            "DamageTaken" => ChallengeMetric::DamageTaken,
                            "HealingTaken" => ChallengeMetric::HealingTaken,
                            "AbilityCount" => ChallengeMetric::AbilityCount,
                            "EffectCount" => ChallengeMetric::EffectCount,
                            "Deaths" => ChallengeMetric::Deaths,
                            "Threat" => ChallengeMetric::Threat,
                            _ => ChallengeMetric::Damage,
                        };
                        draft.set(d);
                    },
                    for metric in ChallengeMetric::all() {
                        option {
                            value: "{metric:?}",
                            selected: draft().metric == *metric,
                            "{metric.label()}"
                        }
                    }
                }
            }

            // ─── Display Settings ────────────────────────────────────────────
            div { class: "form-row-hz",
                label { "Enabled" }
                input {
                    r#type: "checkbox",
                    checked: draft().enabled,
                    onchange: move |e| {
                        let mut d = draft();
                        d.enabled = e.checked();
                        draft.set(d);
                    }
                }
                span { class: "text-muted text-sm", style: "margin-left: 8px;", "(show in overlay)" }
            }

            div { class: "form-row-hz",
                label { "Columns" }
                select {
                    class: "input-inline",
                    value: match draft().columns {
                        ChallengeColumns::TotalPercent => "total_percent",
                        ChallengeColumns::TotalPerSecond => "total_per_second",
                        ChallengeColumns::PerSecondPercent => "per_second_percent",
                        ChallengeColumns::TotalOnly => "total_only",
                        ChallengeColumns::PerSecondOnly => "per_second_only",
                        ChallengeColumns::PercentOnly => "percent_only",
                    },
                    onchange: move |e| {
                        let mut d = draft();
                        d.columns = match e.value().as_str() {
                            "total_per_second" => ChallengeColumns::TotalPerSecond,
                            "per_second_percent" => ChallengeColumns::PerSecondPercent,
                            "total_only" => ChallengeColumns::TotalOnly,
                            "per_second_only" => ChallengeColumns::PerSecondOnly,
                            "percent_only" => ChallengeColumns::PercentOnly,
                            _ => ChallengeColumns::TotalPercent,
                        };
                        draft.set(d);
                    },
                    option { value: "total_percent", selected: matches!(draft().columns, ChallengeColumns::TotalPercent), "Total + Percent" }
                    option { value: "total_per_second", selected: matches!(draft().columns, ChallengeColumns::TotalPerSecond), "Total + Per Second" }
                    option { value: "per_second_percent", selected: matches!(draft().columns, ChallengeColumns::PerSecondPercent), "Per Second + Percent" }
                    option { value: "total_only", selected: matches!(draft().columns, ChallengeColumns::TotalOnly), "Total Only" }
                    option { value: "per_second_only", selected: matches!(draft().columns, ChallengeColumns::PerSecondOnly), "Per Second Only" }
                    option { value: "percent_only", selected: matches!(draft().columns, ChallengeColumns::PercentOnly), "Percent Only" }
                }
            }

            {
                let current_color = draft().color;
                let color_hex = current_color
                    .map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]))
                    .unwrap_or_else(|| "#4a90d9".to_string()); // Default blue

                rsx! {
                    div { class: "form-row-hz",
                        label { "Bar Color" }
                        div { class: "flex-row gap-sm",
                            input {
                                r#type: "color",
                                class: "color-picker",
                                value: "{color_hex}",
                                oninput: move |e| {
                                    if let Some(color) = parse_hex_color(&e.value()) {
                                        let mut d = draft();
                                        d.color = Some([color[0], color[1], color[2], color[3]]);
                                        draft.set(d);
                                    }
                                }
                            }
                            if current_color.is_some() {
                                button {
                                    class: "btn btn-sm",
                                    title: "Use default color",
                                    onclick: move |_| {
                                        let mut d = draft();
                                        d.color = None;
                                        draft.set(d);
                                    },
                                    i { class: "fa-solid fa-rotate-left" }
                                }
                            }
                            if current_color.is_none() {
                                span { class: "text-muted text-sm", "(using default)" }
                            }
                        }
                    }
                }
            }

            // ─── Conditions ──────────────────────────────────────────────────
            div { class: "form-row-hz", style: "align-items: flex-start;",
                label { style: "padding-top: 6px;", "Conditions" }
                div { class: "flex-col gap-xs",
                    if draft().conditions.is_empty() {
                        span { class: "text-sm text-muted", "(matches all events)" }
                    } else {
                        for (idx, condition) in draft().conditions.iter().enumerate() {
                            ChallengeConditionRow {
                                condition: condition.clone(),
                                available_phases: encounter_data.phase_ids(),
                                on_change: move |updated| {
                                    let mut d = draft();
                                    d.conditions[idx] = updated;
                                    draft.set(d);
                                },
                                on_remove: move |_| {
                                    let mut d = draft();
                                    d.conditions.remove(idx);
                                    draft.set(d);
                                },
                            }
                        }
                    }
                    button {
                        class: "btn btn-sm",
                        style: "width: fit-content;",
                        onclick: move |_| {
                            let mut d = draft();
                            d.conditions.push(ChallengeCondition::Phase { phase_ids: vec![] });
                            draft.set(d);
                        },
                        "+ Add Condition"
                    }
                }
            }

            // ─── Actions ─────────────────────────────────────────────────────
            div { class: "form-actions",
                button {
                    class: if has_changes() { "btn btn-success btn-sm" } else { "btn btn-sm" },
                    disabled: !has_changes(),
                    onclick: handle_save,
                    "Save"
                }
                button {
                    class: "btn btn-danger btn-sm",
                    onclick: handle_delete,
                    "Delete"
                }
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
    available_phases: Vec<String>,
    on_change: EventHandler<ChallengeCondition>,
    on_remove: EventHandler<()>,
) -> Element {
    let condition_type = condition.label();

    rsx! {
        div {
            class: "flex gap-sm items-start",
            style: "padding: 8px; background: var(--bg-secondary); border-radius: 4px; margin-bottom: 4px;",

            // Condition type selector
            select {
                class: "input-inline",
                style: "width: 120px; flex-shrink: 0;",
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

            // Condition-specific editor (flex-1 to fill space)
            div { class: "flex-1",
                {
                    match &condition {
                        ChallengeCondition::Phase { phase_ids } => {
                            rsx! {
                                PhaseSelector {
                                    selected: phase_ids.clone(),
                                    available: available_phases.clone(),
                                    on_change: move |ids| {
                                        on_change.call(ChallengeCondition::Phase { phase_ids: ids });
                                    }
                                }
                            }
                        }
                        ChallengeCondition::Source { matcher } => {
                            rsx! {
                                EntityFilterDropdown {
                                    label: "",
                                    value: matcher.clone(),
                                    options: EntityFilter::common_options(),
                                    on_change: move |m| {
                                        on_change.call(ChallengeCondition::Source { matcher: m });
                                    }
                                }
                            }
                        }
                        ChallengeCondition::Target { matcher } => {
                            rsx! {
                                EntityFilterDropdown {
                                    label: "",
                                    value: matcher.clone(),
                                    options: EntityFilter::common_options(),
                                    on_change: move |m| {
                                        on_change.call(ChallengeCondition::Target { matcher: m });
                                    }
                                }
                            }
                        }
                        ChallengeCondition::Ability { ability_ids } => rsx! {
                            IdListInput {
                                ids: ability_ids.clone(),
                                placeholder: "ability_id1, ability_id2, ...",
                                on_change: move |ids| on_change.call(ChallengeCondition::Ability { ability_ids: ids })
                            }
                        },
                        ChallengeCondition::Effect { effect_ids } => rsx! {
                            IdListInput {
                                ids: effect_ids.clone(),
                                placeholder: "effect_id1, effect_id2, ...",
                                on_change: move |ids| on_change.call(ChallengeCondition::Effect { effect_ids: ids })
                            }
                        },
                        ChallengeCondition::Counter { counter_id, operator, value } => {
                            let counter_id_for_select = counter_id.clone();
                            let counter_id_for_input = counter_id.clone();
                            let current_op = *operator;
                            let current_val = *value;
                            rsx! {
                                div { class: "flex gap-xs items-center flex-wrap",
                                    input {
                                        class: "input-inline text-mono",
                                        style: "width: 150px;",
                                        placeholder: "counter_id",
                                        value: "{counter_id}",
                                        oninput: move |e| {
                                            on_change.call(ChallengeCondition::Counter {
                                                counter_id: e.value(),
                                                operator: current_op,
                                                value: current_val,
                                            });
                                        }
                                    }
                                    select {
                                        class: "input-inline",
                                        style: "width: 60px;",
                                        value: "{current_op:?}",
                                        onchange: move |e| {
                                            let op = match e.value().as_str() {
                                                "Eq" => ComparisonOp::Eq,
                                                "Lt" => ComparisonOp::Lt,
                                                "Gt" => ComparisonOp::Gt,
                                                "Lte" => ComparisonOp::Lte,
                                                "Gte" => ComparisonOp::Gte,
                                                "Ne" => ComparisonOp::Ne,
                                                _ => ComparisonOp::Eq,
                                            };
                                            on_change.call(ChallengeCondition::Counter {
                                                counter_id: counter_id_for_select.clone(),
                                                operator: op,
                                                value: current_val,
                                            });
                                        },
                                        for op in ComparisonOp::all() {
                                            option {
                                                value: "{op:?}",
                                                selected: current_op == *op,
                                                "{op.label()}"
                                            }
                                        }
                                    }
                                    input {
                                        r#type: "number",
                                        min: "0",
                                        class: "input-inline",
                                        style: "width: 70px;",
                                        value: "{current_val}",
                                        oninput: move |e| {
                                            if let Ok(v) = e.value().parse::<u32>() {
                                                on_change.call(ChallengeCondition::Counter {
                                                    counter_id: counter_id_for_input.clone(),
                                                    operator: current_op,
                                                    value: v,
                                                });
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
                                div { class: "flex gap-xs items-center",
                                    span { class: "text-sm", "HP:" }
                                    input {
                                        r#type: "number",
                                        min: "0",
                                        max: "100",
                                        class: "input-inline",
                                        style: "width: 70px;",
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
                                    span { class: "text-sm", "to" }
                                    input {
                                        r#type: "number",
                                        min: "0",
                                        max: "100",
                                        class: "input-inline",
                                        style: "width: 70px;",
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
                                    span { class: "text-sm", "%" }
                                }
                            }
                        }
                    }
                }
            }

            // Remove button (flex-shrink-0 to keep fixed size)
            button {
                class: "btn btn-danger btn-xs",
                style: "flex-shrink: 0;",
                onclick: move |_| on_remove.call(()),
                "×"
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Comma-separated ID input
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn IdListInput(
    ids: Vec<u64>,
    placeholder: &'static str,
    on_change: EventHandler<Vec<u64>>,
) -> Element {
    let ids_str = ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    rsx! {
        input {
            class: "input-inline text-mono",
            style: "width: 100%;",
            placeholder: placeholder,
            value: "{ids_str}",
            oninput: move |e| {
                let parsed: Vec<u64> = e.value()
                    .split(',')
                    .filter_map(|s| s.trim().parse().ok())
                    .collect();
                on_change.call(parsed);
            }
        }
    }
}
