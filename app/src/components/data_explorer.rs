//! Data Explorer Panel Component
//!
//! Displays detailed ability breakdown and DPS analysis for encounters.
//! Uses DataFusion SQL queries over parquet files for historical data.

use dioxus::prelude::*;
use std::collections::{HashMap, HashSet};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local as spawn;

use crate::api::{
    self, AbilityBreakdown, AbilityUsageRow, DamageTakenSummary, EncounterTimeline,
    EntityBreakdown, NpcHealthRow, PlayerDeath, RaidOverviewRow, TimeRange,
};
use crate::components::ability_icon::AbilityIcon;
use crate::components::charts_panel::ChartsPanel;
use crate::components::class_icons::{get_class_icon, get_role_icon};
use crate::components::combat_log::CombatLog;
use crate::components::encounter_types::{ChallengeSummary, EncounterSummary, UploadState, group_by_area};
use crate::components::phase_timeline::PhaseTimelineFilter;
use crate::components::rotation_view::RotationView;
use crate::components::{ToastSeverity, use_parsely_upload, use_toast};
use crate::types::{BreakdownMode, CombatLogSessionState, DataTab, SortColumn, SortDirection, UiSessionState, UsageSortColumn, ViewMode};
use crate::utils::js_set;
use baras_types::formatting;

// ─────────────────────────────────────────────────────────────────────────────
// Local Types (not persisted)
// ─────────────────────────────────────────────────────────────────────────────

/// Loading state for async operations
#[derive(Clone, PartialEq, Default)]
enum LoadState {
    #[default]
    Idle,
    Loading,
    Loaded,
    Error(String),
}

/// Sortable columns for the overview table
#[derive(Clone, Copy, PartialEq, Default)]
enum OverviewSort {
    #[default]
    DPS,
    DamageTotal,
    ThreatTotal,
    TPS,
    DamageTakenTotal,
    DTPS,
    APS,
    HealingTotal,
    HPS,
    HealingPct,
    EHPS,
    ShieldingTotal,
    SPS,
    APM,
}

impl OverviewSort {
    fn extract(self, row: &RaidOverviewRow) -> f64 {
        match self {
            Self::DamageTotal => row.damage_total,
            Self::DPS => row.dps,
            Self::ThreatTotal => row.threat_total,
            Self::TPS => row.tps,
            Self::DamageTakenTotal => row.damage_taken_total,
            Self::DTPS => row.dtps,
            Self::APS => row.aps,
            Self::HealingTotal => row.healing_total,
            Self::HPS => row.hps,
            Self::HealingPct => row.healing_pct,
            Self::EHPS => row.ehps,
            Self::ShieldingTotal => row.shielding_given_total,
            Self::SPS => row.sps,
            Self::APM => row.apm,
        }
    }
}

/// Overview table data with pre-calculated totals
#[derive(Clone, PartialEq, Default)]
struct OverviewTableData {
    rows: Vec<RaidOverviewRow>,
    total_damage: f64,
    total_dps: f64,
    total_threat: f64,
    total_tps: f64,
    total_damage_taken: f64,
    total_dtps: f64,
    total_aps: f64,
    total_shielding: f64,
    total_sps: f64,
    total_healing: f64,
    total_hps: f64,
    total_ehps: f64,
}

// ─────────────────────────────────────────────────────────────────────────────
// ECharts JS Interop for Overview Donut Charts
// ─────────────────────────────────────────────────────────────────────────────

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = echarts, js_name = init)]
    fn echarts_init(dom: &web_sys::Element) -> JsValue;

    #[wasm_bindgen(js_namespace = echarts, js_name = getInstanceByDom)]
    fn echarts_get_instance(dom: &web_sys::Element) -> JsValue;
}

fn init_overview_chart(element_id: &str) -> Option<JsValue> {
    let window = web_sys::window()?;
    let document = window.document()?;
    let element = document.get_element_by_id(element_id)?;

    // Check if instance already exists
    let existing = echarts_get_instance(&element);
    if !existing.is_null() && !existing.is_undefined() {
        return Some(existing);
    }

    Some(echarts_init(&element))
}

