//! Charts Panel Component
//!
//! Displays time series charts (DPS, HPS, DTPS) with effect highlighting.
//! Uses ECharts for visualization via wasm-bindgen JS interop.

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local as spawn;

use crate::api::{self, EffectChartData, EffectWindow, TimeRange, TimeSeriesPoint};

// ─────────────────────────────────────────────────────────────────────────────
// ECharts JS Interop
// ─────────────────────────────────────────────────────────────────────────────

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = echarts, js_name = init)]
    fn echarts_init(dom: &web_sys::Element) -> JsValue;

    #[wasm_bindgen(js_namespace = echarts, js_name = getInstanceByDom)]
    fn echarts_get_instance(dom: &web_sys::Element) -> JsValue;
}

fn init_chart(element_id: &str) -> Option<JsValue> {
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

fn resize_chart(chart: &JsValue) {
    let resize = js_sys::Reflect::get(chart, &JsValue::from_str("resize"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok());

    if let Some(func) = resize {
        let _ = func.call0(chart);
    }
}

fn resize_all_charts() {
    for id in ["chart-dps", "chart-hps", "chart-dtps"] {
        if let Some(window) = web_sys::window()
            && let Some(document) = window.document()
            && let Some(element) = document.get_element_by_id(id)
        {
            let instance = echarts_get_instance(&element);
            if !instance.is_null() && !instance.is_undefined() {
                resize_chart(&instance);
            }
        }
    }
}

fn dispose_chart(element_id: &str) {
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

/// Merge overlapping/adjacent windows into continuous regions
fn merge_effect_windows(mut windows: Vec<EffectWindow>) -> Vec<EffectWindow> {
    if windows.is_empty() {
        return windows;
    }
    windows.sort_by(|a, b| a.start_secs.partial_cmp(&b.start_secs).unwrap());
    let mut merged = Vec::with_capacity(windows.len());
    let mut current = windows[0].clone();

    for w in windows.into_iter().skip(1) {
        // If windows overlap or are adjacent, merge them
        if w.start_secs <= current.end_secs {
            current.end_secs = current.end_secs.max(w.end_secs);
        } else {
            merged.push(current);
            current = w;
        }
    }
    merged.push(current);
    merged
}

fn build_time_series_option(
    data: &[TimeSeriesPoint],
    title: &str,
    color: &str,
    fill_color: &str,
    effect_windows: &[(i64, EffectWindow, &str)], // (effect_id, window, color)
    y_axis_name: &str,
) -> JsValue {
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
        &JsValue::from_f64(12.0),
    )
    .unwrap();
    js_sys::Reflect::set(&title_obj, &JsValue::from_str("textStyle"), &title_style).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("title"), &title_obj).unwrap();

    // Grid (leave room for axis labels on both sides)
    let grid = js_sys::Object::new();
    js_sys::Reflect::set(&grid, &JsValue::from_str("left"), &JsValue::from_str("60")).unwrap();
    js_sys::Reflect::set(&grid, &JsValue::from_str("right"), &JsValue::from_str("60")).unwrap();
    js_sys::Reflect::set(&grid, &JsValue::from_str("top"), &JsValue::from_str("35")).unwrap();
    js_sys::Reflect::set(
        &grid,
        &JsValue::from_str("bottom"),
        &JsValue::from_str("25"),
    )
    .unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("grid"), &grid).unwrap();

    // Get min/max time from data to set axis bounds
    let min_time_ms = data.iter().map(|p| p.bucket_start_ms).min().unwrap_or(0);
    let max_time_ms = data.iter().map(|p| p.bucket_start_ms).max().unwrap_or(0);
    let min_time_secs = min_time_ms as f64 / 1000.0;
    let max_time_secs = max_time_ms as f64 / 1000.0;

    // X-Axis (time in seconds) - format as M:SS
    let x_axis = js_sys::Object::new();
    js_sys::Reflect::set(
        &x_axis,
        &JsValue::from_str("type"),
        &JsValue::from_str("value"),
    )
    .unwrap();
    // Set explicit min/max to match data range (only draw x-axis for selected period)
    js_sys::Reflect::set(
        &x_axis,
        &JsValue::from_str("min"),
        &JsValue::from_f64(min_time_secs),
    )
    .unwrap();
    js_sys::Reflect::set(
        &x_axis,
        &JsValue::from_str("max"),
        &JsValue::from_f64(max_time_secs),
    )
    .unwrap();
    let axis_label = js_sys::Object::new();
    js_sys::Reflect::set(
        &axis_label,
        &JsValue::from_str("color"),
        &JsValue::from_str("#888"),
    )
    .unwrap();
    // Formatter function to display M:SS
    let formatter = js_sys::Function::new_with_args(
        "v",
        "var m = Math.floor(v / 60); var s = Math.floor(v % 60); return m + ':' + (s < 10 ? '0' : '') + s;",
    );
    js_sys::Reflect::set(&axis_label, &JsValue::from_str("formatter"), &formatter).unwrap();
    js_sys::Reflect::set(&x_axis, &JsValue::from_str("axisLabel"), &axis_label).unwrap();
    // Hide gridlines
    let x_split = js_sys::Object::new();
    js_sys::Reflect::set(&x_split, &JsValue::from_str("show"), &JsValue::FALSE).unwrap();
    js_sys::Reflect::set(&x_axis, &JsValue::from_str("splitLine"), &x_split).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("xAxis"), &x_axis).unwrap();

    // Dual Y-Axes: Left = raw damage, Right = rate (DPS/HPS)
    let y_axis_arr = js_sys::Array::new();

    // Left Y-Axis (raw damage/healing totals per second)
    let y_axis_left = js_sys::Object::new();
    js_sys::Reflect::set(
        &y_axis_left,
        &JsValue::from_str("type"),
        &JsValue::from_str("value"),
    )
    .unwrap();
    js_sys::Reflect::set(
        &y_axis_left,
        &JsValue::from_str("name"),
        &JsValue::from_str("Burst"),
    )
    .unwrap();
    js_sys::Reflect::set(
        &y_axis_left,
        &JsValue::from_str("position"),
        &JsValue::from_str("left"),
    )
    .unwrap();
    let y_label_left = js_sys::Object::new();
    js_sys::Reflect::set(
        &y_label_left,
        &JsValue::from_str("color"),
        &JsValue::from_str("#666"),
    )
    .unwrap();
    js_sys::Reflect::set(&y_axis_left, &JsValue::from_str("axisLabel"), &y_label_left).unwrap();
    let y_split_left = js_sys::Object::new();
    js_sys::Reflect::set(&y_split_left, &JsValue::from_str("show"), &JsValue::FALSE).unwrap();
    js_sys::Reflect::set(&y_axis_left, &JsValue::from_str("splitLine"), &y_split_left).unwrap();
    y_axis_arr.push(&y_axis_left);

    // Right Y-Axis (rate - DPS/HPS average)
    let y_axis_right = js_sys::Object::new();
    js_sys::Reflect::set(
        &y_axis_right,
        &JsValue::from_str("type"),
        &JsValue::from_str("value"),
    )
    .unwrap();
    js_sys::Reflect::set(
        &y_axis_right,
        &JsValue::from_str("name"),
        &JsValue::from_str(y_axis_name),
    )
    .unwrap();
    js_sys::Reflect::set(
        &y_axis_right,
        &JsValue::from_str("position"),
        &JsValue::from_str("right"),
    )
    .unwrap();
    let y_label_right = js_sys::Object::new();
    js_sys::Reflect::set(
        &y_label_right,
        &JsValue::from_str("color"),
        &JsValue::from_str(color),
    )
    .unwrap();
    js_sys::Reflect::set(
        &y_axis_right,
        &JsValue::from_str("axisLabel"),
        &y_label_right,
    )
    .unwrap();
    let y_split_right = js_sys::Object::new();
    js_sys::Reflect::set(&y_split_right, &JsValue::from_str("show"), &JsValue::FALSE).unwrap();
    js_sys::Reflect::set(
        &y_axis_right,
        &JsValue::from_str("splitLine"),
        &y_split_right,
    )
    .unwrap();
    y_axis_arr.push(&y_axis_right);

    js_sys::Reflect::set(&obj, &JsValue::from_str("yAxis"), &y_axis_arr).unwrap();

    // Tooltip
    let tooltip = js_sys::Object::new();
    js_sys::Reflect::set(
        &tooltip,
        &JsValue::from_str("trigger"),
        &JsValue::from_str("axis"),
    )
    .unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("tooltip"), &tooltip).unwrap();

    // Build time spine: fill ALL seconds within the data range with values (0 if no data)
    // This ensures continuous average calculation even when no events occur
    let bucket_ms: i64 = 1000;

    // Calculate buckets from min to max time (data range)
    let num_buckets = ((max_time_ms - min_time_ms) / bucket_ms + 1) as usize;

    // Create sparse lookup from data
    let sparse: std::collections::HashMap<i64, f64> = data
        .iter()
        .map(|p| (p.bucket_start_ms, p.total_value))
        .collect();

    // Generate dense time series with 0s for missing buckets
    let mut dense_data: Vec<(f64, f64)> = Vec::with_capacity(num_buckets);
    let mut avg_data: Vec<(f64, f64)> = Vec::with_capacity(num_buckets);
    let mut cumulative_sum = 0.0;

    for i in 0..num_buckets {
        let time_ms = min_time_ms + (i as i64) * bucket_ms;
        let time_secs = time_ms as f64 / 1000.0;
        let value = sparse.get(&time_ms).copied().unwrap_or(0.0);

        cumulative_sum += value;
        // Elapsed time since start of this range (for average calculation)
        let elapsed_in_range = (i as f64) + 1.0;
        let avg = (cumulative_sum / elapsed_in_range).round();

        dense_data.push((time_secs, value));
        avg_data.push((time_secs, avg));
    }

    let series_arr = js_sys::Array::new();

    // Series 1: Raw data (thin line with colored fill)
    let series = js_sys::Object::new();
    js_sys::Reflect::set(
        &series,
        &JsValue::from_str("type"),
        &JsValue::from_str("line"),
    )
    .unwrap();
    js_sys::Reflect::set(
        &series,
        &JsValue::from_str("name"),
        &JsValue::from_str("Burst"),
    )
    .unwrap();
    js_sys::Reflect::set(&series, &JsValue::from_str("smooth"), &JsValue::FALSE).unwrap(); // No smoothing for raw data
    js_sys::Reflect::set(
        &series,
        &JsValue::from_str("symbol"),
        &JsValue::from_str("none"),
    )
    .unwrap();
    // Use left Y-axis (index 0) for burst data
    js_sys::Reflect::set(
        &series,
        &JsValue::from_str("yAxisIndex"),
        &JsValue::from_f64(0.0),
    )
    .unwrap();

    // Thin line style for raw data
    let line_style = js_sys::Object::new();
    js_sys::Reflect::set(
        &line_style,
        &JsValue::from_str("color"),
        &JsValue::from_str(color),
    )
    .unwrap();
    js_sys::Reflect::set(
        &line_style,
        &JsValue::from_str("width"),
        &JsValue::from_f64(1.0),
    )
    .unwrap();
    js_sys::Reflect::set(&series, &JsValue::from_str("lineStyle"), &line_style).unwrap();

    // Area style with matching fill color (higher opacity)
    let area_style = js_sys::Object::new();
    js_sys::Reflect::set(
        &area_style,
        &JsValue::from_str("color"),
        &JsValue::from_str(fill_color),
    )
    .unwrap();
    js_sys::Reflect::set(&series, &JsValue::from_str("areaStyle"), &area_style).unwrap();

    // Data points from dense array
    let data_arr = js_sys::Array::new();
    for (x, y) in &dense_data {
        let pair = js_sys::Array::new();
        pair.push(&JsValue::from_f64(*x));
        pair.push(&JsValue::from_f64(*y));
        data_arr.push(&pair);
    }
    js_sys::Reflect::set(&series, &JsValue::from_str("data"), &data_arr).unwrap();

    // Mark areas for effect windows (on raw data series) - vertically stacked per effect
    // Always set markArea (even if empty) to ensure ECharts clears previous highlights
    let mark_area = js_sys::Object::new();
    let mark_data = js_sys::Array::new();

    // Calculate max y value from burst data for bounding mark areas to chart grid
    let max_y_value = dense_data
        .iter()
        .map(|(_, y)| *y)
        .fold(0.0_f64, |a, b| a.max(b));

    // Group windows by effect_id, preserving selection order for consistent lane assignment
    let mut effect_order: Vec<i64> = Vec::new();
    let mut grouped: std::collections::HashMap<i64, (Vec<EffectWindow>, &str)> =
        std::collections::HashMap::new();
    for (eid, window, win_color) in effect_windows.iter() {
        if !effect_order.contains(eid) {
            effect_order.push(*eid);
        }
        grouped
            .entry(*eid)
            .or_insert_with(|| (Vec::new(), *win_color))
            .0
            .push(window.clone());
    }

    let num_effects = effect_order.len();
    for (lane_idx, eid) in effect_order.iter().enumerate() {
        if let Some((windows, win_color)) = grouped.remove(eid) {
            // Merge overlapping windows for this effect
            let merged = merge_effect_windows(windows);

            // Calculate vertical bounds using yAxis data values (bounded to chart area)
            let lane_height = max_y_value / num_effects as f64;
            let y_bottom = lane_idx as f64 * lane_height;
            let y_top = (lane_idx + 1) as f64 * lane_height;

            for window in merged {
                let region = js_sys::Array::new();
                let start = js_sys::Object::new();
                js_sys::Reflect::set(
                    &start,
                    &JsValue::from_str("xAxis"),
                    &JsValue::from_f64(window.start_secs as f64),
                )
                .unwrap();
                // Use yAxis values to bound within chart grid (index 0 = left/burst axis)
                js_sys::Reflect::set(
                    &start,
                    &JsValue::from_str("yAxis"),
                    &JsValue::from_f64(y_top),
                )
                .unwrap();
                // Set per-region itemStyle for individual colors
                let region_style = js_sys::Object::new();
                js_sys::Reflect::set(
                    &region_style,
                    &JsValue::from_str("color"),
                    &JsValue::from_str(win_color),
                )
                .unwrap();
                js_sys::Reflect::set(&start, &JsValue::from_str("itemStyle"), &region_style)
                    .unwrap();
                let end = js_sys::Object::new();
                js_sys::Reflect::set(
                    &end,
                    &JsValue::from_str("xAxis"),
                    &JsValue::from_f64(window.end_secs as f64),
                )
                .unwrap();
                js_sys::Reflect::set(
                    &end,
                    &JsValue::from_str("yAxis"),
                    &JsValue::from_f64(y_bottom),
                )
                .unwrap();
                region.push(&start);
                region.push(&end);
                mark_data.push(&region);
            }
        }
    }
    js_sys::Reflect::set(&mark_area, &JsValue::from_str("data"), &mark_data).unwrap();
    js_sys::Reflect::set(&series, &JsValue::from_str("markArea"), &mark_area).unwrap();

    series_arr.push(&series);

    // Series 2: Moving average (thicker line, no fill)
    let avg_series = js_sys::Object::new();
    js_sys::Reflect::set(
        &avg_series,
        &JsValue::from_str("type"),
        &JsValue::from_str("line"),
    )
    .unwrap();
    js_sys::Reflect::set(
        &avg_series,
        &JsValue::from_str("name"),
        &JsValue::from_str("Average"),
    )
    .unwrap();
    js_sys::Reflect::set(&avg_series, &JsValue::from_str("smooth"), &JsValue::TRUE).unwrap();
    js_sys::Reflect::set(
        &avg_series,
        &JsValue::from_str("symbol"),
        &JsValue::from_str("none"),
    )
    .unwrap();
    // Use right Y-axis (index 1) for average/rate data
    js_sys::Reflect::set(
        &avg_series,
        &JsValue::from_str("yAxisIndex"),
        &JsValue::from_f64(1.0),
    )
    .unwrap();

    // Thicker line style for average
    let avg_line_style = js_sys::Object::new();
    js_sys::Reflect::set(
        &avg_line_style,
        &JsValue::from_str("color"),
        &JsValue::from_str(color),
    )
    .unwrap();
    js_sys::Reflect::set(
        &avg_line_style,
        &JsValue::from_str("width"),
        &JsValue::from_f64(2.5),
    )
    .unwrap();
    js_sys::Reflect::set(
        &avg_series,
        &JsValue::from_str("lineStyle"),
        &avg_line_style,
    )
    .unwrap();

    // Average data points
    let avg_arr = js_sys::Array::new();
    for (x, y) in avg_data {
        let pair = js_sys::Array::new();
        pair.push(&JsValue::from_f64(x));
        pair.push(&JsValue::from_f64(y));
        avg_arr.push(&pair);
    }
    js_sys::Reflect::set(&avg_series, &JsValue::from_str("data"), &avg_arr).unwrap();

    series_arr.push(&avg_series);
    js_sys::Reflect::set(&obj, &JsValue::from_str("series"), &series_arr).unwrap();

    // Animation
    js_sys::Reflect::set(&obj, &JsValue::from_str("animation"), &JsValue::FALSE).unwrap();

    obj.into()
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

