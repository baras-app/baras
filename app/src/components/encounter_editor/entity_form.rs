//! Entity (NPC roster) edit form, shown in the editor drawer

use dioxus::prelude::*;

use crate::components::editor_common::{FormCard, ItemActions, Origin};
use crate::components::form_fields::{CheckField, Field, NumberField, SubSection, TextField, TogglePill};
use crate::types::{Condition, EntityDefinition, EntityFilter, HpMarker, ShieldDefinition, ShieldHpEntry, Trigger};

use super::conditions::ConditionsEditor;
use super::tabs::EncounterData;
use super::triggers::ComposableTriggerEditor;
use super::{DifficultiesEditor, NpcIdChipEditor};

const GROUP_SIZES: [(&str, Option<u8>); 4] = [("All", None), ("4", Some(4)), ("8", Some(8)), ("16", Some(16))];

#[component]
pub(super) fn EntityEditForm(
    entity: EntityDefinition,
    encounter_data: EncounterData,
    on_save: EventHandler<(EntityDefinition, String)>,
    on_delete: EventHandler<EntityDefinition>,
    #[props(default)] on_dirty: EventHandler<bool>,
) -> Element {
    let original_name = entity.name.clone();
    let entity_for_draft = entity.clone();
    let entity_for_delete = entity.clone();
    let original = entity.clone();
    let mut draft = use_signal(|| entity_for_draft);
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
        on_save.call((draft(), original_name.clone()));
    };

    let handle_delete = move |_: ()| {
        on_delete.call(entity_for_delete.clone());
    };

    let d = draft();

    rsx! {
        div { class: "entity-edit-form",
            FormCard { icon: "fa-tag", title: "Identity",
                TextField { label: "Name", value: d.name.clone(), on_input: move |v| draft.write().name = v }
                SubSection { title: "NPC IDs",
                    NpcIdChipEditor { ids: d.ids.clone(), on_change: move |ids| draft.write().ids = ids }
                }
            }

            FormCard { icon: "fa-flag", title: "Flags",
                CheckField { label: "Is boss", checked: d.is_boss, on_change: move |v| draft.write().is_boss = v }
                CheckField {
                    label: "Triggers encounter",
                    help: "Appearance of this target loads timers",
                    checked: d.triggers_encounter.unwrap_or(d.is_boss),
                    on_change: move |v| draft.write().triggers_encounter = Some(v),
                }
                CheckField {
                    label: "Is kill target",
                    help: "Death of all kill targets ends the encounter",
                    checked: d.is_kill_target,
                    on_change: move |v| draft.write().is_kill_target = v,
                }
                CheckField {
                    label: "Show on HP overlay",
                    help: "Display this entity on the Boss HP bar",
                    checked: d.show_on_hp_overlay.unwrap_or(d.is_boss),
                    on_change: move |v| draft.write().show_on_hp_overlay = Some(v),
                }
                CheckField {
                    label: "Pushes out of combat",
                    help: "Hide the HP bar when the boss is pushed out of combat at this HP",
                    checked: d.pushes_at.is_some(),
                    on_change: move |v: bool| draft.write().pushes_at = v.then_some(25.0),
                }
                if let Some(pct) = d.pushes_at {
                    NumberField {
                        label: "Pushes At",
                        value: pct.to_string(),
                        unit: "% HP",
                        min: "0",
                        max: "100",
                        step: "1",
                        on_input: move |v: String| if let Ok(v) = v.parse::<f32>() { draft.write().pushes_at = Some(v) },
                    }
                }
            }

            FormCard { icon: "fa-location-dot", title: "HP Markers",
                action: rsx! {
                    button {
                        class: "btn btn-xs",
                        onclick: move |_| draft.write().hp_markers.push(HpMarker {
                            hp_percent: 50.0,
                            label: String::new(),
                            difficulties: vec![],
                            group_size: None,
                            conditions: vec![],
                        }),
                        "+ Add"
                    }
                },
                div { class: "text-xs text-muted mb-xs", "Visual indicators on the HP bar at key thresholds" }
                for (i, marker) in d.hp_markers.iter().cloned().enumerate() {
                    HpMarkerRow { key: "{i}", draft, i, marker, encounter_data: encounter_data.clone() }
                }
            }

            FormCard { icon: "fa-shield-halved", title: "Shields",
                action: rsx! {
                    button {
                        class: "btn btn-xs",
                        onclick: move |_| draft.write().shields.push(ShieldDefinition {
                            label: String::new(),
                            start_trigger: Trigger::EffectApplied {
                                effects: vec![],
                                source: EntityFilter::Any,
                                target: EntityFilter::Any,
                                position: vec![],
                            },
                            end_trigger: Trigger::EffectRemoved {
                                effects: vec![],
                                source: EntityFilter::Any,
                                target: EntityFilter::Any,
                                position: vec![],
                            },
                            total: 0,
                            hp: vec![],
                            icon_ability_id: None,
                        }),
                        "+ Add"
                    }
                },
                div { class: "text-xs text-muted mb-xs", "Absorb shields shown on the HP bar overlay" }
                for (i, shield) in d.shields.iter().cloned().enumerate() {
                    ShieldEditor { key: "{i}", draft, i, shield, encounter_data: encounter_data.clone() }
                }
            }

            ItemActions {
                has_changes: has_changes(),
                origin: Origin::Custom,
                on_save: handle_save,
                on_delete: handle_delete,
            }
        }
    }
}

