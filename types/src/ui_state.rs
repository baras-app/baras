//! UI session state

use crate::{BreakdownMode, DataTab, LogEventCategory, RaidDebuff, ThreatEventKind, TimeRange, history};
use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// UI Session State - persists across tab switches
// ─────────────────────────────────────────────────────────────────────────────

/// Global UI session state that persists across tab switches (in-memory only).
/// Lives in the App component and flows down via props/signals.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UiSessionState {
    /// Currently active main tab
    pub active_tab: MainTab,

    /// Data Explorer state
    pub data_explorer: DataExplorerState,

    /// Combat Log state (within Data Explorer)
    pub combat_log: CombatLogSessionState,

    /// Encounter Builder state
    pub encounter_builder: EncounterBuilderState,

    /// Effects Editor state
    pub effects_editor: EffectsEditorState,

    /// History tab filters and selection
    pub history: history::HistoryState,

    /// Use European number formatting (swap `.` and `,`)
    pub european_number_format: bool,

    /// Show Parsely percentiles on boss kills
    pub parsely_percentiles: bool,
}

impl UiSessionState {
    /// Reset session-specific state when opening a new file.
    /// Preserves user preferences like show_only_bosses, show_ids, view modes, etc.
    pub fn reset_session(&mut self) {
        self.data_explorer.reset_session();
        self.combat_log.reset_session();
        // Note: encounter_builder and effects_editor are not reset since they
        // contain user configuration, not session-specific data
    }
}

/// Main application tabs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MainTab {
    #[default]
    DataExplorer,
    Overlays,
    EncounterBuilder,
    Effects,
    /// The three history views: per-boss pulls, per-run recaps, the log library
    BossStats,
    RaidSummaries,
    LogManagement,
    /// Character roster and operation lockouts
    Characters,
    /// Warzone and arena record, K/D and trends
    PvpStats,
    /// Training dummy kills by discipline
    DummyStats,
    /// Application settings page
    Settings,
}

impl MainTab {
    /// Which history view this tab shows, if any
    pub fn history_view(self) -> Option<history::HistorySubTab> {
        match self {
            MainTab::BossStats => Some(history::HistorySubTab::Overview),
            MainTab::RaidSummaries => Some(history::HistorySubTab::Operations),
            MainTab::LogManagement => Some(history::HistorySubTab::Library),
            MainTab::PvpStats => Some(history::HistorySubTab::Pvp),
            MainTab::DummyStats => Some(history::HistorySubTab::Dummy),
            _ => None,
        }
    }

    /// Convert to string ID used in old code
    pub fn as_str(&self) -> &'static str {
        match self {
            MainTab::DataExplorer => "explorer",
            MainTab::Overlays => "overlays",
            MainTab::EncounterBuilder => "timers",
            MainTab::Effects => "effects",
            MainTab::BossStats => "boss_stats",
            MainTab::RaidSummaries => "raid_summaries",
            MainTab::LogManagement => "log_management",
            MainTab::Characters => "characters",
            MainTab::PvpStats => "pvp_stats",
            MainTab::DummyStats => "dummy_stats",
            MainTab::Settings => "settings",
        }
    }
}

/// Data Explorer view mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    #[default]
    Overview,
    Charts,
    CombatLog,
    Detailed(DataTab),
    Rotation,
    Usage,
    Uptime,
    RaidDebuffs,
    Aggro,
}

impl ViewMode {
    /// Get the DataTab if in Detailed mode, otherwise None
    pub fn tab(&self) -> Option<DataTab> {
        match self {
            ViewMode::Detailed(tab) => Some(*tab),
            _ => None,
        }
    }
}

/// Sort column for ability breakdown table
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortColumn {
    Target,
    Ability,
    #[default]
    Total,
    Percent,
    Rate,
    Hits,
    Avg,
    CritPct,
    MissPct,
    AvgHit,
    AvgCrit,
    Activations,
    Effective,
    EffectivePct,
    ShieldTotal,
    Sps,
    AttackType,
    DamageType,
    ShldPct,
    Absorbed,
    MaxHit,
    AvgPerActivation,
    AvgPerActivationEff,
}

/// Sort column for ability usage table
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageSortColumn {
    Ability,
    #[default]
    CastCount,
    FirstCast,
    LastCast,
    AvgTime,
    MedianTime,
    MinTime,
    MaxTime,
}

/// Sort direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
    Desc,
    Asc,
}

impl SortDirection {
    pub fn sql(&self) -> &'static str {
        match self {
            SortDirection::Asc => "ASC",
            SortDirection::Desc => "DESC",
        }
    }

    pub fn toggle(&self) -> Self {
        match self {
            SortDirection::Asc => SortDirection::Desc,
            SortDirection::Desc => SortDirection::Asc,
        }
    }
}

