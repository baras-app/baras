//! Encounter Editor
//!
//! Full CRUD for the BossEncounter DSL: timers, phases, counters, challenges, entities.

mod conditions;
mod counters;
mod challenges;
mod entities;
mod new_forms;
mod phases;
mod tabs;
mod timers;
pub mod triggers;

use dioxus::prelude::*;

use crate::api;
use crate::types::{
    AreaListItem, BossListItem, ChallengeListItem, CounterListItem, EntityListItem,
    PhaseListItem, TimerListItem,
};

pub use tabs::BossTabs;

// ─────────────────────────────────────────────────────────────────────────────
// Main Panel
// ─────────────────────────────────────────────────────────────────────────────

#[component]
pub fn EncounterEditorPanel() -> Element {
    // Area index state
    let mut areas = use_signal(Vec::<AreaListItem>::new);
    let mut selected_area = use_signal(|| None::<AreaListItem>);
    let mut loading_areas = use_signal(|| true);

    // Boss state (loaded on area selection)
    let mut bosses = use_signal(Vec::<BossListItem>::new);
    let mut timers = use_signal(Vec::<TimerListItem>::new);
    let mut phases = use_signal(Vec::<PhaseListItem>::new);
    let mut counters = use_signal(Vec::<CounterListItem>::new);
    let mut challenges = use_signal(Vec::<ChallengeListItem>::new);
    let mut entities = use_signal(Vec::<EntityListItem>::new);
    let mut loading_bosses = use_signal(|| false);

    // UI state
    let mut area_filter = use_signal(String::new);
    let mut expanded_boss = use_signal(|| None::<String>);
    let mut show_new_area = use_signal(|| false);
    let mut show_new_boss = use_signal(|| false);
    let mut status_message = use_signal(|| None::<(String, bool)>);

    // Auto-dismiss toast after 3 seconds
    use_effect(move || {
        if status_message().is_some() {
            spawn(async move {
                gloo_timers::future::TimeoutFuture::new(3000).await;
                status_message.set(None);
            });
        }
    });

    // Load area index on mount
    use_effect(move || {
        spawn(async move {
            if let Some(a) = api::get_area_index().await {
                areas.set(a);
            }
            loading_areas.set(false);
        });
    });

    // Load bosses when area is selected
    let mut load_area_data = move |area: AreaListItem| {
        let file_path = area.file_path.clone();
        selected_area.set(Some(area));
        loading_bosses.set(true);
        bosses.set(Vec::new());
        timers.set(Vec::new());
        phases.set(Vec::new());
        counters.set(Vec::new());
        challenges.set(Vec::new());
        entities.set(Vec::new());
        expanded_boss.set(None);

        spawn(async move {
            if let Some(b) = api::get_bosses_for_area(&file_path).await {
                bosses.set(b);
            }
            if let Some(t) = api::get_timers_for_area(&file_path).await {
                timers.set(t);
            }
            if let Some(p) = api::get_phases_for_area(&file_path).await {
                phases.set(p);
            }
            if let Some(c) = api::get_counters_for_area(&file_path).await {
                counters.set(c);
            }
            if let Some(ch) = api::get_challenges_for_area(&file_path).await {
                challenges.set(ch);
            }
            if let Some(e) = api::get_entities_for_area(&file_path).await {
                entities.set(e);
            }
            loading_bosses.set(false);
        });
    };

    // Group areas by category (with filtering)
    let grouped_areas = {
        let filter = area_filter().to_lowercase();
        let mut ops = Vec::new();
        let mut fps = Vec::new();
        let mut lairs = Vec::new();
        let mut other = Vec::new();

        for area in areas() {
            if !filter.is_empty() && !area.name.to_lowercase().contains(&filter) {
                continue;
            }
            match area.category.as_str() {
                "operations" => ops.push(area),
                "flashpoints" => fps.push(area),
                "lair_bosses" => lairs.push(area),
                _ => other.push(area),
            }
        }
        (ops, fps, lairs, other)
    };

    rsx! {
        div { class: "editor-layout",
            // ─── Sidebar: Area List ───────────────────────────────────────────
            div { class: "editor-sidebar",
                div { class: "editor-sidebar-header",
                    span { class: "text-sm text-muted", "Areas" }
                    button {
                        class: "btn btn-success btn-sm",
                        onclick: move |_| show_new_area.set(true),
                        "+ New"
                    }
                }

                div { class: "p-sm",
                    input {
                        class: "input input-sm w-full",
                        r#type: "text",
                        placeholder: "Filter...",
                        value: "{area_filter}",
                        oninput: move |e| area_filter.set(e.value())
                    }
                }

                div { class: "editor-sidebar-content",
                    if loading_areas() {
                        div { class: "empty-state text-sm", "Loading..." }
                    } else {
                        if !grouped_areas.0.is_empty() {
                            AreaCategory {
                                name: "Operations",
                                areas: grouped_areas.0.clone(),
                                selected: selected_area(),
                                on_select: move |a| load_area_data(a),
                            }
                        }
                        if !grouped_areas.1.is_empty() {
                            AreaCategory {
                                name: "Flashpoints",
                                areas: grouped_areas.1.clone(),
                                selected: selected_area(),
                                on_select: move |a| load_area_data(a),
                            }
                        }
                        if !grouped_areas.2.is_empty() {
                            AreaCategory {
                                name: "Lair Bosses",
                                areas: grouped_areas.2.clone(),
                                selected: selected_area(),
                                on_select: move |a| load_area_data(a),
                            }
                        }
                        if !grouped_areas.3.is_empty() {
                            AreaCategory {
                                name: "Other",
                                areas: grouped_areas.3.clone(),
                                selected: selected_area(),
                                on_select: move |a| load_area_data(a),
                            }
                        }
                    }
                }
            }

            // ─── Main Content ─────────────────────────────────────────────────
            div { class: "editor-main",
                if selected_area().is_none() {
                    div { class: "empty-state",
                        div { class: "empty-state-icon", "📂" }
                        "Select an area to edit encounters"
                    }
                } else if loading_bosses() {
                    div { class: "empty-state", "Loading..." }
                } else {
                    // Area header
                    div { class: "flex items-center justify-between mb-md",
                        h2 { class: "text-primary", "{selected_area().map(|a| a.name).unwrap_or_default()}" }
                        button {
                            class: "btn btn-success btn-sm",
                            onclick: move |_| show_new_boss.set(true),
                            "+ New Boss"
                        }
                    }

                    // New boss form
                    if show_new_boss() {
                        if let Some(area) = selected_area() {
                            new_forms::NewBossForm {
                                area: area,
                                on_create: move |new_boss| {
                                    spawn(async move {
                                        if let Some(created) = api::create_boss(&new_boss).await {
                                            let mut current = bosses();
                                            current.push(BossListItem {
                                                id: created.id,
                                                name: created.name,
                                                area_name: created.area_name,
                                                category: String::new(),
                                                file_path: created.file_path,
                                            });
                                            bosses.set(current);
                                            status_message.set(Some(("Boss created".to_string(), false)));
                                        } else {
                                            status_message.set(Some(("Failed to create".to_string(), true)));
                                        }
                                    });
                                    show_new_boss.set(false);
                                },
                                on_cancel: move |_| show_new_boss.set(false),
                            }
                        }
                    }

                    // Boss list
                    if bosses().is_empty() {
                        div { class: "empty-state", "No bosses in this area" }
                    } else {
                        for boss in bosses() {
                            {
                                let is_expanded = expanded_boss() == Some(boss.id.clone());
                                let boss_id = boss.id.clone();
                                let boss_timers: Vec<_> = timers().into_iter()
                                    .filter(|t| t.boss_id == boss.id)
                                    .collect();
                                let timer_count = boss_timers.len();
                                let phase_count = phases().iter().filter(|p| p.boss_id == boss.id).count();
                                let counter_count = counters().iter().filter(|c| c.boss_id == boss.id).count();
                                let challenge_count = challenges().iter().filter(|c| c.boss_id == boss.id).count();
                                let entity_count = entities().iter().filter(|e| e.boss_id == boss.id).count();

                                rsx! {
                                    div { class: "list-item",
                                        div {
                                            class: "list-item-header",
                                            onclick: move |_| {
                                                expanded_boss.set(if is_expanded { None } else { Some(boss_id.clone()) });
                                            },
                                            span { class: "list-item-expand", if is_expanded { "▼" } else { "▶" } }
                                            span { class: "font-medium text-primary", "{boss.name}" }
                                            span { class: "text-xs text-mono text-muted", "{boss.id}" }
                                            if timer_count > 0 {
                                                span { class: "tag", "{timer_count} timers" }
                                            }
                                            if phase_count > 0 {
                                                span { class: "tag", "{phase_count} phases" }
                                            }
                                            if counter_count > 0 {
                                                span { class: "tag", "{counter_count} counters" }
                                            }
                                            if challenge_count > 0 {
                                                span { class: "tag", "{challenge_count} challenges" }
                                            }
                                            if entity_count > 0 {
                                                span { class: "tag", "{entity_count} entities" }
                                            }
                                        }

                                        if is_expanded {
                                            div { class: "list-item-body",
                                                BossTabs {
                                                    boss: boss.clone(),
                                                    timers: boss_timers,
                                                    on_timer_change: move |new_timers: Vec<TimerListItem>| {
                                                        let mut all = timers();
                                                        all.retain(|t| t.boss_id != boss.id);
                                                        all.extend(new_timers);
                                                        timers.set(all);
                                                    },
                                                    on_status: move |msg| status_message.set(Some(msg)),
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // New Area modal
        if show_new_area() {
            new_forms::NewAreaForm {
                on_create: move |new_area| {
                    spawn(async move {
                        if let Some(_) = api::create_area(&new_area).await {
                            if let Some(a) = api::get_area_index().await {
                                areas.set(a);
                            }
                            status_message.set(Some(("Area created".to_string(), false)));
                        } else {
                            status_message.set(Some(("Failed to create".to_string(), true)));
                        }
                    });
                    show_new_area.set(false);
                },
                on_cancel: move |_| show_new_area.set(false),
            }
        }

        // Toast notification (fixed bottom-right)
        if let Some((msg, is_error)) = status_message() {
            div {
                class: "toast",
                style: "position: fixed; bottom: 20px; right: 20px; z-index: 1000; \
                        padding: 12px 16px; border-radius: 6px; \
                        background: #2a2a2e; border: 1px solid #3a3a3e; \
                        box-shadow: 0 4px 12px rgba(0,0,0,0.5); \
                        display: flex; align-items: center; gap: 12px;",
                span {
                    style: if is_error { "color: var(--color-error);" } else { "color: var(--color-success);" },
                    if is_error { "✗" } else { "✓" }
                }
                span { "{msg}" }
                button {
                    class: "btn btn-ghost btn-sm",
                    style: "padding: 2px 6px; min-width: auto;",
                    onclick: move |_| status_message.set(None),
                    "×"
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Area Category (collapsible)
// ─────────────────────────────────────────────────────────────────────────────

#[component]
fn AreaCategory(
    name: &'static str,
    areas: Vec<AreaListItem>,
    selected: Option<AreaListItem>,
    on_select: EventHandler<AreaListItem>,
) -> Element {
    let mut collapsed = use_signal(|| false);

    rsx! {
        div { class: "category-group",
            div {
                class: "category-header",
                onclick: move |_| collapsed.set(!collapsed()),
                span { if collapsed() { "▶" } else { "▼" } }
                span { "{name}" }
                span { class: "sidebar-item-count", "{areas.len()}" }
            }

            if !collapsed() {
                div { class: "category-items",
                    for area in areas {
                        {
                            let is_active = selected.as_ref().map(|s| s.file_path == area.file_path).unwrap_or(false);
                            let area_clone = area.clone();

                            rsx! {
                                div {
                                    class: if is_active { "sidebar-item active" } else { "sidebar-item" },
                                    onclick: move |_| on_select.call(area_clone.clone()),
                                    "{area.name}"
                                    span { class: "sidebar-item-count", "{area.boss_count}" }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
