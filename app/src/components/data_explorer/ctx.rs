//! Shared reactive state of the Data Explorer.
//!
//! Every signal and derived memo of the panel lives in [`ExplorerCtx`], created
//! once by `DataExplorerPanel` and read by its sub-components from context.

use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};

use super::derived::{self, AreaSections};
use super::grouping::{AbilityGroups, group_abilities};
use crate::api::{
    self, AbilityBreakdown, BreakdownMode, DamageTakenSummary, EncounterTimeline, EntityBreakdown, NpcHealthRow,
    EncounterRatings, PlayerDeath, RaidOverviewRow, TimeRange,
};
use crate::components::encounter_types::EncounterSummary;
use crate::types::{CombatLogSessionState, SortColumn, SortDirection, UiSessionState, UsageSortColumn, ViewMode};
use baras_types::{OverviewColumn, OverviewLayouts};

// ─────────────────────────────────────────────────────────────────────────────
// Local Types (not persisted)
// ─────────────────────────────────────────────────────────────────────────────

/// Loading state for async operations
#[derive(Clone, PartialEq, Default)]
pub(super) enum LoadState {
    #[default]
    Idle,
    Loading,
    Loaded,
}

/// Sorted player rows of the overview table
#[derive(Clone, PartialEq, Default)]
pub(super) struct OverviewTableData {
    pub rows: Vec<RaidOverviewRow>,
    /// Rows carry a friend/enemy side: one table per side
    pub pvp: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Context
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
pub(super) struct ExplorerCtx {
    pub state: Signal<UiSessionState>,
    pub encounters: Signal<Vec<EncounterSummary>>,
    pub selected_encounter: Signal<Option<u32>>,
    /// The encounter whose data is painted: follows `selected_encounter` once
    /// that encounter's timeline and page have loaded, in the same update
    pub shown_encounter: Signal<Option<u32>>,
    pub auto_live: Signal<bool>,
    pub auto_follow: Signal<bool>,
    pub live_query_active: Signal<bool>,
    pub transitioning_from_live: Signal<bool>,
    pub live_poll_tick: Signal<u32>,
    pub view_mode: Signal<ViewMode>,
    pub selected_source: Signal<Option<String>>,
    pub selected_source_id: Signal<Option<i64>>,
    pub breakdown_mode: Signal<BreakdownMode>,
    pub show_players_only: Signal<bool>,
    pub show_only_bosses: Signal<bool>,
    pub sort_column: Signal<SortColumn>,
    pub sort_direction: Signal<SortDirection>,
    pub collapsed_sections: Signal<HashSet<String>>,
    pub selected_rotation_anchor: Signal<Option<i64>>,
    pub usage_sort_column: Signal<UsageSortColumn>,
    pub usage_sort_direction: Signal<SortDirection>,
    pub usage_selected_abilities: Signal<Vec<(i64, &'static str)>>,
    pub overview_sort_col: Signal<OverviewColumn>,
    pub overview_layouts: Memo<OverviewLayouts>,
    pub overview_sort_asc: Signal<bool>,
    pub overview_alive_view: Signal<bool>,
    pub overview_chart_col: Signal<OverviewColumn>,
    pub overview_stacked: Signal<bool>,
    pub overview_show_deaths: Signal<bool>,
    pub overview_hovered: Signal<Option<String>>,
    pub sidebar_collapsed: Signal<bool>,
    pub entity_collapsed: Signal<bool>,
    pub overview_fullscreen: Signal<bool>,
    pub expanded_npc_groups: Signal<HashSet<String>>,
    pub expanded_entity_groups: Signal<HashSet<String>>,
    pub combat_log_state: Signal<CombatLogSessionState>,
    pub time_range: Signal<TimeRange>,
    pub prev_encounter: Signal<Option<u32>>,
    pub local_player_name: Signal<Option<String>>,
    pub abilities: Signal<Vec<AbilityBreakdown>>,
    pub entities: Signal<Vec<EntityBreakdown>>,
    pub entity_dmg_totals: Signal<HashMap<String, f64>>,
    pub timeline_state: Signal<LoadState>,
    pub shown_mode: Signal<Option<ViewMode>>,
    pub load_generation: Signal<u32>,
    pub timeline: Signal<Option<EncounterTimeline>>,
    pub overview_data: Signal<Vec<RaidOverviewRow>>,
    /// Parsely percentiles and the encounter they rate (None = nothing rated)
    pub percentiles: Signal<(Option<u32>, EncounterRatings)>,
    /// Parsely kill time percentile per rated kill, by encounter id
    pub ttk_ratings: Signal<HashMap<u64, baras_types::TtkRating>>,
    pub player_deaths: Signal<Vec<PlayerDeath>>,
    pub npc_health: Signal<Vec<NpcHealthRow>>,
    pub last_overview_fetch: Signal<Option<(Option<u32>, TimeRange, bool)>>,
    /// (encounter, view, range) whose entities + abilities arrived with the timeline
    pub preloaded_detail: Signal<Option<(Option<u32>, ViewMode, TimeRange)>>,
    pub death_target_filter: Signal<Option<String>>,
    pub recap_death: Signal<Option<PlayerDeath>>,
    pub dt_summary: Signal<Option<DamageTakenSummary>>,
    pub overview_table_data: Memo<OverviewTableData>,
    pub filtered_history: Memo<Vec<EncounterSummary>>,
    pub sections: Memo<AreaSections>,
    pub entity_list: Memo<Vec<EntityBreakdown>>,
    pub class_icon_lookup: Memo<HashMap<String, String>>,
    pub overview_stats_lookup: Memo<HashMap<String, (f64, f64, bool)>>,
    pub grouped_abilities: Memo<AbilityGroups>,
}

pub(super) fn use_explorer_ctx(state: Signal<UiSessionState>) -> ExplorerCtx {
    // Encounter selection state
    let encounters = use_signal(Vec::<EncounterSummary>::new);

    // Extract persisted state fields into local signals for easier access
    // Initialize from parent state to support navigation from other components
    let selected_encounter = use_signal(|| state.read().data_explorer.selected_encounter);
    let shown_encounter = use_signal(|| *selected_encounter.peek());
    // Auto-live preference: loaded from config, controls whether auto_follow starts enabled.
    let auto_live = use_signal(|| state.read().data_explorer.auto_live);
    // Auto-follow: when true, CombatStarted auto-switches to live mode.
    // Clicking a historical encounter sets this false. Clicking the Live button sets it back to true.
    // Only starts enabled when auto_live preference is on AND no encounter is pre-selected.
    let auto_follow =
        use_signal(|| state.read().data_explorer.auto_live && state.read().data_explorer.selected_encounter.is_none());
    // Live query: true only when in combat and actively polling the buffer
    let live_query_active = use_signal(|| false);
    // Tracks transition from live → historical (combat ended, selecting completed encounter)
    // When true, the timeline effect skips destructive clearing to avoid flash
    let transitioning_from_live = use_signal(|| false);
    let live_poll_tick = use_signal(|| 0u32);
    let view_mode = use_signal(|| state.read().data_explorer.view_mode);
    let selected_source = use_signal(|| state.read().data_explorer.selected_source.clone());
    // Instance id of the selected NPC spawn; None for players and name-keyed selections
    let selected_source_id = use_signal(|| state.read().data_explorer.selected_source_id);
    let breakdown_mode = use_signal(|| state.read().data_explorer.breakdown_mode);
    let show_players_only = use_signal(|| state.read().data_explorer.show_players_only);
    let show_only_bosses = use_signal(|| state.read().data_explorer.show_only_bosses);
    let sort_column = use_signal(|| state.read().data_explorer.sort_column);
    let sort_direction = use_signal(|| state.read().data_explorer.sort_direction);
    let collapsed_sections = use_signal(|| state.read().data_explorer.collapsed_sections.clone());
    let selected_rotation_anchor = use_signal(|| state.read().data_explorer.selected_rotation_anchor);
    let usage_sort_column = use_signal(|| state.read().data_explorer.usage_sort_column);
    let usage_sort_direction = use_signal(|| state.read().data_explorer.usage_sort_direction);
    let usage_selected_abilities = use_signal(Vec::<(i64, &'static str)>::new);

    // Overview table sort (not persisted - default DPS descending)
    let overview_sort_col = use_signal(OverviewColumn::default);
    let overview_layouts = use_memo(move || state.read().data_explorer.overview_columns.clone());
    let overview_sort_asc = use_signal(|| false);
    // Group chart view toggles; live here so they survive encounter switches (the chart remounts)
    let overview_alive_view = use_signal(|| false);
    let overview_chart_col = use_signal(OverviewColumn::default);
    let overview_stacked = use_signal(|| false);
    let overview_show_deaths = use_signal(|| true);
    // Overview table row under the pointer, highlighted on the group chart
    let overview_hovered = use_signal(|| None::<String>);

    // Sidebar collapse states (not persisted - always start expanded)
    let sidebar_collapsed = use_signal(|| false);
    let entity_collapsed = use_signal(|| false);
    let overview_fullscreen = use_signal(|| false);

    // NPC health table: which grouped NPC names are expanded (not persisted)
    let expanded_npc_groups = use_signal(HashSet::<String>::new);
    // Entity sidebar: NPC names whose same-named spawns are expanded
    let expanded_entity_groups = use_signal(HashSet::<String>::new);

    // Combat log state is a separate signal that CombatLog component will modify
    let combat_log_state = use_signal(|| state.read().combat_log.clone());

    // Time range is persisted - restore from saved state
    let time_range = use_signal(|| state.read().data_explorer.time_range);

    // Track previous encounter to detect actual changes vs. initial mount
    // Start with None to trigger initial load
    let prev_encounter = use_signal(|| None::<u32>);

    // Local player name (fetched once on mount for auto-selection)
    let local_player_name = use_signal(|| None::<String>);

    // Query result state (not persisted)
    let abilities = use_signal(Vec::<AbilityBreakdown>::new);
    let entities = use_signal(Vec::<EntityBreakdown>::new);
    // Per-entity damage totals for Rotation/Usage/Charts color coding (name → damage total)
    let entity_dmg_totals = use_signal(HashMap::<String, f64>::new);

    // Loading states (not persisted)
    let timeline_state = use_signal(LoadState::default);
    // The view the content area is painting. It trails `view_mode` until that
    // view's data has landed, so the previous page stays up instead of a blank
    // gap or pop-in. None = nothing loaded to show.
    let shown_mode = use_signal(|| None::<ViewMode>);
    // Generation counter to discard stale async results on rapid encounter switching
    let load_generation = use_signal(|| 0u32);

    // Timeline state
    let timeline = use_signal(|| None::<EncounterTimeline>);

    // Overview data
    let overview_data = use_signal(Vec::<RaidOverviewRow>::new);
    let percentiles = use_signal(|| (None, EncounterRatings::default()));
    let ttk_ratings = use_signal(HashMap::new);
    let player_deaths = use_signal(Vec::<PlayerDeath>::new);
    let npc_health = use_signal(Vec::<NpcHealthRow>::new);
    // Track last (encounter, time_range, was_overview) we fetched overview data for (prevents re-fetch loops)
    // The bool tracks whether deaths/NPC HP were also loaded (only on Overview tab)
    let last_overview_fetch = use_signal(|| None::<(Option<u32>, TimeRange, bool)>);
    let preloaded_detail = use_signal(|| None);

    // Death target filter - set when clicking a death to filter combat log by target
    let death_target_filter = use_signal(|| None::<String>);

    // Death whose recap modal is open (None = closed)
    let recap_death = use_signal(|| None::<PlayerDeath>);

    // DamageTaken summary (shown above the ability table for a selected source)
    let dt_summary: Signal<Option<DamageTakenSummary>> = use_signal(|| None);

    // Memoized overview table data (rows + totals) - prevents recomputation on every render
    let overview_table_data = use_memo(move || {
        derived::overview_table(
            &overview_data.read(),
            *overview_sort_col.read(),
            *overview_sort_asc.read(),
        )
    });

    // Memoized filtered history - only recomputes when encounters or filter changes
    let filtered_history = use_memo(move || derived::filter_history(encounters(), show_only_bosses()));

    // Memoized sections - groups encounters by area
    let sections = use_memo(move || derived::area_sections(&filtered_history()));

    // Memoized entity list for detailed view - filtered by player/all toggle
    let entity_list = use_memo(move || {
        let players_only = *show_players_only.read();
        entities
            .read()
            .iter()
            .filter(|e| {
                if players_only {
                    e.entity_type == "Player" || e.entity_type == "Companion"
                } else {
                    e.entity_type == "Npc"
                }
            })
            .cloned()
            .collect::<Vec<_>>()
    });

    // Memoized class icon lookup from overview data (player name -> class_icon)
    let class_icon_lookup = use_memo(move || {
        overview_data
            .read()
            .iter()
            .filter_map(|row| row.class_icon.as_ref().map(|icon| (row.name.clone(), icon.clone())))
            .collect::<HashMap<String, String>>()
    });

    // Memoized overview stats lookup (player name -> (dps, ehps, is_healer))
    // Used by Rotation/Usage sidebar to show DPS for non-healers and EHPS for healers
    let overview_stats_lookup = use_memo(move || {
        overview_data
            .read()
            .iter()
            .map(|row| {
                let is_healer = row.role_icon.as_deref() == Some("icon_heal.png");
                (row.name.clone(), (row.dps, row.ehps, is_healer))
            })
            .collect::<HashMap<String, (f64, f64, bool)>>()
    });

    // Memoized grouped abilities - groups by target when breakdown mode is enabled
    let grouped_abilities = use_memo(move || {
        let col = *sort_column.read();
        let dir = *sort_direction.read();
        let mode = *breakdown_mode.read();
        group_abilities(&abilities.read(), col, dir, mode)
    });

    ExplorerCtx {
        state,
        encounters,
        selected_encounter,
        shown_encounter,
        auto_live,
        auto_follow,
        live_query_active,
        transitioning_from_live,
        live_poll_tick,
        view_mode,
        selected_source,
        selected_source_id,
        breakdown_mode,
        show_players_only,
        show_only_bosses,
        sort_column,
        sort_direction,
        collapsed_sections,
        selected_rotation_anchor,
        usage_sort_column,
        usage_sort_direction,
        usage_selected_abilities,
        overview_sort_col,
        overview_layouts,
        overview_sort_asc,
        overview_alive_view,
        overview_chart_col,
        overview_stacked,
        overview_show_deaths,
        overview_hovered,
        sidebar_collapsed,
        entity_collapsed,
        overview_fullscreen,
        expanded_npc_groups,
        expanded_entity_groups,
        combat_log_state,
        time_range,
        prev_encounter,
        local_player_name,
        abilities,
        entities,
        entity_dmg_totals,
        timeline_state,
        shown_mode,
        load_generation,
        timeline,
        overview_data,
        percentiles,
        ttk_ratings,
        player_deaths,
        npc_health,
        last_overview_fetch,
        preloaded_detail,
        death_target_filter,
        recap_death,
        dt_summary,
        overview_table_data,
        filtered_history,
        sections,
        entity_list,
        class_icon_lookup,
        overview_stats_lookup,
        grouped_abilities,
    }
}

/// Subscribe the panel to a query result that the view reads only through a
/// memo. A memo tells its readers about a change one scheduler step after the
/// signal it derives from, so without this that part of the page would repaint
/// a beat behind the rest.
pub(super) fn repaint_with<T: 'static>(source: Signal<T>) {
    let _ = source.read();
}

impl ExplorerCtx {
    /// Reloads for `idx` may run: its timeline has loaded and it is the painted
    /// encounter. Until the encounter load lands, `time_range` and `timeline`
    /// still hold the previous fight's, and a query with them flashes values
    /// over the wrong duration. Subscribes the calling effect.
    pub(super) fn timeline_ready(self, idx: Option<u32>) -> bool {
        matches!(*self.timeline_state.read(), LoadState::Loaded) && *self.shown_encounter.read() == idx
    }

    // Filter by source when selected
    // `id` is the NPC instance id when the row is one spawn among same-named copies
    pub(super) fn select_source(self, name: String, id: Option<i64>) {
        let ExplorerCtx {
            shown_encounter,
            view_mode,
            mut selected_source,
            mut selected_source_id,
            time_range,
            mut usage_selected_abilities,
            show_players_only,
            breakdown_mode,
            timeline,
            mut abilities,
            ..
        } = self;

        let idx = *shown_encounter.read();
        let mode = *view_mode.read();
        let current = selected_source.read().clone();
        let current_id = *selected_source_id.read();
        let tr = time_range();

        // Toggle selection (same name + same instance = deselect)
        let (new_source, new_id) = if current.as_ref() == Some(&name) && current_id == id {
            (None, None)
        } else {
            (Some(name.clone()), id)
        };

        selected_source.set(new_source.clone());
        selected_source_id.set(new_id);
        usage_selected_abilities.set(Vec::new());

        // In Rotation mode, just set the source - the RotationView handles its own queries
        let Some(tab) = mode.tab() else {
            return;
        };

        // Use time_range if not default
        let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
            None
        } else {
            Some(tr)
        };

        spawn(async move {
            // Apply entity filter only when no specific source is selected
            let entity_filter: Option<&[&str]> = if new_source.is_none() {
                if *show_players_only.read() {
                    Some(&["Player", "Companion"])
                } else {
                    Some(&["Npc"])
                }
            } else {
                None
            };
            let breakdown = *breakdown_mode.read();
            let duration = timeline.read().as_ref().map(|t| t.duration_secs);
            if let Some(data) = api::query_breakdown(
                tab,
                idx,
                new_source.as_deref(),
                new_id,
                tr_opt.as_ref(),
                entity_filter,
                Some(&breakdown),
                duration,
            )
            .await
            {
                let _ = abilities.try_write().map(|mut w| *w = data);
            }
        });
    }
}