/// Sort column for combat log viewer (server-side sort for paginated virtual scrolling)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatLogSortColumn {
    #[default]
    Time,
    Source,
    Target,
    Type,
    Ability,
    Effect,
    Value,
    Absorbed,
    Overheal,
    Threat,
}

impl CombatLogSortColumn {
    pub fn sql_column(&self) -> &'static str {
        match self {
            CombatLogSortColumn::Time => "combat_time_secs",
            CombatLogSortColumn::Source => "source_name",
            CombatLogSortColumn::Target => "target_name",
            CombatLogSortColumn::Type => "effect_type_id",
            CombatLogSortColumn::Ability => "ability_name",
            CombatLogSortColumn::Effect => "effect_name",
            CombatLogSortColumn::Value => "COALESCE(dmg_effective, 0) + COALESCE(heal_effective, 0)",
            CombatLogSortColumn::Absorbed => "COALESCE(dmg_absorbed, 0)",
            CombatLogSortColumn::Overheal => "GREATEST(COALESCE(heal_amount, 0) - COALESCE(heal_effective, 0), 0)",
            CombatLogSortColumn::Threat => "COALESCE(threat, 0.0)",
        }
    }
}

/// Data Explorer session state
#[derive(Debug, Clone, PartialEq)]
pub struct DataExplorerState {
    /// Selected encounter index
    pub selected_encounter: Option<u32>,
    /// Current view mode (Overview, Charts, CombatLog, Detailed)
    pub view_mode: ViewMode,
    /// Selected source entity for detailed breakdown
    pub selected_source: Option<String>,
    /// Instance id of the selected source when it is one NPC spawn among same-named copies.
    /// `None` for players (and NPCs selected by name), which keeps queries name-keyed.
    pub selected_source_id: Option<i64>,
    /// Breakdown mode toggles
    pub breakdown_mode: BreakdownMode,
    /// Players-only filter
    pub show_players_only: bool,
    /// Bosses-only filter in sidebar
    pub show_only_bosses: bool,
    /// Parsely percentile badges hidden (display only)
    pub hide_percentiles: bool,
    /// Sort column for ability table
    pub sort_column: SortColumn,
    /// Sort direction
    pub sort_direction: SortDirection,
    /// Collapsed sections in sidebar (set of section names: "Operations", "Flashpoints", etc.)
    pub collapsed_sections: std::collections::HashSet<String>,
    /// Timeline selection (time range filter)
    pub time_range: TimeRange,
    /// Selected anchor ability for rotation view (persists across encounters)
    pub selected_rotation_anchor: Option<i64>,
    /// Sort column for usage table
    pub usage_sort_column: UsageSortColumn,
    /// Sort direction for usage table
    pub usage_sort_direction: SortDirection,
    /// Whether live mode auto-activates on combat start (loaded from config)
    pub auto_live: bool,
    /// Overview table column layouts (loaded from config)
    pub overview_columns: crate::OverviewLayouts,
    /// Raid Debuffs and Aggro tab controls
    pub enemy_views: EnemyViewsState,
}

/// Controls of the Raid Debuffs and Aggro tabs. Picks are enemy names: one
/// missing from the encounter shown is ignored, and holds again where it exists.
#[derive(Debug, Clone, PartialEq)]
pub struct EnemyViewsState {
    /// Hide adds in both tabs
    pub bosses_only: bool,
    /// Raid Debuffs timeline: a row (enemy), a column (debuff), or a cell (both)
    pub debuff_enemy: Option<String>,
    pub debuff: Option<RaidDebuff>,
    /// Aggro: the enemy the tab is narrowed to
    pub aggro_enemy: Option<String>,
    /// Aggro timeline lanes are players instead of enemies
    pub aggro_by_player: bool,
    /// Aggro event list filter; None shows every kind
    pub aggro_event_kind: Option<ThreatEventKind>,
}

impl Default for EnemyViewsState {
    fn default() -> Self {
        Self {
            bosses_only: true,
            debuff_enemy: None,
            debuff: None,
            aggro_enemy: None,
            aggro_by_player: false,
            aggro_event_kind: None,
        }
    }
}

impl Default for DataExplorerState {
    fn default() -> Self {
        Self {
            selected_encounter: None,
            view_mode: ViewMode::default(),
            selected_source: None,
            selected_source_id: None,
            breakdown_mode: BreakdownMode::ability_only(),
            show_players_only: true,
            show_only_bosses: false,
            hide_percentiles: false,
            sort_column: SortColumn::default(),
            sort_direction: SortDirection::default(),
            collapsed_sections: std::collections::HashSet::new(),
            time_range: TimeRange::default(),
            selected_rotation_anchor: None,
            usage_sort_column: UsageSortColumn::default(),
            usage_sort_direction: SortDirection::default(),
            auto_live: false,
            overview_columns: Default::default(),
            enemy_views: EnemyViewsState::default(),
        }
    }
}

