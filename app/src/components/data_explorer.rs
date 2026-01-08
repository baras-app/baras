//! Data Explorer Panel Component
//!
//! Displays detailed ability breakdown and DPS analysis for encounters.
//! Uses DataFusion SQL queries over parquet files for historical data.

use dioxus::prelude::*;
use std::collections::HashSet;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local as spawn;

use crate::api::{
    self, AbilityBreakdown, BreakdownMode, DataTab, EncounterTimeline, EntityBreakdown,
    PlayerDeath, RaidOverviewRow, TimeRange,
};
use crate::components::charts_panel::ChartsPanel;
use crate::components::class_icons::{get_class_icon, get_role_icon};
use crate::components::combat_log::CombatLog;
use crate::components::history_panel::EncounterSummary;
use crate::components::phase_timeline::PhaseTimelineFilter;

// ─────────────────────────────────────────────────────────────────────────────
// Sort Types for Ability Table
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Default)]
enum SortColumn {
    Target,
    Ability,
    #[default]
    Total,
    Percent,
    Rate,
    Hits,
    Avg,
    CritPct,
}

#[derive(Clone, Copy, PartialEq, Default)]
enum SortDirection {
    #[default]
    Desc,
    Asc,
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
    js_sys::Reflect::set(
        &title_obj,
        &JsValue::from_str("text"),
        &JsValue::from_str(title),
    )
    .unwrap();
    js_sys::Reflect::set(
        &title_obj,
        &JsValue::from_str("left"),
        &JsValue::from_str("center"),
    )
    .unwrap();
    js_sys::Reflect::set(
        &title_obj,
        &JsValue::from_str("top"),
        &JsValue::from_str("5"),
    )
    .unwrap();
    let title_style = js_sys::Object::new();
    js_sys::Reflect::set(
        &title_style,
        &JsValue::from_str("color"),
        &JsValue::from_str("#e0e0e0"),
    )
    .unwrap();
    js_sys::Reflect::set(
        &title_style,
        &JsValue::from_str("fontSize"),
        &JsValue::from_f64(13.0),
    )
    .unwrap();
    js_sys::Reflect::set(
        &title_style,
        &JsValue::from_str("fontWeight"),
        &JsValue::from_str("600"),
    )
    .unwrap();
    js_sys::Reflect::set(&title_obj, &JsValue::from_str("textStyle"), &title_style).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("title"), &title_obj).unwrap();

    // Tooltip
    let tooltip = js_sys::Object::new();
    js_sys::Reflect::set(
        &tooltip,
        &JsValue::from_str("trigger"),
        &JsValue::from_str("item"),
    )
    .unwrap();
    js_sys::Reflect::set(
        &tooltip,
        &JsValue::from_str("formatter"),
        &JsValue::from_str("{b}: {c} ({d}%)"),
    )
    .unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("tooltip"), &tooltip).unwrap();

    // Series (donut)
    let series_arr = js_sys::Array::new();
    let series = js_sys::Object::new();
    js_sys::Reflect::set(
        &series,
        &JsValue::from_str("type"),
        &JsValue::from_str("pie"),
    )
    .unwrap();
    js_sys::Reflect::set(&series, &JsValue::from_str("radius"), &{
        let arr = js_sys::Array::new();
        arr.push(&JsValue::from_str("35%"));
        arr.push(&JsValue::from_str("65%"));
        arr.into()
    })
    .unwrap();
    js_sys::Reflect::set(&series, &JsValue::from_str("center"), &{
        let arr = js_sys::Array::new();
        arr.push(&JsValue::from_str("50%"));
        arr.push(&JsValue::from_str("55%"));
        arr.into()
    })
    .unwrap();

    // Label formatting
    let label = js_sys::Object::new();
    js_sys::Reflect::set(&label, &JsValue::from_str("show"), &JsValue::TRUE).unwrap();
    js_sys::Reflect::set(
        &label,
        &JsValue::from_str("formatter"),
        &JsValue::from_str("{b}"),
    )
    .unwrap();
    js_sys::Reflect::set(
        &label,
        &JsValue::from_str("color"),
        &JsValue::from_str("#ccc"),
    )
    .unwrap();
    js_sys::Reflect::set(
        &label,
        &JsValue::from_str("fontSize"),
        &JsValue::from_f64(10.0),
    )
    .unwrap();
    js_sys::Reflect::set(&series, &JsValue::from_str("label"), &label).unwrap();

    // Emphasis
    let emphasis = js_sys::Object::new();
    let emph_label = js_sys::Object::new();
    js_sys::Reflect::set(&emph_label, &JsValue::from_str("show"), &JsValue::TRUE).unwrap();
    js_sys::Reflect::set(
        &emph_label,
        &JsValue::from_str("fontSize"),
        &JsValue::from_f64(12.0),
    )
    .unwrap();
    js_sys::Reflect::set(
        &emph_label,
        &JsValue::from_str("fontWeight"),
        &JsValue::from_str("bold"),
    )
    .unwrap();
    js_sys::Reflect::set(&emphasis, &JsValue::from_str("label"), &emph_label).unwrap();
    js_sys::Reflect::set(&series, &JsValue::from_str("emphasis"), &emphasis).unwrap();

    // Item style with base color
    let item_style = js_sys::Object::new();
    js_sys::Reflect::set(
        &item_style,
        &JsValue::from_str("borderColor"),
        &JsValue::from_str("#1a1a1a"),
    )
    .unwrap();
    js_sys::Reflect::set(
        &item_style,
        &JsValue::from_str("borderWidth"),
        &JsValue::from_f64(2.0),
    )
    .unwrap();
    js_sys::Reflect::set(&series, &JsValue::from_str("itemStyle"), &item_style).unwrap();

    // Color palette based on base color with variations
    let colors = generate_color_palette(color, data.len());
    let color_arr = js_sys::Array::new();
    for c in colors {
        color_arr.push(&JsValue::from_str(&c));
    }
    js_sys::Reflect::set(&obj, &JsValue::from_str("color"), &color_arr).unwrap();

    // Data
    let data_arr = js_sys::Array::new();
    for (name, value) in data {
        let item = js_sys::Object::new();
        js_sys::Reflect::set(&item, &JsValue::from_str("name"), &JsValue::from_str(name)).unwrap();
        js_sys::Reflect::set(
            &item,
            &JsValue::from_str("value"),
            &JsValue::from_f64(*value),
        )
        .unwrap();
        data_arr.push(&item);
    }
    js_sys::Reflect::set(&series, &JsValue::from_str("data"), &data_arr).unwrap();

    series_arr.push(&series);
    js_sys::Reflect::set(&obj, &JsValue::from_str("series"), &series_arr).unwrap();

    // No animation for faster renders
    js_sys::Reflect::set(&obj, &JsValue::from_str("animation"), &JsValue::FALSE).unwrap();

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

