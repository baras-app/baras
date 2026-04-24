//! Timer editing tab
//!
//! Full CRUD for boss timers with all fields exposed.
//! Uses BossTimerDefinition DSL type directly.

use dioxus::prelude::*;

use crate::api;
use crate::types::{
    AlertTrigger, AudioConfig, BossTimerDefinition, BossWithPath, Condition, EncounterItem,
    TimerDisplayTarget, Trigger, timer_alert_label,
};
use crate::utils::parse_hex_color;

use super::InlineNameCreator;
use super::conditions::{ConditionsEditor, CounterConditionEditor};
use super::tabs::EncounterData;
use super::triggers::ComposableTriggerEditor;


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
        display_target: TimerDisplayTarget::TimersA,
        audio: AudioConfig::default(),
        roles: vec!["Tank".into(), "Healer".into(), "Dps".into()],
        gcd_secs: None,
        queue_on_expire: false,
        queue_priority: 0,
        queue_remove_trigger: None,
        queue_blocking_timers: Vec::new(),
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

    // Extract timers from BossWithPath
    let timers = boss_with_path.boss.timers.clone();
    let builtin_timer_ids = boss_with_path.builtin_timer_ids.clone();
    let modified_timer_ids = boss_with_path.modified_timer_ids.clone();

    let disabled_count = timers.iter().filter(|t| !t.enabled).count();

    // Filter timers based on toggle
    let visible_timers: Vec<BossTimerDefinition> = if hide_disabled_timers() {
        timers.iter().filter(|t| t.enabled).cloned().collect()
    } else {
        timers.clone()
    };

    rsx! {
        div { class: "timers-tab",
            // Header
            div { class: "flex items-center justify-between mb-sm",
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

            // Timer list
            if visible_timers.is_empty() {
                if timers.is_empty() {
                    div { class: "empty-state text-sm", "No timers defined" }
                } else {
                    div { class: "empty-state text-sm", "All timers are disabled (toggle above to show)" }
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
                                    expanded_timer.set(if is_expanded { None } else { Some(timer_key.clone()) });
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
        div { class: if timer.enabled { "list-item" } else { "list-item item-disabled" },
            // Header row
            div {
                class: "list-item-header timer-row-header",
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
                        title: "Modified: built-in timer you have edited",
                        "M"
                    }
                } else {
                    span {
                        class: "timer-origin timer-origin-custom",
                        title: "Custom: created by you",
                        "C"
                    }
                }

                // Color swatch
                span {
                    class: "color-swatch",
                    style: "background: {color_hex};"
                }

                // Name | ID grouped left-aligned
                div { class: "timer-col-name-id",
                    span { class: "font-medium text-primary truncate", "{timer.name}" }
                    if expanded && is_dirty() {
                        span { class: "unsaved-indicator", title: "Unsaved changes" }
                    }
                    span { class: "text-xs text-mono text-muted", "  {timer.id}" }
                }

                // Overlay tag
                span { class: "timer-col-trigger",
                    span { class: "tag", "{timer.display_target.label()}" }
                }

                // Duration / Alert
                span { class: "timer-col-duration",
                    if timer.is_alert {
                        span { class: "tag tag-alert", "Alert" }
                    } else {
                        span { class: "text-sm text-secondary", "{timer.duration_secs:.1}s" }
                    }
                }

                // Right side - fixed toggle buttons
                div { class: "flex items-center gap-xs", style: "flex-shrink: 0;",
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
                }
            }

            // Edit form
            if expanded {
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
// Timer Edit Form (Full)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn TimerEditForm(
    timer: BossTimerDefinition,
    all_timers: Vec<BossTimerDefinition>,
    #[props(default)] is_builtin: bool,
    #[props(default)] is_modified: bool,
    boss_with_path: BossWithPath,
    encounter_data: EncounterData,
    on_change: EventHandler<Vec<BossTimerDefinition>>,
    on_refetch: EventHandler<()>,
    on_status: EventHandler<(String, bool)>,
    on_collapse: EventHandler<()>,
    #[props(default)] on_dirty: EventHandler<bool>,
) -> Element {
    let timer_display = timer.clone();
    let timer_for_draft = timer.clone();
    let timer_for_delete = timer.clone();
    let timer_id = timer.id.clone();
    let timer_original = timer.clone();
    let mut draft = use_signal(|| timer_for_draft);
    let mut confirm_delete = use_signal(|| false);
    let mut just_saved = use_signal(|| false);

    // Load available sound files once
    let mut sound_files = use_signal(Vec::<String>::new);
    use_future(move || async move {
        sound_files.set(api::list_sound_files().await);
    });

    // Reset just_saved when user makes new changes after saving
    let timer_original_for_effect = timer_original.clone();
    use_effect(move || {
        if draft() != timer_original_for_effect && just_saved() {
            just_saved.set(false);
        }
    });

    let has_changes = use_memo(move || !just_saved() && draft() != timer_original);

    // Notify parent when dirty state changes
    use_effect(move || {
        on_dirty.call(has_changes());
    });
    let color_hex = format!(
        "#{:02x}{:02x}{:02x}",
        draft().color[0],
        draft().color[1],
        draft().color[2]
    );

    // Icon preview for timer bar icon
    let mut icon_preview_url = use_signal(|| None::<String>);
    use_effect(move || {
        let current_draft = draft();
        if let Some(ability_id) = current_draft.icon_ability_id {
            spawn(async move {
                if let Some(url) = api::get_icon_preview(ability_id).await {
                    icon_preview_url.set(Some(url));
                } else {
                    icon_preview_url.set(None);
                }
            });
        } else {
            icon_preview_url.set(None);
        }
    });

    // Save handler
    let handle_save = {
        let timers = all_timers.clone();
        let bwp = boss_with_path.clone();
        move |_| {
            just_saved.set(true);
            let updated = draft();
            let mut current = timers.clone();
            if let Some(idx) = current.iter().position(|t| t.id == updated.id) {
                current[idx] = updated.clone();
                on_change.call(current);
            }
            let boss_id = bwp.boss.id.clone();
            let file_path = bwp.file_path.clone();
            let item = EncounterItem::Timer(updated);
            spawn(async move {
                match api::update_encounter_item(&boss_id, &file_path, &item, None).await {
                    Ok(_) => {
                        on_status.call(("Saved".to_string(), false));
                        on_refetch.call(());
                    }
                    Err(_) => on_status.call(("Failed to save".to_string(), true)),
                }
            });
        }
    };

    // Delete/Reset handler
    let handle_delete = {
        let timer_del = timer_for_delete.clone();
        let timers = all_timers.clone();
        let bwp = boss_with_path.clone();
        let is_reset = is_builtin || is_modified; // Built-in items get "reset", not "delete"
        move |_| {
            let t = timer_del.clone();
            let timers_clone = timers.clone();
            let boss_id = bwp.boss.id.clone();
            let file_path = bwp.file_path.clone();
            spawn(async move {
                match api::delete_encounter_item("timer", &t.id, &boss_id, &file_path).await {
                    Ok(_) => {
                        if is_reset {
                            // For built-in items, refetch to get the original definition back
                            on_status.call(("Reset to built-in".to_string(), false));
                            on_collapse.call(());
                            on_refetch.call(());
                        } else {
                            let filtered: Vec<_> = timers_clone
                                .into_iter()
                                .filter(|timer| timer.id != t.id)
                                .collect();
                            on_change.call(filtered);
                            on_collapse.call(());
                            on_status.call(("Deleted".to_string(), false));
                        }
                    }
                    Err(err) => {
                        on_status.call((err, true));
                    }
                }
            });
        }
    };

    // Duplicate handler
    let handle_duplicate = {
        let timer_dup = timer_display.clone();
        let timers = all_timers.clone();
        let bwp = boss_with_path.clone();
        move |_| {
            let t = timer_dup.clone();
            let ts = timers.clone();
            let boss_id = bwp.boss.id.clone();
            let file_path = bwp.file_path.clone();
            spawn(async move {
                match api::duplicate_encounter_timer(&t.id, &boss_id, &file_path).await {
                    Ok(new_timer) => {
                        let mut current = ts;
                        current.push(new_timer);
                        on_change.call(current);
                        on_status.call(("Duplicated".to_string(), false));
                    }
                    Err(e) => {
                        on_status.call((e, true));
                    }
                }
            });
        }
    };

    // Get other timer IDs for chains_to dropdown
    let other_timer_ids: Vec<String> = all_timers
        .iter()
        .filter(|t| t.id != timer_id)
        .map(|t| t.id.clone())
        .collect();

    rsx! {
        div { class: "list-item-body",
            // ─── Two Column Layout ─────────────────────────────────────────────
            div { class: "timer-edit-grid",
                // ═══ LEFT COLUMN: Identity, Trigger ════════════════════════════
                div { class: "timer-edit-left",

                    // ─── Identity Card ─────────────────────────────────────────
                    div { class: "form-card",
                        div { class: "form-card-header",
                            i { class: "fa-solid fa-tag" }
                            span { "Identity" }
                        }
                        div { class: "form-card-content",
                            div { class: "form-row-hz",
                                label { "Timer ID" }
                                code { class: "tag-muted text-mono text-xs", "{timer_display.id}" }
                            }

                            div { class: "form-row-hz",
                                label { "Name" }
                                input {
                                    class: "input-inline",
                                    r#type: "text",
                                    style: "flex: 1; min-width: 0;",
                                    value: "{draft().name}",
                                    oninput: move |e| {
                                        let mut d = draft();
                                        d.name = e.value();
                                        draft.set(d);
                                    }
                                }
                            }

                            if !draft().is_alert {
                                div { class: "form-row-hz",
                                    label { class: "flex items-center",
                                        "Display Text"
                                        span {
                                            class: "help-icon",
                                            title: "Text shown on the overlay timer bar. Defaults to timer name.",
                                            "?"
                                        }
                                    }
                                    input {
                                        class: "input-inline",
                                        r#type: "text",
                                        style: "flex: 1; min-width: 0;",
                                        placeholder: "(defaults to name)",
                                        value: "{draft().display_text.clone().unwrap_or_default()}",
                                        oninput: move |e| {
                                            let mut d = draft();
                                            d.display_text = if e.value().is_empty() { None } else { Some(e.value()) };
                                            draft.set(d);
                                        }
                                    }
                                }
                            }

                            div { class: "form-row-hz",
                                label { "Difficulties" }
                                div { class: "flex gap-xs",
                                    for diff in ["story", "veteran", "master"] {
                                        {
                                            let diff_str = diff.to_string();
                                            let is_active = draft().difficulties.iter().any(|d| d == diff);

                                            rsx! {
                                                button {
                                                    class: if is_active { "toggle-btn active" } else { "toggle-btn" },
                                                    onclick: {
                                                        let diff_str = diff_str.clone();
                                                        move |_| {
                                                            let mut d = draft();
                                                            if is_active {
                                                                d.difficulties.retain(|x| x != &diff_str);
                                                            } else {
                                                                d.difficulties.push(diff_str.clone());
                                                            }
                                                            draft.set(d);
                                                        }
                                                    },
                                                    "{diff}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            div { class: "form-row-hz",
                                label { "Group Size" }
                                div { class: "flex gap-xs",
                                    for (label, size) in [("All", None), ("8-man", Some(8u8)), ("16-man", Some(16u8))] {
                                        {
                                            let is_active = draft().group_size == size;
                                            rsx! {
                                                button {
                                                    class: if is_active { "toggle-btn active" } else { "toggle-btn" },
                                                    onclick: move |_| {
                                                        let mut d = draft();
                                                        d.group_size = size;
                                                        draft.set(d);
                                                    },
                                                    "{label}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // ─── Display fields ────────────────────────────────
                            div { class: "form-row-hz mt-sm",
                                label { "Color" }
                                input {
                                    class: "color-picker",
                                    r#type: "color",
                                    value: "{color_hex}",
                                    oninput: move |e| {
                                        if let Some(color) = parse_hex_color(&e.value()) {
                                            let mut d = draft();
                                            d.color = color;
                                            draft.set(d);
                                        }
                                    }
                                }
                            }

                            // Icon ID (optional, for display on timer bar)
                            div { class: "form-row-hz",
                                label { class: "flex items-center",
                                    "Icon ID"
                                    span {
                                        class: "help-icon",
                                        title: "Ability ID to use for the icon on the timer bar. Leave blank for no icon.",
                                        "?"
                                    }
                                }
                                input {
                                    r#type: "text",
                                    class: "input-inline",
                                    style: "flex: 1; min-width: 0;",
                                    placeholder: "(none)",
                                    value: "{draft().icon_ability_id.map(|id| id.to_string()).unwrap_or_default()}",
                                    oninput: move |e| {
                                        let mut d = draft();
                                        d.icon_ability_id = if e.value().is_empty() {
                                            None
                                        } else {
                                            e.value().parse::<u64>().ok()
                                        };
                                        draft.set(d);
                                    }
                                }
                                // Icon preview
                                if let Some(ref url) = icon_preview_url() {
                                    img {
                                        src: "{url}",
                                        class: "icon-preview",
                                        width: "24",
                                        height: "24",
                                        alt: "Icon preview"
                                    }
                                } else if draft().icon_ability_id.is_some() {
                                    span { class: "text-muted text-xs", "(not found)" }
                                }
                            }

                            if !draft().is_alert {
                                div { class: "form-row-hz",
                                    label { class: "flex items-center",
                                        "Display Overlay"
                                        span {
                                            class: "help-icon",
                                            title: "Sets which overlay displays this timer when triggered",
                                            "?"
                                        }
                                    }
                                    select {
                                        class: "select",
                                        style: "flex: 1; min-width: 0;",
                                        onchange: move |e| {
                                            let mut d = draft();
                                            d.display_target = match e.value().as_str() {
                                                "timers_b" => TimerDisplayTarget::TimersB,
                                                "ability_queue" => TimerDisplayTarget::AbilityQueue,
                                                "none" => TimerDisplayTarget::None,
                                                _ => TimerDisplayTarget::TimersA,
                                            };
                                            // Clear queue fields when switching away from AbilityQueue
                                            if d.display_target != TimerDisplayTarget::AbilityQueue {
                                                d.queue_on_expire = false;
                                                d.gcd_secs = None;
                                                d.queue_priority = 0;
                                                d.queue_remove_trigger = None;
                                                d.queue_countdown_bar = false;
                                                d.queue_hide_from_next = false;
                                            }
                                            draft.set(d);
                                        },
                                        for target in TimerDisplayTarget::all() {
                                            {
                                                let value = match target {
                                                    TimerDisplayTarget::TimersA => "timers_a",
                                                    TimerDisplayTarget::TimersB => "timers_b",
                                                    TimerDisplayTarget::AbilityQueue => "ability_queue",
                                                    TimerDisplayTarget::None => "none",
                                                };
                                                let is_selected = draft().display_target == *target;
                                                rsx! {
                                                    option {
                                                        value: "{value}",
                                                        selected: is_selected,
                                                        "{target.label()}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                // ── Ability Queue fields (visible only when AbilityQueue is selected)
                                if draft().display_target == TimerDisplayTarget::AbilityQueue {
                                    div { class: "form-row-hz",
                                        label { class: "flex items-center",
                                            "GCD Secs"
                                            span { class: "help-icon", title: "Creates a GCD countdown bar when this timer fires. Leave empty for no GCD bar.", "?" }
                                        }
                                        input {
                                            r#type: "number",
                                            class: "input",
                                            style: "width: 80px;",
                                            min: "0",
                                            max: "10",
                                            step: "0.1",
                                            placeholder: "none",
                                            value: draft().gcd_secs.map(|v| v.to_string()).unwrap_or_default(),
                                            onchange: move |e| {
                                                let mut d = draft();
                                                d.gcd_secs = e.value().parse::<f32>().ok().filter(|&v| v > 0.0);
                                                draft.set(d);
                                            }
                                        }
                                    }
                                    div { class: "form-row-hz",
                                        label { class: "flex items-center",
                                            "Hold as Ready"
                                            span { class: "help-icon", title: "When enabled, the timer stays visible as 'READY' after expiring instead of being removed.", "?" }
                                        }
                                        input {
                                            r#type: "checkbox",
                                            class: "checkbox",
                                            checked: draft().queue_on_expire,
                                            onchange: move |e| {
                                                let mut d = draft();
                                                d.queue_on_expire = e.checked();
                                                draft.set(d);
                                            }
                                        }
                                    }
                                    div { class: "form-row-hz",
                                        label { class: "flex items-center",
                                            "Countdown Mode"
                                            span { class: "help-icon", title: "Render this timer's bar as a trickling-down bar (full → empty) instead of the default filling-up progress bar used by other entries.", "?" }
                                        }
                                        input {
                                            r#type: "checkbox",
                                            class: "checkbox",
                                            checked: draft().queue_countdown_bar,
                                            onchange: move |e| {
                                                let mut d = draft();
                                                d.queue_countdown_bar = e.checked();
                                                draft.set(d);
                                            }
                                        }
                                    }
                                    div { class: "form-row-hz",
                                        label { class: "flex items-center",
                                            "Don't Show as Next"
                                            span { class: "help-icon", title: "When enabled, this timer is never highlighted as the 'next cast' — use for display-only entries (incoming debuffs, lockout/mechanic windows) that aren't castable abilities.", "?" }
                                        }
                                        input {
                                            r#type: "checkbox",
                                            class: "checkbox",
                                            checked: draft().queue_hide_from_next,
                                            onchange: move |e| {
                                                let mut d = draft();
                                                d.queue_hide_from_next = e.checked();
                                                draft.set(d);
                                            }
                                        }
                                    }
                                    div { class: "form-row-hz",
                                        label { class: "flex items-center",
                                            "Priority"
                                            span { class: "help-icon", title: "Sort order for ability queue entries (0–255, higher = shown first). Applies whether the timer holds as ready or expires normally.", "?" }
                                        }
                                        input {
                                            r#type: "number",
                                            class: "input",
                                            style: "width: 80px;",
                                            min: "0",
                                            max: "255",
                                            value: draft().queue_priority.to_string(),
                                            onchange: move |e| {
                                                let mut d = draft();
                                                d.queue_priority = e.value().parse::<u8>().unwrap_or(0);
                                                draft.set(d);
                                            }
                                        }
                                    }
                                    div { class: "form-row-hz", style: "align-items: flex-start;",
                                        label { class: "flex items-center", style: "padding-top: 6px;",
                                            "Blocked By"
                                            span { class: "help-icon", title: "Timers from this encounter that prevent this entry from appearing as 'next cast' while they're active. OR semantics — any one active blocker blocks the entry.", "?" }
                                        }
                                        div { class: "flex-col gap-xs",
                                            // Current blockers as removable chips
                                            if !draft().queue_blocking_timers.is_empty() {
                                                div { class: "flex flex-wrap gap-xs",
                                                    for (idx, blocker) in draft().queue_blocking_timers.iter().cloned().enumerate() {
                                                        {
                                                            rsx! {
                                                                span { class: "chip",
                                                                    "{blocker}"
                                                                    button {
                                                                        class: "chip-remove",
                                                                        onclick: move |_| {
                                                                            let mut d = draft();
                                                                            d.queue_blocking_timers.remove(idx);
                                                                            draft.set(d);
                                                                        },
                                                                        "×"
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            // Dropdown to add a blocker — lists same-encounter timers
                                            // excluding self and already-selected blockers.
                                            select {
                                                class: "select",
                                                style: "width: 220px;",
                                                onchange: move |e| {
                                                    let val = e.value();
                                                    if !val.is_empty() {
                                                        let mut d = draft();
                                                        if !d.queue_blocking_timers.contains(&val) {
                                                            d.queue_blocking_timers.push(val);
                                                            draft.set(d);
                                                        }
                                                    }
                                                },
                                                option { value: "", "(add blocker...)" }
                                                for t in all_timers.iter() {
                                                    {
                                                        let self_name = draft().name.clone();
                                                        let already = draft().queue_blocking_timers.contains(&t.name);
                                                        let is_self = t.name == self_name;
                                                        rsx! {
                                                            if !is_self {
                                                                option {
                                                                    value: "{t.name}",
                                                                    disabled: already,
                                                                    "{t.name}"
                                                                    if already { " ✓" }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    if draft().queue_on_expire {
                                        div { class: "form-row-hz", style: "align-items: flex-start;",
                                            label { class: "flex items-center", style: "padding-top: 6px;",
                                                "Clear On"
                                                span { class: "help-icon", title: "Removes the ready entry when this trigger fires (e.g. ability cast).", "?" }
                                            }
                                            if let Some(remove_trigger) = draft().queue_remove_trigger.clone() {
                                                div { class: "flex-col gap-xs",
                                                    ComposableTriggerEditor {
                                                        trigger: remove_trigger.clone(),
                                                        encounter_data: encounter_data.clone(),
                                                        on_change: move |t| {
                                                            let mut d = draft();
                                                            d.queue_remove_trigger = Some(t);
                                                            draft.set(d);
                                                        }
                                                    }
                                                    button {
                                                        class: "btn btn-sm",
                                                        style: "width: fit-content;",
                                                        onclick: move |_| {
                                                            let mut d = draft();
                                                            d.queue_remove_trigger = None;
                                                            draft.set(d);
                                                        },
                                                        "Remove Clear Trigger"
                                                    }
                                                }
                                            } else {
                                                div { class: "flex-col gap-xs",
                                                    span { class: "text-muted text-sm", "(default: combat end)" }
                                                    button {
                                                        class: "btn btn-sm",
                                                        onclick: move |_| {
                                                            let mut d = draft();
                                                            d.queue_remove_trigger = Some(Trigger::CombatStart);
                                                            draft.set(d);
                                                        },
                                                        "+ Add Clear Trigger"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ─── Trigger Card ──────────────────────────────────────────
                    div { class: "form-card",
                        div { class: "form-card-header",
                            i { class: "fa-solid fa-bolt" }
                            span { "Trigger" }
                        }
                        div { class: "form-card-content",
                            div { class: "form-row-hz", style: "align-items: flex-start;",
                                label { class: "flex items-center", style: "padding-top: 6px;",
                                    "Trigger"
                                    span {
                                        class: "help-icon",
                                        title: "The game event that starts this timer",
                                        "?"
                                    }
                                }
                                ComposableTriggerEditor {
                                    trigger: draft().trigger.clone(),
                                    encounter_data: encounter_data.clone(),
                                    on_change: move |t| {
                                        let mut d = draft();
                                        d.trigger = t;
                                        draft.set(d);
                                    }
                                }
                            }

                            if !draft().is_alert {
                                div { class: "form-row-hz",
                                    label { class: "flex items-center",
                                        "Chains To"
                                        span {
                                            class: "help-icon",
                                            title: "Starts another timer when this one expires",
                                            "?"
                                        }
                                    }
                                    {
                                        let selected_timer = draft().chains_to.clone().unwrap_or_default();
                                        rsx! {
                                            select {
                                                class: "select",
                                                style: "flex: 1; min-width: 0;",
                                                onchange: move |e| {
                                                    let mut d = draft();
                                                    d.chains_to = if e.value().is_empty() { None } else { Some(e.value()) };
                                                    draft.set(d);
                                                },
                                                option { value: "", selected: selected_timer.is_empty(), "(none)" }
                                                for tid in &other_timer_ids {
                                                    option {
                                                        value: "{tid}",
                                                        selected: tid == &selected_timer,
                                                        "{tid}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                div { class: "form-row-hz", style: "align-items: flex-start;",
                                    label { class: "flex items-center", style: "padding-top: 6px;",
                                        "Cancel On"
                                        span {
                                            class: "help-icon",
                                            title: "Cancels this timer when the trigger fires. Default: combat end",
                                            "?"
                                        }
                                    }
                                    if let Some(cancel) = draft().cancel_trigger.clone() {
                                        div { class: "flex-col gap-xs",
                                            ComposableTriggerEditor {
                                                trigger: cancel.clone(),
                                                encounter_data: encounter_data.clone(),
                                                on_change: move |t| {
                                                    let mut d = draft();
                                                    d.cancel_trigger = Some(t);
                                                    draft.set(d);
                                                }
                                            }
                                            button {
                                                class: "btn btn-sm",
                                                style: "width: fit-content;",
                                                onclick: move |_| {
                                                    let mut d = draft();
                                                    d.cancel_trigger = None;
                                                    draft.set(d);
                                                },
                                                "Remove Cancel Trigger"
                                            }
                                        }
                                    } else {
                                        div { class: "flex-col gap-xs",
                                            span { class: "text-muted text-sm", "(default: combat end)" }
                                            button {
                                                class: "btn btn-sm",
                                                onclick: move |_| {
                                                    let mut d = draft();
                                                    d.cancel_trigger = Some(Trigger::CombatStart);
                                                    draft.set(d);
                                                },
                                                "+ Add Cancel Trigger"
                                            }
                                        }
                                    }
                                }
                            }

                            // ─── Conditions subsection ─────────────────────────
                            span { class: "text-sm font-bold text-secondary mt-sm",
                                "Conditions"
                                span {
                                    class: "help-icon",
                                    title: "State conditions that must ALL be true for this timer to be active. Use for phase, counter, HP, or entity state guards.",
                                    "?"
                                }
                            }

                            ConditionsEditor {
                                conditions: draft().conditions.clone(),
                                encounter_data: encounter_data.clone(),
                                on_change: move |c| {
                                    let mut d = draft();
                                    d.conditions = c;
                                    draft.set(d);
                                }
                            }

                            // Legacy fields: show migration banner if old fields have values
                            if !draft().phases.is_empty() || draft().counter_condition.is_some() {
                                div { class: "legacy-conditions-banner",
                                    i { class: "fa-solid fa-circle-info" }
                                    span { "Legacy phase/counter fields detected." }
                                    button {
                                        class: "btn btn-sm",
                                        title: "Move legacy phase and counter conditions into the new Conditions system",
                                        onclick: move |_| {
                                            let mut d = draft();
                                            // Migrate phases -> PhaseActive condition
                                            if !d.phases.is_empty() {
                                                d.conditions.push(Condition::PhaseActive {
                                                    phase_ids: d.phases.clone(),
                                                });
                                                d.phases.clear();
                                            }
                                            // Migrate counter_condition -> CounterCompare condition
                                            if let Some(cc) = d.counter_condition.take() {
                                                d.conditions.push(Condition::CounterCompare {
                                                    counter_id: cc.counter_id,
                                                    operator: cc.operator,
                                                    value: cc.value,
                                                });
                                            }
                                            draft.set(d);
                                        },
                                        "Migrate"
                                    }
                                }

                                div { class: "form-row-hz mt-xs",
                                    label { class: "flex items-center text-tertiary",
                                        "Phases (legacy)"
                                    }
                                    PhaseSelector {
                                        selected: draft().phases.clone(),
                                        available: encounter_data.phase_ids(),
                                        on_change: move |p| {
                                            let mut d = draft();
                                            d.phases = p;
                                            draft.set(d);
                                        }
                                    }
                                }

                                div { class: "form-row-hz",
                                    label { class: "flex items-center text-tertiary",
                                        "Counter (legacy)"
                                    }
                                    CounterConditionEditor {
                                        condition: draft().counter_condition.clone(),
                                        counters: encounter_data.counter_ids(),
                                        on_change: move |c| {
                                            let mut d = draft();
                                            d.counter_condition = c;
                                            draft.set(d);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // ═══ RIGHT COLUMN: Timing, Alerts, Audio ═══════════════════════
                div { class: "timer-edit-right",

                    // ─── Timing Card ───────────────────────────────────────────
                    div { class: "form-card",
                        div { class: "form-card-header",
                            i { class: "fa-solid fa-clock" }
                            span { "Timing" }
                        }
                        div { class: "form-card-content",
                            div { class: "form-row-hz",
                                label { class: "flex items-center",
                                    "Instant Alert Only"
                                    span {
                                        class: "help-icon",
                                        title: "Shows a brief alert notification instead of a countdown timer bar",
                                        "?"
                                    }
                                }
                                input {
                                    r#type: "checkbox",
                                    checked: draft().is_alert,
                                    onchange: move |e| {
                                        let mut d = draft();
                                        d.is_alert = e.checked();
                                        draft.set(d);
                                    }
                                }
                            }

                            if !draft().is_alert {
                                div { class: "form-row-hz",
                                    label { class: "flex items-center",
                                        "Duration"
                                        span {
                                            class: "help-icon",
                                            title: "How long the countdown timer runs in seconds",
                                            "?"
                                        }
                                    }
                                    input {
                                        class: "input-inline",
                                        r#type: "number",
                                        step: "any",
                                        min: "0",
                                        style: "width: 70px;",
                                        value: "{draft().duration_secs}",
                                        oninput: move |e| {
                                            if let Ok(val) = e.value().parse::<f32>() {
                                                let mut d = draft();
                                                d.duration_secs = val;
                                                draft.set(d);
                                            }
                                        }
                                    }
                                    span { class: "text-muted", "sec" }
                                }

                                div { class: "form-row-hz",
                                    label { class: "flex items-center",
                                        "Show at"
                                        span {
                                            class: "help-icon",
                                            title: "Only show the timer when this many seconds remain. 0 = always visible",
                                            "?"
                                        }
                                    }
                                    input {
                                        r#type: "number",
                                        class: "input-inline",
                                        style: "width: 60px;",
                                        step: "any",
                                        min: "0",
                                        max: "{draft().duration_secs}",
                                        value: "{draft().show_at_secs}",
                                        oninput: move |e| {
                                            if let Ok(val) = e.value().parse::<f32>() {
                                                let mut d = draft();
                                                d.show_at_secs = val.min(d.duration_secs).max(0.0);
                                                draft.set(d);
                                            }
                                        }
                                    }
                                    span { class: "text-sm text-secondary", "sec remaining" }
                                }

                                div { class: "form-row-hz",
                                    label { "Options" }
                                    div { class: "flex gap-md flex-wrap",
                                        label { class: "flex items-center gap-xs text-sm",
                                            input {
                                                r#type: "checkbox",
                                                checked: draft().can_be_refreshed,
                                                onchange: move |e| {
                                                    let mut d = draft();
                                                    d.can_be_refreshed = e.checked();
                                                    draft.set(d);
                                                }
                                            }
                                            span { class: "flex items-center",
                                                "Can Refresh"
                                                span {
                                                    class: "help-icon",
                                                    title: "Resets the timer duration if triggered again while already running",
                                                    "?"
                                                }
                                            }
                                        }
                                        div { class: "flex items-center gap-xs",
                                            span { class: "text-sm text-secondary flex items-center",
                                                "Repeats"
                                                span {
                                                    class: "help-icon",
                                                    title: "Number of times this timer auto-restarts after expiring. 0 = no repeat",
                                                    "?"
                                                }
                                            }
                                            input {
                                                class: "input-inline",
                                                r#type: "number",
                                                min: "0",
                                                max: "255",
                                                style: "width: 50px;",
                                                value: "{draft().repeats}",
                                                oninput: move |e| {
                                                    if let Ok(val) = e.value().parse::<u8>() {
                                                        let mut d = draft();
                                                        d.repeats = val;
                                                        draft.set(d);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ─── Alerts Card ────────────────────────────────────────────
                    div { class: "form-card",
                        div { class: "form-card-header",
                            i { class: "fa-solid fa-bell" }
                            span { "Alerts" }
                        }
                        div { class: "form-card-content",
                            div { class: "form-row-hz",
                                label { class: "flex items-center",
                                    "Alert Text"
                                    span {
                                        class: "help-icon",
                                        title: "Text shown in the alert notification. Defaults to timer name",
                                        "?"
                                    }
                                }
                                input {
                                    class: "input-inline",
                                    r#type: "text",
                                    style: "flex: 1; min-width: 0;",
                                    placeholder: "(timer name)",
                                    value: "{draft().alert_text.clone().unwrap_or_default()}",
                                    oninput: move |e| {
                                        let mut d = draft();
                                        d.alert_text = if e.value().is_empty() { None } else { Some(e.value()) };
                                        draft.set(d);
                                    }
                                }
                            }

                            if draft().is_alert {
                                // Instant alerts always fire on trigger — no choice needed
                                div { class: "form-row-hz",
                                    label { class: "flex items-center",
                                        "Alert On"
                                        span {
                                            class: "help-icon",
                                            title: "Instant alerts always fire when triggered",
                                            "?"
                                        }
                                    }
                                    span { class: "text-muted text-sm", "On trigger (instant)" }
                                }
                            } else {
                                div { class: "form-row-hz",
                                    label { class: "flex items-center",
                                        "Alert On"
                                        span {
                                            class: "help-icon",
                                            title: "When to show the alert text: on timer start, on timer end, or never",
                                            "?"
                                        }
                                    }
                                    select {
                                        class: "select-inline",
                                        value: "{timer_alert_label(&draft().alert_on)}",
                                        onchange: move |e| {
                                            let mut d = draft();
                                            d.alert_on = match e.value().as_str() {
                                                "Timer Start" => AlertTrigger::OnApply,
                                                "Timer End" => AlertTrigger::OnExpire,
                                                _ => AlertTrigger::None,
                                            };
                                            draft.set(d);
                                        },
                                        for trigger in AlertTrigger::all() {
                                            {
                                                let label = timer_alert_label(trigger);
                                                rsx! {
                                                    option {
                                                        value: "{label}",
                                                        selected: *trigger == draft().alert_on,
                                                        "{label}"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ─── Audio Card ─────────────────────────────────────────────
                    div { class: "form-card",
                        div { class: "form-card-header",
                            i { class: "fa-solid fa-volume-up" }
                            span { "Audio" }
                        }
                        div { class: "form-card-content",
                            label { class: "flex items-center gap-xs text-sm",
                                input {
                                    r#type: "checkbox",
                                    checked: draft().audio.enabled,
                                    onchange: move |e| {
                                        let mut d = draft();
                                        d.audio.enabled = e.checked();
                                        draft.set(d);
                                    }
                                }
                                "Enable Audio"
                            }

                            if draft().audio.enabled {
                                div { class: "form-row-hz mt-sm",
                                    label { "Sound" }
                                    div { class: "flex items-center gap-xs", style: "flex: 1; min-width: 0;",
                                        select {
                                            class: "select-inline",
                                            style: "flex: 1; min-width: 0;",
                                            value: "{draft().audio.file.clone().unwrap_or_default()}",
                                            onchange: move |e| {
                                                let mut d = draft();
                                                d.audio.file = if e.value().is_empty() { None } else { Some(e.value()) };
                                                draft.set(d);
                                            },
                                            option { value: "", selected: draft().audio.file.is_none(), "(none)" }
                                            for name in sound_files().iter() {
                                                {
                                                    let is_selected = draft().audio.file.as_deref() == Some(name.as_str());
                                                    rsx! {
                                                        option { key: "{name}", value: "{name}", selected: is_selected, "{name}" }
                                                    }
                                                }
                                            }
                                            if let Some(ref path) = draft().audio.file {
                                                if !path.is_empty() && !sound_files().contains(path) {
                                                    option { value: "{path}", selected: true, "{path} (custom)" }
                                                }
                                            }
                                        }
                                        button {
                                            class: "btn btn-sm",
                                            r#type: "button",
                                            onclick: move |_| {
                                                spawn(async move {
                                                    if let Some(path) = api::pick_audio_file().await {
                                                        let lower = path.to_lowercase();
                                                        if lower.ends_with(".mp3") || lower.ends_with(".wav") {
                                                            let mut d = draft();
                                                            d.audio.file = Some(path);
                                                            draft.set(d);
                                                        }
                                                    }
                                                });
                                            },
                                            "Browse"
                                        }
                                        if draft().audio.file.is_some() {
                                            button {
                                                class: "btn btn-sm",
                                                r#type: "button",
                                                title: "Preview sound",
                                                onclick: move |_| {
                                                    if let Some(ref file) = draft().audio.file {
                                                        let file = file.clone();
                                                        spawn(async move {
                                                            api::preview_sound(&file).await;
                                                        });
                                                    }
                                                },
                                                "Play"
                                            }
                                        }
                                    }
                                }

                                // Audio timing options (only for countdown timers)
                                if !draft().is_alert {
                                    div { class: "form-row-hz",
                                        label { class: "flex items-center",
                                            "Audio Offset"
                                            span {
                                                class: "help-icon",
                                                title: "When to play the sound relative to timer expiration",
                                                "?"
                                            }
                                        }
                                        select {
                                            class: "select-inline",
                                            style: "flex: 1; min-width: 0;",
                                            value: "{draft().audio.offset}",
                                            onchange: move |e| {
                                                if let Ok(val) = e.value().parse::<u8>() {
                                                    let mut d = draft();
                                                    d.audio.offset = val;
                                                    draft.set(d);
                                                }
                                            },
                                            option { value: "0", "On expiration" }
                                            option { value: "1", "1s before" }
                                            option { value: "2", "2s before" }
                                            option { value: "3", "3s before" }
                                            option { value: "4", "4s before" }
                                            option { value: "5", "5s before" }
                                            option { value: "6", "6s before" }
                                            option { value: "7", "7s before" }
                                            option { value: "8", "8s before" }
                                            option { value: "9", "9s before" }
                                            option { value: "10", "10s before" }
                                        }
                                    }

                                    div { class: "form-row-hz",
                                        label { class: "flex items-center",
                                            "Voice"
                                            span {
                                                class: "help-icon",
                                                title: "Voice countdown starting at the specified seconds remaining",
                                                "?"
                                            }
                                        }
                                        div { class: "flex items-center gap-md", style: "flex: 1; min-width: 0;",
                                            select {
                                                class: "select-inline",
                                                style: "flex: 1; min-width: 0;",
                                                value: "{draft().audio.countdown_start}",
                                                onchange: move |e| {
                                                    if let Ok(val) = e.value().parse::<u8>() {
                                                        let mut d = draft();
                                                        d.audio.countdown_start = val;
                                                        draft.set(d);
                                                    }
                                                },
                                                option { value: "0", "Off" }
                                                option { value: "3", "3s" }
                                                option { value: "5", "5s" }
                                                option { value: "10", "10s" }
                                            }
                                            select {
                                                class: "select-inline",
                                                style: "flex: 1; min-width: 0;",
                                                value: "{draft().audio.countdown_voice.clone().unwrap_or_else(|| \"Amy\".to_string())}",
                                                onchange: move |e| {
                                                    let mut d = draft();
                                                    d.audio.countdown_voice = if e.value() == "Amy" { None } else { Some(e.value()) };
                                                    draft.set(d);
                                                },
                                                option { value: "Amy", "Amy" }
                                                option { value: "Jim", "Jim" }
                                                option { value: "Yolo", "Yolo" }
                                                option { value: "Nerevar", "Nerevar" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // ─── Advanced Card ───────────────────────────────────────
                    div { class: "form-card",
                        div { class: "form-card-header",
                            i { class: "fa-solid fa-triangle-exclamation" }
                            span { "Advanced" }
                        }
                        div { class: "form-card-content",
                            label { class: "flex items-center gap-xs text-sm",
                                input {
                                    r#type: "checkbox",
                                    checked: draft().enabled,
                                    onchange: move |e| {
                                        let mut d = draft();
                                        d.enabled = e.checked();
                                        draft.set(d);
                                    }
                                }
                                "Timer Enabled"
                            }
                            if !draft().enabled {
                                div { class: "text-xs text-warning mt-xs",
                                    i { class: "fa-solid fa-triangle-exclamation" }
                                    " Disabling a timer may break phases, counters, or other timers that depend on it."
                                }
                            }
                        }
                    }
                }
            }

            // ─── Actions ────────────────────────────────────────────────────────
            div { class: "form-actions",
                button {
                    class: if has_changes() { "btn btn-success btn-sm" } else { "btn btn-sm" },
                    disabled: !has_changes(),
                    onclick: handle_save,
                    "Save"
                }
                button {
                    class: "btn btn-primary btn-sm",
                    onclick: handle_duplicate,
                    "Duplicate"
                }

                // Reset/Delete logic:
                // - Built-in unmodified: no delete button (nothing to reset)
                // - Modified built-in: "Reset to Built-in" (calls delete + refetch)
                // - Custom: "Delete" with confirmation
                if is_builtin {
                    // Pure built-in, unmodified — no action needed
                } else if is_modified {
                    // Modified built-in — offer reset
                    if confirm_delete() {
                        span { class: "flex items-center gap-xs ml-auto",
                            "Reset to built-in?"
                            button {
                                class: "btn btn-warning btn-sm",
                                onclick: handle_delete,
                                "Yes"
                            }
                            button {
                                class: "btn btn-sm",
                                onclick: move |_| confirm_delete.set(false),
                                "No"
                            }
                        }
                    } else {
                        button {
                            class: "btn btn-warning btn-sm ml-auto",
                            onclick: move |_| confirm_delete.set(true),
                            "Reset to Built-in"
                        }
                    }
                } else {
                    // Custom item — offer delete
                    if confirm_delete() {
                        span { class: "flex items-center gap-xs ml-auto",
                            "Delete?"
                            button {
                                class: "btn btn-danger btn-sm",
                                onclick: handle_delete,
                                "Yes"
                            }
                            button {
                                class: "btn btn-sm",
                                onclick: move |_| confirm_delete.set(false),
                                "No"
                            }
                        }
                    } else {
                        button {
                            class: "btn btn-danger btn-sm ml-auto",
                            onclick: move |_| confirm_delete.set(true),
                            "Delete"
                        }
                    }
                }
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
                class: "select",
                style: "min-width: 160px; text-align: left;",
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
                span { class: "ml-auto", "▾" }
            }

            // Dropdown menu (fixed position to escape overflow clipping)
            if dropdown_open() {
                div {
                    class: "phase-dropdown",
                    style: "position: fixed; left: {dropdown_pos().0}px; top: {dropdown_pos().1}px; z-index: 10000; background: #1e1e2e; border: 1px solid var(--border-medium); border-radius: var(--radius-sm); padding: var(--space-xs); min-width: 160px; max-height: 200px; overflow-y: auto; box-shadow: 0 4px 12px rgba(0,0,0,0.5);",

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