impl DataExplorerState {
    /// Reset session-specific state (encounter selection, time range, etc.)
    /// while preserving user preferences (show_only_bosses, view_mode, sort settings).
    pub fn reset_session(&mut self) {
        self.selected_encounter = None;
        self.selected_source = None;
        self.selected_source_id = None;
        self.time_range = TimeRange::default();
        self.selected_rotation_anchor = None;
        // Preserve: show_only_bosses, view_mode, breakdown_mode, show_players_only,
        // sort_column, sort_direction, collapsed_sections
    }
}

/// Combat Log session state (filters, scroll position, etc.)
#[derive(Debug, Clone, PartialEq)]
pub struct CombatLogSessionState {
    /// Current encounter index being viewed
    pub encounter_idx: Option<u32>,
    /// Source filter
    pub source_filter: Option<String>,
    /// Target filter
    pub target_filter: Option<String>,
    /// Search text
    pub search_text: String,
    /// Event categories shown; empty = all
    pub event_categories: Vec<LogEventCategory>,
    /// Show IDs toggle (IMPORTANT: now persisted!)
    pub show_ids: bool,
    /// Scroll position
    pub scroll_offset: f64,
    /// Sort column
    pub sort_column: CombatLogSortColumn,
    /// Sort direction
    pub sort_direction: SortDirection,
}

impl Default for CombatLogSessionState {
    fn default() -> Self {
        Self {
            encounter_idx: None,
            source_filter: None,
            target_filter: None,
            search_text: String::new(),
            event_categories: Vec::new(),
            show_ids: true,
            scroll_offset: 0.0,
            sort_column: CombatLogSortColumn::default(),
            sort_direction: SortDirection::default(),
        }
    }
}

impl CombatLogSessionState {
    /// Reset session-specific state (encounter, filters, scroll position)
    /// while preserving user preferences (show_ids, filter toggles).
    pub fn reset_session(&mut self) {
        self.encounter_idx = None;
        self.source_filter = None;
        self.target_filter = None;
        self.search_text = String::new();
        self.scroll_offset = 0.0;
        // Preserve: show_ids, event_categories
    }
}

/// Encounter Builder session state
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EncounterBuilderState {
    /// Selected area file path
    pub selected_area_path: Option<String>,
    /// Selected area name (for display)
    pub selected_area_name: Option<String>,
    /// Expanded boss name
    pub expanded_boss: Option<String>,
    /// Active tab within boss editor (timers, phases, counters, etc.)
    pub active_boss_tab: Option<String>,
    /// Filter text in area sidebar
    pub area_filter: String,
    /// Expanded timer ID within Timers tab
    pub expanded_timer: Option<String>,
    /// Expanded phase ID within Phases tab
    pub expanded_phase: Option<String>,
    /// Expanded counter ID within Counters tab
    pub expanded_counter: Option<String>,
    /// Expanded challenge ID within Challenges tab
    pub expanded_challenge: Option<String>,
    /// Expanded entity ID within Entities tab
    pub expanded_entity: Option<String>,
    /// Whether to hide disabled timers in the Timers tab
    pub hide_disabled_timers: bool,
    /// Whether to hide disabled phases in the Phases tab
    pub hide_disabled_phases: bool,
    /// Whether to hide disabled counters in the Counters tab
    pub hide_disabled_counters: bool,
    /// Whether the area sidebar is folded
    pub sidebar_collapsed: bool,
}

/// Effects Editor session state
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectsEditorState {
    /// Expanded effect ID
    pub expanded_effect: Option<String>,
    /// Search query
    pub search_query: String,
    /// Whether to hide disabled effects
    pub hide_disabled_effects: bool,
    /// Sidebar filters
    pub filters: EffectFilters,
    /// Whether the filter sidebar is folded
    pub sidebar_collapsed: bool,
}

/// Which effects the class filter lists: those tied to disciplines, the
/// general ones (no discipline set, live on every class), or both
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum EffectScope {
    #[default]
    Both,
    ClassOnly,
    GeneralOnly,
}

/// Effects editor filter bar selection; empty / `None` = all
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EffectFilters {
    /// Origin label ("Custom", "Modified", "Built-in")
    pub origin: Option<String>,
    /// Display target labels, or the "no overlay" entry; any of them matches
    pub overlays: Vec<String>,
    /// Class pair label, "All classes" or "Unassigned"; `None` = the logged-in
    /// character's class
    pub class: Option<String>,
    /// Class-specific effects, general ones, or both
    pub scope: EffectScope,
}
