//! Challenge editing tab
//!
//! CRUD for boss challenge definitions.
//! Uses ChallengeDefinition DSL type directly.

use crate::components::editor_sort::{ENCOUNTER_SORT_AZ, SortToggle, apply_rank, use_stable_rank};
use dioxus::prelude::*;

use crate::components::editor_drawer::{EditorDrawer, use_drawer_guard};
use crate::components::editor_common::{ENCOUNTER_ORIGIN, Origin, OriginBadge, OriginFilter, origin_counts};
use crate::api;
use crate::types::{
    BossWithPath, ChallengeColumns, ChallengeDefinition, ChallengeMetric, EncounterItem,
};

use super::challenge_form::ChallengeEditForm;
use super::tabs::EncounterData;
use super::InlineNameCreator;

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
    let guard = use_drawer_guard();
    // Extract challenges from BossWithPath
    let challenges = boss_with_path.boss.challenges.clone();
    // A–Z when toggled; renaming leaves the row in place
    let rank = use_stable_rank(challenges.iter().map(|x| (x.id.as_str(), x.name.as_str())), ENCOUNTER_SORT_AZ());
    let mut sorted_challenges = challenges.clone();
    apply_rank(&mut sorted_challenges, &rank.read(), |x| &x.id);
    let origin_of = |id: &str| {
        Origin::of(
            boss_with_path.builtin_challenge_ids.iter().any(|b| b == id),
            boss_with_path.modified_challenge_ids.iter().any(|m| m == id),
        )
    };
    let origin_counts = origin_counts(challenges.iter().map(|x| origin_of(&x.id)));
    let origin_filter = ENCOUNTER_ORIGIN();
    sorted_challenges.retain(|x| origin_filter.is_none_or(|o| origin_of(&x.id) == o));

    rsx! {
        div { class: "challenges-tab",
            // Header
            div { class: "flex items-center justify-between mb-sm editor-tab-header",
                div { class: "flex items-center gap-sm",
                    span { class: "text-sm text-secondary", "{challenges.len()} challenges" }
                    SortToggle { az: ENCOUNTER_SORT_AZ.signal() }
                    OriginFilter { selected: origin_filter, counts: origin_counts, kind: "challenges", on_pick: move |o| *ENCOUNTER_ORIGIN.write() = o }
                }
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
            } else if sorted_challenges.is_empty() {
                div { class: "empty-state text-sm", "No challenges match the filters" }
            } else {
                for challenge in sorted_challenges.clone() {
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
                                    guard.switch(expanded_challenge, if is_expanded { None } else { Some(challenge_key.clone()) });
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
        div { class: "list-item", class: if expanded { "expanded" }, class: if !challenge.enabled { "item-disabled" },
            // Header row
            div {
                class: "list-item-header",
                onclick: move |_| on_toggle.call(()),

                span { class: "font-medium", "{challenge.name}" }
                if expanded && is_dirty() {
                    span { class: "unsaved-indicator", title: "Unsaved changes" }
                }
                span { class: "text-xs text-mono text-muted", "{challenge.id}" }
                span { class: "tag", "{metric_label}" }
                if condition_count > 0 {
                    span { class: "tag tag-secondary", "{condition_count} conditions" }
                }

                // Right side - enable toggle
                div { class: "flex items-center gap-xs ml-auto shrink-0",
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
                    OriginBadge { origin: Origin::of(is_builtin, is_modified), kind: "challenge" }
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
                        EditorDrawer { title: challenge.name.clone(), dirty: is_dirty(), on_close: on_collapse,
                            ChallengeEditForm {
                                challenge: challenge.clone(),
                                encounter_data: encounter_data,
                                is_builtin: is_builtin,
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
                    class: "input-inline text-mono w-md",
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
