//! Timer editing tab
//!
//! Full CRUD for boss timers with all fields exposed.
//! Uses BossTimerDefinition DSL type directly.

use crate::components::editor_sort::{ENCOUNTER_SORT_AZ, SortToggle, apply_rank, use_stable_rank};
use dioxus::prelude::*;

use crate::components::editor_drawer::{EditorDrawer, use_drawer_guard};
use crate::components::editor_common::{ENCOUNTER_ORIGIN, Origin, OriginBadge, OriginFilter, origin_counts};
use crate::api;
use crate::components::ability_icon::ColorIcon;
use crate::types::{
    AlertTrigger, AudioConfig, BossTimerDefinition, BossWithPath, EncounterItem,
    TimerDisplayTarget, Trigger,
};

use super::InlineNameCreator;
use super::timer_form::TimerEditForm;
use super::tabs::EncounterData;


// ─────────────────────────────────────────────────────────────────────────────
// Timers Tab
// ─────────────────────────────────────────────────────────────────────────────

/// Create a default timer definition with sensible defaults
fn default_timer(name: String) -> BossTimerDefinition {
    BossTimerDefinition {
        id: String::new(), // Backend generates from name
        name,
        display_text: None,
        trigger: Trigger::CombatStart,
        duration_secs: 30.0,
        is_alert: false,
        alert_on: AlertTrigger::default(),
        alert_text: None,
        alert_countdown_secs: None,
        color: [255, 128, 0, 255], // Orange
        icon_ability_id: None,
        conditions: vec![],
        phases: vec![],
        counter_condition: None,
        difficulties: vec![
            "story".to_string(),
            "veteran".to_string(),
            "master".to_string(),
        ],
        group_size: None,
        enabled: true,
        can_be_refreshed: false,
        repeats: 0,
        chains_to: None,
        cancel_trigger: None,
        alert_at_secs: None,
        show_on_raid_frames: false,
        show_at_secs: 0.0,
        display_targets: vec![TimerDisplayTarget::TimersA],
        audio: AudioConfig::default(),
        roles: vec!["Tank".into(), "Healer".into(), "Dps".into()],
        gcd_secs: None,
        queue_on_expire: false,
        queue_priority: 0,
        queue_remove_trigger: None,
        queue_blocking_timers: Vec::new(),
        queue_blocking_condition: None,
        queue_next_audio: None,
        queue_countdown_bar: false,
        queue_hide_from_next: false,
    }
}

