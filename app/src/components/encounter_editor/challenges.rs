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

use super::tabs::EncounterData;
use super::timers::PhaseSelector;
use super::triggers::EntityFilterDropdown;
use super::{DifficultiesEditor, InlineNameCreator};

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
        difficulties: vec![],
    }
}

#[component]
pub fn ChallengesTab(
    boss_with_path: BossWithPath,
    encounter_data: EncounterData,
    expanded_challenge: Signal<Option<String>>,
    on_change: EventHandler<Vec<ChallengeDefinition>>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
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
    let mut is_dirty = use_signal(|| false);
    let metric_label = challenge.metric.label();
    let condition_count = challenge.conditions.len();

    // Origin classification
    let is_builtin = boss_with_path.builtin_challenge_ids.contains(&challenge.id);
    let is_modified = boss_with_path
        .modified_challenge_ids
        .contains(&challenge.id);

    // Clones for enable toggle closure
    let challenge_for_enable = challenge.clone();
    let challenges_for_enable = all_challenges.clone();
    let bwp_for_enable = boss_with_path.clone();

    // Extract context for API calls
    let boss_id = boss_with_path.boss.id.clone();
    let file_path = boss_with_path.file_path.clone();

    rsx! {
        div { class: if challenge.enabled { "list-item" } else { "list-item item-disabled" },
            // Header row
            div {
                class: "list-item-header",
                onclick: move |_| on_toggle.call(()),

                // Expand arrow
                span { class: "list-item-expand", if expanded { "▼" } else { "▶" } }

                // Origin indicator (B)uilt-in / (M)odified / (C)ustom
                if is_builtin {
                    span {
                        class: "timer-origin timer-origin-builtin",
                        title: "Built-in: ships with the app",
                        "B"
                    }
                } else if is_modified {
                    span {
                        class: "timer-origin timer-origin-modified",
                        title: "Modified: built-in challenge you have edited",
                        "M"
                    }
                } else {
                    span {
                        class: "timer-origin timer-origin-custom",
                        title: "Custom: created by you",
                        "C"
                    }
                }

                span { class: "font-medium", "{challenge.name}" }
                if expanded && is_dirty() {
                    span { class: "unsaved-indicator", title: "Unsaved changes" }
                }
                span { class: "tag", "{metric_label}" }
                if condition_count > 0 {
                    span { class: "tag tag-secondary", "{condition_count} conditions" }
                }

                // Right side - enable toggle
                div { class: "flex items-center gap-xs", style: "margin-left: auto; flex-shrink: 0;",
                    span {
                        class: "row-toggle",
                        title: if challenge.enabled { "Disable challenge" } else { "Enable challenge" },
                        onclick: move |e| {
                            e.stop_propagation();
                            let mut updated = challenge_for_enable.clone();
                            updated.enabled = !updated.enabled;
                            let mut current = challenges_for_enable.clone();
                            if let Some(idx) = current.iter().position(|c| c.id == updated.id) {
                                current[idx] = updated.clone();
                                on_change.call(current);
                            }
                            let boss_id = bwp_for_enable.boss.id.clone();
                            let file_path = bwp_for_enable.file_path.clone();
                            let item = EncounterItem::Challenge(updated);
                            spawn(async move {
                                let _ = api::update_encounter_item(&boss_id, &file_path, &item, None).await;
                            });
                        },
                        span {
                            class: if challenge.enabled { "text-success" } else { "text-muted" },
                            if challenge.enabled { "✓" } else { "○" }
                        }
                    }
                }
            }

            // Expanded content
            if expanded {
                {
                    let all_challenges_for_save = all_challenges.clone();
                    let boss_id_save = boss_id.clone();
                    let file_path_save = file_path.clone();
                    let boss_id_delete = boss_id.clone();
                    let file_path_delete = file_path.clone();

                    rsx! {
                        div { class: "list-item-body",
                            ChallengeEditForm {
                                challenge: challenge.clone(),
                                encounter_data: encounter_data,
                                on_dirty: move |dirty: bool| is_dirty.set(dirty),
                                on_save: move |updated: ChallengeDefinition| {
                                    // Update parent state synchronously so props refresh and dirty indicator clears
                                    let mut current = all_challenges_for_save.clone();
                                    if let Some(idx) = current.iter().position(|c| c.id == updated.id) {
                                        current[idx] = updated.clone();
                                        on_change.call(current);
                                    }
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
    #[props(default)] on_dirty: EventHandler<bool>,
) -> Element {
    // Clone values needed for closures and display
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

    let handle_save = move |_| {
        just_saved.set(true);
        let updated = draft();
        on_save.call(updated);
    };

    let handle_delete = move |_| {
        on_delete.call(challenge_for_delete.clone());
    };

    rsx! {
        div { class: "challenge-edit-form",
            div { class: "encounter-item-grid",
                // ═══ LEFT: Identity Card ═════════════════════════════════════
                div { class: "form-card",
                    div { class: "form-card-header",
                        i { class: "fa-solid fa-tag" }
                        span { "Identity" }
                    }
                    div { class: "form-card-content",
                        div { class: "form-row-hz",
                            label { "Challenge ID" }
                            code { class: "tag-muted text-mono text-xs", "{challenge_id_display}" }
                        }

                        div { class: "form-row-hz",
                            label { "Name" }
                            input {
                                class: "input-inline",
                                style: "width: 220px;",
                                value: "{draft().name}",
                                oninput: move |e| {
                                    let mut d = draft();
                                    d.name = e.value();
                                    draft.set(d);
                                }
                            }
                        }

                        div { class: "form-row-hz",
                            label { "Display Text" }
                            input {
                                class: "input-inline",
                                style: "width: 220px;",
                                placeholder: "(defaults to name)",
                                value: "{draft().display_text.clone().unwrap_or_default()}",
                                oninput: move |e| {
                                    let mut d = draft();
                                    d.display_text = if e.value().is_empty() { None } else { Some(e.value()) };
                                    draft.set(d);
                                }
                            }
                        }

                        div { class: "form-row-hz",
                            label { "Description" }
                            input {
                                class: "input-inline",
                                style: "width: 220px;",
                                placeholder: "(optional)",
                                value: "{draft().description.clone().unwrap_or_default()}",
                                oninput: move |e| {
                                    let mut d = draft();
                                    d.description = if e.value().is_empty() { None } else { Some(e.value()) };
                                    draft.set(d);
                                }
                            }
                        }

                        // ─── Display Settings ──────────────────────────────────
                        span { class: "text-sm font-bold text-secondary mt-sm", "Display" }

                        div { class: "form-row-hz mt-xs",
                            label { class: "flex items-center",
                                "Metric"
                                span {
                                    class: "help-icon",
                                    title: "What combat metric to track for this challenge",
                                    "?"
                                }
                            }
                            select {
                                class: "input-inline",
                                value: "{draft().metric:?}",
                                onchange: move |e| {
                                    let mut d = draft();
                                    d.metric = match e.value().as_str() {
                                        "Damage" => ChallengeMetric::Damage,
                                        "Healing" => ChallengeMetric::Healing,
                                        "EffectiveHealing" => ChallengeMetric::EffectiveHealing,
                                        "DamageTaken" => ChallengeMetric::DamageTaken,
                                        "HealingTaken" => ChallengeMetric::HealingTaken,
                                        "AbilityCount" => ChallengeMetric::AbilityCount,
                                        "EffectCount" => ChallengeMetric::EffectCount,
                                        "EffectStacks" => ChallengeMetric::EffectStacks,
                                        "DamageAbsorbed" => ChallengeMetric::DamageAbsorbed,
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

                        div { class: "form-row-hz",
                            label { class: "flex items-center",
                                "Columns"
                                span {
                                    class: "help-icon",
                                    title: "Which data columns to show in the challenge overlay",
                                    "?"
                                }
                            }
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
                                .unwrap_or_else(|| "#4a90d9".to_string());

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
                        }

                        div { class: "form-row-hz",
                            label { class: "flex items-center",
                                "Difficulties"
                                span {
                                    class: "help-icon",
                                    title: "Only track this challenge on selected difficulties. Empty = all difficulties.",
                                    "?"
                                }
                            }
                            DifficultiesEditor {
                                difficulties: draft().difficulties.clone(),
                                on_change: move |updated: Vec<String>| {
                                    let mut d = draft();
                                    d.difficulties = updated;
                                    draft.set(d);
                                }
                            }
                        }
                    }
                }

                // ═══ RIGHT: Conditions Card ══════════════════════════════════
                div { class: "form-card",
                    div { class: "form-card-header",
                        i { class: "fa-solid fa-filter" }
                        span { "Conditions" }
                    }
                    div { class: "form-card-content",
                        if draft().conditions.is_empty() {
                            span { class: "text-sm text-muted", "(matches all events)" }
                        } else {
                            for (idx, condition) in draft().conditions.iter().enumerate() {
                                ChallengeConditionRow {
                                    condition: condition.clone(),
                                    encounter_data: encounter_data.clone(),
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
                                        class: "select",
                                        style: "width: 55px; flex-shrink: 0;",
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
                                        class: "input-inline",
                                        style: "width: 55px; flex-shrink: 0;",
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
                                span { class: "text-sm text-muted", "to" }
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

// ─────────────────────────────────────────────────────────────────────────────
// Chip-based ID editor (adapted from NpcIdChipEditor for u64 IDs)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn IdChipEditor(
    ids: Vec<u64>,
    placeholder: &'static str,
    on_change: EventHandler<Vec<u64>>,
) -> Element {
    let mut new_input = use_signal(String::new);
    let ids_for_keydown = ids.clone();
    let ids_for_click = ids.clone();

    rsx! {
        div { class: "flex-col gap-xs",
            // ID chips
            if !ids.is_empty() {
                div { class: "flex flex-wrap gap-xs mb-xs",
                    for (idx, id) in ids.iter().enumerate() {
                        {
                            let ids_clone = ids.clone();
                            rsx! {
                                span { class: "chip text-mono",
                                    "{id}"
                                    button {
                                        class: "chip-remove",
                                        onclick: move |_| {
                                            let mut new_ids = ids_clone.clone();
                                            new_ids.remove(idx);
                                            on_change.call(new_ids);
                                        },
                                        "×"
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Add new ID
            div { class: "flex gap-xs",
                input {
                    r#type: "text",
                    class: "input-inline text-mono",
                    style: "width: 150px;",
                    placeholder: placeholder,
                    value: "{new_input}",
                    oninput: move |e| new_input.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter && !new_input().trim().is_empty()
                            && let Ok(id) = new_input().trim().parse::<u64>() {
                                let mut new_ids = ids_for_keydown.clone();
                                if !new_ids.contains(&id) {
                                    new_ids.push(id);
                                    on_change.call(new_ids);
                                }
                                new_input.set(String::new());
                            }
                    }
                }
                button {
                    class: "btn btn-sm",
                    onclick: move |_| {
                        if let Ok(id) = new_input().trim().parse::<u64>() {
                            let mut new_ids = ids_for_click.clone();
                            if !new_ids.contains(&id) {
                                new_ids.push(id);
                                on_change.call(new_ids);
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
