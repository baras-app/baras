//! Data Explorer Panel Component
//!
//! Displays detailed ability breakdown and DPS analysis for encounters.
//! Uses DataFusion SQL queries over parquet files for historical data.

use dioxus::prelude::*;

use baras_types::ReplayMapStatus;

use crate::api::{PlayerDeath, TimeRange};
use crate::components::combat_log::CombatLog;
use crate::components::death_recap_drawer::DeathRecapDrawer;
use crate::components::enemy_views::{live_notice, loading};
use crate::components::mirror_names;
use crate::components::phase_timeline::PhaseTimelineFilter;
use crate::components::raid_debuffs::RaidDebuffsView;
use crate::components::replay::{MapPrompt, ReplayView};
use crate::components::threat::ThreatView;
use crate::types::{UiSessionState, ViewMode};

mod ability_rows;
mod ability_table;
mod ctx;
mod derived;
mod detail_loaders;
mod detailed_view;
mod dummy_charts;
mod dummy_view;
mod encounter_sidebar;
mod entity_sidebar;
mod grouping;
mod loaders;
mod overview_view;
mod sync;
mod usage_tab;
mod view_tabs;

use ctx::{ExplorerCtx, LoadState, use_explorer_ctx};
use detailed_view::detailed_view;
use dummy_view::{DummyParseView, dummy_target};
use encounter_sidebar::encounter_sidebar;
use overview_view::overview_view;
use view_tabs::{encounter_strip, view_tab_bar};

#[derive(Props, Clone, PartialEq)]
pub struct DataExplorerProps {
    /// Unified UI session state (includes all persisted state for this panel)
    pub state: Signal<UiSessionState>,
}