#[component]
pub fn TimersTab(
    boss_with_path: BossWithPath,
    encounter_data: EncounterData,
    expanded_timer: Signal<Option<String>>,
    hide_disabled_timers: Signal<bool>,
    on_change: EventHandler<Vec<BossTimerDefinition>>,
    on_refetch: EventHandler<()>,
    on_status: EventHandler<(String, bool)>,
) -> Element {
    let guard = use_drawer_guard();

    // Extract timers from BossWithPath
    let timers = boss_with_path.boss.timers.clone();
    // A–Z when toggled; renaming leaves the row in place
    let rank = use_stable_rank(timers.iter().map(|x| (x.id.as_str(), x.name.as_str())), ENCOUNTER_SORT_AZ());
    let builtin_timer_ids = boss_with_path.builtin_timer_ids.clone();
    let modified_timer_ids = boss_with_path.modified_timer_ids.clone();

    let disabled_count = timers.iter().filter(|t| !t.enabled).count();

    // Filter timers based on toggle
    let mut visible_timers: Vec<BossTimerDefinition> = if hide_disabled_timers() {
        timers.iter().filter(|t| t.enabled).cloned().collect()
    } else {
        timers.clone()
    };
    apply_rank(&mut visible_timers, &rank.read(), |x| &x.id);
    let origin_of = |id: &str| Origin::of(builtin_timer_ids.iter().any(|b| b == id), modified_timer_ids.iter().any(|m| m == id));
    let origin_counts = origin_counts(timers.iter().map(|x| origin_of(&x.id)));
    let origin_filter = ENCOUNTER_ORIGIN();
    visible_timers.retain(|x| origin_filter.is_none_or(|o| origin_of(&x.id) == o));

    rsx! {
        div { class: "timers-tab",
            // Header
            div { class: "flex items-center justify-between mb-sm editor-tab-header",
                div { class: "flex items-center gap-sm",
                    span { class: "text-sm text-secondary", "{timers.len()} timers" }
                    if disabled_count > 0 {
                        label { class: "flex items-center gap-xs text-xs text-muted cursor-pointer",
                            input {
                                r#type: "checkbox",
                                checked: hide_disabled_timers(),
                                onchange: move |e| hide_disabled_timers.set(e.checked()),
                            }
                            "Hide disabled ({disabled_count})"
                        }
                    }
                    SortToggle { az: ENCOUNTER_SORT_AZ.signal() }
                    OriginFilter { selected: origin_filter, counts: origin_counts, kind: "timers", on_pick: move |o| *ENCOUNTER_ORIGIN.write() = o }
                }
                div { class: "flex items-center gap-sm",
                    if !timers.is_empty() {
                        {
                            let all_hidden = timers.iter().all(|t| t.roles.is_empty());
                            let bwp_bulk = boss_with_path.clone();
                            let timers_for_bulk = timers.clone();
                            rsx! {
                                button {
                                    class: "btn btn-sm text-xs",
                                    title: if all_hidden { "Show all timers for all roles" } else { "Hide all timers for all roles" },
                                    onclick: move |_| {
                                        let new_roles: Vec<String> = if all_hidden {
                                            vec!["Tank".into(), "Healer".into(), "Dps".into()]
                                        } else {
                                            vec![]
                                        };
                                        let mut updated = timers_for_bulk.clone();
                                        for t in &mut updated {
                                            t.roles = new_roles.clone();
                                        }
                                        on_change.call(updated);
                                        let boss_id = bwp_bulk.boss.id.clone();
                                        let file_path = bwp_bulk.file_path.clone();
                                        let roles = new_roles.clone();
                                        spawn(async move {
                                            let _ = api::set_all_timer_roles(&boss_id, &file_path, &roles).await;
                                            on_refetch.call(());
                                        });
                                    },
                                    if all_hidden { "Show All" } else { "Hide All" }
                                }
                            }
                        }
                    }
                    {
                        let bwp = boss_with_path.clone();
                        let timers_for_create = timers.clone();
                        rsx! {
                            InlineNameCreator {
                                button_label: "+ New Timer",
                                placeholder: "Timer name...",
                                on_create: move |name: String| {
                                    let timers_clone = timers_for_create.clone();
                                    let boss_id = bwp.boss.id.clone();
                                    let file_path = bwp.file_path.clone();
                                    let timer = default_timer(name);
                                    let item = EncounterItem::Timer(timer);
                                    spawn(async move {
                                        match api::create_encounter_item(&boss_id, &file_path, &item).await {
                                            Ok(EncounterItem::Timer(created)) => {
                                                let created_id = created.id.clone();
                                                let mut current = timers_clone;
                                                current.push(created);
                                                on_change.call(current);
                                                expanded_timer.set(Some(created_id));
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
            }

            // Timer list
            if visible_timers.is_empty() {
                if timers.is_empty() {
                    div { class: "empty-state text-sm", "No timers defined" }
                } else {
                    div { class: "empty-state text-sm", "No timers match the filters" }
                }
            } else {
                for timer in visible_timers {
                    {
                        let timer_key = timer.id.clone();
                        let is_expanded = expanded_timer() == Some(timer_key.clone());
                        let timers_for_row = timers.clone();
                        let timer_is_builtin = builtin_timer_ids.contains(&timer.id);
                        let timer_is_modified = modified_timer_ids.contains(&timer.id);

                        rsx! {
                            TimerRow {
                                key: "{timer_key}",
                                timer: timer.clone(),
                                all_timers: timers_for_row,
                                is_builtin: timer_is_builtin,
                                is_modified: timer_is_modified,
                                boss_with_path: boss_with_path.clone(),
                                encounter_data: encounter_data.clone(),
                                expanded: is_expanded,
                                on_toggle: move |_| {
                                    guard.switch(expanded_timer, if is_expanded { None } else { Some(timer_key.clone()) });
                                },
                                on_change: on_change,
                                on_refetch: on_refetch,
                                on_status: on_status,
                                on_collapse: move |_| expanded_timer.set(None),
                            }
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Timer Row
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn TimerRow(
    timer: BossTimerDefinition,
    all_timers: Vec<BossTimerDefinition>,
    is_builtin: bool,
    is_modified: bool,
    boss_with_path: BossWithPath,
    encounter_data: EncounterData,
    expanded: bool,
    on_toggle: EventHandler<()>,
    on_change: EventHandler<Vec<BossTimerDefinition>>,
    on_refetch: EventHandler<()>,
    on_status: EventHandler<(String, bool)>,
    on_collapse: EventHandler<()>,
) -> Element {
    let mut is_dirty = use_signal(|| false);
    let color_hex = format!(
        "#{:02x}{:02x}{:02x}",
        timer.color[0], timer.color[1], timer.color[2]
    );
    let timer_for_visibility = timer.clone();
    let timer_for_audio = timer.clone();
    let timer_for_tank = timer.clone();
    let timer_for_healer = timer.clone();
    let timer_for_dps = timer.clone();
    let timers_for_visibility = all_timers.clone();
    let timers_for_audio = all_timers.clone();
    let timers_for_tank = all_timers.clone();
    let timers_for_healer = all_timers.clone();
    let timers_for_dps = all_timers.clone();
    let bwp_for_visibility = boss_with_path.clone();
    let bwp_for_audio = boss_with_path.clone();
    let bwp_for_tank = boss_with_path.clone();
    let bwp_for_healer = boss_with_path.clone();
    let bwp_for_dps = boss_with_path.clone();
    let has_tank = timer.roles.contains(&"Tank".to_string());
    let has_healer = timer.roles.contains(&"Healer".to_string());
    let has_dps = timer.roles.contains(&"Dps".to_string());
    let is_role_visible = !timer.roles.is_empty();

    rsx! {
        div { class: "list-item", class: if expanded { "expanded" }, class: if !timer.enabled { "item-disabled" },
            // Header row
            div {
                class: "list-item-header timer-row-header",
                onclick: move |_| on_toggle.call(()),

                ColorIcon { icon_id: timer.icon_ability_id, color: color_hex.clone() }

                // Name | ID grouped left-aligned
                div { class: "row-col-name-id",
                    span { class: "font-medium text-primary truncate", "{timer.name}" }
                    if expanded && is_dirty() {
                        span { class: "unsaved-indicator", title: "Unsaved changes" }
                    }
                    span { class: "text-xs text-mono text-muted", "  {timer.id}" }
                }

                // Overlay tag
                span { class: "row-col-trigger",
                    {
                        let targets_label = match timer.display_targets.len() {
                            0 => "None".to_string(),
                            1 => timer.display_targets[0].label().to_string(),
                            n => format!("{n} overlays"),
                        };
                        rsx! { span { class: "tag", "{targets_label}" } }
                    }
                }

                // Duration / Alert
                span { class: "row-col-duration",
                    if timer.is_alert {
                        span { class: "tag tag-alert", "Alert" }
                    } else {
                        span { class: "text-sm text-secondary", "{timer.duration_secs:.1}s" }
                    }
                }

                // Right side - fixed toggle buttons
                div { class: "flex items-center gap-xs shrink-0",
                    // Visibility toggle (clickable without expanding)
                    span {
                        class: "row-toggle",
                        title: if is_role_visible { "Hide for all roles" } else { "Show for all roles" },
                        onclick: move |e| {
                            e.stop_propagation();
                            let mut updated = timer_for_visibility.clone();
                            if !updated.roles.is_empty() {
                                updated.roles = vec![];
                            } else {
                                updated.roles = vec!["Tank".into(), "Healer".into(), "Dps".into()];
                            }
                            let mut current = timers_for_visibility.clone();
                            if let Some(idx) = current.iter().position(|t| t.id == updated.id) {
                                current[idx] = updated.clone();
                                on_change.call(current);
                            }
                            let boss_id = bwp_for_visibility.boss.id.clone();
                            let file_path = bwp_for_visibility.file_path.clone();
                            let item = EncounterItem::Timer(updated);
                            spawn(async move {
                                let _ = api::update_encounter_item(&boss_id, &file_path, &item, None).await;
                                on_refetch.call(());
                            });
                        },
                        i {
                            class: if is_role_visible { "fa-solid fa-eye text-success" } else { "fa-solid fa-eye-slash text-muted" },
                        }
                    }

                    // Audio toggle (clickable without expanding)
                    span {
                        class: "row-toggle",
                        title: if timer.audio.enabled { "Disable audio" } else { "Enable audio" },
                        onclick: move |e| {
                            e.stop_propagation();
                            let mut updated = timer_for_audio.clone();
                            updated.audio.enabled = !updated.audio.enabled;
                            let mut current = timers_for_audio.clone();
                            if let Some(idx) = current.iter().position(|t| t.id == updated.id) {
                                current[idx] = updated.clone();
                                on_change.call(current);
                            }
                            let boss_id = bwp_for_audio.boss.id.clone();
                            let file_path = bwp_for_audio.file_path.clone();
                            let item = EncounterItem::Timer(updated);
                            spawn(async move {
                                let _ = api::update_encounter_item(&boss_id, &file_path, &item, None).await;
                                on_refetch.call(());
                            });
                        },
                        span {
                            class: if timer.audio.enabled { "text-primary" } else { "text-muted" },
                            if timer.audio.enabled { "🔊" } else { "🔇" }
                        }
                    }

                    // Role toggles [T] [H] [D]
                    div { class: "flex items-center role-toggle-group",
                        {role_toggle("T", "Tank", has_tank, timer_for_tank, timers_for_tank, bwp_for_tank, on_change, on_refetch)}
                        {role_toggle("H", "Healer", has_healer, timer_for_healer, timers_for_healer, bwp_for_healer, on_change, on_refetch)}
                        {role_toggle("D", "Dps", has_dps, timer_for_dps, timers_for_dps, bwp_for_dps, on_change, on_refetch)}
                    }
                    OriginBadge { origin: Origin::of(is_builtin, is_modified), kind: "timer" }
                }
            }

            // Edit form
            if expanded {
                EditorDrawer { title: timer.name.clone(), dirty: is_dirty(), on_close: on_collapse,
                TimerEditForm {
                    timer: timer.clone(),
                    all_timers: all_timers,
                    is_builtin: is_builtin,
                    is_modified: is_modified,
                    boss_with_path: boss_with_path,
                    encounter_data: encounter_data,
                    on_change: on_change,
                    on_refetch: on_refetch,
                    on_status: on_status,
                    on_collapse: on_collapse,
                    on_dirty: move |dirty: bool| is_dirty.set(dirty),
                }
                }
            }
        }
    }
}

/// Render a single role toggle button [T], [H], or [D]
fn role_toggle(
    label: &str,
    role_name: &str,
    active: bool,
    timer: BossTimerDefinition,
    all_timers: Vec<BossTimerDefinition>,
    bwp: BossWithPath,
    on_change: EventHandler<Vec<BossTimerDefinition>>,
    on_refetch: EventHandler<()>,
) -> Element {
    let role_name = role_name.to_string();
    let css_class = match label {
        "T" => if active { "role-toggle role-toggle-tank active" } else { "role-toggle" },
        "H" => if active { "role-toggle role-toggle-healer active" } else { "role-toggle" },
        _ => if active { "role-toggle role-toggle-dps active" } else { "role-toggle" },
    };
    let title = if active {
        format!("Hide for {}", match label { "T" => "Tanks", "H" => "Healers", _ => "DPS" })
    } else {
        format!("Show for {}", match label { "T" => "Tanks", "H" => "Healers", _ => "DPS" })
    };
    rsx! {
        span {
            class: css_class,
            title: title,
            onclick: move |e| {
                e.stop_propagation();
                let mut updated = timer.clone();
                if active {
                    updated.roles.retain(|r| r != &role_name);
                } else {
                    updated.roles.push(role_name.clone());
                }
                let mut current = all_timers.clone();
                if let Some(idx) = current.iter().position(|t| t.id == updated.id) {
                    current[idx] = updated.clone();
                    on_change.call(current);
                }
                let boss_id = bwp.boss.id.clone();
                let file_path = bwp.file_path.clone();
                let item = EncounterItem::Timer(updated);
                spawn(async move {
                    let _ = api::update_encounter_item(&boss_id, &file_path, &item, None).await;
                    on_refetch.call(());
                });
            },
            span {
                class: if active { "" } else { "text-muted" },
                "{label}"
            }
        }
    }
}


// ─────────────────────────────────────────────────────────────────────────────
// Phase Selector (multi-select dropdown)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn PhaseSelector(
    selected: Vec<String>,
    available: Vec<String>,
    on_change: EventHandler<Vec<String>>,
) -> Element {
    let mut dropdown_open = use_signal(|| false);
    let mut dropdown_pos = use_signal(|| (0.0f64, 0.0f64));

    // Display text
    let display = if selected.is_empty() {
        "(all phases)".to_string()
    } else if selected.len() == 1 {
        selected[0].clone()
    } else {
        format!("{} phases", selected.len())
    };

    rsx! {
        div {
            class: "phase-selector",
            // Dropdown trigger
            button {
                class: "select-trigger min-w-md",
                onclick: move |e| {
                    if !dropdown_open() {
                        // Use element_coordinates to find offset within button,
                        // then subtract from client_coordinates to get button origin
                        let click = e.client_coordinates();
                        let offset = e.element_coordinates();
                        let btn_left = click.x - offset.x;
                        let btn_bottom = click.y - offset.y + 30.0;
                        dropdown_pos.set((btn_left, btn_bottom));
                    }
                    dropdown_open.set(!dropdown_open());
                },
                "{display}"
            }

            // Dropdown menu (fixed position to escape overflow clipping)
            if dropdown_open() {
                div {
                    class: "floating-menu",
                    style: "left: {dropdown_pos().0}px; top: {dropdown_pos().1}px;",

                    if available.is_empty() {
                        span { class: "text-muted text-sm", "No phases defined" }
                    } else {
                        // "All" option (clears selection)
                        label { class: "flex items-center gap-xs text-sm p-xs cursor-pointer",
                            input {
                                r#type: "checkbox",
                                checked: selected.is_empty(),
                                onchange: move |_| {
                                    on_change.call(vec![]);
                                    dropdown_open.set(false);
                                }
                            }
                            "(all phases)"
                        }

                        // Individual phases
                        for phase in available.iter() {
                            {
                                let phase_id = phase.clone();
                                let is_selected = selected.contains(&phase_id);
                                let selected_clone = selected.clone();

                                rsx! {
                                    label { class: "flex items-center gap-xs text-sm p-xs cursor-pointer",
                                        input {
                                            r#type: "checkbox",
                                            checked: is_selected,
                                            onchange: move |_| {
                                                let mut new_selected = selected_clone.clone();
                                                if is_selected {
                                                    new_selected.retain(|p| p != &phase_id);
                                                } else {
                                                    new_selected.push(phase_id.clone());
                                                }
                                                on_change.call(new_selected);
                                            }
                                        }
                                        "{phase}"
                                    }
                                }
                            }
                        }
                    }

                    // Close button
                    button {
                        class: "btn btn-sm w-full mt-xs",
                        onclick: move |_| dropdown_open.set(false),
                        "Done"
                    }
                }
            }
        }
    }
}