fn set_chart_option(chart: &JsValue, option: &JsValue) {
    let set_option = js_sys::Reflect::get(chart, &JsValue::from_str("setOption"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok());

    if let Some(func) = set_option {
        let _ = func.call1(chart, option);
    }
}

fn resize_overview_chart(chart: &JsValue) {
    let resize = js_sys::Reflect::get(chart, &JsValue::from_str("resize"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok());

    if let Some(func) = resize {
        let _ = func.call0(chart);
    }
}

fn dispose_overview_chart(element_id: &str) {
    if let Some(window) = web_sys::window()
        && let Some(document) = window.document()
        && let Some(element) = document.get_element_by_id(element_id)
    {
        let instance = echarts_get_instance(&element);
        if !instance.is_null() && !instance.is_undefined() {
            let dispose = js_sys::Reflect::get(&instance, &JsValue::from_str("dispose"))
                .ok()
                .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
            if let Some(func) = dispose {
                let _ = func.call0(&instance);
            }
        }
    }
}

/// Dispose all overview donut charts - call when leaving overview or changing encounters
fn dispose_all_overview_charts() {
    dispose_overview_chart("donut-damage");
    dispose_overview_chart("donut-threat");
    dispose_overview_chart("donut-healing");
    dispose_overview_chart("donut-taken");
}

/// Resize all overview donut charts - call on window resize
fn resize_all_overview_charts() {
    for id in [
        "donut-damage",
        "donut-threat",
        "donut-healing",
        "donut-taken",
    ] {
        if let Some(window) = web_sys::window()
            && let Some(document) = window.document()
            && let Some(element) = document.get_element_by_id(id)
        {
            let instance = echarts_get_instance(&element);
            if !instance.is_null() && !instance.is_undefined() {
                resize_overview_chart(&instance);
            }
        }
    }
}

/// Build donut chart option for ECharts
fn build_donut_option(title: &str, data: &[(String, f64)], color: &str) -> JsValue {
    let obj = js_sys::Object::new();

    // Title
    let title_obj = js_sys::Object::new();
    js_set(&title_obj, "text", &JsValue::from_str(title));
    js_set(&title_obj, "left", &JsValue::from_str("center"));
    js_set(&title_obj, "top", &JsValue::from_str("5"));
    let title_style = js_sys::Object::new();
    js_set(&title_style, "color", &JsValue::from_str("#e0e0e0"));
    js_set(&title_style, "fontSize", &JsValue::from_f64(13.0));
    js_set(&title_style, "fontWeight", &JsValue::from_str("600"));
    js_set(&title_obj, "textStyle", &title_style);
    js_set(&obj, "title", &title_obj);

    // Tooltip
    let tooltip = js_sys::Object::new();
    js_set(&tooltip, "trigger", &JsValue::from_str("item"));
    js_set(&tooltip, "formatter", &JsValue::from_str("{b}: {c} ({d}%)"));
    js_set(&obj, "tooltip", &tooltip);

    // Series (donut)
    let series_arr = js_sys::Array::new();
    let series = js_sys::Object::new();
    js_set(&series, "type", &JsValue::from_str("pie"));
    let radius_arr = js_sys::Array::new();
    radius_arr.push(&JsValue::from_str("35%"));
    radius_arr.push(&JsValue::from_str("65%"));
    js_set(&series, "radius", &radius_arr);
    let center_arr = js_sys::Array::new();
    center_arr.push(&JsValue::from_str("50%"));
    center_arr.push(&JsValue::from_str("55%"));
    js_set(&series, "center", &center_arr);

    // Label formatting - outside labels with overflow handling
    let label = js_sys::Object::new();
    js_set(&label, "show", &JsValue::TRUE);
    js_set(&label, "formatter", &JsValue::from_str("{b}"));
    js_set(&label, "color", &JsValue::from_str("#ccc"));
    js_set(&label, "fontSize", &JsValue::from_f64(10.0));
    js_set(&label, "overflow", &JsValue::from_str("truncate"));
    js_set(&label, "ellipsis", &JsValue::from_str(".."));
    js_set(&series, "label", &label);

    // Label layout - keep labels within chart bounds
    let label_layout = js_sys::Object::new();
    js_set(&label_layout, "hideOverlap", &JsValue::TRUE);
    js_set(&series, "labelLayout", &label_layout);

    // Emphasis
    let emphasis = js_sys::Object::new();
    let emph_label = js_sys::Object::new();
    js_set(&emph_label, "show", &JsValue::TRUE);
    js_set(&emph_label, "fontSize", &JsValue::from_f64(12.0));
    js_set(&emph_label, "fontWeight", &JsValue::from_str("bold"));
    js_set(&emphasis, "label", &emph_label);
    js_set(&series, "emphasis", &emphasis);

    // Item style with base color
    let item_style = js_sys::Object::new();
    js_set(&item_style, "borderColor", &JsValue::from_str("#1a1a1a"));
    js_set(&item_style, "borderWidth", &JsValue::from_f64(2.0));
    js_set(&series, "itemStyle", &item_style);

    // Color palette based on base color with variations
    let colors = generate_color_palette(color, data.len());
    let color_arr = js_sys::Array::new();
    for c in colors {
        color_arr.push(&JsValue::from_str(&c));
    }
    js_set(&obj, "color", &color_arr);

    // Data
    let data_arr = js_sys::Array::new();
    for (name, value) in data {
        let item = js_sys::Object::new();
        js_set(&item, "name", &JsValue::from_str(name));
        js_set(&item, "value", &JsValue::from_f64(*value));
        data_arr.push(&item);
    }
    js_set(&series, "data", &data_arr);

    series_arr.push(&series);
    js_set(&obj, "series", &series_arr);

    // No animation for faster renders
    js_set(&obj, "animation", &JsValue::FALSE);

    obj.into()
}

/// Generate a color palette with variations from a base HSL color
fn generate_color_palette(base_color: &str, count: usize) -> Vec<String> {
    // Parse base HSL values from color string like "hsl(0, 70%, 60%)"
    let (h, s, l) = parse_hsl(base_color).unwrap_or((0.0, 70.0, 60.0));

    let mut colors = Vec::with_capacity(count);
    for i in 0..count {
        // Vary lightness and slightly vary hue for each slice
        let hue_offset = (i as f64 * 15.0) % 360.0;
        let light_offset = (i as f64 * 5.0) % 20.0 - 10.0;
        let new_h = (h + hue_offset) % 360.0;
        let new_l = (l + light_offset).clamp(35.0, 75.0);
        colors.push(format!("hsl({:.0}, {:.0}%, {:.0}%)", new_h, s, new_l));
    }
    colors
}

fn parse_hsl(color: &str) -> Option<(f64, f64, f64)> {
    // Parse "hsl(h, s%, l%)" format
    let color = color.trim();
    if !color.starts_with("hsl(") || !color.ends_with(")") {
        return None;
    }
    let inner = &color[4..color.len() - 1];
    let parts: Vec<&str> = inner.split(',').collect();
    if parts.len() != 3 {
        return None;
    }
    let h: f64 = parts[0].trim().parse().ok()?;
    let s: f64 = parts[1].trim().trim_end_matches('%').parse().ok()?;
    let l: f64 = parts[2].trim().trim_end_matches('%').parse().ok()?;
    Some((h, s, l))
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────



// group_by_area is now in encounter_types module

// ─────────────────────────────────────────────────────────────────────────────
// Component
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct DataExplorerProps {
    /// Unified UI session state (includes all persisted state for this panel)
    pub state: Signal<UiSessionState>,
}

#[component]
pub fn DataExplorerPanel(mut props: DataExplorerProps) -> Element {
    // Encounter selection state
    let mut encounters = use_signal(Vec::<EncounterSummary>::new);
    
    // Extract persisted state fields into local signals for easier access
    // Initialize from parent state to support navigation from other components
    let mut selected_encounter = use_signal(|| props.state.read().data_explorer.selected_encounter);
    let mut view_mode = use_signal(|| props.state.read().data_explorer.view_mode);
    let mut selected_source = use_signal(|| props.state.read().data_explorer.selected_source.clone());
    let mut breakdown_mode = use_signal(|| props.state.read().data_explorer.breakdown_mode);
    let mut show_players_only = use_signal(|| props.state.read().data_explorer.show_players_only);
    let mut show_only_bosses = use_signal(|| props.state.read().data_explorer.show_only_bosses);
    let mut sort_column = use_signal(|| props.state.read().data_explorer.sort_column);
    let mut sort_direction = use_signal(|| props.state.read().data_explorer.sort_direction);
    let mut collapsed_sections = use_signal(|| props.state.read().data_explorer.collapsed_sections.clone());
    let selected_rotation_anchor = use_signal(|| props.state.read().data_explorer.selected_rotation_anchor);
    let mut usage_sort_column = use_signal(|| props.state.read().data_explorer.usage_sort_column);
    let mut usage_sort_direction = use_signal(|| props.state.read().data_explorer.usage_sort_direction);
    let mut usage_selected_abilities = use_signal(Vec::<(i64, &'static str)>::new);

    // Overview table sort (not persisted - default DPS descending)
    let mut overview_sort_col = use_signal(OverviewSort::default);
    let mut overview_sort_asc = use_signal(|| false);

    // Sidebar collapse states (not persisted - always start expanded)
    let mut sidebar_collapsed = use_signal(|| false);
    let mut entity_collapsed = use_signal(|| false);
    let mut overview_fullscreen = use_signal(|| false);

    // Parsely upload state for per-encounter uploads
    let mut parsely_upload = use_parsely_upload();
    let mut upload_states = use_signal(HashMap::<u64, UploadState>::new);

    // Combat log state is a separate signal that CombatLog component will modify
    let mut combat_log_state = use_signal(|| props.state.read().combat_log.clone());
    
    // Time range is persisted - restore from saved state
    let mut time_range = use_signal(|| props.state.read().data_explorer.time_range);
    
    // Track previous encounter to detect actual changes vs. initial mount
    // Start with None to trigger initial load
    let mut prev_encounter = use_signal(|| None::<u32>);
    
    // Sync local signals back to unified state when they change
    // Read all values first to create subscriptions, then write to parent state
    // NOTE: show_only_bosses is NOT synced here to avoid bidirectional sync loop.
    // It's synced in the onchange handler instead.
    use_effect(move || {
        let enc = *selected_encounter.read();
        let vm = *view_mode.read();
        let src = selected_source.read().clone();
        let bm = *breakdown_mode.read();
        let players = *show_players_only.read();
        let col = *sort_column.read();
        let dir = *sort_direction.read();
        let sections = collapsed_sections.read().clone();
        let tr = *time_range.read();
        let anchor = *selected_rotation_anchor.read();
        let u_col = *usage_sort_column.read();
        let u_dir = *usage_sort_direction.read();
        let combat = combat_log_state.read().clone();
        
        if let Ok(mut state) = props.state.try_write() {
            state.data_explorer.selected_encounter = enc;
            state.data_explorer.view_mode = vm;
            state.data_explorer.selected_source = src;
            state.data_explorer.breakdown_mode = bm;
            state.data_explorer.show_players_only = players;
            state.data_explorer.sort_column = col;
            state.data_explorer.sort_direction = dir;
            state.data_explorer.collapsed_sections = sections;
            state.data_explorer.time_range = tr;
            state.data_explorer.selected_rotation_anchor = anchor;
            state.data_explorer.usage_sort_column = u_col;
            state.data_explorer.usage_sort_direction = u_dir;
            state.combat_log = combat;
        }
    });
    
    // Sync show_only_bosses from parent state when it changes (e.g., config loaded at startup)
    // This handles the race condition where the component mounts before async config loading completes
    // NOTE: We only sync parent→local here. Local→parent sync happens in the onchange handler
    // to avoid a bidirectional sync loop that causes "sticky" toggle behavior.
    use_effect(move || {
        let parent_bosses = props.state.read().data_explorer.show_only_bosses;
        if *show_only_bosses.read() != parent_bosses {
            show_only_bosses.set(parent_bosses);
        }
    });
    
    // NOTE: selected_encounter is initialized from parent state on mount (line 332).
    // This handles navigation from external components that set selected_encounter before mounting.
    // We do NOT have a continuous parent→local sync effect here because it creates
    // a bidirectional sync loop with the local→parent sync above, causing
    // encounter clicks in the sidebar to be ignored/sticky.
    
    // Sync show_ids from parent state when it changes (e.g., config loaded at startup)
    use_effect(move || {
        let parent_show_ids = props.state.read().combat_log.show_ids;
        if combat_log_state.read().show_ids != parent_show_ids {
            combat_log_state.write().show_ids = parent_show_ids;
        }
    });

    // Local player name (fetched once on mount for auto-selection)
    let local_player_name = use_signal(|| None::<String>);
    {
        let mut local_player_name = local_player_name;
        use_future(move || async move {
            if let Some(info) = api::get_session_info().await {
                let _ = local_player_name.try_write().map(|mut w| *w = info.player_name);
            }
        });
    }

    // Query result state (not persisted)
    let mut abilities = use_signal(Vec::<AbilityBreakdown>::new);
    let mut entities = use_signal(Vec::<EntityBreakdown>::new);
    // Per-entity damage totals for Rotation/Usage/Charts color coding (name → damage total)
    let mut entity_dmg_totals = use_signal(HashMap::<String, f64>::new);

    // Loading states (not persisted)
    let mut timeline_state = use_signal(LoadState::default);
    let mut content_state = use_signal(LoadState::default);
    // Generation counter to discard stale async results on rapid encounter switching
    let mut load_generation = use_signal(|| 0u32);

    // Timeline state
    let mut timeline = use_signal(|| None::<EncounterTimeline>);

    // Overview data
    let mut overview_data = use_signal(Vec::<RaidOverviewRow>::new);
    let mut player_deaths = use_signal(Vec::<PlayerDeath>::new);
    let mut npc_health = use_signal(Vec::<NpcHealthRow>::new);
    // Track last (encounter, time_range, was_overview) we fetched overview data for (prevents re-fetch loops)
    // The bool tracks whether deaths/NPC HP were also loaded (only on Overview tab)
    let mut last_overview_fetch = use_signal(|| None::<(Option<u32>, TimeRange, bool)>);

    // Death target filter - set when clicking a death to filter combat log by target
    let mut death_target_filter = use_signal(|| None::<String>);

    // Memoized overview table data (rows + totals) - prevents recomputation on every render
    let overview_table_data = use_memo(move || {
        let data = overview_data.read();
        let sort_col = *overview_sort_col.read();
        let sort_asc = *overview_sort_asc.read();
        let mut rows: Vec<RaidOverviewRow> = data
            .iter()
            .filter(|r| r.entity_type == "Player" || r.entity_type == "Companion")
            .cloned()
            .collect();

        // Sort rows by selected column
        rows.sort_by(|a, b| {
            let cmp = sort_col.extract(a).partial_cmp(&sort_col.extract(b)).unwrap_or(std::cmp::Ordering::Equal);
            if sort_asc { cmp } else { cmp.reverse() }
        });

        // Calculate totals
        OverviewTableData {
            total_damage: rows.iter().map(|r| r.damage_total).sum(),
            total_dps: rows.iter().map(|r| r.dps).sum(),
            total_threat: rows.iter().map(|r| r.threat_total).sum(),
            total_tps: rows.iter().map(|r| r.tps).sum(),
            total_damage_taken: rows.iter().map(|r| r.damage_taken_total).sum(),
            total_dtps: rows.iter().map(|r| r.dtps).sum(),
            total_aps: rows.iter().map(|r| r.aps).sum(),
            total_shielding: rows.iter().map(|r| r.shielding_given_total).sum(),
            total_sps: rows.iter().map(|r| r.sps).sum(),
            total_healing: rows.iter().map(|r| r.healing_total).sum(),
            total_hps: rows.iter().map(|r| r.hps).sum(),
            total_ehps: rows.iter().map(|r| r.ehps).sum(),
            rows,
        }
    });

    // Memoized chart data for overview donut charts (derived from table data)
    let chart_data = use_memo(move || {
        let table_data = overview_table_data.read();

        let damage_data: Vec<(String, f64)> = table_data
            .rows
            .iter()
            .filter(|r| r.damage_total > 0.0)
            .map(|r| (r.name.clone(), r.damage_total))
            .collect();
        let threat_data: Vec<(String, f64)> = table_data
            .rows
            .iter()
            .filter(|r| r.threat_total > 0.0)
            .map(|r| (r.name.clone(), r.threat_total))
            .collect();
        let healing_data: Vec<(String, f64)> = table_data
            .rows
            .iter()
            .filter(|r| r.healing_effective > 0.0)
            .map(|r| (r.name.clone(), r.healing_effective))
            .collect();
        let taken_data: Vec<(String, f64)> = table_data
            .rows
            .iter()
            .filter(|r| r.damage_taken_total > 0.0)
            .map(|r| (r.name.clone(), r.damage_taken_total))
            .collect();

        (damage_data, threat_data, healing_data, taken_data)
    });

    // Effect to initialize/update overview donut charts when data changes
    use_effect(move || {
        let (damage_data, threat_data, healing_data, taken_data) = chart_data();
        let is_overview = matches!(*view_mode.read(), ViewMode::Overview);

        // Dispose charts when not showing overview (cleanup old instances)
        if !is_overview {
            dispose_all_overview_charts();
            return;
        }

        // Only initialize charts when overview is visible and we have an encounter
        if selected_encounter.read().is_none() {
            return;
        }

        spawn(async move {
            // Small delay to ensure DOM is ready
            gloo_timers::future::TimeoutFuture::new(100).await;

            // Damage chart
            if !damage_data.is_empty()
                && let Some(chart) = init_overview_chart("donut-damage")
            {
                let opt = build_donut_option("Damage", &damage_data, "hsl(0, 70%, 60%)");
                set_chart_option(&chart, &opt);
                resize_overview_chart(&chart);
            }

            // Threat chart
            if !threat_data.is_empty()
                && let Some(chart) = init_overview_chart("donut-threat")
            {
                let opt = build_donut_option("Threat", &threat_data, "hsl(210, 70%, 55%)");
                set_chart_option(&chart, &opt);
                resize_overview_chart(&chart);
            }

            // Healing chart (effective healing)
            if !healing_data.is_empty()
                && let Some(chart) = init_overview_chart("donut-healing")
            {
                let opt =
                    build_donut_option("Effective Healing", &healing_data, "hsl(120, 50%, 50%)");
                set_chart_option(&chart, &opt);
                resize_overview_chart(&chart);
            }

            // Damage Taken chart
            if !taken_data.is_empty()
                && let Some(chart) = init_overview_chart("donut-taken")
            {
                let opt = build_donut_option("Damage Taken", &taken_data, "hsl(30, 70%, 55%)");
                set_chart_option(&chart, &opt);
                resize_overview_chart(&chart);
            }
        });
    });

    // Window resize listener for overview donut charts
    use_effect(|| {
        let closure = Closure::wrap(Box::new(move || {
            resize_all_overview_charts();
        }) as Box<dyn Fn()>);

        if let Some(window) = web_sys::window() {
            let _ =
                window.add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref());
        }

        closure.forget();
    });

    // Load encounter list on mount

    use_effect(move || {
        spawn(async move {
            if let Some(list) = api::get_encounter_history().await {
                let _ = encounters.try_write().map(|mut w| *w = list); // ← safe
            }
        });
    });
    // Store unlisten handle for cleanup (Tauri returns an unlisten function)
    let mut unlisten_handle = use_signal(|| None::<js_sys::Function>);

    // Listen for session updates (refresh on combat end, file load)
    use_future(move || async move {
        let closure = Closure::new(move |event: JsValue| {
            // Extract payload from event object (Tauri events have { payload: "..." } structure)
            if let Ok(payload) = js_sys::Reflect::get(&event, &JsValue::from_str("payload"))
                && let Some(event_type) = payload.as_string()
                && (event_type.contains("CombatEnded") || event_type.contains("FileLoaded"))
            {
                // Reset selection only on file load (new file invalidates old encounter indices)
                // Use try_write to handle signal being dropped when component unmounts
                if event_type.contains("FileLoaded") {
                    let _ = selected_encounter.try_write().map(|mut w| *w = None);
                    // Reset selected player on new file (different players)
                    let _ = selected_source.try_write().map(|mut w| *w = None);
                }
                spawn(async move {
                    // Refresh encounter list
                    if let Some(list) = api::get_encounter_history().await {
                        let _ = encounters.try_write().map(|mut w| *w = list);
                    }
                });
            }
        });
        let handle = api::tauri_listen("session-updated", &closure).await;
        // Store the unlisten function for cleanup
        if let Ok(func) = handle.dyn_into::<js_sys::Function>() {
            let _ = unlisten_handle.try_write().map(|mut w| *w = Some(func));
        }
        closure.forget();
    });

    // Listen for parsely upload success (DOM CustomEvent from ParselyUploadModal)
    use_future(move || async move {
        if let Some(window) = web_sys::window() {
            let closure = Closure::<dyn Fn(web_sys::Event)>::new(move |_event: web_sys::Event| {
                // Refresh encounter list to pick up persisted parsely links
                spawn(async move {
                    if let Some(list) = api::get_encounter_history().await {
                        let _ = encounters.try_write().map(|mut w| *w = list);
                    }
                });
            });
            let _ = window.add_event_listener_with_callback(
                "parsely-upload-success",
                closure.as_ref().unchecked_ref(),
            );
            closure.forget();
        }
    });

    // Cleanup on component unmount
    use_drop(move || {
        dispose_all_overview_charts();
        // Call unlisten to clean up the event listener
        if let Some(func) = unlisten_handle.peek().as_ref() {
            let _ = func.call0(&JsValue::NULL);
        }
    });

    // Load timeline when encounter changes - prerequisite for all data loading
    // Uses generation counter to discard stale async results on rapid switching
    use_effect(move || {
        let idx = *selected_encounter.read();
        let prev_idx = *prev_encounter.peek();
        
        // Check if this is an actual encounter change vs. initial mount with same encounter
        let is_encounter_change = idx != prev_idx;
        if is_encounter_change {
            prev_encounter.set(idx);
        }

        // Dispose charts immediately when encounter changes
        dispose_all_overview_charts();

        // Increment generation to invalidate any in-flight requests
        let generation = *load_generation.peek() + 1;
        load_generation.set(generation);

        // Clear ALL previous data when encounter changes (but preserve some state on initial mount)
        let _ = abilities.try_write().map(|mut w| *w = Vec::new());
        let _ = entities.try_write().map(|mut w| *w = Vec::new());
        let _ = overview_data.try_write().map(|mut w| *w = Vec::new());
        let _ = player_deaths.try_write().map(|mut w| *w = Vec::new());
        let _ = npc_health.try_write().map(|mut w| *w = Vec::new());
        let _ = last_overview_fetch.try_write().map(|mut w| *w = None);
        let _ = timeline.try_write().map(|mut w| *w = None);
        // Only reset time_range if encounter actually changed (not on initial mount restore)
        // Note: selected_source is intentionally preserved across encounter changes
        // so users can track the same player across pulls. If the player isn't in the
        // new encounter, auto-selection logic will fall back to the local player.
        if is_encounter_change {
            let _ = time_range
                .try_write()
                .map(|mut w| *w = TimeRange::default());
        }
        let _ = timeline_state.try_write().map(|mut w| *w = LoadState::Idle);
        let _ = content_state.try_write().map(|mut w| *w = LoadState::Idle);

        // Load timeline for selected encounter (None = live encounter)
        let _ = timeline_state
            .try_write()
            .map(|mut w| *w = LoadState::Loading);

        // Capture whether we should restore time_range after timeline loads
        let restore_time_range = !is_encounter_change;
        let saved_time_range = *time_range.peek();

        spawn(async move {
            // Check if this request is still current
            if *load_generation.peek() != generation {
                return; // Stale request, discard
            }

            match api::query_encounter_timeline(idx).await {
                Some(tl) => {
                    // Double-check generation before applying
                    if *load_generation.peek() != generation {
                        return;
                    }
                    let dur = tl.duration_secs;
                    // Only set to full range if not restoring saved state
                    if restore_time_range && saved_time_range.end > 0.0 {
                        // Keep the saved time_range (already set from init)
                    } else {
                        let _ = time_range
                            .try_write()
                            .map(|mut w| *w = TimeRange::full(dur));
                    }
                    let _ = timeline.try_write().map(|mut w| *w = Some(tl));
                    let _ = timeline_state
                        .try_write()
                        .map(|mut w| *w = LoadState::Loaded);
                }
                None => {
                    // Retry up to 2 times with backoff — the session may still be loading
                    let delays = [500, 1000];
                    let mut succeeded = false;
                    for delay_ms in delays {
                        if *load_generation.peek() != generation {
                            return; // Stale request
                        }
                        gloo_timers::future::TimeoutFuture::new(delay_ms).await;
                        if *load_generation.peek() != generation {
                            return; // Stale request
                        }
                        if let Some(tl) = api::query_encounter_timeline(idx).await {
                            if *load_generation.peek() != generation {
                                return;
                            }
                            let dur = tl.duration_secs;
                            if restore_time_range && saved_time_range.end > 0.0 {
                                // Keep the saved time_range
                            } else {
                                let _ = time_range
                                    .try_write()
                                    .map(|mut w| *w = TimeRange::full(dur));
                            }
                            let _ = timeline.try_write().map(|mut w| *w = Some(tl));
                            let _ = timeline_state
                                .try_write()
                                .map(|mut w| *w = LoadState::Loaded);
                            succeeded = true;
                            break;
                        }
                    }
                    if !succeeded {
                        if *load_generation.peek() != generation {
                            return;
                        }
                        let _ = timeline_state
                            .try_write()
                            .map(|mut w| *w = LoadState::Idle);
                    }
                }
            }
        });
    });

    // Load overview data when timeline is loaded and view_mode/time_range changes
    // Overview data provides class icons for all views + full data for Overview tab
    use_effect(move || {
        let idx = *selected_encounter.read();
        let mode = *view_mode.read();
        let is_overview = matches!(mode, ViewMode::Overview);
        let tr = time_range();
        let tl_state = timeline_state();

        // Only proceed when timeline is loaded
        if !matches!(tl_state, LoadState::Loaded) || idx.is_none() {
            return;
        }

        // Check if we've already fetched for this (encounter, time_range) combo
        let last = last_overview_fetch.read().clone();
        if let Some((last_idx, last_tr, had_overview)) = last {
            // Non-overview tabs only need class icons — any loaded data for this encounter is fine
            if !is_overview && last_idx == idx {
                return;
            }
            // Overview tab: skip if same encounter+range AND deaths/NPC HP were already loaded
            if is_overview && last_idx == idx && last_tr == tr && had_overview {
                return;
            }
        }

        // Set content loading state for Overview tab
        if is_overview {
            let _ = content_state
                .try_write()
                .map(|mut w| *w = LoadState::Loading);
        }

        spawn(async move {
            let full_duration = timeline.read().as_ref().map(|t| t.duration_secs);
            let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
                None
            } else {
                Some(tr)
            };

            // Use selected time range duration for rate calculations, or full fight duration
            let duration = if let Some(ref range) = tr_opt {
                Some(range.end - range.start)
            } else {
                full_duration
            };

            // Load raid overview - single attempt
            // None typically means no data available (no encounters dir, etc.) - not an error
            if let Some(data) = api::query_raid_overview(idx, tr_opt.as_ref(), duration).await {
                let _ = overview_data.try_write().map(|mut w| *w = data);
            } else {
                // Don't cache failed fetches — allow retry on next effect trigger
                if is_overview {
                    let _ = content_state
                        .try_write()
                        .map(|mut w| *w = LoadState::Loaded);
                }
                return;
            }

            // Load player deaths + NPC health (only needed for Overview tab)
            if is_overview {
                if let Some(deaths) = api::query_player_deaths(idx).await {
                    let _ = player_deaths.try_write().map(|mut w| *w = deaths);
                }
                if let Some(npcs) = api::query_npc_health(idx, tr_opt.as_ref()).await {
                    let _ = npc_health.try_write().map(|mut w| *w = npcs);
                }
                let _ = content_state
                    .try_write()
                    .map(|mut w| *w = LoadState::Loaded);
            }

            // Cache after all queries complete — bool tracks whether overview-specific data was loaded
            let _ = last_overview_fetch
                .try_write()
                .map(|mut w| *w = Some((idx, tr, is_overview)));
        });
    });

    // Lazy load: Detailed tab data (entities + abilities) for Damage/Healing/etc tabs
    use_effect(move || {
        let idx = *selected_encounter.read();
        let mode = *view_mode.read();
        let tr = time_range();
        let tl_state = timeline_state();

        // Extract tab if in detailed mode, or use Damage tab for Rotation/Usage/Charts mode
        let tab = match mode {
            ViewMode::Detailed(tab) => tab,
            ViewMode::Rotation | ViewMode::Usage | ViewMode::Charts => DataTab::Damage,
            _ => {
                // Clear detailed data when not in detailed/rotation/usage/charts mode
                // Note: selected_source is preserved so it syncs across tabs
                let _ = entities.try_write().map(|mut w| *w = Vec::new());
                let _ = abilities.try_write().map(|mut w| *w = Vec::new());
                return;
            }
        };

        // Only load when timeline is loaded and we have an encounter
        if !matches!(tl_state, LoadState::Loaded) || idx.is_none() {
            return;
        }

        let _ = content_state
            .try_write()
            .map(|mut w| *w = LoadState::Loading);

        spawn(async move {
            let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
                None
            } else {
                Some(tr)
            };

            // Load entity breakdown - single attempt
            // For Rotation/Usage/Charts, merge Damage + Healing entities so healers with 0 dmg appear
            let entity_data = if matches!(mode, ViewMode::Rotation | ViewMode::Usage | ViewMode::Charts) {
                let dmg = api::query_entity_breakdown(DataTab::Damage, idx, tr_opt.as_ref()).await.unwrap_or_default();
                let heal = api::query_entity_breakdown(DataTab::Healing, idx, tr_opt.as_ref()).await.unwrap_or_default();
                // Track per-entity damage totals for color coding
                let dmg_map: HashMap<String, f64> = dmg.iter()
                    .map(|e| (e.source_name.clone(), e.total_value))
                    .collect();
                entity_dmg_totals.set(dmg_map);
                let mut merged: HashMap<String, EntityBreakdown> = HashMap::new();
                for e in dmg.into_iter().chain(heal) {
                    merged.entry(e.source_name.clone())
                        .and_modify(|existing| {
                            existing.total_value += e.total_value;
                            existing.abilities_used = existing.abilities_used.max(e.abilities_used);
                        })
                        .or_insert(e);
                }
                let mut result: Vec<_> = merged.into_values().collect();
                result.sort_by(|a, b| b.total_value.partial_cmp(&a.total_value).unwrap_or(std::cmp::Ordering::Equal));
                result
            } else {
                match api::query_entity_breakdown(tab, idx, tr_opt.as_ref()).await {
                    Some(data) => data,
                    None => {
                        // No data available - just mark as loaded with empty data
                        let _ = content_state
                            .try_write()
                            .map(|mut w| *w = LoadState::Loaded);
                        return;
                    }
                }
            };

            // Auto-select player: keep held selection if it exists in this encounter,
            // otherwise fall back to local player, then first player
            let auto_selected = {
                let current = selected_source.read().clone();
                let players = entity_data.iter().filter(|e| e.entity_type == "Player" || e.entity_type == "Companion");
                // If we have a held selection and it exists in the new encounter, keep it
                if let Some(ref name) = current {
                    if players.clone().any(|e| &e.source_name == name) {
                        current
                    } else {
                        // Held player not in this encounter — fall back
                        let local_name = local_player_name.read();
                        if let Some(name) = local_name.as_deref() {
                            players.clone().find(|e| e.source_name == name)
                                .or_else(|| players.clone().next())
                                .map(|e| e.source_name.clone())
                        } else {
                            players.clone().next().map(|e| e.source_name.clone())
                        }
                    }
                } else {
                    // No held selection — auto-select local player or first player
                    let local_name = local_player_name.read();
                    if let Some(name) = local_name.as_deref() {
                        players.clone().find(|e| e.source_name == name)
                            .or_else(|| players.clone().next())
                            .map(|e| e.source_name.clone())
                    } else {
                        players.clone().next().map(|e| e.source_name.clone())
                    }
                }
            };

            let _ = entities.try_write().map(|mut w| *w = entity_data);

            // Load ability breakdown for selected (or auto-selected) source
            let breakdown = *breakdown_mode.read();
            if let Some(data) = api::query_breakdown(
                tab,
                idx,
                auto_selected.as_deref(),
                tr_opt.as_ref(),
                None, // No entity filter when source is selected
                Some(&breakdown),
                timeline.read().as_ref().map(|t| t.duration_secs),
            )
            .await
            {
                let _ = abilities.try_write().map(|mut w| *w = data);
            }

            // Set selected source after abilities loaded
            if selected_source.read().is_none() && auto_selected.is_some() {
                let _ = selected_source.try_write().map(|mut w| *w = auto_selected);
            }

            let _ = content_state
                .try_write()
                .map(|mut w| *w = LoadState::Loaded);
        });
    });

    // NOTE: Time range changes are now handled by the tab-specific effects above
    // They read time_range() which triggers reload when it changes

    // Reload abilities when entity filter or breakdown mode changes
    use_effect(move || {
        let players_only = *show_players_only.read();
        let breakdown = *breakdown_mode.read();
        let idx = *selected_encounter.read();
        let view = *view_mode.read();
        let src = selected_source.read().clone();
        let tr = time_range();
        let tl_state = timeline_state();

        // Extract tab if in detailed mode
        let Some(tab) = view.tab() else {
            return;
        };

        // Skip if no encounter or timeline not loaded
        if idx.is_none() || !matches!(tl_state, LoadState::Loaded) {
            return;
        }

        spawn(async move {
            // Apply entity filter only when no specific source is selected
            let entity_filter: Option<&[&str]> = if src.is_none() {
                if players_only {
                    Some(&["Player", "Companion"])
                } else {
                    Some(&["Npc"])
                }
            } else {
                None
            };
            let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
                None
            } else {
                Some(tr)
            };
            let duration = timeline.read().as_ref().map(|t| t.duration_secs);
            if let Some(data) = api::query_breakdown(
                tab,
                idx,
                src.as_deref(),
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
    });

    // DamageTaken summary (fetched when DamageTaken tab + source selected)
    let mut dt_summary: Signal<Option<DamageTakenSummary>> = use_signal(|| None);
    use_effect(move || {
        let view = *view_mode.read();
        let idx = *selected_encounter.read();
        let src = selected_source.read().clone();
        let tr = time_range();
        let tl_state = timeline_state();

        // Only fetch for DamageTaken tab with a selected source
        if !matches!(view, ViewMode::Detailed(DataTab::DamageTaken)) || src.is_none() || idx.is_none() || !matches!(tl_state, LoadState::Loaded) {
            dt_summary.set(None);
            return;
        }

        let entity_name = src.unwrap();
        spawn(async move {
            let tr_opt = if tr.start == 0.0 && tr.end == 0.0 { None } else { Some(tr) };
            // Don't filter source entity types — the target_name filter already scopes the query
            let result = api::query_damage_taken_summary(idx, &entity_name, tr_opt.as_ref(), None).await;
            let _ = dt_summary.try_write().map(|mut w| *w = result);
        });
    });

    // Filter by source when selected
    let mut on_source_click = move |name: String| {
        let idx = *selected_encounter.read();
        let mode = *view_mode.read();
        let current = selected_source.read().clone();
        let tr = time_range();

        // Toggle selection
        let new_source = if current.as_ref() == Some(&name) {
            None
        } else {
            Some(name.clone())
        };

        selected_source.set(new_source.clone());
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
    };

    // Memoized filtered history - only recomputes when encounters or filter changes
    // When filtering for boss-only, propagate is_phase_start to the next visible
    // encounter so phase boundaries aren't lost when trash encounters are hidden
    let filtered_history = use_memo(move || {
        let history = encounters();
        let bosses_only = show_only_bosses();
        if bosses_only {
            let mut pending_phase_start = false;
            history
                .into_iter()
                .filter_map(|mut e| {
                    if e.is_phase_start {
                        pending_phase_start = true;
                    }
                    if e.boss_name.is_some() {
                        if pending_phase_start {
                            e.is_phase_start = true;
                            pending_phase_start = false;
                        }
                        Some(e)
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            history
        }
    });

    // Memoized sections - groups encounters by area
    let sections = use_memo(move || {
        let filtered = filtered_history();
        group_by_area(&filtered)
            .into_iter()
            .map(|(area, diff, encs)| {
                let mut reversed: Vec<_> = encs.into_iter().cloned().collect();
                reversed.reverse();
                (area, diff, reversed)
            })
            .rev()
            .collect::<Vec<_>>()
    });

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
            .filter_map(|row| {
                row.class_icon
                    .as_ref()
                    .map(|icon| (row.name.clone(), icon.clone()))
            })
            .collect::<HashMap<String, String>>()
    });

    // Memoized role icon lookup from overview data (player name -> role_icon)
    let _role_icon_lookup = use_memo(move || {
        overview_data
            .read()
            .iter()
            .filter_map(|row| {
                row.role_icon
                    .as_ref()
                    .map(|icon| (row.name.clone(), icon.clone()))
            })
            .collect::<HashMap<String, String>>()
    });

    // Group stats for hierarchical display
    #[derive(Clone, Default, PartialEq)]
    struct GroupStats {
        target: Option<String>,
        first_hit: Option<f32>,
        total: f64,
        percent: f64,
        rate: f64,
        hits: i64,
        avg: f64,
        crit_pct: f64,
        miss_count: i64,
        activation_count: i64,
        crit_total: f64,
        effective_total: f64,
        shield_total: f64,
        shield_rate: f64,
        // DamageTaken-specific
        shield_count: i64,
        absorbed_total: f64,
    }

    // Memoized grouped abilities - groups by target when breakdown mode is enabled
    let grouped_abilities = use_memo(move || {
        let col = *sort_column.read();
        let dir = *sort_direction.read();
        let mode = *breakdown_mode.read();
        let list: Vec<AbilityBreakdown> = abilities.read().clone();

        // Sort function for abilities within groups
        let sort_abilities = |mut items: Vec<AbilityBreakdown>| -> Vec<AbilityBreakdown> {
            items.sort_by(|a, b| {
                let cmp = match col {
                    SortColumn::Target | SortColumn::Ability => a.ability_name.cmp(&b.ability_name),
                    SortColumn::Total => a
                        .total_value
                        .partial_cmp(&b.total_value)
                        .unwrap_or(std::cmp::Ordering::Equal),
                    SortColumn::Percent => a
                        .percent_of_total
                        .partial_cmp(&b.percent_of_total)
                        .unwrap_or(std::cmp::Ordering::Equal),
                    SortColumn::Rate => a
                        .dps
                        .partial_cmp(&b.dps)
                        .unwrap_or(std::cmp::Ordering::Equal),
                    SortColumn::Hits => a.hit_count.cmp(&b.hit_count),
                    SortColumn::Avg => a
                        .avg_hit
                        .partial_cmp(&b.avg_hit)
                        .unwrap_or(std::cmp::Ordering::Equal),
                    SortColumn::CritPct => a
                        .crit_rate
                        .partial_cmp(&b.crit_rate)
                        .unwrap_or(std::cmp::Ordering::Equal),
                    SortColumn::MissPct => {
                        let a_total = a.hit_count + a.miss_count;
                        let b_total = b.hit_count + b.miss_count;
                        let a_pct = if a_total > 0 { a.miss_count as f64 / a_total as f64 } else { 0.0 };
                        let b_pct = if b_total > 0 { b.miss_count as f64 / b_total as f64 } else { 0.0 };
                        a_pct.partial_cmp(&b_pct).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    SortColumn::AvgHit => a
                        .avg_hit
                        .partial_cmp(&b.avg_hit)
                        .unwrap_or(std::cmp::Ordering::Equal),
                    SortColumn::AvgCrit => {
                        let a_avg = if a.crit_count > 0 { a.crit_total / a.crit_count as f64 } else { 0.0 };
                        let b_avg = if b.crit_count > 0 { b.crit_total / b.crit_count as f64 } else { 0.0 };
                        a_avg.partial_cmp(&b_avg).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    SortColumn::Activations => a.activation_count.cmp(&b.activation_count),
                    SortColumn::Effective => a
                        .effective_total
                        .partial_cmp(&b.effective_total)
                        .unwrap_or(std::cmp::Ordering::Equal),
                    SortColumn::EffectivePct => {
                        let a_pct = if a.total_value > 0.0 { a.effective_total / a.total_value } else { 0.0 };
                        let b_pct = if b.total_value > 0.0 { b.effective_total / b.total_value } else { 0.0 };
                        a_pct.partial_cmp(&b_pct).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    SortColumn::ShieldTotal => {
                        let a_val = if a.is_shield { a.total_value } else { 0.0 };
                        let b_val = if b.is_shield { b.total_value } else { 0.0 };
                        a_val.partial_cmp(&b_val).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    SortColumn::Sps => {
                        let a_val = if a.is_shield { a.dps } else { 0.0 };
                        let b_val = if b.is_shield { b.dps } else { 0.0 };
                        a_val.partial_cmp(&b_val).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    SortColumn::AttackType => a.attack_type.cmp(&b.attack_type),
                    SortColumn::DamageType => a.damage_type.cmp(&b.damage_type),
                    SortColumn::ShldPct => {
                        let a_pct = if a.hit_count > 0 { a.shield_count as f64 / a.hit_count as f64 } else { 0.0 };
                        let b_pct = if b.hit_count > 0 { b.shield_count as f64 / b.hit_count as f64 } else { 0.0 };
                        a_pct.partial_cmp(&b_pct).unwrap_or(std::cmp::Ordering::Equal)
                    }
                    SortColumn::Absorbed => a.absorbed_total.partial_cmp(&b.absorbed_total).unwrap_or(std::cmp::Ordering::Equal),
                };
                match dir {
                    SortDirection::Asc => cmp,
                    SortDirection::Desc => cmp.reverse(),
                }
            });
            items
        };

        // If not grouping by target, return flat list with empty stats
        if !mode.by_target_type && !mode.by_target_instance {
            return vec![(GroupStats::default(), sort_abilities(list))];
        }

        // Group by target (using target_name + target_log_id for instance mode)
        use std::collections::BTreeMap;
        let mut groups: BTreeMap<(String, Option<i64>), Vec<AbilityBreakdown>> = BTreeMap::new();

        for ability in list {
            let target = ability.target_name.clone().unwrap_or_default();
            // Use target_log_id for instance grouping (unique per NPC spawn)
            let instance_key = if mode.by_target_instance {
                ability.target_log_id
            } else {
                None
            };
            groups
                .entry((target, instance_key))
                .or_default()
                .push(ability);
        }

        // Convert to vec with aggregate group stats
        let mut result: Vec<(GroupStats, Vec<AbilityBreakdown>)> = groups
            .into_iter()
            .map(|((target, _instance_key), abilities)| {
                let total: f64 = abilities.iter().map(|a| a.total_value).sum();
                let percent: f64 = abilities.iter().map(|a| a.percent_of_total).sum();
                let rate: f64 = abilities.iter().map(|a| a.dps).sum();
                let hits: i64 = abilities.iter().map(|a| a.hit_count).sum();
                let crits: i64 = abilities.iter().map(|a| a.crit_count).sum();
                let first_hit = abilities.first().and_then(|a| a.target_first_hit_secs);
                let avg = if hits > 0 { total / hits as f64 } else { 0.0 };
                let crit_pct = if hits > 0 {
                    crits as f64 / hits as f64 * 100.0
                } else {
                    0.0
                };
                let miss_count: i64 = abilities.iter().map(|a| a.miss_count).sum();
                // Activations are per-ability (not per-target), so sum unique abilities' counts
                let activation_count: i64 = abilities.iter().map(|a| a.activation_count).sum();
                let crit_total: f64 = abilities.iter().map(|a| a.crit_total).sum();
                let effective_total: f64 = abilities.iter().map(|a| a.effective_total).sum();
                let shield_total: f64 = abilities.iter().filter(|a| a.is_shield).map(|a| a.total_value).sum();
                let shield_rate: f64 = abilities.iter().filter(|a| a.is_shield).map(|a| a.dps).sum();
                let shield_count: i64 = abilities.iter().map(|a| a.shield_count).sum();
                let absorbed_total: f64 = abilities.iter().map(|a| a.absorbed_total).sum();

                let stats = GroupStats {
                    target: Some(target),
                    first_hit,
                    total,
                    percent,
                    rate,
                    hits,
                    avg,
                    crit_pct,
                    miss_count,
                    activation_count,
                    crit_total,
                    effective_total,
                    shield_total,
                    shield_rate,
                    shield_count,
                    absorbed_total,
                };
                (stats, sort_abilities(abilities))
            })
            .collect();

        // Sort groups by total (descending by default)
        result.sort_by(|a, b| {
            let cmp = match col {
                SortColumn::Target => a.0.target.cmp(&b.0.target),
                _ => {
                    a.0.total
                        .partial_cmp(&b.0.total)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
            };
            match col {
                SortColumn::Target => {
                    if dir == SortDirection::Asc {
                        cmp
                    } else {
                        cmp.reverse()
                    }
                }
                _ => cmp.reverse(),
            }
        });

        result
    });

    let eu = props.state.read().european_number_format;
    let format_number = |n: f64| formatting::format_compact_f64(n, eu);
    let format_pct = |n: f64| formatting::format_pct(n, eu);

    rsx! {
        div { class: "data-explorer",
            // Sidebar with encounter list
            aside { class: if *sidebar_collapsed.read() { "explorer-sidebar collapsed" } else { "explorer-sidebar" },
                div { class: "sidebar-header",
                    div { class: "sidebar-header-row",
                        if !*sidebar_collapsed.read() {
                            h3 {
                                i { class: "fa-solid fa-list" }
                                " Encounters"
                            }
                        }
                        button {
                            class: "sidebar-collapse-btn",
                            title: if *sidebar_collapsed.read() { "Expand encounters" } else { "Collapse encounters" },
                            onclick: move |_| { let v = *sidebar_collapsed.read(); sidebar_collapsed.set(!v); },
                            i { class: if *sidebar_collapsed.read() { "fa-solid fa-angles-right" } else { "fa-solid fa-angles-left" } }
                        }
                    }
                    if !*sidebar_collapsed.read() {
                        div { class: "history-controls",
                            label { class: "toggle-switch-label boss-filter-toggle",
                                span { class: "toggle-switch",
                                    input {
                                        r#type: "checkbox",
                                        checked: *show_only_bosses.read(),
                                        onchange: move |e| {
                                            let checked = e.checked();
                                            show_only_bosses.set(checked);
                                            // Update parent state immediately (avoids bidirectional sync loop)
                                            if let Ok(mut state) = props.state.try_write() {
                                                state.data_explorer.show_only_bosses = checked;
                                            }
                                            let mut toast = use_toast();
                                            spawn(async move {
                                                if let Some(mut cfg) = api::get_config().await {
                                                    cfg.show_only_bosses = checked;
                                                    if let Err(err) = api::update_config(&cfg).await {
                                                        toast.show(format!("Failed to save settings: {}", err), ToastSeverity::Normal);
                                                    }
                                                }
                                            });
                                        }
                                    }
                                    span { class: "toggle-slider" }
                                }
                                span { class: "toggle-text", "Bosses Only" }
                            }
                            span { class: "encounter-count",
                                "{filtered_history().len()}"
                                if *show_only_bosses.read() { " / {encounters().len()}" }
                            }
                        }
                    }
                }

                if !*sidebar_collapsed.read() {
                    div { class: "sidebar-encounter-list",
                        if encounters().is_empty() {
                            div { class: "sidebar-empty",
                                i { class: "fa-solid fa-inbox" }
                                p { "No encounters" }
                                p { class: "hint", "Load a log file to see encounters" }
                            }
                        } else {
                            for (idx, (area_name, difficulty, area_encounters)) in sections().iter().enumerate() {
                                {
                                    let section_key = format!("{}_{}", idx, area_name);
                                    let is_collapsed = collapsed_sections().contains(&section_key);
                                    let section_key_toggle = section_key.clone();
                                    let chevron_class = if is_collapsed { "fa-chevron-right" } else { "fa-chevron-down" };

                                    rsx! {
                                        // Area header (collapsible)
                                        div {
                                            class: "sidebar-section-header",
                                            onclick: move |_| {
                                                let mut set = collapsed_sections();
                                                if set.contains(&section_key_toggle) {
                                                    set.remove(&section_key_toggle);
                                                } else {
                                                    set.insert(section_key_toggle.clone());
                                                }
                                                collapsed_sections.set(set);
                                            },
                                            i { class: "fa-solid {chevron_class} collapse-icon" }
                                            span { class: "section-area", "{area_name}" }
                                            if let Some(diff) = difficulty {
                                                span { class: "section-difficulty", " • {diff}" }
                                            }
                                            span { class: "section-count", " ({area_encounters.len()})" }
                                        }

                                        // Encounter items (hidden if collapsed)
                                        if !is_collapsed {
                                            for enc in area_encounters.iter() {
                                                {
                                                    // Use actual encounter_id for parquet file lookup
                                                    let enc_idx = enc.encounter_id as u32;
                                                    let enc_id = enc.encounter_id;
                                                    let is_selected = *selected_encounter.read() == Some(enc_idx);
                                                    let success_class = if enc.success { "success" } else { "wipe" };
                                                    let npc_list = enc.npc_names.join(", ");
                                                    let persisted_link = enc.parsely_link.clone();
                                                    let current_upload_state = upload_states()
                                                        .get(&enc_id)
                                                        .cloned()
                                                        .unwrap_or(UploadState::Idle);
                                                    let start_line = enc.event_start_line.unwrap_or(0);
                                                    let end_line = enc.event_end_line.unwrap_or(0);
                                                    let area_line = enc.area_entered_line;

                                                    rsx! {
                                                        div {
                                                            class: if is_selected { "sidebar-encounter-item selected" } else { "sidebar-encounter-item" },
                                                            onclick: move |_| selected_encounter.set(Some(enc_idx)),
                                                            div { class: "encounter-main",
                                                                span { class: "encounter-name", "{enc.display_name}" }
                                                                div { class: "encounter-main-right",
                                                                    // Parsely upload / link button
                                                                    {
                                                                        // Determine the link to show (persisted takes priority, then transient success)
                                                                        let link_url = persisted_link.clone().or_else(|| {
                                                                            if let UploadState::Success(ref url) = current_upload_state {
                                                                                Some(url.clone())
                                                                            } else {
                                                                                None
                                                                            }
                                                                        });

                                                                        if let Some(url) = link_url {
                                                                            rsx! {
                                                                                button {
                                                                                    class: "parsely-upload-btn success",
                                                                                    title: "View on Parsely",
                                                                                    onclick: move |e| {
                                                                                        e.stop_propagation();
                                                                                        let u = url.clone();
                                                                                        spawn(async move { api::open_url(&u).await; });
                                                                                    },
                                                                                    i { class: "fa-solid fa-external-link" }
                                                                                }
                                                                            }
                                                                        } else {
                                                                            match current_upload_state {
                                                                                UploadState::Idle => rsx! {
                                                                                    button {
                                                                                        class: "parsely-upload-btn",
                                                                                        title: "Upload to Parsely",
                                                                                        onclick: {
                                                                                            let encounter_name = enc.display_name.clone();
                                                                                            move |e| {
                                                                                                e.stop_propagation();
                                                                                                let name = encounter_name.clone();
                                                                                                spawn(async move {
                                                                                                    if let Some(path) = api::get_active_file().await {
                                                                                                        parsely_upload.open_encounter(
                                                                                                            path,
                                                                                                            name,
                                                                                                            enc_id,
                                                                                                            start_line,
                                                                                                            end_line,
                                                                                                            area_line,
                                                                                                        );
                                                                                                    }
                                                                                                });
                                                                                            }
                                                                                        },
                                                                                        i { class: "fa-solid fa-upload" }
                                                                                    }
                                                                                },
                                                                                UploadState::Uploading => rsx! {
                                                                                    span { class: "parsely-upload-btn uploading",
                                                                                        i { class: "fa-solid fa-spinner fa-spin" }
                                                                                    }
                                                                                },
                                                                                UploadState::Success(_) => rsx! {
                                                                                    // Handled above via link_url
                                                                                },
                                                                                UploadState::Error(ref err) => rsx! {
                                                                                    button {
                                                                                        class: "parsely-upload-btn error",
                                                                                        title: "Error: {err}. Click to retry.",
                                                                                        onclick: move |e| {
                                                                                            e.stop_propagation();
                                                                                            upload_states.with_mut(|states| {
                                                                                                states.insert(enc_id, UploadState::Idle);
                                                                                            });
                                                                                        },
                                                                                        i { class: "fa-solid fa-triangle-exclamation" }
                                                                                    }
                                                                                },
                                                                            }
                                                                        }
                                                                    }
                                                                    // Result indicator (success/wipe)
                                                                    span { class: "result-indicator {success_class}",
                                                                        if enc.success {
                                                                            i { class: "fa-solid fa-check" }
                                                                        } else {
                                                                            i { class: "fa-solid fa-skull" }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            div { class: "encounter-meta",
                                                                if let Some(time) = &enc.start_time {
                                                                    span { class: "encounter-time", "{time}" }
                                                                }
                                                                span { class: "encounter-duration", "({formatting::format_duration(enc.duration_seconds)})" }
                                                            }
                                                            // NPC names (if available)
                                                            if !npc_list.is_empty() {
                                                                div { class: "encounter-npcs",
                                                                    i { class: "fa-solid fa-skull-crossbones" }
                                                                    " {npc_list}"
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
                    }
                }
            }

            // Data Panel (main content area)
            div { class: if *overview_fullscreen.read() { "data-panel fullscreen" } else { "data-panel" },
                if selected_encounter.read().is_none() {
                    div { class: "panel-placeholder",
                        i { class: "fa-solid fa-chart-bar" }
                        p { "Select an encounter" }
                        p { class: "hint", "Choose an encounter from the sidebar to view detailed breakdown" }
                    }
                } else {
                    // Phase timeline filter (when timeline is loaded)
                    if let Some(tl) = timeline.read().as_ref() {
                        PhaseTimelineFilter {
                            timeline: tl.clone(),
                            range: time_range(),
                            on_range_change: move |new_range: TimeRange| {
                                time_range.set(new_range);
                            }
                        }
                    }

                    // Selected encounter indicator (shown when sidebar collapsed or fullscreen)
                    if *sidebar_collapsed.read() || *overview_fullscreen.read() {
                        if let Some(enc_idx) = *selected_encounter.read() {
                            if let Some(enc) = encounters().iter().find(|e| e.encounter_id as u32 == enc_idx) {
                                div { class: "selected-entity-indicator",
                                    i { class: "fa-solid fa-crosshairs" }
                                    span { "{enc.display_name}" }
                                    span { class: "indicator-meta", "({formatting::format_duration(enc.duration_seconds)})" }
                                }
                            }
                        }
                    }

                    // Data tab selector (Overview, Damage, Healing, Damage Taken, Healing Taken, Charts)
                    div { class: "data-tab-selector",
                        button {
                            class: if matches!(view_mode(), ViewMode::Overview) { "data-tab active" } else { "data-tab" },
                            onclick: move |_| view_mode.set(ViewMode::Overview),
                            "Overview"
                        }
                        button {
                            class: if matches!(view_mode(), ViewMode::Charts) { "data-tab active" } else { "data-tab" },
                            onclick: move |_| view_mode.set(ViewMode::Charts),
                            "Charts"
                        }
                        button {
                            class: if matches!(view_mode(), ViewMode::Detailed(DataTab::Damage)) { "data-tab active" } else { "data-tab" },
                            onclick: move |_| view_mode.set(ViewMode::Detailed(DataTab::Damage)),
                            "Damage"
                        }
                        button {
                            class: if matches!(view_mode(), ViewMode::Detailed(DataTab::Healing)) { "data-tab active" } else { "data-tab" },
                            onclick: move |_| view_mode.set(ViewMode::Detailed(DataTab::Healing)),
                            "Healing"
                        }
                        button {
                            class: if matches!(view_mode(), ViewMode::Detailed(DataTab::DamageTaken)) { "data-tab active" } else { "data-tab" },
                            onclick: move |_| view_mode.set(ViewMode::Detailed(DataTab::DamageTaken)),
                            "Damage Taken"
                        }
                        button {
                            class: if matches!(view_mode(), ViewMode::Detailed(DataTab::HealingTaken)) { "data-tab active" } else { "data-tab" },
                            onclick: move |_| view_mode.set(ViewMode::Detailed(DataTab::HealingTaken)),
                            "Healing Taken"
                        }
                        button {
                            class: if matches!(view_mode(), ViewMode::CombatLog) { "data-tab active" } else { "data-tab" },
                            onclick: move |_| { death_target_filter.set(None); view_mode.set(ViewMode::CombatLog); },
                            "Combat Log"
                        }
                        button {
                            class: if matches!(view_mode(), ViewMode::Usage) { "data-tab active" } else { "data-tab" },
                            onclick: move |_| view_mode.set(ViewMode::Usage),
                            "Ability Usage"
                        }
                        button {
                            class: if matches!(view_mode(), ViewMode::Rotation) { "data-tab active" } else { "data-tab" },
                            onclick: move |_| view_mode.set(ViewMode::Rotation),
                            "Rotation"
                        }
                        button {
                            class: "panel-fullscreen-btn",
                            title: if *overview_fullscreen.read() { "Exit fullscreen" } else { "Expand to fullscreen" },
                            onclick: move |_| { let v = *overview_fullscreen.read(); overview_fullscreen.set(!v); },
                            i { class: if *overview_fullscreen.read() { "fa-solid fa-down-left-and-up-right-to-center" } else { "fa-solid fa-up-right-and-down-left-from-center" } }
                        }
                    }

                    // Loading/Error state display
                    match content_state() {
                        LoadState::Loading => rsx! {
                            div { class: "loading-banner",
                                i { class: "fa-solid fa-spinner fa-spin" }
                                " Loading..."
                            }
                        },
                        LoadState::Error(msg) => rsx! {
                            div { class: "error-banner",
                                i { class: "fa-solid fa-exclamation-triangle" }
                                " {msg}"
                            }
                        },
                        _ => rsx! {}
                    }

                    // Content area - Overview, Charts, Combat Log, or Detailed view
                    // Use view_mode() instead of *view_mode.read() to avoid holding borrow across onclick handlers
                    if matches!(view_mode(), ViewMode::CombatLog) {
                        // Combat Log Panel
                        if let Some(enc_idx) = *selected_encounter.read() {
                            CombatLog {
                                key: "{enc_idx}",
                                encounter_idx: enc_idx,
                                time_range: time_range(),
                                initial_target: death_target_filter(),
                                state: combat_log_state,
                                on_range_change: move |new_range: TimeRange| {
                                    time_range.set(new_range);
                                },
                                european: eu,
                            }
                        }
                    } else if matches!(view_mode(), ViewMode::Overview) {
                        // Raid Overview - Donut Charts + Table
                        // Uses memoized overview_table_data - charts initialized via use_effect above
                        div { class: "overview-section",
                            // Death Tracker (only shown if deaths occurred) - at top for visibility
                            {
                                let deaths = player_deaths.read();
                                rsx! {
                                    if !deaths.is_empty() {
                                        div { class: "death-tracker",
                                            h4 { class: "death-tracker-title",
                                                i { class: "fa-solid fa-skull" }
                                                " Deaths ({deaths.len()})"
                                            }
                                            div { class: "death-list",
                                                for death in deaths.iter() {
                                                    {
                                                        let name = death.name.clone();
                                                        let death_time = death.death_time_secs;
                                                        let time_str = formatting::format_duration(death_time as i64);
                                                        rsx! {
                                                            button {
                                                                class: "death-item",
                                                                title: "Click to view 10 seconds before death in Combat Log",
                                                                onclick: {
                                                                    let player_name = name.clone();
                                                                    move |_| {
                                                                        let start = (death_time - 10.0).max(0.0);
                                                                        time_range.set(TimeRange { start, end: death_time });
                                                                        death_target_filter.set(Some(player_name.clone()));
                                                                        view_mode.set(ViewMode::CombatLog);
                                                                    }
                                                                },
                                                                span { class: "death-name", "{name}" }
                                                                span { class: "death-time", "@ {time_str}" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Overview table - uses memoized data
                            {
                                let table_data = overview_table_data.read();
                                rsx! {
                                    table { class: "overview-table",
                                        thead {
                                            tr {
                                                th { class: "name-col", "Name" }
                                                th { class: "section-header", colspan: "2", "Damage Dealt" }
                                                th { class: "section-header", colspan: "2", "Threat" }
                                                th { class: "section-header", colspan: "3", "Damage Taken" }
                                                th { class: "section-header", colspan: "4", "Healing" }
                                                th { class: "section-header", colspan: "2", "Shielding" }
                                                th { class: "section-header", colspan: "1", "Activity" }
                                            }
                                            {
                                                let cur = *overview_sort_col.read();
                                                let asc = *overview_sort_asc.read();
                                                let arrow = |col: OverviewSort| -> &'static str {
                                                    if cur == col { if asc { " \u{25B2}" } else { " \u{25BC}" } } else { "" }
                                                };
                                                let sort_click = move |col: OverviewSort| {
                                                    move |_: MouseEvent| {
                                                        if *overview_sort_col.read() == col {
                                                            let was_asc = *overview_sort_asc.peek();
                                                            overview_sort_asc.set(!was_asc);
                                                        } else {
                                                            overview_sort_col.set(col);
                                                            overview_sort_asc.set(false);
                                                        }
                                                    }
                                                };
                                                rsx! {
                                                    tr { class: "sub-header",
                                                        th {}
                                                        th { class: "num sortable", onclick: sort_click(OverviewSort::DamageTotal), "Total{arrow(OverviewSort::DamageTotal)}" }
                                                        th { class: "num sortable", onclick: sort_click(OverviewSort::DPS), "DPS{arrow(OverviewSort::DPS)}" }
                                                        th { class: "num sortable", onclick: sort_click(OverviewSort::ThreatTotal), "Total{arrow(OverviewSort::ThreatTotal)}" }
                                                        th { class: "num sortable", onclick: sort_click(OverviewSort::TPS), "TPS{arrow(OverviewSort::TPS)}" }
                                                        th { class: "num sortable", onclick: sort_click(OverviewSort::DamageTakenTotal), "Total{arrow(OverviewSort::DamageTakenTotal)}" }
                                                        th { class: "num sortable", onclick: sort_click(OverviewSort::DTPS), "DTPS{arrow(OverviewSort::DTPS)}" }
                                                        th { class: "num sortable", onclick: sort_click(OverviewSort::APS), "APS{arrow(OverviewSort::APS)}" }
                                                        th { class: "num sortable", onclick: sort_click(OverviewSort::HealingTotal), "Total{arrow(OverviewSort::HealingTotal)}" }
                                                        th { class: "num sortable", onclick: sort_click(OverviewSort::HPS), "HPS{arrow(OverviewSort::HPS)}" }
                                                        th { class: "num sortable", onclick: sort_click(OverviewSort::HealingPct), "%{arrow(OverviewSort::HealingPct)}" }
                                                        th { class: "num sortable", onclick: sort_click(OverviewSort::EHPS), "EHPS{arrow(OverviewSort::EHPS)}" }
                                                        th { class: "num sortable", onclick: sort_click(OverviewSort::ShieldingTotal), "Total{arrow(OverviewSort::ShieldingTotal)}" }
                                                        th { class: "num sortable", onclick: sort_click(OverviewSort::SPS), "SPS{arrow(OverviewSort::SPS)}" }
                                                        th { class: "num sortable", onclick: sort_click(OverviewSort::APM), "APM{arrow(OverviewSort::APM)}" }
                                                    }
                                                }
                                            }
                                        }
                                        tbody {
                                            for row in table_data.rows.iter() {
                                                tr {
                                                    td { class: "name-col",
                                                        span { class: "name-with-icon",
                                                            if let Some(role_name) = &row.role_icon {
                                                                if let Some(role_asset) = get_role_icon(role_name) {
                                                                    img {
                                                                        class: "role-icon",
                                                                        src: *role_asset,
                                                                        alt: ""
                                                                    }
                                                                }
                                                            }
                                                            if let Some(icon_name) = &row.class_icon {
                                                                if let Some(icon_asset) = get_class_icon(icon_name) {
                                                                    img {
                                                                        class: "class-icon",
                                                                        src: *icon_asset,
                                                                        title: "{row.discipline_name.as_deref().unwrap_or(\"\")}",
                                                                        alt: ""
                                                                    }
                                                                }
                                                            }
                                                            "{row.name}"
                                                        }
                                                    }
                                                    td { class: "num dmg", "{format_number(row.damage_total)}" }
                                                    td { class: "num dmg", "{format_number(row.dps)}" }
                                                    td { class: "num threat", "{format_number(row.threat_total)}" }
                                                    td { class: "num threat", "{format_number(row.tps)}" }
                                                    td { class: "num taken", "{format_number(row.damage_taken_total)}" }
                                                    td { class: "num taken", "{format_number(row.dtps)}" }
                                                    td { class: "num taken", "{format_number(row.aps)}" }
                                                    td { class: "num heal", "{format_number(row.healing_total)}" }
                                                    td { class: "num heal", "{format_number(row.hps)}" }
                                                    td { class: "num heal", "{format_pct(row.healing_pct)}" }
                                                    td { class: "num heal", "{format_number(row.ehps)}" }
                                                    td { class: "num shield", "{format_number(row.shielding_given_total)}" }
                                                    td { class: "num shield", "{format_number(row.sps)}" }
                                                    td { class: "num apm", "{formatting::format_decimal_f64(row.apm, 1, eu)}" }
                                                }
                                            }
                                        }
                                        tfoot {
                                            tr { class: "totals-row",
                                                td { class: "name-col", "Group Total" }
                                                td { class: "num dmg", "{format_number(table_data.total_damage)}" }
                                                td { class: "num dmg", "{format_number(table_data.total_dps)}" }
                                                td { class: "num threat", "{format_number(table_data.total_threat)}" }
                                                td { class: "num threat", "{format_number(table_data.total_tps)}" }
                                                td { class: "num taken", "{format_number(table_data.total_damage_taken)}" }
                                                td { class: "num taken", "{format_number(table_data.total_dtps)}" }
                                                td { class: "num taken", "{format_number(table_data.total_aps)}" }
                                                td { class: "num heal", "{format_number(table_data.total_healing)}" }
                                                td { class: "num heal", "{format_number(table_data.total_hps)}" }
                                                td { class: "num heal", "" }
                                                td { class: "num heal", "{format_number(table_data.total_ehps)}" }
                                                td { class: "num shield", "{format_number(table_data.total_shielding)}" }
                                                td { class: "num shield", "{format_number(table_data.total_sps)}" }
                                                td { class: "num apm" }
                                            }
                                        }
                                    }

                                    // Challenge Results (above donuts when available)
                                    {
                                        let challenges: Vec<ChallengeSummary> = if let Some(enc_idx) = *selected_encounter.read() {
                                            encounters().iter()
                                                .find(|e| e.encounter_id as u32 == enc_idx)
                                                .map(|e| e.challenges.clone())
                                                .unwrap_or_default()
                                        } else {
                                            Vec::new()
                                        };
                                        let format_compact = |n: i64| formatting::format_compact(n, eu);
                                        rsx! {
                                            if !challenges.is_empty() {
                                                div { class: "challenge-results",
                                                    h4 { class: "challenge-results-header",
                                                        i { class: "fa-solid fa-trophy" }
                                                        " Challenges"
                                                    }
                                                    div { class: "challenge-cards",
                                                        for challenge in challenges.iter() {
                                                            {
                                                                let duration_str = formatting::format_duration(challenge.duration_secs as i64);
                                                                let per_sec_str = challenge.per_second
                                                                    .map(|ps| format_compact(ps as i64))
                                                                    .unwrap_or_default();

                                                                rsx! {
                                                                    div { class: "challenge-card",
                                                                        div { class: "challenge-card-header",
                                                                            span { class: "challenge-name", "{challenge.name}" }
                                                                            span { class: "challenge-total",
                                                                                "{format_compact(challenge.total_value)}"
                                                                                if !per_sec_str.is_empty() {
                                                                                    span { class: "text-muted", " ({per_sec_str}/s)" }
                                                                                }
                                                                            }
                                                                            span { class: "challenge-duration text-muted", "{duration_str}" }
                                                                        }
                                                                        if !challenge.by_player.is_empty() {
                                                                            div { class: "challenge-players",
                                                                                for player in challenge.by_player.iter() {
                                                                                    {
                                                                                        let ps_str = player.per_second
                                                                                            .map(|ps| format_compact(ps as i64))
                                                                                            .unwrap_or_default();
                                                                                        rsx! {
                                                                                            div { class: "challenge-player-row",
                                                                                                div {
                                                                                                    class: "challenge-bar-fill",
                                                                                                    style: "width: {player.percent:.1}%",
                                                                                                }
                                                                                                span { class: "challenge-player-name", "{player.name}" }
                                                                                                if !ps_str.is_empty() {
                                                                                                    span { class: "challenge-player-ps", "{ps_str}/s" }
                                                                                                }
                                                                                                span { class: "challenge-player-value", "{format_compact(player.value)}" }
                                                                                                span { class: "challenge-player-pct", "{formatting::format_pct_f32(player.percent, eu)}" }
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
                                                }
                                            }
                                        }
                                    }

                                    // Donut Charts Grid (2x2 below table)
                                    div { class: "overview-charts-section",
                                        h4 { class: "overview-charts-title", "Breakdown by Player" }
                                        div { class: "overview-charts-grid",
                                            div { id: "donut-damage", class: "overview-donut-chart" }
                                            div { id: "donut-threat", class: "overview-donut-chart" }
                                            div { id: "donut-healing", class: "overview-donut-chart" }
                                            div { id: "donut-taken", class: "overview-donut-chart" }
                                        }
                                    }

                                    // NPC Health Table - split into columns of 12
                                    {
                                        let npcs = npc_health.read();
                                        let chunks: Vec<&[NpcHealthRow]> = npcs.chunks(15).collect();
                                        rsx! {
                                            if !npcs.is_empty() {
                                                div { class: "npc-health-section",
                                                    h4 { class: "npc-health-title",
                                                        i { class: "fa-solid fa-heart-pulse" }
                                                        " NPC Health ({npcs.len()})"
                                                    }
                                                    div { class: "npc-health-grid",
                                                        for chunk in chunks.iter() {
                                                            table { class: "npc-health-table",
                                                                thead {
                                                                    tr {
                                                                        th { "Name" }
                                                                        th { class: "num", "HP" }
                                                                        th { class: "num", "Max" }
                                                                        th { class: "num", "%" }
                                                                    }
                                                                }
                                                                tbody {
                                                                    for npc in chunk.iter() {
                                                                        {
                                                                            let hp_class = if npc.final_hp == 0 { "dead" } else { "alive" };
                                                                            let seen_str = formatting::format_duration(npc.first_seen_secs as i64);
                                                                            let death_str = npc.death_time_secs.map(|t| formatting::format_duration(t as i64));
                                                                            rsx! {
                                                                                tr { class: "npc-row {hp_class}",
                                                                                    td {
                                                                                        span { class: "npc-name", "{npc.name}" }
                                                                                        span { class: "npc-seen-time", " @{seen_str}" }
                                                                                        if let Some(ref dt) = death_str {
                                                                                            span { class: "npc-death-time",
                                                                                                i { class: "fa-solid fa-skull" }
                                                                                                " {dt}"
                                                                                            }
                                                                                        }
                                                                                    }
                                                                                    td { class: "num", "{format_number(npc.final_hp as f64)}" }
                                                                                    td { class: "num", "{format_number(npc.max_hp as f64)}" }
                                                                                    td { class: "num", "{formatting::format_pct_f32(npc.final_hp_pct, eu)}" }
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
                                    }

                                }
                            }
                        }
                    } else if matches!(view_mode(), ViewMode::Detailed(_) | ViewMode::Rotation | ViewMode::Usage | ViewMode::Charts) {
                        // Two-column layout (Detailed breakdown, Rotation, Usage, or Charts)
                        {
                        let current_tab = if let ViewMode::Detailed(tab) = view_mode() { Some(tab) } else { None };
                        rsx! {
                        div { class: "explorer-content",
                            // Entity breakdown sidebar
                            div { class: if *entity_collapsed.read() { "entity-section collapsed" } else { "entity-section" },
                                div { class: "entity-header",
                                    if !*entity_collapsed.read() {
                                        h4 {
                                            if current_tab.is_some_and(|t| !t.is_outgoing()) { "Targets" } else { "Sources" }
                                        }
                                    }
                                    button {
                                        class: "sidebar-collapse-btn",
                                        title: if *entity_collapsed.read() { "Expand entities" } else { "Collapse entities" },
                                        onclick: move |_| { let v = *entity_collapsed.read(); entity_collapsed.set(!v); },
                                        i { class: if *entity_collapsed.read() { "fa-solid fa-angles-right" } else { "fa-solid fa-angles-left" } }
                                    }
                                }
                                if !*entity_collapsed.read() {
                                    div { class: "entity-filter-tabs",
                                        button {
                                            class: if *show_players_only.read() { "filter-tab active" } else { "filter-tab" },
                                            onclick: move |_| show_players_only.set(true),
                                            "Player"
                                        }
                                        button {
                                            class: if !*show_players_only.read() { "filter-tab active" } else { "filter-tab" },
                                            onclick: move |_| show_players_only.set(false),
                                            "NPC"
                                        }
                                    }
                                    div { class: "entity-list",
                                        // Uses memoized entity_list
                                        {
                                        let tr = *time_range.read();
                                        let duration = if tr.start != 0.0 || tr.end != 0.0 {
                                            tr.end - tr.start
                                        } else {
                                            timeline.read().as_ref().map(|t| t.duration_secs).unwrap_or(1.0)
                                        } as f64;
                                        let duration = if duration > 0.0 { duration } else { 1.0 };
                                        let mode = view_mode();
                                        let show_values = !matches!(mode, ViewMode::Charts);
                                        rsx! {
                                        for entity in entity_list().iter() {
                                            {
                                                let name = entity.source_name.clone();
                                                let is_selected = selected_source.read().as_ref() == Some(&name);
                                                let is_npc = entity.entity_type == "Npc";
                                                let class_icon = class_icon_lookup().get(&name).cloned();
                                                let per_sec = entity.total_value / duration;
                                                // Color class for entity value based on tab
                                                let value_class = match mode {
                                                    ViewMode::Detailed(DataTab::Damage) => "entity-value value-damage",
                                                    ViewMode::Detailed(DataTab::Healing | DataTab::HealingTaken) => "entity-value value-healing",
                                                    ViewMode::Detailed(DataTab::DamageTaken) => "entity-value value-dtps",
                                                    ViewMode::Rotation | ViewMode::Usage => {
                                                        let dmg = entity_dmg_totals.read().get(&name).copied().unwrap_or(0.0);
                                                        if dmg >= entity.total_value - dmg { "entity-value value-damage" } else { "entity-value value-healing" }
                                                    }
                                                    _ => "entity-value",
                                                };
                                                let icon_class = "entity-class-icon";
                                                rsx! {
                                                    div {
                                                        class: if is_selected { "entity-row selected" } else if is_npc { "entity-row npc" } else { "entity-row" },
                                                        onclick: {
                                                            let name = name.clone();
                                                            move |_| on_source_click(name.clone())
                                                        },
                                                        span { class: "entity-name",
                                                            if let Some(icon_name) = &class_icon {
                                                                if let Some(icon_asset) = get_class_icon(icon_name) {
                                                                    img {
                                                                        class: "{icon_class}",
                                                                        src: *icon_asset,
                                                                        alt: ""
                                                                    }
                                                                }
                                                            }
                                                            "{entity.source_name}"
                                                        }
                                                        if show_values {
                                                            span { class: "{value_class}", "{format_number(per_sec)}/s" }
                                                            span { class: "entity-abilities", "{entity.abilities_used} abilities" }
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

                            if matches!(view_mode(), ViewMode::Charts) {
                                // Charts view in right column
                                div { class: "charts-main",
                                    if *entity_collapsed.read() {
                                        if let Some(name) = selected_source.read().as_ref() {
                                            div { class: "selected-entity-indicator",
                                                i { class: "fa-solid fa-user" }
                                                span { "{name}" }
                                            }
                                        }
                                    }
                                    if let Some(tl) = timeline.read().as_ref() {
                                        {
                                            let tr = time_range();
                                            let duration = if tr.start != 0.0 || tr.end != 0.0 {
                                                tr.end - tr.start
                                            } else {
                                                tl.duration_secs
                                            };
                                            rsx! {
                                                ChartsPanel {
                                                    key: "{selected_encounter():?}",
                                                    encounter_idx: *selected_encounter.read(),
                                                    duration_secs: duration,
                                                    time_range: tr,
                                                    selected_source: selected_source,
                                                    european: eu,
                                                    on_time_range_change: move |new_range: TimeRange| {
                                                        time_range.set(new_range);
                                                    },
                                                }
                                            }
                                        }
                                    }
                                }
                            } else if matches!(view_mode(), ViewMode::Rotation) {
                                // Rotation view in right column
                                div { class: "ability-section",
                                    if *entity_collapsed.read() {
                                        if let Some(name) = selected_source.read().as_ref() {
                                            div { class: "selected-entity-indicator",
                                                i { class: "fa-solid fa-user" }
                                                span { "{name}" }
                                            }
                                        }
                                    }
                                    RotationView {
                                        encounter_idx: *selected_encounter.read(),
                                        time_range: time_range(),
                                        selected_source: selected_source.read().clone(),
                                        selected_anchor: selected_rotation_anchor,
                                        on_range_change: move |new_range: TimeRange| {
                                            time_range.set(new_range);
                                        },
                                        european: eu,
                                    }
                                }
                            } else if matches!(view_mode(), ViewMode::Usage) {
                                // Ability Usage view in right column
                                div { class: "ability-section usage-section",
                                    if *entity_collapsed.read() {
                                        if let Some(name) = selected_source.read().as_ref() {
                                            div { class: "selected-entity-indicator",
                                                i { class: "fa-solid fa-user" }
                                                span { "{name}" }
                                            }
                                        }
                                    }
                                    {render_usage_tab(
                                        selected_encounter,
                                        selected_source,
                                        time_range,
                                        usage_sort_column,
                                        usage_sort_direction,
                                        usage_selected_abilities,
                                        timeline,
                                        eu,
                                    )}
                                }
                            } else if let Some(current_tab) = current_tab {
                            // Ability breakdown table
                            div { class: "ability-section",
                                if *entity_collapsed.read() {
                                    if let Some(name) = selected_source.read().as_ref() {
                                        div { class: "selected-entity-indicator",
                                            i { class: "fa-solid fa-user" }
                                            span { "{name}" }
                                        }
                                    }
                                }
                                // DamageTaken summary panel
                                if current_tab == DataTab::DamageTaken {
                                    if let Some(ref summary) = *dt_summary.read() {
                                        div { class: "damage-taken-summary",
                                            div { class: "dt-summary-col",
                                                div { class: "dt-summary-row",
                                                    span { class: "dt-summary-label", "Internal/Elemental" }
                                                    span { class: "dt-summary-pct", "{format_pct(summary.internal_elemental_pct)}" }
                                                    span { class: "dt-summary-val", "{format_number(summary.internal_elemental_total)}" }
                                                }
                                                div { class: "dt-summary-row",
                                                    span { class: "dt-summary-label", "Kinetic/Energy" }
                                                    span { class: "dt-summary-pct", "{format_pct(summary.kinetic_energy_pct)}" }
                                                    span { class: "dt-summary-val", "{format_number(summary.kinetic_energy_total)}" }
                                                }
                                                div { class: "dt-summary-row",
                                                    span { class: "dt-summary-label", "Force/Tech" }
                                                    span { class: "dt-summary-pct", "{format_pct(summary.force_tech_pct)}" }
                                                    span { class: "dt-summary-val", "{format_number(summary.force_tech_total)}" }
                                                }
                                                div { class: "dt-summary-row",
                                                    span { class: "dt-summary-label", "Melee/Ranged" }
                                                    span { class: "dt-summary-pct", "{format_pct(summary.melee_ranged_pct)}" }
                                                    span { class: "dt-summary-val", "{format_number(summary.melee_ranged_total)}" }
                                                }
                                            }
                                            div { class: "dt-summary-col",
                                                div { class: "dt-summary-row",
                                                    span { class: "dt-summary-label", "Avoided" }
                                                    span { class: "dt-summary-pct", "{format_pct(summary.avoided_pct)}" }
                                                    span { class: "dt-summary-val" }
                                                }
                                                div { class: "dt-summary-row",
                                                    span { class: "dt-summary-label", "Shielded" }
                                                    span { class: "dt-summary-pct", "{format_pct(summary.shielded_pct)}" }
                                                    span { class: "dt-summary-val" }
                                                }
                                                div { class: "dt-summary-row",
                                                    span { class: "dt-summary-label", "Absorbed (self)" }
                                                    span { class: "dt-summary-pct", "{format_pct(summary.absorbed_self_pct)}" }
                                                    span { class: "dt-summary-val",
                                                        if summary.absorbed_self_total > 0.0 {
                                                            "{format_number(summary.absorbed_self_total)}"
                                                        }
                                                    }
                                                }
                                                div { class: "dt-summary-row",
                                                    span { class: "dt-summary-label", "Absorbed (given)" }
                                                    span { class: "dt-summary-pct", "{format_pct(summary.absorbed_given_pct)}" }
                                                    span { class: "dt-summary-val",
                                                        if summary.absorbed_given_total > 0.0 {
                                                            "{format_number(summary.absorbed_given_total)}"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                // Header with breakdown controls only
                                div { class: "ability-header",
                                    // Breakdown mode toggles (nested hierarchy)
                                    // Labels change based on tab: outgoing uses "Target", incoming uses "Source"
                                    // Instance mode only makes sense for damage tabs (NPCs have multiple spawns)
                                    {
                                        let tab = current_tab;
                                        let is_outgoing = tab.is_outgoing();
                                        let type_label = if is_outgoing { "Target type" } else { "Source type" };
                                        let instance_label = if is_outgoing { "Target instance" } else { "Source instance" };
                                        // Instance mode only for Damage/DamageTaken (NPCs), not Healing (players don't have instances)
                                        let show_instance = matches!(tab, DataTab::Damage | DataTab::DamageTaken);
                                        rsx! {
                                            div { class: "breakdown-controls",
                                                span { class: "breakdown-label", "Breakdown by" }
                                                div { class: "breakdown-options",
                                                    label { class: "breakdown-option primary",
                                                        input {
                                                            r#type: "checkbox",
                                                            checked: breakdown_mode.read().by_ability,
                                                            // Can only disable if target type/instance is enabled (need at least one grouping)
                                                            disabled: !breakdown_mode.read().by_target_type && !breakdown_mode.read().by_target_instance,
                                                            onchange: move |e| {
                                                                let mut mode = *breakdown_mode.read();
                                                                mode.by_ability = e.checked();
                                                                breakdown_mode.set(mode);
                                                            }
                                                        }
                                                        "Ability"
                                                    }
                                                    div { class: "breakdown-nested",
                                                        label { class: "breakdown-option",
                                                            input {
                                                                r#type: "checkbox",
                                                                checked: breakdown_mode.read().by_target_type,
                                                                onchange: move |e| {
                                                                    let mut mode = *breakdown_mode.read();
                                                                    mode.by_target_type = e.checked();
                                                                    // If disabling target type, also disable target instance
                                                                    if !e.checked() {
                                                                        mode.by_target_instance = false;
                                                                        // Re-enable ability if nothing else selected
                                                                        mode.by_ability = true;
                                                                    }
                                                                    breakdown_mode.set(mode);
                                                                }
                                                            }
                                                            "{type_label}"
                                                        }
                                                        if show_instance {
                                                            label { class: "breakdown-option nested",
                                                                input {
                                                                    r#type: "checkbox",
                                                                    checked: breakdown_mode.read().by_target_instance,
                                                                    disabled: !breakdown_mode.read().by_target_type,
                                                                    onchange: move |e| {
                                                                        let mut mode = *breakdown_mode.read();
                                                                        mode.by_target_instance = e.checked();
                                                                        breakdown_mode.set(mode);
                                                                    }
                                                                }
                                                                "{instance_label}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                // Table with dynamic columns (sortable)
                                {
                                let mode = *breakdown_mode.read();
                                let tab = current_tab;
                                let show_breakdown_col = mode.by_target_type || mode.by_target_instance;
                                let show_ability_col = mode.by_ability;
                                let breakdown_col_label = if tab.is_outgoing() { "Target" } else { "Source" };
                                let rate_label = tab.rate_label();
                                let is_damage_tab = tab == DataTab::Damage || tab == DataTab::DamageTaken;
                                let is_damage_taken = tab == DataTab::DamageTaken;
                                let is_healing_tab = tab.is_healing();
                                let current_sort = *sort_column.read();
                                let current_dir = *sort_direction.read();

                                // Helper to get sort indicator class
                                let sort_class = |col: SortColumn| -> &'static str {
                                    if current_sort == col {
                                        match current_dir {
                                            SortDirection::Asc => "sortable sorted-asc",
                                            SortDirection::Desc => "sortable sorted-desc",
                                        }
                                    } else {
                                        "sortable"
                                    }
                                };

                                // Macro-like helper for sort click (inline to avoid closure issues)
                                let sort_click = |col: SortColumn, is_text: bool| {
                                    move |_| {
                                        if *sort_column.read() == col {
                                            let new_dir = match *sort_direction.read() {
                                                SortDirection::Asc => SortDirection::Desc,
                                                SortDirection::Desc => SortDirection::Asc,
                                            };
                                            sort_direction.set(new_dir);
                                        } else {
                                            sort_column.set(col);
                                            sort_direction.set(if is_text { SortDirection::Asc } else { SortDirection::Desc });
                                        }
                                    }
                                };

                                // Compute split averages for group stats
                                let group_miss_pct = |s: &GroupStats| -> f64 {
                                    let total = s.hits + s.miss_count;
                                    if total > 0 { s.miss_count as f64 / total as f64 * 100.0 } else { 0.0 }
                                };
                                let group_avg_hit = |s: &GroupStats| -> f64 {
                                    if s.hits > 0 { s.total / s.hits as f64 } else { 0.0 }
                                };
                                let group_avg_crit = |s: &GroupStats| -> f64 {
                                    let crits = (s.crit_pct / 100.0 * s.hits as f64) as i64;
                                    if crits > 0 { s.crit_total / crits as f64 } else { 0.0 }
                                };
                                let group_eff_pct = |s: &GroupStats| -> f64 {
                                    if s.total > 0.0 { s.effective_total / s.total * 100.0 } else { 0.0 }
                                };

                                // Compute split averages for ability rows
                                let ability_avg_crit = |a: &AbilityBreakdown| -> f64 {
                                    if a.crit_count > 0 { a.crit_total / a.crit_count as f64 } else { 0.0 }
                                };
                                let ability_miss_pct = |a: &AbilityBreakdown| -> f64 {
                                    let total = a.hit_count + a.miss_count;
                                    if total > 0 { a.miss_count as f64 / total as f64 * 100.0 } else { 0.0 }
                                };
                                let ability_eff_pct = |a: &AbilityBreakdown| -> f64 {
                                    if a.total_value > 0.0 { a.effective_total / a.total_value * 100.0 } else { 0.0 }
                                };

                                let table_class = if is_damage_tab { "ability-table damage-tab" } else if is_healing_tab { "ability-table healing-tab" } else { "ability-table" };

                                rsx! {
                                    table { class: table_class,
                                        thead {
                                            tr {
                                                th {
                                                    class: if show_breakdown_col { sort_class(SortColumn::Target) } else { sort_class(SortColumn::Ability) },
                                                    onclick: if show_breakdown_col { sort_click(SortColumn::Target, true) } else { sort_click(SortColumn::Ability, true) },
                                                    if show_breakdown_col {
                                                        "{breakdown_col_label} / Ability"
                                                    } else {
                                                        "Ability"
                                                    }
                                                }
                                                // Activations and Hits as columns #2 and #3
                                                if show_ability_col {
                                                    th {
                                                        class: "num {sort_class(SortColumn::Activations)}",
                                                        onclick: sort_click(SortColumn::Activations, false),
                                                        "Activations"
                                                    }
                                                }
                                                th {
                                                    class: "num {sort_class(SortColumn::Hits)}",
                                                    onclick: sort_click(SortColumn::Hits, false),
                                                    "Hits"
                                                }
                                                th {
                                                    class: "num col-val {sort_class(SortColumn::Total)}",
                                                    onclick: sort_click(SortColumn::Total, false),
                                                    "Total"
                                                }
                                                th {
                                                    class: "num col-pct col-pct-bar {sort_class(SortColumn::Percent)}",
                                                    onclick: sort_click(SortColumn::Percent, false),
                                                    "%"
                                                }
                                                if is_healing_tab {
                                                    th {
                                                        class: "num col-val {sort_class(SortColumn::Effective)}",
                                                        onclick: sort_click(SortColumn::Effective, false),
                                                        "Effective"
                                                    }
                                                    th {
                                                        class: "num col-pct {sort_class(SortColumn::EffectivePct)}",
                                                        onclick: sort_click(SortColumn::EffectivePct, false),
                                                        "Eff%"
                                                    }
                                                }
                                                th {
                                                    class: "num col-val {sort_class(SortColumn::Rate)}",
                                                    onclick: sort_click(SortColumn::Rate, false),
                                                    "{rate_label}"
                                                }
                                                if is_healing_tab {
                                                    th {
                                                        class: "num col-val {sort_class(SortColumn::Effective)}",
                                                        onclick: sort_click(SortColumn::Effective, false),
                                                        "EHPS"
                                                    }
                                                    th {
                                                        class: "num col-shield {sort_class(SortColumn::ShieldTotal)}",
                                                        onclick: sort_click(SortColumn::ShieldTotal, false),
                                                        "Shielded"
                                                    }
                                                    th {
                                                        class: "num col-shield {sort_class(SortColumn::Sps)}",
                                                        onclick: sort_click(SortColumn::Sps, false),
                                                        "SPS"
                                                    }
                                                }
                                                if is_damage_tab {
                                                    th {
                                                        class: "num col-pct {sort_class(SortColumn::MissPct)}",
                                                        onclick: sort_click(SortColumn::MissPct, false),
                                                        if is_damage_taken { "Def%" } else { "Miss%" }
                                                    }
                                                }
                                                if is_damage_taken {
                                                    th {
                                                        class: "num col-pct {sort_class(SortColumn::ShldPct)}",
                                                        onclick: sort_click(SortColumn::ShldPct, false),
                                                        "Shld%"
                                                    }
                                                    th {
                                                        class: "num col-val {sort_class(SortColumn::Absorbed)}",
                                                        onclick: sort_click(SortColumn::Absorbed, false),
                                                        "Abs"
                                                    }
                                                    th {
                                                        class: "num col-dmg-type {sort_class(SortColumn::AttackType)}",
                                                        onclick: sort_click(SortColumn::AttackType, true),
                                                        "AT"
                                                    }
                                                    th {
                                                        class: "num col-dmg-type {sort_class(SortColumn::DamageType)}",
                                                        onclick: sort_click(SortColumn::DamageType, true),
                                                        "DT"
                                                    }
                                                }
                                                if !is_damage_taken {
                                                    th {
                                                        class: "num col-pct {sort_class(SortColumn::CritPct)}",
                                                        onclick: sort_click(SortColumn::CritPct, false),
                                                        "Crit%"
                                                    }
                                                }
                                                th {
                                                    class: "num col-avg col-avg-first {sort_class(SortColumn::Avg)}",
                                                    onclick: sort_click(SortColumn::Avg, false),
                                                    "Avg"
                                                }
                                                if is_damage_tab {
                                                    th {
                                                        class: "num col-avg {sort_class(SortColumn::AvgHit)}",
                                                        onclick: sort_click(SortColumn::AvgHit, false),
                                                        "Hit"
                                                    }
                                                }
                                                th {
                                                    class: "num col-avg {sort_class(SortColumn::AvgCrit)}",
                                                    onclick: sort_click(SortColumn::AvgCrit, false),
                                                    "Crit"
                                                }
                                            }
                                        }
                                        tbody {
                                            for (stats, abilities) in grouped_abilities().iter() {
                                                if let Some(target) = &stats.target {
                                                    tr { class: "group-header",
                                                        td { class: "group-target",
                                                            i { class: "fa-solid fa-caret-down group-icon" }
                                                            "{target}"
                                                            if let Some(t) = stats.first_hit {
                                                                span { class: "target-time",
                                                                    " @{(t as i32) / 60}:{(t as i32) % 60:02}"
                                                                }
                                                            }
                                                        }
                                                        if show_ability_col {
                                                            td { class: "num group-stat", "{stats.activation_count}" }
                                                        }
                                                        td { class: "num group-stat", "{stats.hits}" }
                                                        td { class: "num group-stat col-val", "{format_number(stats.total)}" }
                                                        td { class: "num group-stat col-pct col-pct-bar",
                                                            div { class: "pct-bar-track",
                                                                span { class: "pct-bar-fill", style: "width: {stats.percent}%;" }
                                                                span { class: "pct-text", "{format_pct(stats.percent)}" }
                                                            }
                                                        }
                                                        if is_healing_tab {
                                                            td { class: "num group-stat col-val", "{format_number(stats.effective_total)}" }
                                                            td { class: "num group-stat col-pct", "{format_pct(group_eff_pct(stats))}" }
                                                        }
                                                        td { class: "num group-stat col-val", "{format_number(stats.rate)}" }
                                                        if is_healing_tab {
                                                            td { class: "num group-stat col-val",
                                                                {
                                                                    let ehps = if stats.total > 0.0 { stats.rate * stats.effective_total / stats.total } else { 0.0 };
                                                                    format_number(ehps)
                                                                }
                                                            }
                                                            td { class: "num group-stat col-shield",
                                                                if stats.shield_total > 0.0 { "{format_number(stats.shield_total)}" } else { "-" }
                                                            }
                                                            td { class: "num group-stat col-shield",
                                                                if stats.shield_rate > 0.0 { "{format_number(stats.shield_rate)}" } else { "-" }
                                                            }
                                                        }
                                                        if is_damage_tab {
                                                            td { class: "num group-stat col-pct", "{format_pct(group_miss_pct(stats))}" }
                                                        }
                                                        if is_damage_taken {
                                                            td { class: "num group-stat col-pct",
                                                                {
                                                                    let total = stats.hits + stats.shield_count;
                                                                    if total > 0 {
                                                                        format_pct(stats.shield_count as f64 / total as f64 * 100.0)
                                                                    } else {
                                                                        "-".to_string()
                                                                    }
                                                                }
                                                            }
                                                            td { class: "num group-stat col-val",
                                                                if stats.absorbed_total > 0.0 { "{format_number(stats.absorbed_total)}" } else { "-" }
                                                            }
                                                            td { class: "num group-stat col-dmg-type", "-" }
                                                            td { class: "num group-stat col-dmg-type", "-" }
                                                        }
                                                        if !is_damage_taken {
                                                            td { class: "num group-stat col-pct", "{format_pct(stats.crit_pct)}" }
                                                        }
                                                        td { class: "num group-stat col-avg col-avg-first",
                                                            {
                                                                let overall = if is_damage_tab {
                                                                    let total_attempts = stats.hits + stats.miss_count;
                                                                    if total_attempts > 0 { format_number(stats.total / total_attempts as f64) } else { format_number(0.0) }
                                                                } else {
                                                                    format_number(stats.avg)
                                                                };
                                                                overall
                                                            }
                                                        }
                                                        if is_damage_tab {
                                                            td { class: "num group-stat col-avg", "{format_number(group_avg_hit(stats))}" }
                                                        }
                                                        td { class: "num group-stat col-avg", "{format_number(group_avg_crit(stats))}" }
                                                    }
                                                }
                                                if show_ability_col {
                                                    for (idx, ability) in abilities.iter().enumerate() {
                                                        tr { key: "{stats.target.as_deref().unwrap_or(\"\")}-{idx}-{ability.ability_id}", class: if ability.is_shield { "ability-row shield-row" } else if stats.target.is_some() { "ability-row indented" } else { "ability-row" },
                                                            td { class: "ability-name-cell",
                                                                span { class: "ability-name-inner",
                                                                    AbilityIcon { ability_id: ability.ability_id }
                                                                    if !ability.ability_name.is_empty() {
                                                                        "{ability.ability_name}"
                                                                    } else {
                                                                        "Ability #{ability.ability_id}"
                                                                    }
                                                                    if ability.is_shield {
                                                                        span { class: "shield-badge", " (shield)" }
                                                                    }
                                                                }
                                                            }
                                                            // Activations and Hits as columns #2 and #3
                                                            td { class: "num",
                                                                if ability.activation_count > 0 {
                                                                    "{ability.activation_count}"
                                                                } else {
                                                                    "-"
                                                                }
                                                            }
                                                            td { class: "num", "{ability.hit_count}" }
                                                            // Total with inline bar
                                                            td { class: "num col-val", "{format_number(ability.total_value)}" }
                                                            // %
                                                            td { class: "num col-pct col-pct-bar",
                                                                div { class: "pct-bar-track",
                                                                    span { class: "pct-bar-fill", style: "width: {ability.percent_of_total}%;" }
                                                                    span { class: "pct-text", "{format_pct(ability.percent_of_total)}" }
                                                                }
                                                            }
                                                            if is_healing_tab {
                                                                td { class: "num col-val", "{format_number(ability.effective_total)}" }
                                                                td { class: "num col-pct", "{format_pct(ability_eff_pct(ability))}" }
                                                            }
                                                            td { class: "num col-val", "{format_number(ability.dps)}" }
                                                            if is_healing_tab {
                                                                td { class: "num col-val",
                                                                    {
                                                                        let ehps = if ability.total_value > 0.0 { ability.dps * ability.effective_total / ability.total_value } else { 0.0 };
                                                                        format_number(ehps)
                                                                    }
                                                                }
                                                                td { class: "num col-shield",
                                                                    if ability.is_shield { "{format_number(ability.total_value)}" } else { "-" }
                                                                }
                                                                td { class: "num col-shield",
                                                                    if ability.is_shield { "{format_number(ability.dps)}" } else { "-" }
                                                                }
                                                            }
                                                            if is_damage_tab {
                                                                td { class: "num col-pct",
                                                                    if ability.is_shield { "-" } else { "{format_pct(ability_miss_pct(ability))}" }
                                                                }
                                                            }
                                                            if is_damage_taken {
                                                                td { class: "num col-pct",
                                                                    {
                                                                        if ability.hit_count > 0 {
                                                                            format_pct(ability.shield_count as f64 / ability.hit_count as f64 * 100.0)
                                                                        } else {
                                                                            "-".to_string()
                                                                        }
                                                                    }
                                                                }
                                                                td { class: "num col-val",
                                                                    if ability.absorbed_total > 0.0 { "{format_number(ability.absorbed_total)}" } else { "-" }
                                                                }
                                                                td { class: "num col-dmg-type",
                                                                    {
                                                                        match ability.attack_type.as_str() {
                                                                            "" => "-",
                                                                            other => other,
                                                                        }
                                                                    }
                                                                }
                                                                td { class: "num col-dmg-type",
                                                                    {
                                                                        match ability.damage_type.as_str() {
                                                                            "" => "-",
                                                                            other => other,
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                            if !is_damage_taken {
                                                                td { class: "num col-pct",
                                                                    if ability.is_shield { "-" } else { "{format_pct(ability.crit_rate)}" }
                                                                }
                                                            }
                                                            td { class: "num col-avg col-avg-first",
                                                                {
                                                                    let overall = if is_damage_tab {
                                                                        let total_attempts = ability.hit_count + ability.miss_count;
                                                                        if total_attempts > 0 { ability.total_value / total_attempts as f64 } else { 0.0 }
                                                                    } else {
                                                                        ability.avg_hit
                                                                    };
                                                                    format_number(overall)
                                                                }
                                                            }
                                                            if is_damage_tab {
                                                                td { class: "num col-avg",
                                                                    if ability.is_shield { "-" } else { "{format_number(ability.avg_hit)}" }
                                                                }
                                                            }
                                                            td { class: "num col-avg",
                                                                if ability.is_shield || ability.crit_count == 0 { "-" } else { "{format_number(ability_avg_crit(ability))}" }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        tfoot {
                                            {
                                                let groups = grouped_abilities();
                                                let total_hits: i64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.hit_count)).sum();
                                                let total_activations: i64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.activation_count)).sum();
                                                let total_val: f64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.total_value)).sum();
                                                let total_rate: f64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.dps)).sum();
                                                let total_eff: f64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.effective_total)).sum();
                                                let total_miss: i64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.miss_count)).sum();
                                                let total_crits: i64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.crit_count)).sum();
                                                let total_shield_val: f64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().filter(|a| a.is_shield).map(|a| a.total_value)).sum();
                                                let total_shield_rate: f64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().filter(|a| a.is_shield).map(|a| a.dps)).sum();
                                                let total_shield_count: i64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.shield_count)).sum();
                                                let total_absorbed: f64 = groups.iter().flat_map(|(_, abilities)| abilities.iter().map(|a| a.absorbed_total)).sum();
                                                let crit_pct = if total_hits > 0 { total_crits as f64 / total_hits as f64 * 100.0 } else { 0.0 };
                                                let avg = if is_damage_tab {
                                                    let attempts = total_hits + total_miss;
                                                    if attempts > 0 { total_val / attempts as f64 } else { 0.0 }
                                                } else if total_hits > 0 {
                                                    total_val / total_hits as f64
                                                } else {
                                                    0.0
                                                };
                                                let miss_pct = {
                                                    let attempts = total_hits + total_miss;
                                                    if attempts > 0 { total_miss as f64 / attempts as f64 * 100.0 } else { 0.0 }
                                                };
                                                let eff_pct = if total_val > 0.0 { total_eff / total_val * 100.0 } else { 0.0 };
                                                let ehps = if total_val > 0.0 { total_rate * total_eff / total_val } else { 0.0 };
                                                let shld_pct = if total_hits > 0 {
                                                    total_shield_count as f64 / total_hits as f64 * 100.0
                                                } else { 0.0 };

                                                rsx! {
                                                    tr { class: "totals-row",
                                                        td { "Total" }
                                                        if show_ability_col {
                                                            td { class: "num", "{total_activations}" }
                                                        }
                                                        td { class: "num", "{total_hits}" }
                                                        td { class: "num col-val", "{format_number(total_val)}" }
                                                        td { class: "num col-pct col-pct-bar" }
                                                        if is_healing_tab {
                                                            td { class: "num col-val", "{format_number(total_eff)}" }
                                                            td { class: "num col-pct", "{format_pct(eff_pct)}" }
                                                        }
                                                        td { class: "num col-val", "{format_number(total_rate)}" }
                                                        if is_healing_tab {
                                                            td { class: "num col-val", "{format_number(ehps)}" }
                                                            td { class: "num col-shield",
                                                                if total_shield_val > 0.0 { "{format_number(total_shield_val)}" } else { "-" }
                                                            }
                                                            td { class: "num col-shield",
                                                                if total_shield_rate > 0.0 { "{format_number(total_shield_rate)}" } else { "-" }
                                                            }
                                                        }
                                                        if is_damage_tab {
                                                            td { class: "num col-pct", "{format_pct(miss_pct)}" }
                                                        }
                                                        if is_damage_taken {
                                                            td { class: "num col-pct", "{format_pct(shld_pct)}" }
                                                            td { class: "num col-val",
                                                                if total_absorbed > 0.0 { "{format_number(total_absorbed)}" } else { "-" }
                                                            }
                                                            td { class: "num col-dmg-type" }
                                                            td { class: "num col-dmg-type" }
                                                        }
                                                        if !is_damage_taken {
                                                            td { class: "num col-pct", "{format_pct(crit_pct)}" }
                                                        }
                                                        td { class: "num col-avg col-avg-first", "{format_number(avg)}" }
                                                        if is_damage_tab {
                                                            td { class: "num col-avg" }
                                                        }
                                                        td { class: "num col-avg" }
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
                        }
                    }
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Ability Usage Tab
// ─────────────────────────────────────────────────────────────────────────────

/// Color palette for selected abilities in the timeline chart.
/// Matches the effect span colors from ChartsPanel for consistency.
const USAGE_COLORS: [&str; 6] = [
    "rgba(255, 200, 50, 0.85)",  // Gold
    "rgba(100, 200, 255, 0.85)", // Cyan
    "rgba(255, 100, 150, 0.85)", // Pink
    "rgba(150, 255, 100, 0.85)", // Lime
    "rgba(200, 150, 255, 0.85)", // Purple
    "rgba(255, 180, 100, 0.85)", // Orange
];

/// Format a time value in seconds with 3 decimal places.
fn format_secs(secs: f32, european: bool) -> String {
    formatting::format_decimal(secs, 3, european)
}

/// Sort ability usage rows in place by the given column and direction.
fn sort_usage_rows(rows: &mut [AbilityUsageRow], column: UsageSortColumn, direction: SortDirection) {
    let cmp = |a: &AbilityUsageRow, b: &AbilityUsageRow| -> std::cmp::Ordering {
        let ord = match column {
            UsageSortColumn::Ability => a.ability_name.to_lowercase().cmp(&b.ability_name.to_lowercase()),
            UsageSortColumn::CastCount => a.cast_count.cmp(&b.cast_count),
            UsageSortColumn::FirstCast => a.first_cast_secs.partial_cmp(&b.first_cast_secs).unwrap_or(std::cmp::Ordering::Equal),
            UsageSortColumn::LastCast => a.last_cast_secs.partial_cmp(&b.last_cast_secs).unwrap_or(std::cmp::Ordering::Equal),
            UsageSortColumn::AvgTime => a.avg_time_between.partial_cmp(&b.avg_time_between).unwrap_or(std::cmp::Ordering::Equal),
            UsageSortColumn::MedianTime => a.median_time_between.partial_cmp(&b.median_time_between).unwrap_or(std::cmp::Ordering::Equal),
            UsageSortColumn::MinTime => a.min_time_between.partial_cmp(&b.min_time_between).unwrap_or(std::cmp::Ordering::Equal),
            UsageSortColumn::MaxTime => a.max_time_between.partial_cmp(&b.max_time_between).unwrap_or(std::cmp::Ordering::Equal),
        };
        match direction {
            SortDirection::Asc => ord,
            SortDirection::Desc => ord.reverse(),
        }
    };
    rows.sort_by(cmp);
}

/// Build an ECharts scatter chart option for the ability usage timeline.
/// `duration_secs` defines the x-axis extent (0..duration) so the chart
/// always spans the full encounter regardless of when abilities were cast.
fn build_usage_timeline_option(
    rows: &[AbilityUsageRow],
    selected: &[(i64, &str)],
    duration_secs: f32,
) -> JsValue {
    let option = js_sys::Object::new();

    // Tooltip — show ability name + M:SS.mmm timestamp on hover
    let tooltip = js_sys::Object::new();
    js_set(&tooltip, "trigger", &JsValue::from_str("item"));
    js_set(&tooltip, "backgroundColor", &JsValue::from_str("rgba(30, 30, 30, 0.95)"));
    js_set(&tooltip, "borderColor", &JsValue::from_str("rgba(255, 255, 255, 0.1)"));
    let tip_text_style = js_sys::Object::new();
    js_set(&tip_text_style, "color", &JsValue::from_str("#ccc"));
    js_set(&tip_text_style, "fontSize", &JsValue::from_f64(12.0));
    js_set(&tooltip, "textStyle", &tip_text_style);
    // Format tooltip as "AbilityName at M:SS.mmm"
    let tip_formatter = js_sys::Function::new_with_args(
        "p",
        "var v = p.value[0]; var ms = Math.round(v * 1000); var m = Math.floor(ms / 60000); \
         var rem = ms % 60000; var s = Math.floor(rem / 1000); var frac = rem % 1000; \
         var pad = s < 10 ? '0' : ''; var msPad = frac < 100 ? (frac < 10 ? '00' : '0') : ''; \
         return p.seriesName + ' at ' + m + ':' + pad + s + '.' + msPad + frac;",
    );
    js_set(&tooltip, "formatter", &tip_formatter);
    js_set(&option, "tooltip", &tooltip);

    // Grid — give Y-axis labels enough room on the left
    let grid = js_sys::Object::new();
    js_set(&grid, "left", &JsValue::from_str("20"));
    js_set(&grid, "right", &JsValue::from_str("20"));
    js_set(&grid, "top", &JsValue::from_str("10"));
    js_set(&grid, "bottom", &JsValue::from_str("30"));
    js_set(&grid, "containLabel", &JsValue::from_bool(true));
    js_set(&option, "grid", &grid);

    // Collect ability names in selection order for Y-axis categories (reversed so
    // first-selected appears at top)
    let mut categories = js_sys::Array::new();
    let mut series_arr = js_sys::Array::new();

    for (ability_id, color) in selected.iter().rev() {
        if let Some(row) = rows.iter().find(|r| r.ability_id == *ability_id) {
            categories.push(&JsValue::from_str(&row.ability_name));
            let cat_idx = categories.length() - 1;

            // Track line: thin horizontal line spanning 0..duration through the dots
            let track_data = js_sys::Array::new();
            let start_pt = js_sys::Array::new();
            start_pt.push(&JsValue::from_f64(0.0));
            start_pt.push(&JsValue::from_f64(cat_idx as f64));
            track_data.push(&start_pt);
            let end_pt = js_sys::Array::new();
            end_pt.push(&JsValue::from_f64(duration_secs as f64));
            end_pt.push(&JsValue::from_f64(cat_idx as f64));
            track_data.push(&end_pt);

            let track_series = js_sys::Object::new();
            js_set(&track_series, "type", &JsValue::from_str("line"));
            js_set(&track_series, "data", &track_data);
            js_set(&track_series, "symbol", &JsValue::from_str("none"));
            js_set(&track_series, "silent", &JsValue::from_bool(true));
            // Derive a subtle version of the color for the track line
            let track_color = color.replace("0.85)", "0.25)");
            let track_line_style = js_sys::Object::new();
            js_set(&track_line_style, "color", &JsValue::from_str(&track_color));
            js_set(&track_line_style, "width", &JsValue::from_f64(1.0));
            js_set(&track_series, "lineStyle", &track_line_style);
            // No legend entry for track lines
            js_set(&track_series, "legendHoverLink", &JsValue::from_bool(false));
            series_arr.push(&track_series);

            // Scatter dots: the actual cast events on top of the track
            let data = js_sys::Array::new();
            for &t in &row.timestamps {
                let point = js_sys::Array::new();
                point.push(&JsValue::from_f64(t as f64));
                point.push(&JsValue::from_f64(cat_idx as f64));
                data.push(&point);
            }

            let series = js_sys::Object::new();
            js_set(&series, "name", &JsValue::from_str(&row.ability_name));
            js_set(&series, "type", &JsValue::from_str("scatter"));
            js_set(&series, "data", &data);
            js_set(&series, "symbolSize", &JsValue::from_f64(8.0));
            let item_style = js_sys::Object::new();
            js_set(&item_style, "color", &JsValue::from_str(color));
            js_set(&series, "itemStyle", &item_style);
            series_arr.push(&series);
        }
    }

    // X-Axis: value axis spanning 0 to encounter duration
    let x_axis = js_sys::Object::new();
    js_set(&x_axis, "type", &JsValue::from_str("value"));
    js_set(&x_axis, "min", &JsValue::from_f64(0.0));
    js_set(&x_axis, "max", &JsValue::from_f64(duration_secs as f64));
    let x_axis_label = js_sys::Object::new();
    js_set(&x_axis_label, "color", &JsValue::from_str("#aaa"));
    js_set(&x_axis_label, "fontSize", &JsValue::from_f64(11.0));
    // Format axis labels as M:SS
    let axis_formatter = js_sys::Function::new_with_args(
        "v",
        "var m = Math.floor(v / 60); var s = Math.floor(v % 60); return m + ':' + (s < 10 ? '0' : '') + s;",
    );
    js_set(&x_axis_label, "formatter", &axis_formatter);
    js_set(&x_axis, "axisLabel", &x_axis_label);
    let x_line = js_sys::Object::new();
    let x_line_style = js_sys::Object::new();
    js_set(&x_line_style, "color", &JsValue::from_str("rgba(255,255,255,0.15)"));
    js_set(&x_line, "lineStyle", &x_line_style);
    js_set(&x_axis, "axisLine", &x_line);
    let split_line = js_sys::Object::new();
    let split_style = js_sys::Object::new();
    js_set(&split_style, "color", &JsValue::from_str("rgba(255,255,255,0.05)"));
    js_set(&split_line, "lineStyle", &split_style);
    js_set(&x_axis, "splitLine", &split_line);
    js_set(&option, "xAxis", &x_axis);

    // Y-Axis: one category row per selected ability
    let y_axis = js_sys::Object::new();
    js_set(&y_axis, "type", &JsValue::from_str("category"));
    js_set(&y_axis, "data", &categories);
    let y_axis_label = js_sys::Object::new();
    js_set(&y_axis_label, "color", &JsValue::from_str("#ccc"));
    js_set(&y_axis_label, "fontSize", &JsValue::from_f64(11.0));
    js_set(&y_axis, "axisLabel", &y_axis_label);
    let y_line = js_sys::Object::new();
    js_set(&y_line, "show", &JsValue::from_bool(false));
    js_set(&y_axis, "axisLine", &y_line);
    let y_tick = js_sys::Object::new();
    js_set(&y_tick, "show", &JsValue::from_bool(false));
    js_set(&y_axis, "axisTick", &y_tick);
    // No splitLines — the per-ability track lines serve as visual separators
    let y_split = js_sys::Object::new();
    js_set(&y_split, "show", &JsValue::from_bool(false));
    js_set(&y_axis, "splitLine", &y_split);
    js_set(&option, "yAxis", &y_axis);

    js_set(&option, "series", &series_arr);
    js_set(&option, "animation", &JsValue::from_bool(false));

    option.into()
}

/// Render the ability usage tab content (table + timeline chart).
#[allow(clippy::too_many_arguments)]
fn render_usage_tab(
    selected_encounter: Signal<Option<u32>>,
    selected_source: Signal<Option<String>>,
    time_range: Signal<TimeRange>,
    mut usage_sort_column: Signal<UsageSortColumn>,
    mut usage_sort_direction: Signal<SortDirection>,
    mut selected_abilities: Signal<Vec<(i64, &'static str)>>,
    timeline: Signal<Option<EncounterTimeline>>,
    european: bool,
) -> Element {
    // Fetch usage data reactively
    let usage_data = use_resource(move || {
        let enc = *selected_encounter.read();
        let src = selected_source.read().clone();
        let tr = *time_range.read();
        async move {
            let source = src?;
            let tr_opt = if tr.start != 0.0 || tr.end != 0.0 { Some(tr) } else { None };
            api::query_ability_usage(&source, enc, tr_opt.as_ref()).await
        }
    });

    let sort_col = *usage_sort_column.read();
    let sort_dir = *usage_sort_direction.read();

    // Handle sort column click: toggle direction if same column, else set new column desc
    let mut on_sort = move |col: UsageSortColumn| {
        if *usage_sort_column.read() == col {
            let dir = *usage_sort_direction.read();
            usage_sort_direction.set(match dir {
                SortDirection::Asc => SortDirection::Desc,
                SortDirection::Desc => SortDirection::Asc,
            });
        } else {
            usage_sort_column.set(col);
            usage_sort_direction.set(SortDirection::Desc);
        }
    };

    // Sort indicator
    let sort_indicator = move |col: UsageSortColumn| -> &'static str {
        if sort_col == col {
            match sort_dir {
                SortDirection::Asc => " \u{25B2}",
                SortDirection::Desc => " \u{25BC}",
            }
        } else {
            ""
        }
    };

    // Build sorted rows
    let rows: Vec<AbilityUsageRow> = match &*usage_data.read() {
        Some(Some(data)) => {
            let mut sorted = data.clone();
            sort_usage_rows(&mut sorted, sort_col, sort_dir);
            sorted
        }
        _ => Vec::new(),
    };

    let current_selected = selected_abilities.read().clone();

    let num_selected = current_selected.len();

    // Update timeline chart when selections or data change.
    // All signal reads must be inside the closure so Dioxus tracks dependencies.
    let chart_rows = rows.clone();
    use_effect(move || {
        let sel = selected_abilities.read().clone();
        // Read timeline duration inside the effect so it re-triggers when timeline loads
        let dur = {
            let tr = *time_range.read();
            if tr.start != 0.0 || tr.end != 0.0 {
                tr.end
            } else {
                timeline.read().as_ref().map(|t| t.duration_secs).unwrap_or(1.0)
            }
        };
        let rows_for_chart = chart_rows.clone();

        if sel.is_empty() {
            return;
        }

        // Defer chart init to ensure the DOM has been updated with visible dimensions
        spawn(async move {
            gloo_timers::future::TimeoutFuture::new(100).await;
            if let Some(chart) = init_overview_chart("usage-timeline-chart") {
                // Resize first so ECharts picks up the current container dimensions
                let resize_fn = js_sys::Reflect::get(&chart, &JsValue::from_str("resize"))
                    .ok()
                    .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
                if let Some(func) = &resize_fn {
                    let _ = func.call0(&chart);
                }

                let option = build_usage_timeline_option(&rows_for_chart, &sel, dur);
                let not_merge = js_sys::Object::new();
                js_set(&not_merge, "notMerge", &JsValue::from_bool(true));
                let set_option = js_sys::Reflect::get(&chart, &JsValue::from_str("setOption"))
                    .ok()
                    .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
                if let Some(func) = set_option {
                    let _ = func.call2(&chart, &option, &not_merge);
                }

                // Resize again after setOption in case the option changed dimensions
                if let Some(func) = &resize_fn {
                    let _ = func.call0(&chart);
                }
            }
        });
    });

    let eu = european;

    rsx! {
        // Ability Usage Table
        div { class: "usage-table-container",
            table { class: "usage-table effect-table",
                thead {
                    tr {
                        th {
                            class: "ability-name-cell sortable",
                            onclick: move |_| on_sort(UsageSortColumn::Ability),
                            "Action{sort_indicator(UsageSortColumn::Ability)}"
                        }
                        th {
                            class: "num sortable",
                            onclick: move |_| on_sort(UsageSortColumn::CastCount),
                            "Usages{sort_indicator(UsageSortColumn::CastCount)}"
                        }
                        th {
                            class: "num sortable",
                            onclick: move |_| on_sort(UsageSortColumn::FirstCast),
                            "First Usage{sort_indicator(UsageSortColumn::FirstCast)}"
                        }
                        th {
                            class: "num sortable",
                            onclick: move |_| on_sort(UsageSortColumn::LastCast),
                            "Last Usage{sort_indicator(UsageSortColumn::LastCast)}"
                        }
                        th {
                            class: "num sortable",
                            onclick: move |_| on_sort(UsageSortColumn::AvgTime),
                            "Avg Time{sort_indicator(UsageSortColumn::AvgTime)}"
                        }
                        th {
                            class: "num sortable",
                            onclick: move |_| on_sort(UsageSortColumn::MedianTime),
                            "Median{sort_indicator(UsageSortColumn::MedianTime)}"
                        }
                        th {
                            class: "num sortable",
                            onclick: move |_| on_sort(UsageSortColumn::MinTime),
                            "Min Time{sort_indicator(UsageSortColumn::MinTime)}"
                        }
                        th {
                            class: "num sortable",
                            onclick: move |_| on_sort(UsageSortColumn::MaxTime),
                            "Max Time{sort_indicator(UsageSortColumn::MaxTime)}"
                        }
                    }
                }
                tbody {
                    if rows.is_empty() {
                        tr {
                            td { colspan: "8", class: "empty-message",
                                "No ability usage data available. Select a player from the sidebar."
                            }
                        }
                    } else {
                        for row in rows.iter() {
                            {
                            let aid = row.ability_id;
                            let selected_color = current_selected.iter().find(|(id, _)| *id == aid).map(|(_, c)| *c);
                            let is_selected = selected_color.is_some();
                            let style = if let Some(c) = selected_color {
                                format!("--effect-color: {c};")
                            } else {
                                String::new()
                            };
                            rsx! {
                                tr { key: "{aid}",
                                    class: if is_selected { "selected" } else { "" },
                                    style: "{style}",
                                    onclick: move |_| {
                                        let mut sel = selected_abilities.read().clone();
                                        if let Some(pos) = sel.iter().position(|(id, _)| *id == aid) {
                                            sel.remove(pos);
                                        } else {
                                            let next_color = USAGE_COLORS[sel.len() % USAGE_COLORS.len()];
                                            sel.push((aid, next_color));
                                        }
                                        selected_abilities.set(sel);
                                    },
                                    td { class: "ability-name-cell",
                                        span { class: "ability-name-inner",
                                            AbilityIcon { ability_id: row.ability_id }
                                            span { "{row.ability_name}" }
                                            span { class: "ability-id-muted", " ({row.ability_id})" }
                                        }
                                    }
                                    td { class: "num", "{row.cast_count}" }
                                    td { class: "num", "{formatting::format_duration_ms(row.first_cast_secs)}" }
                                    td { class: "num", "{formatting::format_duration_ms(row.last_cast_secs)}" }
                                    if row.cast_count >= 2 {
                                        td { class: "num", "{format_secs(row.avg_time_between, eu)}" }
                                        td { class: "num", "{format_secs(row.median_time_between, eu)}" }
                                        td { class: "num", "{format_secs(row.min_time_between, eu)}" }
                                        td { class: "num", "{format_secs(row.max_time_between, eu)}" }
                                    } else {
                                        td { class: "num muted", "-" }
                                        td { class: "num muted", "-" }
                                        td { class: "num muted", "-" }
                                        td { class: "num muted", "-" }
                                    }
                                }
                            }
                            }
                        }
                    }
                }
            }
        }

        // Timeline Chart — height scales with number of selected abilities
        div { class: "usage-timeline-container",
            if num_selected == 0 {
                div { class: "usage-timeline-hint",
                    i { class: "fa-solid fa-chart-line" }
                    " Click on abilities above to visualize their cast timeline"
                }
            }
            {
                // 40px per ability row + 60px padding for axes, minimum 120px
                let chart_height = if num_selected == 0 { 0 } else { (num_selected * 40 + 60).max(120) };
                rsx! {
                    div {
                        id: "usage-timeline-chart",
                        class: "usage-timeline-chart",
                        style: if num_selected == 0 { "height: 0px; overflow: hidden;".to_string() } else { format!("height: {chart_height}px;") },
                    }
                }
            }
        }
    }
}