#[component]
pub fn DataExplorerPanel(props: DataExplorerProps) -> Element {
    let ctx = use_explorer_ctx(props.state);

    // Mirror-class naming map (fetched once per app lifetime)
    mirror_names::ensure_loaded();

    sync::use_state_sync(ctx);
    loaders::use_loaders(ctx);

    // Filter by source when selected. Created here so the query it spawns belongs
    // to the panel and outlives the entity sidebar that triggers it.
    let on_source_click = use_callback(move |(name, id): (String, Option<i64>)| ctx.select_source(name, id));

    let ExplorerCtx {
        selected_encounter,
        shown_encounter,
        live_query_active,
        mut view_mode,
        mut time_range,
        combat_log_state,
        overview_fullscreen,
        timeline_state,
        mut shown_mode,
        timeline,
        player_deaths,
        mut death_target_filter,
        mut recap_death,
        overview_table_data,
        ..
    } = ctx;

    // A memo, so this component's own filter and sort writes to the shared state don't re-render it
    let shared = props.state;
    let eu = *use_memo(move || shared.read().european_number_format).read();

    rsx! {
        div { class: if ctx.state.read().data_explorer.hide_percentiles { "data-explorer hide-percentiles" } else { "data-explorer" },
            // Sidebar with encounter list
            {encounter_sidebar(ctx)}

            // Data Panel (main content area)
            div { class: if *overview_fullscreen.read() { "data-panel fullscreen" } else { "data-panel" },
                if selected_encounter.read().is_none() && !*live_query_active.read() {
                    div { class: "panel-placeholder",
                        i { class: "fa-solid fa-chart-bar" }
                        p { "Select an encounter" }
                        p { class: "hint", "Choose an encounter from the sidebar to view detailed breakdown" }
                    }
                } else {
                    // Phase timeline filter — always reserve space to prevent layout shift
                    div { class: "phase-timeline-slot",
                        if let Some(tl) = timeline.read().as_ref() {
                            PhaseTimelineFilter {
                                timeline: tl.clone(),
                                range: time_range(),
                                on_range_change: move |new_range: TimeRange| {
                                    time_range.set(new_range);
                                }
                            }
                        }
                    }

                    // Selected encounter indicator (shown when sidebar collapsed or fullscreen)
                    {encounter_strip(ctx)}

                    // Data tab selector (Overview, Damage, Healing, Damage Taken, Healing Taken, Charts)
                    {view_tab_bar(ctx)}

                    // Content area — fixed-height flex region below tab bar
                    // Prevents combat log filters from shifting the tab bar position
                    div { class: "tab-content-area",
                    // Paints `shown_mode`, which only moves once that view's data is in
                    // Combat log: mounted as soon as its tab is clicked but kept
                    // invisible until its first page is in, so the previous view
                    // stays painted and the rows never pop into an empty table.
                    if shown_mode() == Some(ViewMode::CombatLog)
                        || (view_mode() == ViewMode::CombatLog && matches!(timeline_state(), LoadState::Loaded))
                    {
                        div {
                            class: if shown_mode() == Some(ViewMode::CombatLog) { "combat-log-host" } else { "combat-log-host pending" },
                            CombatLog {
                                key: "{shown_encounter():?}",
                                encounter_idx: shown_encounter(),
                                time_range: time_range(),
                                initial_target: death_target_filter(),
                                state: combat_log_state,
                                on_range_change: move |new_range: TimeRange| {
                                    time_range.set(new_range);
                                },
                                on_ready: move |_| {
                                    if *view_mode.peek() == ViewMode::CombatLog {
                                        shown_mode.set(Some(ViewMode::CombatLog));
                                    }
                                },
                                european: eu,
                            }
                        }
                    }

                    if let Some(shown) = shown_mode() {

                    // `shown` is a Copy value, so no signal borrow is held across onclick handlers
                    if matches!(shown, ViewMode::CombatLog) {
                        // Rendered by the host above
                    } else if matches!(shown, ViewMode::Overview) {
                        // A dummy parse has one player and no mechanics: its own summary
                        if let Some((encounter_idx, player)) = dummy_target(ctx) {
                            DummyParseView {
                                encounter_idx,
                                player,
                                european: eu,
                                chart: overview_view::overview_chart(ctx, eu),
                            }
                        } else {
                            {overview_view(ctx, eu)}
                        }
                    } else if matches!(shown, ViewMode::Detailed(_) | ViewMode::Rotation | ViewMode::Usage | ViewMode::Charts | ViewMode::Uptime) {
                        // Two-column layout (Detailed breakdown, Rotation, Usage, or Charts)
                        {detailed_view(ctx, shown, eu, on_source_click)}
                    } else if matches!(shown, ViewMode::RaidDebuffs | ViewMode::Aggro | ViewMode::Replay) && (ctx.live_query_active)() {
                        // All read the whole fight: a live one has no end yet
                        {live_notice()}
                    } else if matches!(shown, ViewMode::RaidDebuffs) {
                        RaidDebuffsView {
                            // Remounts per encounter, so it fades in like the other views
                            key: "{shown_encounter():?}",
                            encounter_idx: shown_encounter(),
                            time_range: time_range(),
                            duration_secs: ctx.timeline.read().as_ref().map_or(0.0, |t| t.duration_secs),
                            overview: ctx.overview_data,
                            state: ctx.enemy_views,
                            european: eu,
                        }
                    } else if matches!(shown, ViewMode::Aggro) {
                        ThreatView {
                            key: "{shown_encounter():?}",
                            encounter_idx: shown_encounter(),
                            time_range: time_range(),
                            duration_secs: ctx.timeline.read().as_ref().map_or(0.0, |t| t.duration_secs),
                            overview: ctx.overview_data,
                            state: ctx.enemy_views,
                            european: eu,
                        }
                    } else if matches!(shown, ViewMode::Replay) && !ctx.replay_available(shown_encounter()) {
                        div { class: "uptime-empty", "Replay is available for operation boss fights that have a map" }
                    } else if matches!(shown, ViewMode::Replay) && ctx.replay_map_status(shown_encounter()) == ReplayMapStatus::Loading {
                        {loading()}
                    } else if matches!(shown, ViewMode::Replay)
                        && let status = ctx.replay_map_status(shown_encounter())
                        && (status == ReplayMapStatus::NeedsConsent
                            || (status == ReplayMapStatus::Unavailable && !cfg!(debug_assertions)))
                    {
                        MapPrompt {
                            failed: status == ReplayMapStatus::Unavailable,
                            on_download: move |_| ctx.load_replay_maps(shown_encounter(), true),
                        }
                    } else if matches!(shown, ViewMode::Replay) {
                        ReplayView {
                            key: "{shown_encounter():?}",
                            encounter_idx: shown_encounter(),
                            phases: ctx.timeline.read().as_ref().map(|t| t.phases.clone()).unwrap_or_default(),
                            maps: ctx.replay_maps.read().1.maps.clone(),
                            european: eu,
                            jump: (ctx.replay_jump)(),
                            range: ctx
                                .timeline
                                .read()
                                .as_ref()
                                .filter(|t| !time_range().is_full(t.duration_secs))
                                .map(|_| (time_range().start, time_range().end)),
                            on_recap: move |(name, at): (String, f32)| {
                                // The death in the explorer's list closest to that moment
                                let death = player_deaths
                                    .read()
                                    .iter()
                                    .filter(|d| d.name == name)
                                    .min_by(|a, b| (a.death_time_secs - at).abs().total_cmp(&(b.death_time_secs - at).abs()))
                                    .cloned();
                                recap_death.set(death);
                            },
                        }
                    }

                    } // end timeline-loaded gate
                    } // end tab-content-area
                }
            }

            // Death recap drawer (opened from the Overview's death tracker); lives at
            // the root so it stays open across tabs, and last so Esc closes it before fullscreen
            if let Some(d) = recap_death() {
                DeathRecapDrawer {
                    encounter_idx: shown_encounter(),
                    player_name: d.name.clone(),
                    death_time_secs: d.death_time_secs,
                    european: eu,
                    on_close: move |_| recap_death.set(None),
                    player: overview_table_data.read().rows.iter().find(|r| r.name == d.name && r.faction == d.faction).cloned(),
                    roster: overview_table_data.read().rows.clone(),
                    deaths: player_deaths.read().clone(),
                    on_select: move |d: PlayerDeath| recap_death.set(Some(d)),
                    on_replay: ctx.replay_available(shown_encounter()).then(|| {
                        EventHandler::new(move |_| {
                            if let Some(d) = recap_death() {
                                let mut jump = ctx.replay_jump;
                                jump.set(Some((d.name.clone(), d.death_time_secs)));
                                view_mode.set(ViewMode::Replay);
                            }
                        })
                    }),
                    on_open_combat_log: move |window_start: f32| {
                        if let Some(d) = recap_death() {
                            time_range.set(TimeRange {
                                start: window_start.max(0.0),
                                end: d.death_time_secs + 1.5,
                            });
                            death_target_filter.set(Some(d.name.clone()));
                            view_mode.set(ViewMode::CombatLog);
                        }
                    },
                }
            }
        }
    }
}