fn format_duration(secs: f32) -> String {
    let total_secs = secs as i32;
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{}:{:02}", mins, secs)
}

fn format_pct(pct: f32) -> String {
    format!("{:.1}%", pct)
}

// ─────────────────────────────────────────────────────────────────────────────
// Component
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Props, Clone, PartialEq)]
pub struct ChartsPanelProps {
    /// Encounter index (None = live)
    pub encounter_idx: Option<u32>,
    /// Total duration in seconds
    pub duration_secs: f32,
    /// Time range filter
    pub time_range: TimeRange,
}

#[component]
pub fn ChartsPanel(props: ChartsPanelProps) -> Element {
    // Mirror time_range prop into a signal for reactivity
    let mut time_range_signal = use_signal(|| props.time_range.clone());

    // Update signal when props change (runs on every render with new props)
    if *time_range_signal.read() != props.time_range {
        time_range_signal.set(props.time_range.clone());
    }

    // Entity selection (default to none - show aggregated data)
    let mut selected_entity = use_signal(|| None::<String>);
    let mut entities = use_signal(Vec::<String>::new);

    // Chart visibility toggles
    let mut show_dps = use_signal(|| true);
    let mut show_hps = use_signal(|| true);
    let mut show_dtps = use_signal(|| true);

    // Time series data
    let mut dps_data = use_signal(Vec::<TimeSeriesPoint>::new);
    let mut hps_data = use_signal(Vec::<TimeSeriesPoint>::new);
    let mut dtps_data = use_signal(Vec::<TimeSeriesPoint>::new);

    // Effect data
    let mut active_effects = use_signal(Vec::<EffectChartData>::new);
    let mut passive_effects = use_signal(Vec::<EffectChartData>::new);
    // Multiple selected effects with assigned colors
    let mut selected_effects = use_signal(Vec::<(i64, &'static str)>::new);
    // (effect_id, window, color) - includes effect_id for grouping/stacking
    let mut effect_windows = use_signal(Vec::<(i64, EffectWindow, &'static str)>::new);

    // Loading state
    let mut loading = use_signal(|| false);

    // Bucket size for time series (1 second)
    let bucket_ms: i64 = 1000;

    // Effect highlight colors (for multiple selections)
    const EFFECT_COLORS: [&str; 6] = [
        "rgba(255, 200, 50, 0.35)",  // Gold
        "rgba(100, 200, 255, 0.35)", // Cyan
        "rgba(255, 100, 150, 0.35)", // Pink
        "rgba(150, 255, 100, 0.35)", // Lime
        "rgba(200, 150, 255, 0.35)", // Purple
        "rgba(255, 180, 100, 0.35)", // Orange
    ];

    // Load entities on mount and auto-select first player (with retry for race conditions)
    use_effect({
        let idx = props.encounter_idx;
        move || {
            spawn(async move {
                // Retry up to 3 seconds if data not ready
                for attempt in 0..10 {
                    if let Some(data) = api::query_raid_overview(idx, None, None).await {
                        let names: Vec<String> = data
                            .into_iter()
                            .filter(|r| r.entity_type == "Player" || r.entity_type == "Companion")
                            .map(|r| r.name)
                            .collect();
                        if !names.is_empty() {
                            // Auto-select first player
                            if let Some(first) = names.first() {
                                selected_entity.set(Some(first.clone()));
                            }
                            entities.set(names);
                            return;
                        }
                    }
                    if attempt < 9 {
                        gloo_timers::future::TimeoutFuture::new(300).await;
                    }
                }
            });
        }
    });

    // Load time series data when entity or time range changes
    use_effect(move || {
        let idx = props.encounter_idx;
        let tr = time_range_signal.read().clone();
        let entity = selected_entity.read().clone();

        spawn(async move {
            loading.set(true);

            // Fetch all three time series
            let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
                None
            } else {
                Some(&tr)
            };

            if let Some(data) =
                api::query_dps_over_time(idx, bucket_ms, entity.as_deref(), tr_opt).await
            {
                dps_data.set(data);
            }
            if let Some(data) =
                api::query_hps_over_time(idx, bucket_ms, entity.as_deref(), tr_opt).await
            {
                hps_data.set(data);
            }
            if let Some(data) =
                api::query_dtps_over_time(idx, bucket_ms, entity.as_deref(), tr_opt).await
            {
                dtps_data.set(data);
            }

            loading.set(false);
        });
    });

    // Load effect uptime data when entity or time range changes
    use_effect(move || {
        let idx = props.encounter_idx;
        let duration = props.duration_secs;
        let tr = time_range_signal.read().clone();
        let entity = selected_entity.read().clone();

        spawn(async move {
            let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
                None
            } else {
                Some(&tr)
            };

            if let Some(data) =
                api::query_effect_uptime(idx, entity.as_deref(), tr_opt, duration).await
            {
                let (active, passive): (Vec<_>, Vec<_>) =
                    data.into_iter().partition(|e| e.is_active);
                active_effects.set(active);
                passive_effects.set(passive);
            }
        });
    });

    // Load effect windows when selected effects or time range changes
    use_effect(move || {
        let idx = props.encounter_idx;
        let duration = props.duration_secs;
        let tr = time_range_signal.read().clone();
        let effects = selected_effects.read().clone();
        let entity = selected_entity.read().clone();

        if effects.is_empty() {
            effect_windows.set(Vec::new());
        } else {
            spawn(async move {
                let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
                    None
                } else {
                    Some(&tr)
                };
                let mut all_windows = Vec::new();
                for (eid, color) in effects {
                    if let Some(windows) =
                        api::query_effect_windows(idx, eid, entity.as_deref(), tr_opt, duration)
                            .await
                    {
                        for w in windows {
                            all_windows.push((eid, w, color));
                        }
                    }
                }
                effect_windows.set(all_windows);
            });
        }
    });

    // Update charts when data changes - read signals inside effect to track dependencies
    use_effect(move || {
        // Read all reactive signals to establish dependencies
        let show_dps_val = *show_dps.read();
        let show_hps_val = *show_hps.read();
        let show_dtps_val = *show_dtps.read();
        let dps = dps_data.read().clone();
        let hps = hps_data.read().clone();
        let dtps = dtps_data.read().clone();
        let windows = effect_windows.read().clone();

        // Dispose hidden charts immediately to prevent overlap
        if !show_dps_val {
            dispose_chart("chart-dps");
        }
        if !show_hps_val {
            dispose_chart("chart-hps");
        }
        if !show_dtps_val {
            dispose_chart("chart-dtps");
        }

        spawn(async move {
            // Delay to ensure DOM elements exist after render
            gloo_timers::future::TimeoutFuture::new(150).await;

            if show_dps_val
                && !dps.is_empty()
                && let Some(chart) = init_chart("chart-dps")
            {
                let option = build_time_series_option(
                    &dps,
                    "DPS",
                    "#e74c3c",
                    "rgba(231, 76, 60, 0.15)",
                    &windows,
                    "DPS",
                );
                set_chart_option(&chart, &option);
            }

            if show_hps_val
                && !hps.is_empty()
                && let Some(chart) = init_chart("chart-hps")
            {
                let option = build_time_series_option(
                    &hps,
                    "HPS",
                    "#2ecc71",
                    "rgba(46, 204, 113, 0.15)",
                    &windows,
                    "HPS",
                );
                set_chart_option(&chart, &option);
            }

            if show_dtps_val
                && !dtps.is_empty()
                && let Some(chart) = init_chart("chart-dtps")
            {
                let option = build_time_series_option(
                    &dtps,
                    "DTPS",
                    "#e67e22",
                    "rgba(230, 126, 34, 0.15)",
                    &windows,
                    "DTPS",
                );
                set_chart_option(&chart, &option);
            }

            // Resize all visible charts after DOM has settled
            gloo_timers::future::TimeoutFuture::new(50).await;
            resize_all_charts();
        });
    });

    // Window resize listener - resize all ECharts instances
    use_effect(|| {
        use wasm_bindgen::closure::Closure;

        let closure = Closure::wrap(Box::new(move || {
            resize_all_charts();
        }) as Box<dyn Fn()>);

        if let Some(window) = web_sys::window() {
            let _ =
                window.add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref());
        }

        // Keep closure alive and remove listener on cleanup
        closure.forget();
    });

    // Cleanup charts on unmount
    use_drop(move || {
        dispose_chart("chart-dps");
        dispose_chart("chart-hps");
        dispose_chart("chart-dtps");
    });

    let entity_list = entities.read().clone();
    let active = active_effects.read().clone();
    let passive = passive_effects.read().clone();
    let current_effects = selected_effects.read().clone();

    let dps_empty = dps_data.read().is_empty();
    let hps_empty = hps_data.read().is_empty();
    let dtps_empty = dtps_data.read().is_empty();

    rsx! {
        div { class: "charts-panel",
            // Entity sidebar
            aside { class: "charts-sidebar",
                div { class: "sidebar-section",
                    h4 { "Player" }
                    div { class: "entity-list",
                        for name in entity_list.iter() {
                            {
                                let n = name.clone();
                                let is_selected = selected_entity.read().as_ref() == Some(&n);
                                rsx! {
                                    div {
                                        class: if is_selected { "entity-item selected" } else { "entity-item" },
                                        onclick: {
                                            let n = n.clone();
                                            move |_| {
                                                let current = selected_entity.read().clone();
                                                if current.as_ref() == Some(&n) {
                                                    selected_entity.set(None);
                                                } else {
                                                    selected_entity.set(Some(n.clone()));
                                                }
                                            }
                                        },
                                        "{name}"
                                    }
                                }
                            }
                        }
                    }
                }
                div { class: "sidebar-section",
                    h4 { "Charts" }
                    div { class: "chart-toggles",
                        label {
                            input {
                                r#type: "checkbox",
                                checked: *show_dps.read(),
                                onchange: move |e| show_dps.set(e.checked())
                            }
                            span { class: "toggle-dps", "DPS" }
                        }
                        label {
                            input {
                                r#type: "checkbox",
                                checked: *show_hps.read(),
                                onchange: move |e| show_hps.set(e.checked())
                            }
                            span { class: "toggle-hps", "HPS" }
                        }
                        label {
                            input {
                                r#type: "checkbox",
                                checked: *show_dtps.read(),
                                onchange: move |e| show_dtps.set(e.checked())
                            }
                            span { class: "toggle-dtps", "DTPS" }
                        }
                    }
                }
            }

            // Main content area (charts + effects below)
            div { class: "charts-main",
                // Charts area
                div { class: "charts-area",
                    if *loading.read() {
                        div { class: "charts-loading",
                            i { class: "fa-solid fa-spinner fa-spin" }
                            " Loading..."
                        }
                    }
                    if *show_dps.read() {
                        if dps_empty && !*loading.read() {
                            div { class: "chart-empty", "No damage dealt in fight" }
                        } else {
                            div { id: "chart-dps", class: "chart-container" }
                        }
                    }
                    if *show_hps.read() {
                        if hps_empty && !*loading.read() {
                            div { class: "chart-empty", "No healing in fight" }
                        } else {
                            div { id: "chart-hps", class: "chart-container" }
                        }
                    }
                    if *show_dtps.read() {
                        if dtps_empty && !*loading.read() {
                            div { class: "chart-empty", "No damage taken in fight" }
                        } else {
                            div { id: "chart-dtps", class: "chart-container" }
                        }
                    }
                }

                // Effects section (below charts)
                div { class: "effects-row",
                    // Active effects
                    div { class: "effects-section",
                        h4 { "Active Effects" }
                        if active.is_empty() {
                            div { class: "effects-empty", "No active effects" }
                        } else {
                            div { class: "effect-table-wrapper",
                                table { class: "effect-table",
                                thead {
                                    tr {
                                        th { "Effect" }
                                        th { class: "num", "Procs" }
                                        th { class: "num", "Uptime" }
                                        th { class: "num", "%" }
                                    }
                                }
                                tbody {
                                    for effect in active.iter() {
                                        {
                                            let eid = effect.effect_id;
                                            let selected_color = current_effects.iter().find(|(id, _)| *id == eid).map(|(_, c)| *c);
                                            let is_selected = selected_color.is_some();
                                            rsx! {
                                                tr {
                                                    class: if is_selected { "selected" } else { "" },
                                                    style: if let Some(c) = selected_color { format!("--effect-color: {c};") } else { String::new() },
                                                    onclick: move |_| {
                                                        let mut effects = selected_effects.read().clone();
                                                        if let Some(pos) = effects.iter().position(|(id, _)| *id == eid) {
                                                            effects.remove(pos);
                                                        } else {
                                                            let next_color = EFFECT_COLORS[effects.len() % EFFECT_COLORS.len()];
                                                            effects.push((eid, next_color));
                                                        }
                                                        selected_effects.set(effects);
                                                    },
                                                    td { "{effect.effect_name}" }
                                                    td { class: "num", "{effect.count}" }
                                                    td { class: "num", "{format_duration(effect.total_duration_secs)}" }
                                                    td { class: "num", "{format_pct(effect.uptime_pct)}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            }
                        }
                    }

                    // Passive effects
                    div { class: "effects-section",
                        h4 { "Passive Effects" }
                        if passive.is_empty() {
                            div { class: "effects-empty", "No passive effects" }
                        } else {
                            div { class: "effect-table-wrapper",
                                table { class: "effect-table",
                                    thead {
                                        tr {
                                            th { "Effect" }
                                            th { class: "num", "Procs" }
                                            th { class: "num", "Uptime" }
                                            th { class: "num", "%" }
                                        }
                                    }
                                    tbody {
                                    for effect in passive.iter() {
                                        {
                                            let eid = effect.effect_id;
                                            let selected_color = current_effects.iter().find(|(id, _)| *id == eid).map(|(_, c)| *c);
                                            let is_selected = selected_color.is_some();
                                            rsx! {
                                                tr {
                                                    class: if is_selected { "selected" } else { "" },
                                                    style: if let Some(c) = selected_color { format!("--effect-color: {c};") } else { String::new() },
                                                    onclick: move |_| {
                                                        let mut effects = selected_effects.read().clone();
                                                        if let Some(pos) = effects.iter().position(|(id, _)| *id == eid) {
                                                            effects.remove(pos);
                                                        } else {
                                                            let next_color = EFFECT_COLORS[effects.len() % EFFECT_COLORS.len()];
                                                            effects.push((eid, next_color));
                                                        }
                                                        selected_effects.set(effects);
                                                    },
                                                    td { "{effect.effect_name}" }
                                                    td { class: "num", "{effect.count}" }
                                                    td { class: "num", "{format_duration(effect.total_duration_secs)}" }
                                                    td { class: "num", "{format_pct(effect.uptime_pct)}" }
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