/// One HP marker: threshold + label, its difficulty/size filters, and
/// collapsible conditions
#[component]
fn HpMarkerRow(mut draft: Signal<EntityDefinition>, i: usize, marker: HpMarker, encounter_data: EncounterData) -> Element {
    let mut conditions_expanded = use_signal(|| false);
    let condition_count = marker.conditions.len();

    rsx! {
        div { class: "hp-marker",
            div { class: "hp-marker-row",
                span { class: "hp-marker-pct",
                    input {
                        class: "field-number",
                        r#type: "number",
                        step: "1",
                        min: "0",
                        max: "100",
                        title: "HP threshold",
                        value: "{marker.hp_percent}",
                        oninput: move |e| if let Ok(v) = e.value().parse::<f32>() { draft.write().hp_markers[i].hp_percent = v },
                    }
                    span { class: "field-unit", "%" }
                }
                input {
                    class: "hp-marker-label",
                    r#type: "text",
                    placeholder: "Label...",
                    value: "{marker.label}",
                    oninput: move |e| draft.write().hp_markers[i].label = e.value(),
                }
                span { class: "hp-marker-pills", title: "Difficulties (none picked = all)",
                    DifficultiesEditor {
                        difficulties: marker.difficulties.clone(),
                        on_change: move |updated: Vec<String>| draft.write().hp_markers[i].difficulties = updated,
                    }
                }
                span { class: "hp-marker-pills", title: "Group size",
                    i { class: "fa-solid fa-users hp-marker-pills-icon" }
                    for (label, size) in GROUP_SIZES {
                        TogglePill {
                            label,
                            active: marker.group_size == size,
                            on_click: move |_| draft.write().hp_markers[i].group_size = size,
                        }
                    }
                }
                span { class: "hp-marker-actions",
                    button {
                        class: if conditions_expanded() { "btn btn-xs btn-active" } else { "btn btn-xs" },
                        title: "Toggle conditions editor",
                        onclick: move |_| conditions_expanded.toggle(),
                        if condition_count > 0 && !conditions_expanded() { "{{ }} {condition_count}" } else { "{{ }}" }
                    }
                    button {
                        class: "btn btn-danger btn-xs",
                        title: "Remove marker",
                        onclick: move |_| { draft.write().hp_markers.remove(i); },
                        "×"
                    }
                }
            }
            if conditions_expanded() {
                SubSection { title: "Conditions",
                    ConditionsEditor {
                        conditions: marker.conditions.clone(),
                        encounter_data,
                        on_change: move |updated: Vec<Condition>| draft.write().hp_markers[i].conditions = updated,
                    }
                }
            }
        }
    }
}