fn format_number(n: f64) -> String {
    let n = n as i64;
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

fn format_pct(n: f64) -> String {
    format!("{:.1}%", n)
}

fn format_duration(secs: i64) -> String {
    let mins = secs / 60;
    let secs = secs % 60;
    format!("{}:{:02}", mins, secs)
}

/// Group encounters into sections by area (based on is_phase_start flag)
fn group_by_area(
    encounters: &[EncounterSummary],
) -> Vec<(String, Option<String>, Vec<&EncounterSummary>)> {
    let mut sections: Vec<(String, Option<String>, Vec<&EncounterSummary>)> = Vec::new();

    for enc in encounters.iter() {
        if enc.is_phase_start || sections.is_empty() {
            sections.push((enc.area_name.clone(), enc.difficulty.clone(), vec![enc]));
        } else if let Some(section) = sections.last_mut() {
            section.2.push(enc);
        }
    }

    sections
}

// ─────────────────────────────────────────────────────────────────────────────
// Component
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct DataExplorerProps {
    /// Initial encounter index (None = show selector)
    #[props(default)]
    pub encounter_idx: Option<u32>,
}

#[component]
pub fn DataExplorerPanel(props: DataExplorerProps) -> Element {
    // Encounter selection state
    let mut encounters = use_signal(Vec::<EncounterSummary>::new);
    let mut selected_encounter = use_signal(|| props.encounter_idx);

    // Sidebar state
    let mut show_only_bosses = use_signal(|| false);
    let mut collapsed_sections = use_signal(HashSet::<String>::new);

    // Query result state
    let mut abilities = use_signal(Vec::<AbilityBreakdown>::new);
    let mut entities = use_signal(Vec::<EntityBreakdown>::new);
    let mut selected_source = use_signal(|| None::<String>);
    let mut loading = use_signal(|| false);
    let mut error_msg = use_signal(|| None::<String>);

    // Entity filter: true = players/companions only, false = show all (including NPCs)
    let mut show_players_only = use_signal(|| true);

    // Timeline state
    let mut timeline = use_signal(|| None::<EncounterTimeline>);
    let mut time_range = use_signal(|| TimeRange::default());

    // Breakdown mode state (toggles for grouping)
    let mut breakdown_mode = use_signal(|| BreakdownMode::ability_only());

    // Data tab state (Damage, Healing, DamageTaken, HealingTaken)
    let mut selected_tab = use_signal(|| DataTab::Damage);

    // Ability table sort state
    let mut sort_column = use_signal(SortColumn::default);
    let mut sort_direction = use_signal(SortDirection::default);

    // Overview mode - true shows raid overview (default), false shows detailed tabs
    let mut show_overview = use_signal(|| true);
    let mut overview_data = use_signal(Vec::<RaidOverviewRow>::new);
    let mut player_deaths = use_signal(Vec::<PlayerDeath>::new);

    // Charts mode - show time series charts with effect highlighting
    let mut show_charts = use_signal(|| false);

    // Combat Log mode - show virtualized combat log table
    let mut show_combat_log = use_signal(|| false);

    // Death search text - set when clicking a death to search combat log (source OR target)
    let mut death_search_text = use_signal(|| None::<String>);

    // Memoized overview table data (rows + totals) - prevents recomputation on every render
    let overview_table_data = use_memo(move || {
        let data = overview_data.read();
        let rows: Vec<RaidOverviewRow> = data
            .iter()
            .filter(|r| r.entity_type == "Player" || r.entity_type == "Companion")
            .cloned()
            .collect();

        // Calculate totals
        let total_damage: f64 = rows.iter().map(|r| r.damage_total).sum();
        let total_dps: f64 = rows.iter().map(|r| r.dps).sum();
        let total_threat: f64 = rows.iter().map(|r| r.threat_total).sum();
        let total_tps: f64 = rows.iter().map(|r| r.tps).sum();
        let total_damage_taken: f64 = rows.iter().map(|r| r.damage_taken_total).sum();
        let total_dtps: f64 = rows.iter().map(|r| r.dtps).sum();
        let total_aps: f64 = rows.iter().map(|r| r.aps).sum();
        let total_healing: f64 = rows.iter().map(|r| r.healing_total).sum();
        let total_hps: f64 = rows.iter().map(|r| r.hps).sum();
        let total_ehps: f64 = rows.iter().map(|r| r.ehps).sum();

        (
            rows,
            total_damage,
            total_dps,
            total_threat,
            total_tps,
            total_damage_taken,
            total_dtps,
            total_aps,
            total_healing,
            total_hps,
            total_ehps,
        )
    });

    // Memoized chart data for overview donut charts (derived from table data)
    let chart_data = use_memo(move || {
        let (rows, ..) = overview_table_data();

        let damage_data: Vec<(String, f64)> = rows
            .iter()
            .filter(|r| r.damage_total > 0.0)
            .map(|r| (r.name.clone(), r.damage_total))
            .collect();
        let threat_data: Vec<(String, f64)> = rows
            .iter()
            .filter(|r| r.threat_total > 0.0)
            .map(|r| (r.name.clone(), r.threat_total))
            .collect();
        let healing_data: Vec<(String, f64)> = rows
            .iter()
            .filter(|r| r.healing_effective > 0.0)
            .map(|r| (r.name.clone(), r.healing_effective))
            .collect();
        let taken_data: Vec<(String, f64)> = rows
            .iter()
            .filter(|r| r.damage_taken_total > 0.0)
            .map(|r| (r.name.clone(), r.damage_taken_total))
            .collect();

        (damage_data, threat_data, healing_data, taken_data)
    });

    // Effect to initialize/update overview donut charts when data changes
    use_effect(move || {
        let (damage_data, threat_data, healing_data, taken_data) = chart_data();
        let is_overview = *show_overview.read() && !*show_charts.read() && !*show_combat_log.read();

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
                encounters.set(list);
            }
        });
    });

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
                }
                spawn(async move {
                    // Refresh encounter list
                    if let Some(list) = api::get_encounter_history().await {
                        let _ = encounters.try_write().map(|mut w| *w = list);
                    }
                });
            }
        });
        api::tauri_listen("session-updated", &closure).await;
        closure.forget();
    });

    // Cleanup on component unmount
    use_drop(move || {
        dispose_all_overview_charts();
    });

    // Load data when encounter changes - ONLY loads timeline (shared by all views)
    // Each view loads its own data lazily
    use_effect(move || {
        let idx = *selected_encounter.read();

        // Dispose charts immediately when encounter changes
        dispose_all_overview_charts();

        spawn(async move {
            // Clear ALL previous data when encounter changes
            abilities.set(Vec::new());
            entities.set(Vec::new());
            overview_data.set(Vec::new());
            player_deaths.set(Vec::new());
            selected_source.set(None);
            timeline.set(None);
            time_range.set(TimeRange::default());
            error_msg.set(None);

            if idx.is_none() {
                return; // No encounter selected
            }

            loading.set(true);

            // Only load timeline (needed for phase filter in all views)
            if let Some(tl) = api::query_encounter_timeline(idx).await {
                let dur = tl.duration_secs;
                time_range.set(TimeRange::full(dur));
                timeline.set(Some(tl));
            }

            loading.set(false);
        });
    });

    // Lazy load: Overview tab data (overview_data + deaths)
    use_effect(move || {
        let idx = *selected_encounter.read();
        let is_overview = *show_overview.read() && !*show_charts.read() && !*show_combat_log.read();
        let tr = time_range();
        let has_timeline = timeline.read().is_some();

        // Only load when Overview tab is active, have an encounter, AND timeline is loaded
        if !is_overview || idx.is_none() || !has_timeline {
            // Clear overview data when leaving the tab
            if !is_overview {
                overview_data.set(Vec::new());
                player_deaths.set(Vec::new());
            }
            return;
        }

        spawn(async move {
            loading.set(true);
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

            // Load raid overview (with retry for race conditions - up to 3 seconds)
            for attempt in 0..10 {
                if let Some(data) = api::query_raid_overview(idx, tr_opt.as_ref(), duration).await
                    && !data.is_empty() {
                        overview_data.set(data);
                        break;
                }
                if attempt < 9 {
                    gloo_timers::future::TimeoutFuture::new(300).await;
                }
            }

            // Load player deaths
            if let Some(deaths) = api::query_player_deaths(idx).await {
                player_deaths.set(deaths);
            }

            loading.set(false);
        });
    });

    // Lazy load: Detailed tab data (entities + abilities) for Damage/Healing/etc tabs
    use_effect(move || {
        let idx = *selected_encounter.read();
        let tab = *selected_tab.read();
        let is_detailed =
            !*show_overview.read() && !*show_charts.read() && !*show_combat_log.read();
        let tr = time_range();
        let has_timeline = timeline.read().is_some();

        // Only load when a detailed tab is active, have an encounter, AND timeline is loaded
        if !is_detailed || idx.is_none() || !has_timeline {
            // Clear detailed data when leaving
            if !is_detailed {
                entities.set(Vec::new());
                abilities.set(Vec::new());
                selected_source.set(None);
            }
            return;
        }

        spawn(async move {
            loading.set(true);
            let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
                None
            } else {
                Some(tr)
            };

            // Load entity breakdown (with retry for race conditions - up to 3 seconds)
            let mut entity_data = Vec::new();
            for attempt in 0..10 {
                if let Some(data) = api::query_entity_breakdown(tab, idx, tr_opt.as_ref()).await
                    && !data.is_empty() {
                        entity_data = data;
                        break;
                }
                if attempt < 9 {
                    gloo_timers::future::TimeoutFuture::new(300).await;
                }
            }
            if entity_data.is_empty() {
                loading.set(false);
                return;
            }

            // Auto-select first player if none selected
            let auto_selected = if selected_source.read().is_none() {
                entity_data
                    .iter()
                    .find(|e| e.entity_type == "Player" || e.entity_type == "Companion")
                    .map(|e| e.source_name.clone())
            } else {
                selected_source.read().clone()
            };

            entities.set(entity_data);

            // Load ability breakdown for selected (or auto-selected) source
            let mode = *breakdown_mode.read();
            if let Some(data) = api::query_breakdown(
                tab,
                idx,
                auto_selected.as_deref(),
                tr_opt.as_ref(),
                None, // No entity filter when source is selected
                Some(&mode),
                timeline.read().as_ref().map(|t| t.duration_secs),
            )
            .await
            {
                abilities.set(data);
            }

            // Set selected source after abilities loaded (triggers downstream effects cleanly)
            if selected_source.read().is_none() && auto_selected.is_some() {
                selected_source.set(auto_selected);
            }

            loading.set(false);
        });
    });

    // NOTE: Time range changes are now handled by the tab-specific effects above
    // They read time_range() which triggers reload when it changes

    // Reload abilities when entity filter or breakdown mode changes
    use_effect(move || {
        let players_only = *show_players_only.read();
        let mode = *breakdown_mode.read();
        let idx = *selected_encounter.read();
        let tab = *selected_tab.read();
        let src = selected_source.read().clone();
        let tr = time_range();
        let is_detailed =
            !*show_overview.read() && !*show_charts.read() && !*show_combat_log.read();
        let has_timeline = timeline.read().is_some();

        // Skip if not on detailed tab, no encounter, or timeline not loaded
        if !is_detailed || idx.is_none() || !has_timeline {
            return;
        }

        spawn(async move {
            loading.set(true);
            // Apply entity filter only when no specific source is selected
            let entity_filter: Option<&[&str]> = if src.is_none() && players_only {
                Some(&["Player", "Companion"])
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
                Some(&mode),
                duration,
            )
            .await
            {
                abilities.set(data);
            }
            loading.set(false);
        });
    });

    // Filter by source when selected
    let mut on_source_click = move |name: String| {
        let idx = *selected_encounter.read();
        let tab = *selected_tab.read();
        let current = selected_source.read().clone();
        let tr = time_range();

        // Toggle selection
        let new_source = if current.as_ref() == Some(&name) {
            None
        } else {
            Some(name.clone())
        };

        selected_source.set(new_source.clone());

        // Use time_range if not default
        let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
            None
        } else {
            Some(tr)
        };

        spawn(async move {
            loading.set(true);
            // Apply entity filter only when no specific source is selected
            let entity_filter: Option<&[&str]> =
                if new_source.is_none() && *show_players_only.read() {
                    Some(&["Player", "Companion"])
                } else {
                    None
                };
            let mode = *breakdown_mode.read();
            let duration = timeline.read().as_ref().map(|t| t.duration_secs);
            if let Some(data) = api::query_breakdown(
                tab,
                idx,
                new_source.as_deref(),
                tr_opt.as_ref(),
                entity_filter,
                Some(&mode),
                duration,
            )
            .await
            {
                abilities.set(data);
            }
            loading.set(false);
        });
    };

    // Memoized filtered history - only recomputes when encounters or filter changes
    let filtered_history = use_memo(move || {
        let history = encounters();
        let bosses_only = show_only_bosses();
        if bosses_only {
            history
                .into_iter()
                .filter(|e| e.boss_name.is_some())
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
            .filter(|e| !players_only || e.entity_type == "Player" || e.entity_type == "Companion")
            .cloned()
            .collect::<Vec<_>>()
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

                let stats = GroupStats {
                    target: Some(target),
                    first_hit,
                    total,
                    percent,
                    rate,
                    hits,
                    avg,
                    crit_pct,
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

    rsx! {
        div { class: "data-explorer",
            // Sidebar with encounter list
            aside { class: "explorer-sidebar",
                div { class: "sidebar-header",
                    h3 {
                        i { class: "fa-solid fa-list" }
                        " Encounters"
                    }
                    div { class: "history-controls",
                        label { class: "boss-filter-toggle",
                            input {
                                r#type: "checkbox",
                                checked: *show_only_bosses.read(),
                                onchange: move |e| show_only_bosses.set(e.checked())
                            }
                            span { "Bosses Only" }
                        }
                        span { class: "encounter-count",
                            "{filtered_history().len()}"
                            if *show_only_bosses.read() { " / {encounters().len()}" }
                        }
                    }
                }

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
                                                let is_selected = *selected_encounter.read() == Some(enc_idx);
                                                let success_class = if enc.success { "success" } else { "wipe" };

                                                rsx! {
                                                    div {
                                                        class: if is_selected { "sidebar-encounter-item selected" } else { "sidebar-encounter-item" },
                                                        onclick: move |_| selected_encounter.set(Some(enc_idx)),
                                                        div { class: "encounter-main",
                                                            span { class: "encounter-name", "{enc.display_name}" }
                                                            span { class: "result-indicator {success_class}",
                                                                if enc.success {
                                                                    i { class: "fa-solid fa-check" }
                                                                } else {
                                                                    i { class: "fa-solid fa-skull" }
                                                                }
                                                            }
                                                        }
                                                        div { class: "encounter-meta",
                                                            if let Some(time) = &enc.start_time {
                                                                span { class: "encounter-time", "{time}" }
                                                            }
                                                            span { class: "encounter-duration", "({format_duration(enc.duration_seconds)})" }
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
            div { class: "data-panel",
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

                    // Data tab selector (Overview, Damage, Healing, Damage Taken, Healing Taken, Charts)
                    div { class: "data-tab-selector",
                        button {
                            class: if *show_overview.read() && !*show_charts.read() && !*show_combat_log.read() { "data-tab active" } else { "data-tab" },
                            onclick: move |_| { show_overview.set(true); show_charts.set(false); show_combat_log.set(false); },
                            "Overview"
                        }
                        button {
                            class: if !*show_overview.read() && !*show_charts.read() && !*show_combat_log.read() && *selected_tab.read() == DataTab::Damage { "data-tab active" } else { "data-tab" },
                            onclick: move |_| { show_overview.set(false); show_charts.set(false); show_combat_log.set(false); selected_tab.set(DataTab::Damage); },
                            "Damage"
                        }
                        button {
                            class: if !*show_overview.read() && !*show_charts.read() && !*show_combat_log.read() && *selected_tab.read() == DataTab::Healing { "data-tab active" } else { "data-tab" },
                            onclick: move |_| { show_overview.set(false); show_charts.set(false); show_combat_log.set(false); selected_tab.set(DataTab::Healing); },
                            "Healing"
                        }
                        button {
                            class: if !*show_overview.read() && !*show_charts.read() && !*show_combat_log.read() && *selected_tab.read() == DataTab::DamageTaken { "data-tab active" } else { "data-tab" },
                            onclick: move |_| { show_overview.set(false); show_charts.set(false); show_combat_log.set(false); selected_tab.set(DataTab::DamageTaken); },
                            "Damage Taken"
                        }
                        button {
                            class: if !*show_overview.read() && !*show_charts.read() && !*show_combat_log.read() && *selected_tab.read() == DataTab::HealingTaken { "data-tab active" } else { "data-tab" },
                            onclick: move |_| { show_overview.set(false); show_charts.set(false); show_combat_log.set(false); selected_tab.set(DataTab::HealingTaken); },
                            "Healing Taken"
                        }
                        button {
                            class: if *show_charts.read() { "data-tab active" } else { "data-tab" },
                            onclick: move |_| { show_overview.set(false); show_charts.set(true); show_combat_log.set(false); },
                            "Charts"
                        }
                        button {
                            class: if *show_combat_log.read() { "data-tab active" } else { "data-tab" },
                            onclick: move |_| { death_search_text.set(None); show_overview.set(false); show_charts.set(false); show_combat_log.set(true); },
                            "Combat Log"
                        }
                    }

                    // Error display
                    if let Some(err) = error_msg.read().as_ref() {
                        div { class: "error-message", "{err}" }
                    }

                    // Content area - Overview, Charts, Combat Log, or Detailed view
                    if *show_combat_log.read() {
                        // Combat Log Panel
                        if let Some(enc_idx) = *selected_encounter.read() {
                            CombatLog {
                                encounter_idx: enc_idx,
                                time_range: time_range(),
                                initial_search: death_search_text(),
                            }
                        }
                    } else if *show_charts.read() {
                        // Charts Panel
                        if let Some(tl) = timeline.read().as_ref() {
                            ChartsPanel {
                                encounter_idx: *selected_encounter.read(),
                                duration_secs: tl.duration_secs,
                                time_range: time_range(),
                            }
                        }
                    } else if *show_overview.read() {
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
                                                        let time_str = format_duration(death_time as i64);
                                                        rsx! {
                                                            button {
                                                                class: "death-item",
                                                                title: "Click to view 10 seconds before death in Combat Log",
                                                                onclick: {
                                                                    let player_name = name.clone();
                                                                    move |_| {
                                                                        let start = (death_time - 10.0).max(0.0);
                                                                        time_range.set(TimeRange { start, end: death_time });
                                                                        death_search_text.set(Some(player_name.clone()));
                                                                        show_overview.set(false);
                                                                        show_charts.set(false);
                                                                        show_combat_log.set(true);
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
                                let (rows, total_damage, total_dps, total_threat, total_tps,
                                     total_damage_taken, total_dtps, total_aps, total_healing,
                                     total_hps, total_ehps) = overview_table_data();
                                rsx! {
                                    table { class: "overview-table",
                                        thead {
                                            tr {
                                                th { class: "name-col", "Name" }
                                                th { class: "section-header", colspan: "2", "Damage Dealt" }
                                                th { class: "section-header", colspan: "2", "Threat" }
                                                th { class: "section-header", colspan: "3", "Damage Taken" }
                                                th { class: "section-header", colspan: "4", "Healing" }
                                            }
                                            tr { class: "sub-header",
                                                th {}
                                                th { class: "num", "Total" }
                                                th { class: "num", "DPS" }
                                                th { class: "num", "Total" }
                                                th { class: "num", "TPS" }
                                                th { class: "num", "Total" }
                                                th { class: "num", "DTPS" }
                                                th { class: "num", "APS" }
                                                th { class: "num", "Total" }
                                                th { class: "num", "HPS" }
                                                th { class: "num", "%" }
                                                th { class: "num", "EHPS" }
                                            }
                                        }
                                        tbody {
                                            for row in rows.iter() {
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
                                                                    {
                                                                        let class_css = icon_name.trim_end_matches(".png");
                                                                        rsx! {
                                                                            img {
                                                                                class: "class-icon class-{class_css}",
                                                                                src: *icon_asset,
                                                                                title: "{row.discipline_name.as_deref().unwrap_or(\"\")}",
                                                                                alt: ""
                                                                            }
                                                                        }
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
                                                }
                                            }
                                        }
                                        tfoot {
                                            tr { class: "totals-row",
                                                td { class: "name-col", "Group Total" }
                                                td { class: "num dmg", "{format_number(total_damage)}" }
                                                td { class: "num dmg", "{format_number(total_dps)}" }
                                                td { class: "num threat", "{format_number(total_threat)}" }
                                                td { class: "num threat", "{format_number(total_tps)}" }
                                                td { class: "num taken", "{format_number(total_damage_taken)}" }
                                                td { class: "num taken", "{format_number(total_dtps)}" }
                                                td { class: "num taken", "{format_number(total_aps)}" }
                                                td { class: "num heal", "{format_number(total_healing)}" }
                                                td { class: "num heal", "{format_number(total_hps)}" }
                                                td { class: "num heal", "" }
                                                td { class: "num heal", "{format_number(total_ehps)}" }
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
                                }
                            }
                        }
                    } else {
                        // Two-column layout (Detailed breakdown)
                        div { class: "explorer-content",
                            // Entity breakdown (source filter for outgoing, target filter for incoming)
                            div { class: "entity-section",
                                div { class: "entity-header",
                                    h4 {
                                        if selected_tab.read().is_outgoing() { "Sources" } else { "Targets" }
                                    }
                                    div { class: "entity-filter-tabs",
                                        button {
                                            class: if *show_players_only.read() { "filter-tab active" } else { "filter-tab" },
                                            onclick: move |_| show_players_only.set(true),
                                            "Players"
                                        }
                                        button {
                                            class: if !*show_players_only.read() { "filter-tab active" } else { "filter-tab" },
                                            onclick: move |_| show_players_only.set(false),
                                            "All"
                                        }
                                    }
                                }
                                div { class: "entity-list",
                                    // Uses memoized entity_list
                                    for entity in entity_list().iter() {
                                        {
                                            let name = entity.source_name.clone();
                                            let is_selected = selected_source.read().as_ref() == Some(&name);
                                            let is_npc = entity.entity_type == "Npc";
                                            rsx! {
                                                div {
                                                    class: if is_selected { "entity-row selected" } else if is_npc { "entity-row npc" } else { "entity-row" },
                                                    onclick: {
                                                        let name = name.clone();
                                                        move |_| on_source_click(name.clone())
                                                    },
                                                    span { class: "entity-name", "{entity.source_name}" }
                                                    span { class: "entity-value", "{format_number(entity.total_value)}" }
                                                    span { class: "entity-abilities", "{entity.abilities_used} abilities" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Ability breakdown table
                            div { class: "ability-section",
                                // Header with breakdown controls only
                                div { class: "ability-header",
                                    // Breakdown mode toggles (nested hierarchy)
                                    // Labels change based on tab: outgoing uses "Target", incoming uses "Source"
                                    // Instance mode only makes sense for damage tabs (NPCs have multiple spawns)
                                    {
                                        let tab = *selected_tab.read();
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
                                let tab = *selected_tab.read();
                                let show_breakdown_col = mode.by_target_type || mode.by_target_instance;
                                let show_ability_col = mode.by_ability;
                                let breakdown_col_label = if tab.is_outgoing() { "Target" } else { "Source" };
                                let rate_label = tab.rate_label();
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

                                rsx! {
                                    table { class: "ability-table",
                                        thead {
                                            tr {
                                                // First column: Target/Ability (hierarchical) or just Ability
                                                th {
                                                    class: if show_breakdown_col { sort_class(SortColumn::Target) } else { sort_class(SortColumn::Ability) },
                                                    onclick: if show_breakdown_col { sort_click(SortColumn::Target, true) } else { sort_click(SortColumn::Ability, true) },
                                                    if show_breakdown_col {
                                                        "{breakdown_col_label} / Ability"
                                                    } else {
                                                        "Ability"
                                                    }
                                                }
                                                th {
                                                    class: "num {sort_class(SortColumn::Total)}",
                                                    onclick: sort_click(SortColumn::Total, false),
                                                    "Total"
                                                }
                                                th {
                                                    class: "num {sort_class(SortColumn::Percent)}",
                                                    onclick: sort_click(SortColumn::Percent, false),
                                                    "%"
                                                }
                                                th {
                                                    class: "num {sort_class(SortColumn::Rate)}",
                                                    onclick: sort_click(SortColumn::Rate, false),
                                                    "{rate_label}"
                                                }
                                                th {
                                                    class: "num {sort_class(SortColumn::Hits)}",
                                                    onclick: sort_click(SortColumn::Hits, false),
                                                    "Hits"
                                                }
                                                th {
                                                    class: "num {sort_class(SortColumn::Avg)}",
                                                    onclick: sort_click(SortColumn::Avg, false),
                                                    "Avg"
                                                }
                                                th {
                                                    class: "num {sort_class(SortColumn::CritPct)}",
                                                    onclick: sort_click(SortColumn::CritPct, false),
                                                    "Crit%"
                                                }
                                            }
                                        }
                                        tbody {
                                            for (stats, abilities) in grouped_abilities().iter() {
                                                // Group header row (when grouping by target)
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
                                                        td { class: "num group-stat", "{format_number(stats.total)}" }
                                                        td { class: "num group-stat", "{format_pct(stats.percent)}" }
                                                        td { class: "num group-stat", "{format_number(stats.rate)}" }
                                                        td { class: "num group-stat", "{stats.hits}" }
                                                        td { class: "num group-stat", "{format_number(stats.avg)}" }
                                                        td { class: "num group-stat", "{format_pct(stats.crit_pct)}" }
                                                    }
                                                }
                                                // Ability rows (only shown when Ability breakdown is enabled)
                                                if show_ability_col {
                                                    for ability in abilities.iter() {
                                                        tr { class: if stats.target.is_some() { "ability-row indented" } else { "ability-row" },
                                                            td { class: "ability-name-cell", "{ability.ability_name}" }
                                                            td { class: "num", "{format_number(ability.total_value)}" }
                                                            td { class: "num pct-cell",
                                                                span { class: "pct-bar", style: "width: {ability.percent_of_total.min(100.0)}%;" }
                                                                span { class: "pct-text", "{format_pct(ability.percent_of_total)}" }
                                                            }
                                                            td { class: "num", "{format_number(ability.dps)}" }
                                                            td { class: "num", "{ability.hit_count}" }
                                                            td { class: "num", "{format_number(ability.avg_hit)}" }
                                                            td { class: "num", "{format_pct(ability.crit_rate)}" }
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