/// One shield: name and icon, start/end triggers, and HP per difficulty/size
#[component]
fn ShieldEditor(mut draft: Signal<EntityDefinition>, i: usize, shield: ShieldDefinition, encounter_data: EncounterData) -> Element {
    rsx! {
        div { class: "editor-subcard",
            Field { label: "Shield",
                input {
                    placeholder: "Shield name...",
                    value: "{shield.label}",
                    oninput: move |e| draft.write().shields[i].label = e.value(),
                }
                button {
                    class: "btn btn-danger btn-xs",
                    title: "Remove shield",
                    onclick: move |_| { draft.write().shields.remove(i); },
                    "Remove"
                }
            }
            Field {
                label: "Icon ID",
                help: "Ability ID for the icon shown beside the shield amount. Leave blank to use the start trigger's first effect ID.",
                input {
                    placeholder: "(auto)",
                    value: "{shield.icon_ability_id.map(|id| id.to_string()).unwrap_or_default()}",
                    oninput: move |e| draft.write().shields[i].icon_ability_id = e.value().parse::<u64>().ok(),
                }
            }

            SubSection { title: "Start On",
                ComposableTriggerEditor {
                    trigger: shield.start_trigger.clone(),
                    encounter_data: encounter_data.clone(),
                    on_change: move |t| draft.write().shields[i].start_trigger = t,
                    hide_timer_only: true,
                }
            }
            SubSection { title: "End On",
                ComposableTriggerEditor {
                    trigger: shield.end_trigger.clone(),
                    encounter_data: encounter_data.clone(),
                    on_change: move |t| draft.write().shields[i].end_trigger = t,
                    hide_timer_only: true,
                }
            }

            SubSection { title: "HP Values",
                div { class: "flex-col gap-xs",
                    if shield.hp.is_empty() {
                        span { class: "conditions-editor-empty", "No HP values defined" }
                    }
                    for (j, entry) in shield.hp.iter().cloned().enumerate() {
                        div {
                            class: "editor-subcard-row",
                            div { class: "flex items-center gap-xs",
                                span { class: "field-unit", "Diff:" }
                                DifficultiesEditor {
                                    difficulties: entry.difficulties.clone(),
                                    on_change: move |v: Vec<String>| draft.write().shields[i].hp[j].difficulties = v,
                                }
                                if entry.difficulties.is_empty() {
                                    span { class: "field-unit", "(all)" }
                                }
                            }
                            div { class: "flex items-center gap-xs",
                                span { class: "field-unit", "Size:" }
                                for (label, size) in GROUP_SIZES {
                                    TogglePill {
                                        label,
                                        active: entry.group_size == size,
                                        on_click: move |_| draft.write().shields[i].hp[j].group_size = size,
                                    }
                                }
                            }
                            div { class: "flex items-center gap-xs",
                                span { class: "field-unit", "HP:" }
                                input {
                                    class: "field-number text-mono",
                                    r#type: "number",
                                    value: "{entry.total}",
                                    oninput: move |e| if let Ok(v) = e.value().parse::<i64>() { draft.write().shields[i].hp[j].total = v },
                                }
                            }
                            button {
                                class: "btn btn-danger btn-xs",
                                class: "ml-auto",
                                onclick: move |_| { draft.write().shields[i].hp.remove(j); },
                                "×"
                            }
                        }
                    }
                    button {
                        class: "btn-dashed text-sm",
                        onclick: move |_| draft.write().shields[i].hp.push(ShieldHpEntry {
                            difficulties: vec![],
                            group_size: None,
                            total: 0,
                        }),
                        "+ Add HP Value"
                    }
                }
            }
        }
    }
}
