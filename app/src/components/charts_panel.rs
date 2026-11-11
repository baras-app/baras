//! Charts Panel Component
//!
//! Displays time series charts (DPS, HPS, DTPS) with effect highlighting.
//! Uses ECharts for visualization via wasm-bindgen JS interop.

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local as spawn;

use crate::api::{self, EffectChartData, EffectWindow, HpPoint, TimeRange, TimeSeriesPoint};
use crate::components::ability_icon::AbilityIcon;
use crate::components::echarts::{
    dispose_chart, get_chart, init_chart, next_animation_frame, resize_chart, set_chart_option,
};
use crate::utils::{css_var, css_var_or, js_set};
use baras_types::formatting;

/// Sort column for the effects table.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
enum EffectSort {
    Name,
    Applications,
    Uptime,
    #[default]
    Pct,
}

fn resize_all_charts() {
    for id in ["chart-dps", "chart-hps", "chart-dtps", "chart-hp"] {
        if let Some(chart) = get_chart(id) {
            resize_chart(&chart);
        }
    }
}

/// Merge overlapping/adjacent windows into continuous regions
fn merge_effect_windows(mut windows: Vec<EffectWindow>) -> Vec<EffectWindow> {
    if windows.is_empty() {
        return windows;
    }
    windows.sort_by(|a, b| {
        a.start_secs
            .partial_cmp(&b.start_secs)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
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

/// Build the ECharts `markArea` object for effect-highlight lanes. Windows are
/// grouped by effect (preserving selection order), merged, and stacked into
/// vertical lanes scaled to `y_scale` (burst max for rate charts, 100 for HP%).
/// Returns an object with an (possibly empty) `data` array — an empty array
/// clears any previous highlights when merged into an existing series.
fn build_mark_area_object(
    effect_windows: &[(i64, EffectWindow, &str)],
    y_scale: f64,
) -> js_sys::Object {
    let mark_area = js_sys::Object::new();
    let mark_data = js_sys::Array::new();

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
            let merged = merge_effect_windows(windows);
            let lane_height = y_scale / num_effects as f64;
            let y_bottom = lane_idx as f64 * lane_height;
            let y_top = (lane_idx + 1) as f64 * lane_height;

            for window in merged {
                let region = js_sys::Array::new();
                let start = js_sys::Object::new();
                js_set(&start, "xAxis", &JsValue::from_f64(window.start_secs as f64));
                js_set(&start, "yAxis", &JsValue::from_f64(y_top));
                let region_style = js_sys::Object::new();
                js_set(&region_style, "color", &JsValue::from_str(win_color));
                js_set(&start, "itemStyle", &region_style);
                let end = js_sys::Object::new();
                js_set(&end, "xAxis", &JsValue::from_f64(window.end_secs as f64));
                js_set(&end, "yAxis", &JsValue::from_f64(y_bottom));
                region.push(&start);
                region.push(&end);
                mark_data.push(&region);
            }
        }
    }
    js_set(&mark_area, "data", &mark_data);
    mark_area
}

/// Update ONLY the effect-highlight `markArea` on a chart's burst series (index 0)
/// via a merging `setOption`, leaving the (expensive) series data untouched. This
/// is what makes effect-selection changes cheap instead of a full chart rebuild.
fn apply_effect_markareas(chart: &JsValue, effect_windows: &[(i64, EffectWindow, &str)], y_scale: f64) {
    let mark_area = build_mark_area_object(effect_windows, y_scale);
    let series0 = js_sys::Object::new();
    js_set(&series0, "markArea", &mark_area);
    let series_arr = js_sys::Array::new();
    series_arr.push(&series0);
    let opt = js_sys::Object::new();
    js_set(&opt, "series", &series_arr);
    set_chart_option(chart, &opt.into());
}

/// Await one animation frame — used in place of a fixed sleep to wait for the DOM
/// to be laid out/painted before initializing charts.
fn build_time_series_option(
    data: &[TimeSeriesPoint],
    secondary_data: Option<&[TimeSeriesPoint]>,
    title: &str,
    color: &str,
    secondary_color: Option<&str>,
    fill_color: &str,
    effect_windows: &[(i64, EffectWindow, &str)], // (effect_id, window, color)
    y_axis_name: &str,
    animate: bool,
) -> JsValue {
    let obj = js_sys::Object::new();

    // Title
    let title_obj = js_sys::Object::new();
    js_set(&title_obj, "text", &JsValue::from_str(title));
    js_set(&title_obj, "left", &JsValue::from_str("center"));
    let title_style = js_sys::Object::new();
    js_set(&title_style, "color", &JsValue::from_str(&css_var("--text-primary")));
    js_set(&title_style, "fontSize", &JsValue::from_f64(12.0));
    js_set(&title_obj, "textStyle", &title_style);
    js_set(&obj, "title", &title_obj);

    // Grid (leave room for axis labels on both sides)
    let grid = js_sys::Object::new();
    js_set(&grid, "left", &JsValue::from_str("60"));
    js_set(&grid, "right", &JsValue::from_str("60"));
    js_set(&grid, "top", &JsValue::from_str("35"));
    js_set(&grid, "bottom", &JsValue::from_str("25"));
    js_set(&obj, "grid", &grid);

    // Get min/max time from data to set axis bounds
    let min_time_ms = data.iter().map(|p| p.bucket_start_ms).min().unwrap_or(0);
    let max_time_ms = data.iter().map(|p| p.bucket_start_ms).max().unwrap_or(0);
    let min_time_secs = min_time_ms as f64 / 1000.0;
    let max_time_secs = max_time_ms as f64 / 1000.0;

    // X-Axis (time in seconds) - format as M:SS
    let x_axis = js_sys::Object::new();
    js_set(&x_axis, "type", &JsValue::from_str("value"));
    // Set explicit min/max to match data range (only draw x-axis for selected period)
    js_set(&x_axis, "min", &JsValue::from_f64(min_time_secs));
    js_set(&x_axis, "max", &JsValue::from_f64(max_time_secs));
    let axis_label = js_sys::Object::new();
    js_set(&axis_label, "color", &JsValue::from_str(&css_var("--text-muted")));
    // Formatter function to display M:SS
    let formatter = js_sys::Function::new_with_args(
        "v",
        "var m = Math.floor(v / 60); var s = Math.floor(v % 60); return m + ':' + (s < 10 ? '0' : '') + s;",
    );
    js_set(&axis_label, "formatter", &formatter);
    js_set(&x_axis, "axisLabel", &axis_label);
    // Hide gridlines
    let x_split = js_sys::Object::new();
    js_set(&x_split, "show", &JsValue::FALSE);
    js_set(&x_axis, "splitLine", &x_split);
    js_set(&obj, "xAxis", &x_axis);

    // Dual Y-Axes: Left = raw damage, Right = rate (DPS/HPS)
    let y_axis_arr = js_sys::Array::new();

    // Left Y-Axis (raw damage/healing totals per second)
    let y_axis_left = js_sys::Object::new();
    js_set(&y_axis_left, "type", &JsValue::from_str("value"));
    js_set(&y_axis_left, "name", &JsValue::from_str("Burst"));
    js_set(&y_axis_left, "position", &JsValue::from_str("left"));
    let y_label_left = js_sys::Object::new();
    js_set(&y_label_left, "color", &JsValue::from_str(&css_var("--text-disabled")));
    js_set(&y_axis_left, "axisLabel", &y_label_left);
    let y_split_left = js_sys::Object::new();
    js_set(&y_split_left, "show", &JsValue::FALSE);
    js_set(&y_axis_left, "splitLine", &y_split_left);
    y_axis_arr.push(&y_axis_left);

    // Right Y-Axis (rate - DPS/HPS average)
    let y_axis_right = js_sys::Object::new();
    js_set(&y_axis_right, "type", &JsValue::from_str("value"));
    js_set(&y_axis_right, "name", &JsValue::from_str(y_axis_name));
    js_set(&y_axis_right, "position", &JsValue::from_str("right"));
    let y_label_right = js_sys::Object::new();
    js_set(&y_label_right, "color", &JsValue::from_str(color));
    js_set(&y_axis_right, "axisLabel", &y_label_right);
    let y_split_right = js_sys::Object::new();
    js_set(&y_split_right, "show", &JsValue::FALSE);
    js_set(&y_axis_right, "splitLine", &y_split_right);
    y_axis_arr.push(&y_axis_right);

    js_set(&obj, "yAxis", &y_axis_arr);

    // Tooltip
    let tooltip = js_sys::Object::new();
    js_set(&tooltip, "trigger", &JsValue::from_str("axis"));
    let tip_formatter = js_sys::Function::new_with_args(
        "params",
        concat!(
            "if (!params || !params.length) return '';",
            "var t = params[0].value[0];",
            "var m = Math.floor(t / 60);",
            "var s = Math.floor(t % 60);",
            "var time = m + ':' + (s < 10 ? '0' : '') + s;",
            "var r = '<div style=\"margin-bottom:4px\">' + time + '</div>';",
            "for (var i = 0; i < params.length; i++) {",
            "  var p = params[i];",
            "  var val = Math.round(p.value[1]).toLocaleString();",
            "  if (p.seriesName === 'Burst') {",
            "    r += '<div>' + p.seriesName",
            "      + '<span style=\"float:right;margin-left:20px;font-weight:bold\">'",
            "      + val + '</span></div>';",
            "  } else {",
            "    r += '<div><span style=\"display:inline-block;width:10px;height:10px;",
            "border-radius:50%;background:' + p.color + ';margin-right:5px\"></span>'",
            "      + p.seriesName",
            "      + '<span style=\"float:right;margin-left:20px;font-weight:bold\">'",
            "      + val + '</span></div>';",
            "  }",
            "}",
            "return r;",
        ),
    );
    js_set(&tooltip, "formatter", &tip_formatter);
    js_set(&obj, "tooltip", &tooltip);

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
    js_set(&series, "type", &JsValue::from_str("line"));
    js_set(&series, "name", &JsValue::from_str("Burst"));
    js_set(&series, "sampling", &JsValue::from_str("lttb")); // downsample to pixels
    js_set(&series, "smooth", &JsValue::FALSE); // No smoothing for raw data
    js_set(&series, "symbol", &JsValue::from_str("none"));
    // Use left Y-axis (index 0) for burst data
    js_set(&series, "yAxisIndex", &JsValue::from_f64(0.0));

    // Thin line style for raw data
    let line_style = js_sys::Object::new();
    js_set(&line_style, "color", &JsValue::from_str(color));
    js_set(&line_style, "width", &JsValue::from_f64(1.0));
    js_set(&series, "lineStyle", &line_style);
    let item_style = js_sys::Object::new();
    js_set(&item_style, "color", &JsValue::from_str(color));
    js_set(&series, "itemStyle", &item_style);

    // Area style with matching fill color (higher opacity)
    let area_style = js_sys::Object::new();
    js_set(&area_style, "color", &JsValue::from_str(fill_color));
    js_set(&series, "areaStyle", &area_style);

    // Data points from dense array
    let data_arr = js_sys::Array::new();
    for (x, y) in &dense_data {
        let pair = js_sys::Array::new();
        pair.push(&JsValue::from_f64(*x));
        pair.push(&JsValue::from_f64(*y));
        data_arr.push(&pair);
    }
    js_set(&series, "data", &data_arr);

    // Mark areas for effect windows (on raw data series), stacked per effect and
    // bounded to the burst data's max so they sit inside the grid. Always set
    // (even if empty) so ECharts clears previous highlights.
    let max_y_value = dense_data.iter().map(|(_, y)| *y).fold(0.0_f64, f64::max);
    let mark_area = build_mark_area_object(effect_windows, max_y_value);
    js_set(&series, "markArea", &mark_area);

    series_arr.push(&series);

    // Series 2: Moving average (thicker line, no fill)
    let avg_series = js_sys::Object::new();
    js_set(&avg_series, "type", &JsValue::from_str("line"));
    js_set(&avg_series, "name", &JsValue::from_str("Average"));
    js_set(&avg_series, "sampling", &JsValue::from_str("lttb"));
    js_set(&avg_series, "smooth", &JsValue::TRUE);
    js_set(&avg_series, "symbol", &JsValue::from_str("none"));
    // Use right Y-axis (index 1) for average/rate data
    js_set(&avg_series, "yAxisIndex", &JsValue::from_f64(1.0));

    // Thicker line style for average
    let avg_line_style = js_sys::Object::new();
    js_set(&avg_line_style, "color", &JsValue::from_str(color));
    js_set(&avg_line_style, "width", &JsValue::from_f64(2.5));
    js_set(&avg_series, "lineStyle", &avg_line_style);
    let avg_item_style = js_sys::Object::new();
    js_set(&avg_item_style, "color", &JsValue::from_str(color));
    js_set(&avg_series, "itemStyle", &avg_item_style);

    // Average data points
    let avg_arr = js_sys::Array::new();
    for (x, y) in avg_data {
        let pair = js_sys::Array::new();
        pair.push(&JsValue::from_f64(x));
        pair.push(&JsValue::from_f64(y));
        avg_arr.push(&pair);
    }
    js_set(&avg_series, "data", &avg_arr);

    series_arr.push(&avg_series);

    // Optional secondary series (e.g., EHPS alongside raw HPS)
    if let (Some(sec_data), Some(sec_color)) = (secondary_data, secondary_color) {
        // Build secondary sparse lookup
        let sec_sparse: std::collections::HashMap<i64, f64> = sec_data
            .iter()
            .map(|p| (p.bucket_start_ms, p.total_value))
            .collect();

        // Generate secondary average from the same time spine
        let mut sec_avg_data: Vec<(f64, f64)> = Vec::with_capacity(num_buckets);
        let mut sec_cumulative = 0.0;
        for i in 0..num_buckets {
            let time_ms = min_time_ms + (i as i64) * bucket_ms;
            let time_secs = time_ms as f64 / 1000.0;
            let value = sec_sparse.get(&time_ms).copied().unwrap_or(0.0);
            sec_cumulative += value;
            let elapsed_in_range = (i as f64) + 1.0;
            let avg = (sec_cumulative / elapsed_in_range).round();
            sec_avg_data.push((time_secs, avg));
        }

        let sec_series = js_sys::Object::new();
        js_set(&sec_series, "type", &JsValue::from_str("line"));
        js_set(&sec_series, "name", &JsValue::from_str("Effective"));
        js_set(&sec_series, "sampling", &JsValue::from_str("lttb"));
        js_set(&sec_series, "smooth", &JsValue::TRUE);
        js_set(&sec_series, "symbol", &JsValue::from_str("none"));
        js_set(&sec_series, "yAxisIndex", &JsValue::from_f64(1.0));

        let sec_line_style = js_sys::Object::new();
        js_set(&sec_line_style, "color", &JsValue::from_str(sec_color));
        js_set(&sec_line_style, "width", &JsValue::from_f64(2.5));
        js_set(&sec_series, "lineStyle", &sec_line_style);
        let sec_item_style = js_sys::Object::new();
        js_set(&sec_item_style, "color", &JsValue::from_str(sec_color));
        js_set(&sec_series, "itemStyle", &sec_item_style);

        let sec_arr = js_sys::Array::new();
        for (x, y) in sec_avg_data {
            let pair = js_sys::Array::new();
            pair.push(&JsValue::from_f64(x));
            pair.push(&JsValue::from_f64(y));
            sec_arr.push(&pair);
        }
        js_set(&sec_series, "data", &sec_arr);
        series_arr.push(&sec_series);
    }

    js_set(&obj, "series", &series_arr);

    // Animation — enabled during live query for smooth chart growth
    js_set(&obj, "animation", &JsValue::from_bool(animate));
    if animate {
        js_set(&obj, "animationDuration", &JsValue::from_f64(500.0));
        js_set(&obj, "animationEasing", &JsValue::from_str("linear"));
    }

    // Brush selection for drag-to-select time range
    apply_brush_config(&obj);

    obj.into()
}

/// Build HP% chart option — dual y-axis: left = HP% (0–100%), right = EHT (effective healing taken).
/// HP% is a gold line with gradient fill. EHT is a green burst line with area fill on the right axis.
/// Data points carry [time_secs, hp_pct, current_hp, max_hp] for rich tooltips.
fn build_hp_chart_option(
    data: &[HpPoint],
    eht_data: Option<&[TimeSeriesPoint]>,
    effect_windows: &[(i64, EffectWindow, &str)],
    animate: bool,
) -> JsValue {
    let obj = js_sys::Object::new();

    // Title
    let title_obj = js_sys::Object::new();
    js_set(&title_obj, "text", &JsValue::from_str("HP%"));
    js_set(&title_obj, "left", &JsValue::from_str("center"));
    let title_style = js_sys::Object::new();
    js_set(&title_style, "color", &JsValue::from_str(&css_var("--text-primary")));
    js_set(&title_style, "fontSize", &JsValue::from_f64(12.0));
    js_set(&title_obj, "textStyle", &title_style);
    js_set(&obj, "title", &title_obj);

    // Grid (right margin matches other dual-axis charts for alignment)
    let grid = js_sys::Object::new();
    js_set(&grid, "left", &JsValue::from_str("60"));
    js_set(&grid, "right", &JsValue::from_str("60"));
    js_set(&grid, "top", &JsValue::from_str("35"));
    js_set(&grid, "bottom", &JsValue::from_str("25"));
    js_set(&obj, "grid", &grid);

    let min_time_ms = data.iter().map(|p| p.bucket_start_ms).min().unwrap_or(0);
    let max_time_ms = data.iter().map(|p| p.bucket_start_ms).max().unwrap_or(0);
    let min_time_secs = min_time_ms as f64 / 1000.0;
    let max_time_secs = max_time_ms as f64 / 1000.0;

    // X-Axis
    let x_axis = js_sys::Object::new();
    js_set(&x_axis, "type", &JsValue::from_str("value"));
    js_set(&x_axis, "min", &JsValue::from_f64(min_time_secs));
    js_set(&x_axis, "max", &JsValue::from_f64(max_time_secs));
    let axis_label = js_sys::Object::new();
    js_set(&axis_label, "color", &JsValue::from_str(&css_var("--text-muted")));
    let formatter = js_sys::Function::new_with_args(
        "v",
        "var m = Math.floor(v / 60); var s = Math.floor(v % 60); return m + ':' + (s < 10 ? '0' : '') + s;",
    );
    js_set(&axis_label, "formatter", &formatter);
    js_set(&x_axis, "axisLabel", &axis_label);
    let x_split = js_sys::Object::new();
    js_set(&x_split, "show", &JsValue::FALSE);
    js_set(&x_axis, "splitLine", &x_split);
    js_set(&obj, "xAxis", &x_axis);

    // Dual Y-Axes: Left = HP% (0–100%), Right = EHT (auto-scaled)
    let y_axis_arr = js_sys::Array::new();

    // Left Y-Axis: HP%
    let y_axis_left = js_sys::Object::new();
    js_set(&y_axis_left, "type", &JsValue::from_str("value"));
    js_set(&y_axis_left, "name", &JsValue::from_str("HP%"));
    js_set(&y_axis_left, "position", &JsValue::from_str("left"));
    js_set(&y_axis_left, "min", &JsValue::from_f64(0.0));
    js_set(&y_axis_left, "max", &JsValue::from_f64(100.0));
    let y_label_left = js_sys::Object::new();
    js_set(&y_label_left, "color", &JsValue::from_str(&css_var_or("--lt-yellow", "#f1c40f")));
    js_set(&y_axis_left, "axisLabel", &y_label_left);
    let y_split_left = js_sys::Object::new();
    js_set(&y_split_left, "show", &JsValue::FALSE);
    js_set(&y_axis_left, "splitLine", &y_split_left);
    y_axis_arr.push(&y_axis_left);

    // Right Y-Axis: EHT (effective healing taken)
    let y_axis_right = js_sys::Object::new();
    js_set(&y_axis_right, "type", &JsValue::from_str("value"));
    js_set(&y_axis_right, "name", &JsValue::from_str("EHT"));
    js_set(&y_axis_right, "position", &JsValue::from_str("right"));
    let y_label_right = js_sys::Object::new();
    js_set(&y_label_right, "color", &JsValue::from_str(&css_var_or("--lt-green", "#2ecc71")));
    js_set(&y_axis_right, "axisLabel", &y_label_right);
    let y_split_right = js_sys::Object::new();
    js_set(&y_split_right, "show", &JsValue::FALSE);
    js_set(&y_axis_right, "splitLine", &y_split_right);
    y_axis_arr.push(&y_axis_right);

    js_set(&obj, "yAxis", &y_axis_arr);

    // Tooltip — shows HP%, absolute HP, and EHT value when available
    let tooltip = js_sys::Object::new();
    js_set(&tooltip, "trigger", &JsValue::from_str("axis"));
    let tip_formatter = js_sys::Function::new_with_args(
        "params",
        concat!(
            "if (!params || !params.length) return '';",
            "var p = params[0];",
            "var v = p.value;",
            "var t = v[0];",
            "var m = Math.floor(t / 60);",
            "var s = Math.floor(t % 60);",
            "var time = m + ':' + (s < 10 ? '0' : '') + s;",
            "var pct = v[1].toFixed(1) + '%';",
            "var hp = Math.round(v[2]).toLocaleString();",
            "var max = Math.round(v[3]).toLocaleString();",
            "var result = time + '  ' + pct + '  (' + hp + ' / ' + max + ')';",
            "for (var i = 1; i < params.length; i++) {",
            "  if (params[i].seriesName === 'EHT') {",
            "    var eht = Math.round(params[i].value[1]).toLocaleString();",
            "    result += '<br/><span style=\"color:#2ecc71\">EHT: ' + eht + '</span>';",
            "  }",
            "}",
            "return result;",
        ),
    );
    js_set(&tooltip, "formatter", &tip_formatter);
    js_set(&obj, "tooltip", &tooltip);

    // Build data: each point is [time_secs, hp_pct, current_hp, max_hp]
    let series_arr = js_sys::Array::new();
    let series = js_sys::Object::new();
    js_set(&series, "type", &JsValue::from_str("line"));
    js_set(&series, "name", &JsValue::from_str("HP%"));
    js_set(&series, "sampling", &JsValue::from_str("lttb"));
    js_set(&series, "smooth", &JsValue::TRUE);
    js_set(&series, "symbol", &JsValue::from_str("none"));
    // Use left Y-axis (index 0) for HP%
    js_set(&series, "yAxisIndex", &JsValue::from_f64(0.0));
    // Tell ECharts to encode y from dimension 1 (hp_pct)
    let encode = js_sys::Object::new();
    js_set(&encode, "x", &JsValue::from_f64(0.0));
    js_set(&encode, "y", &JsValue::from_f64(1.0));
    js_set(&series, "encode", &encode);

    let line_style = js_sys::Object::new();
    js_set(&line_style, "color", &JsValue::from_str("#f1c40f"));
    js_set(&line_style, "width", &JsValue::from_f64(2.0));
    js_set(&series, "lineStyle", &line_style);

    // Gradient fill
    let area_style = js_sys::Object::new();
    js_set(
        &area_style,
        "color",
        &JsValue::from_str("rgba(241, 196, 15, 0.15)"),
    );
    js_set(&series, "areaStyle", &area_style);

    // Data is already forward-filled by the backend query
    let data_arr = js_sys::Array::new();
    for p in data {
        let point = js_sys::Array::new();
        point.push(&JsValue::from_f64(p.bucket_start_ms as f64 / 1000.0));
        point.push(&JsValue::from_f64(p.hp_pct));
        point.push(&JsValue::from_f64(p.current_hp as f64));
        point.push(&JsValue::from_f64(p.max_hp as f64));
        data_arr.push(&point);
    }
    js_set(&series, "data", &data_arr);

    // Mark areas for effect windows — HP% axis is fixed 0–100, so scale lanes to 100.
    let mark_area = build_mark_area_object(effect_windows, 100.0);
    js_set(&series, "markArea", &mark_area);

    series_arr.push(&series);

    // Secondary series: EHT (effective healing taken) burst values on right y-axis
    if let Some(eht) = eht_data {
        let bucket_ms: i64 = 1000;
        let num_buckets = ((max_time_ms - min_time_ms) / bucket_ms + 1) as usize;

        // Build sparse lookup from EHT data
        let eht_sparse: std::collections::HashMap<i64, f64> = eht
            .iter()
            .map(|p| (p.bucket_start_ms, p.total_value))
            .collect();

        // Generate dense time series with 0s for missing buckets
        let mut eht_dense: Vec<(f64, f64)> = Vec::with_capacity(num_buckets);
        for i in 0..num_buckets {
            let time_ms = min_time_ms + (i as i64) * bucket_ms;
            let time_secs = time_ms as f64 / 1000.0;
            let value = eht_sparse.get(&time_ms).copied().unwrap_or(0.0);
            eht_dense.push((time_secs, value));
        }

        let eht_series = js_sys::Object::new();
        js_set(&eht_series, "type", &JsValue::from_str("line"));
        js_set(&eht_series, "name", &JsValue::from_str("EHT"));
        js_set(&eht_series, "sampling", &JsValue::from_str("lttb"));
        js_set(&eht_series, "smooth", &JsValue::FALSE);
        js_set(&eht_series, "symbol", &JsValue::from_str("none"));
        // Use right Y-axis (index 1) for EHT burst data
        js_set(&eht_series, "yAxisIndex", &JsValue::from_f64(1.0));

        // Thin green line
        let eht_line_style = js_sys::Object::new();
        js_set(&eht_line_style, "color", &JsValue::from_str("#2ecc71"));
        js_set(&eht_line_style, "width", &JsValue::from_f64(1.0));
        js_set(&eht_series, "lineStyle", &eht_line_style);

        // Subtle green area fill
        let eht_area_style = js_sys::Object::new();
        js_set(
            &eht_area_style,
            "color",
            &JsValue::from_str("rgba(46, 204, 113, 0.15)"),
        );
        js_set(&eht_series, "areaStyle", &eht_area_style);

        let eht_arr = js_sys::Array::new();
        for (x, y) in &eht_dense {
            let pair = js_sys::Array::new();
            pair.push(&JsValue::from_f64(*x));
            pair.push(&JsValue::from_f64(*y));
            eht_arr.push(&pair);
        }
        js_set(&eht_series, "data", &eht_arr);

        series_arr.push(&eht_series);
    }

    js_set(&obj, "series", &series_arr);
    js_set(&obj, "animation", &JsValue::from_bool(animate));
    if animate {
        js_set(&obj, "animationDuration", &JsValue::from_f64(500.0));
        js_set(&obj, "animationEasing", &JsValue::from_str("linear"));
    }

    // Brush selection for drag-to-select time range
    apply_brush_config(&obj);

    obj.into()
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper Functions
// ─────────────────────────────────────────────────────────────────────────────

/// Add brush configuration to an ECharts option for drag-to-select time range.
/// Always active — no toolbox icon needed. Reset via the phase timeline bar.
fn apply_brush_config(obj: &js_sys::Object) {
    // Brush configuration — x-axis only, always active
    let brush = js_sys::Object::new();
    let x_axis_index = js_sys::Array::new();
    x_axis_index.push(&JsValue::from_f64(0.0));
    js_set(&brush, "xAxisIndex", &x_axis_index);
    js_set(&brush, "brushType", &JsValue::from_str("lineX"));
    js_set(&brush, "throttleType", &JsValue::from_str("debounce"));
    js_set(&brush, "throttleDelay", &JsValue::from_f64(300.0));
    // Hide the toolbox — brush is always on
    let toolbox_arr = js_sys::Array::new();
    js_set(&brush, "toolbox", &toolbox_arr);
    // Style for the brush selection rectangle
    let brush_style = js_sys::Object::new();
    js_set(&brush_style, "color", &JsValue::from_str("rgba(100, 200, 255, 0.15)"));
    js_set(&brush_style, "borderColor", &JsValue::from_str("rgba(100, 200, 255, 0.6)"));
    js_set(&brush_style, "borderWidth", &JsValue::from_f64(1.0));
    js_set(&brush, "brushStyle", &brush_style);

    js_set(obj, "brush", &brush);

    // Explicitly hide the top-level toolbox — ECharts auto-creates one when
    // brush is configured, even if brush.toolbox is empty.
    let toolbox = js_sys::Object::new();
    js_set(&toolbox, "show", &JsValue::from_bool(false));
    js_set(obj, "toolbox", &toolbox);
}

/// Activate the brush tool on a chart instance via dispatchAction.
/// Required when there's no toolbox — the brush won't auto-activate otherwise.
fn activate_brush(chart: &JsValue) {
    if let Ok(dispatch_fn) = js_sys::Reflect::get(chart, &JsValue::from_str("dispatchAction")) {
        if let Some(func) = dispatch_fn.dyn_ref::<js_sys::Function>() {
            let action = js_sys::Object::new();
            js_set(&action, "type", &JsValue::from_str("takeGlobalCursor"));
            let brush_opt = js_sys::Object::new();
            js_set(&brush_opt, "brushType", &JsValue::from_str("lineX"));
            js_set(&action, "key", &JsValue::from_str("brush"));
            js_set(&action, "brushOption", &brush_opt);
            let _ = func.call1(chart, &action);
        }
    }
}

/// Register a brush-end handler on an ECharts instance that calls back with the
/// selected time range (start_secs, end_secs). After applying, the brush selection
/// is cleared from the chart.
fn register_brush_handler(chart: &JsValue, mut time_range_sink: Signal<Option<(f64, f64)>>) {
    use wasm_bindgen::closure::Closure;

    let chart_clone = chart.clone();
    let closure = Closure::wrap(Box::new(move |params: JsValue| {
        // ECharts brushEnd event: params.areas is an array of brush areas.
        // Each area has coordRange: [startValue, endValue] in x-axis coordinates.
        if let Ok(areas) = js_sys::Reflect::get(&params, &JsValue::from_str("areas")) {
            if let Some(arr) = areas.dyn_ref::<js_sys::Array>() {
                if arr.length() > 0 {
                    let area = arr.get(0);
                    if let Ok(coord_range) =
                        js_sys::Reflect::get(&area, &JsValue::from_str("coordRange"))
                    {
                        if let Some(range_arr) = coord_range.dyn_ref::<js_sys::Array>() {
                            if range_arr.length() >= 2 {
                                let start = range_arr.get(0).as_f64().unwrap_or(0.0);
                                let end = range_arr.get(1).as_f64().unwrap_or(0.0);
                                if end > start {
                                    time_range_sink.set(Some((start, end)));
                                }
                            }
                        }
                    }
                    // Clear the brush selection visual after applying
                    if let Ok(dispatch_fn) = js_sys::Reflect::get(
                        &chart_clone,
                        &JsValue::from_str("dispatchAction"),
                    ) {
                        if let Some(func) = dispatch_fn.dyn_ref::<js_sys::Function>() {
                            let action = js_sys::Object::new();
                            js_set(&action, "type", &JsValue::from_str("brush"));
                            let empty_areas = js_sys::Array::new();
                            js_set(&action, "areas", &empty_areas);
                            let _ = func.call1(&chart_clone, &action);
                        }
                    }
                }
            }
        }
    }) as Box<dyn FnMut(JsValue)>);

    // Call chart.on("brushEnd", closure) — use unchecked_ref to cast FnMut closure for JS
    if let Ok(on_fn) = js_sys::Reflect::get(chart, &JsValue::from_str("on")) {
        if let Some(func) = on_fn.dyn_ref::<js_sys::Function>() {
            let _ = func.call2(
                chart,
                &JsValue::from_str("brushEnd"),
                closure.as_ref().unchecked_ref(),
            );
        }
    }

    closure.forget();
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
    /// Shared selected player signal (synced with Detailed tabs)
    pub selected_source: Signal<Option<String>>,
    /// Instance id when the selection is one NPC spawn among same-named copies
    pub selected_source_id: Signal<Option<i64>>,
    /// European number format (swaps `.` and `,`)
    #[props(default)]
    pub european: bool,
    /// Callback when the user brush-selects a time range on a chart
    #[props(default)]
    pub on_time_range_change: EventHandler<TimeRange>,
    /// Layout signals — used to trigger chart resize when sidebars/fullscreen toggle
    pub sidebar_collapsed: Signal<bool>,
    pub entity_collapsed: Signal<bool>,
    pub overview_fullscreen: Signal<bool>,
}

#[component]
pub fn ChartsPanel(props: ChartsPanelProps) -> Element {
    // Mirror time_range prop into a signal for reactivity
    let mut time_range_signal = use_signal(|| props.time_range.clone());
    
    // Mirror encounter_idx prop into a signal for reactivity
    let mut encounter_idx_signal = use_signal(|| props.encounter_idx);

    // Update signals when props change (runs on every render with new props)
    if *time_range_signal.read() != props.time_range {
        time_range_signal.set(props.time_range.clone());
    }
    if *encounter_idx_signal.read() != props.encounter_idx {
        encounter_idx_signal.set(props.encounter_idx);
    }

    // Entity selection — shared with Detailed tabs via parent signal.
    // Queries take a selector string: the name, or `id:<n>` to pin one NPC spawn.
    let selected_entity = use_memo(move || {
        let name = (props.selected_source)();
        let id = (props.selected_source_id)();
        name.map(|n| id.map_or(n, |id| format!("id:{id}")))
    });

    // Chart visibility toggles
    let mut show_dps = use_signal(|| true);
    let mut show_hps = use_signal(|| true);
    let mut show_dtps = use_signal(|| true);
    let mut show_hp = use_signal(|| true);

    // Time series data
    let mut dps_data = use_signal(Vec::<TimeSeriesPoint>::new);
    let mut hps_data = use_signal(Vec::<TimeSeriesPoint>::new);
    let mut ehps_data = use_signal(Vec::<TimeSeriesPoint>::new);
    let mut dtps_data = use_signal(Vec::<TimeSeriesPoint>::new);
    let mut hp_data = use_signal(Vec::<HpPoint>::new);
    let mut eht_data = use_signal(Vec::<TimeSeriesPoint>::new);

    // Effect data
    let mut active_effects = use_signal(Vec::<EffectChartData>::new);
    let mut passive_effects = use_signal(Vec::<EffectChartData>::new);
    let mut effect_sort_col = use_signal(EffectSort::default);
    let mut effect_sort_asc = use_signal(|| false);
    // Multiple selected effects with assigned colors
    let mut selected_effects = use_signal(Vec::<(i64, &'static str)>::new);
    // (effect_id, window, color) - includes effect_id for grouping/stacking
    let mut effect_windows = use_signal(Vec::<(i64, EffectWindow, &'static str)>::new);
    // Source filter: "all", "self", "other_players", "npcs"
    let mut source_filter = use_signal(|| "all".to_string());

    // Loading state
    let mut loading = use_signal(|| false);

    // Epoch counter to discard stale async results
    let mut load_epoch = use_signal(|| 0u32);

    // Brush selection sink — written by ECharts brush callbacks, consumed by use_effect below
    let brush_selection: Signal<Option<(f64, f64)>> = use_signal(|| None);

    // Propagate brush selections to parent via on_time_range_change callback
    {
        let on_change = props.on_time_range_change.clone();
        use_effect(move || {
            if let Some((start, end)) = *brush_selection.read() {
                // The brush coords are in seconds (x-axis values).
                // If the current time range has an offset, the chart x-axis
                // already shows absolute combat-time seconds, so use directly.
                on_change.call(TimeRange {
                    start: start as f32,
                    end: end as f32,
                });
            }
        });
    }

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

    // Load time series data when entity or time range changes
    use_effect(move || {
        let idx = encounter_idx_signal();
        let tr = time_range_signal.read().clone();
        let entity = selected_entity.read().clone();

        // Wait for entity auto-selection before loading
        let Some(entity) = entity else { return };

        // Bump load_epoch to invalidate any in-flight tasks
        let current_gen = *load_epoch.peek() + 1;
        load_epoch.set(current_gen);

        spawn(async move {
            // Only show loading spinner for historical encounters (not live query)
            if idx.is_some() {
                loading.set(true);
            }

            let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
                None
            } else {
                Some(&tr)
            };

            // Fetch all data series first, then batch-write signals to trigger
            // only one chart redraw instead of up to 5 intermediate redraws.
            // Without batching, each .set() between .await points allows the
            // runtime to process re-renders, causing visible chart flicker.
            let new_dps = api::query_dps_over_time(idx, bucket_ms, Some(&entity), tr_opt).await;
            let new_hps = api::query_hps_over_time(idx, bucket_ms, Some(&entity), tr_opt).await;
            let new_ehps = api::query_ehps_over_time(idx, bucket_ms, Some(&entity), tr_opt).await;
            let new_dtps = api::query_dtps_over_time(idx, bucket_ms, Some(&entity), tr_opt).await;
            let new_eht = api::query_eht_over_time(idx, bucket_ms, Some(&entity), tr_opt).await;
            let new_hp = api::query_hp_over_time(idx, bucket_ms, Some(&entity), tr_opt).await;

            // Check epoch once after all fetches — discard if stale
            if *load_epoch.read() != current_gen { return; }

            // Batch-write all signals without any .await between them so
            // Dioxus coalesces them into a single re-render
            if let Some(data) = new_dps { dps_data.set(data); }
            if let Some(data) = new_hps { hps_data.set(data); }
            if let Some(data) = new_ehps { ehps_data.set(data); }
            if let Some(data) = new_dtps { dtps_data.set(data); }
            if let Some(data) = new_eht { eht_data.set(data); }
            if let Some(data) = new_hp { hp_data.set(data); }

            loading.set(false);
        });
    });

    // Load effect uptime data when entity, time range, or source filter changes
    use_effect(move || {
        let idx = encounter_idx_signal();
        let duration = props.duration_secs;
        let tr = time_range_signal.read().clone();
        let entity = selected_entity.read().clone();
        let sf = source_filter.read().clone();

        let Some(entity) = entity else { return };

        // Clear stale effect data immediately so icons don't show wrong player's effects
        active_effects.set(Vec::new());
        passive_effects.set(Vec::new());
        selected_effects.set(Vec::new());
        effect_windows.set(Vec::new());

        spawn(async move {
            let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
                None
            } else {
                Some(&tr)
            };
            let sf_opt = if sf == "all" { None } else { Some(sf.as_str()) };

            if let Some(data) =
                api::query_effect_uptime(idx, Some(&entity), tr_opt, duration, sf_opt).await
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
        let idx = encounter_idx_signal();
        let duration = props.duration_secs;
        let tr = time_range_signal.read().clone();
        let effects = selected_effects.read().clone();
        let entity = selected_entity.read().clone();
        let sf = source_filter.read().clone();

        if effects.is_empty() {
            effect_windows.set(Vec::new());
        } else {
            spawn(async move {
                let tr_opt = if tr.start == 0.0 && tr.end == 0.0 {
                    None
                } else {
                    Some(&tr)
                };
                let sf_opt = if sf == "all" { None } else { Some(sf.as_str()) };
                let mut all_windows = Vec::new();
                for (eid, color) in effects {
                    if let Some(windows) =
                        api::query_effect_windows(idx, eid, entity.as_deref(), tr_opt, duration, sf_opt)
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
        let show_hp_val = *show_hp.read();
        let dps = dps_data.read().clone();
        let hps = hps_data.read().clone();
        let ehps = ehps_data.read().clone();
        let dtps = dtps_data.read().clone();
        let eht = eht_data.read().clone();
        let hp = hp_data.read().clone();
        // peek (not read): effect-window changes are applied by a separate, cheap
        // markArea-only effect below, so they must NOT re-trigger this full rebuild.
        // Using current windows here keeps the build correct if they loaded first.
        let windows = effect_windows.peek().clone();

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
        if !show_hp_val {
            dispose_chart("chart-hp");
        }

        let is_live_update = encounter_idx_signal().is_none();

        // During live updates, update charts synchronously to avoid flash.
        // During historical loads, use spawn with a DOM-ready delay.
        if is_live_update {
            // Live: update in-place, no animation, no brush re-registration, no resize.
            // Brush handlers are already registered from the initial render;
            // re-registering would leak closures and cause a visual cursor reset flash.
            if show_dps_val && !dps.is_empty() {
                if let Some(chart) = init_chart("chart-dps") {
                    let option = build_time_series_option(
                        &dps, None, "DPS", "#e74c3c", None,
                        "rgba(231, 76, 60, 0.15)", &windows, "DPS", false,
                    );
                    set_chart_option(&chart, &option);
                }
            }
            if show_hps_val && !hps.is_empty() {
                if let Some(chart) = init_chart("chart-hps") {
                    let ehps_ref = if !ehps.is_empty() { Some(ehps.as_slice()) } else { None };
                    let option = build_time_series_option(
                        &hps, ehps_ref, "HPS", "#2ecc71", Some("#3498db"),
                        "rgba(46, 204, 113, 0.15)", &windows, "HPS", false,
                    );
                    set_chart_option(&chart, &option);
                }
            }
            if show_dtps_val && !dtps.is_empty() {
                if let Some(chart) = init_chart("chart-dtps") {
                    let option = build_time_series_option(
                        &dtps, None, "DTPS", "#e67e22", None,
                        "rgba(230, 126, 34, 0.15)", &windows, "DTPS", false,
                    );
                    set_chart_option(&chart, &option);
                }
            }
            if show_hp_val && !hp.is_empty() {
                if let Some(chart) = init_chart("chart-hp") {
                    let eht_ref = if !eht.is_empty() { Some(eht.as_slice()) } else { None };
                    let option = build_hp_chart_option(&hp, eht_ref, &windows, false);
                    set_chart_option(&chart, &option);
                }
            }
        } else {
            // Historical: wait for the DOM to lay out/paint (two frames), then init
            // + brush + resize. Two rAFs replace a fixed 150ms sleep.
            spawn(async move {
                next_animation_frame().await;
                next_animation_frame().await;

                if show_dps_val && !dps.is_empty() {
                    if let Some(chart) = init_chart("chart-dps") {
                        let option = build_time_series_option(
                            &dps, None, "DPS", "#e74c3c", None,
                            "rgba(231, 76, 60, 0.15)", &windows, "DPS", false,
                        );
                        set_chart_option(&chart, &option);
                        register_brush_handler(&chart, brush_selection);
                        activate_brush(&chart);
                    }
                }
                if show_hps_val && !hps.is_empty() {
                    if let Some(chart) = init_chart("chart-hps") {
                        let ehps_ref = if !ehps.is_empty() { Some(ehps.as_slice()) } else { None };
                        let option = build_time_series_option(
                            &hps, ehps_ref, "HPS", "#2ecc71", Some("#3498db"),
                            "rgba(46, 204, 113, 0.15)", &windows, "HPS", false,
                        );
                        set_chart_option(&chart, &option);
                        register_brush_handler(&chart, brush_selection);
                        activate_brush(&chart);
                    }
                }
                if show_dtps_val && !dtps.is_empty() {
                    if let Some(chart) = init_chart("chart-dtps") {
                        let option = build_time_series_option(
                            &dtps, None, "DTPS", "#e67e22", None,
                            "rgba(230, 126, 34, 0.15)", &windows, "DTPS", false,
                        );
                        set_chart_option(&chart, &option);
                        register_brush_handler(&chart, brush_selection);
                        activate_brush(&chart);
                    }
                }
                if show_hp_val && !hp.is_empty() {
                    if let Some(chart) = init_chart("chart-hp") {
                        let eht_ref = if !eht.is_empty() { Some(eht.as_slice()) } else { None };
                        let option = build_hp_chart_option(&hp, eht_ref, &windows, false);
                        set_chart_option(&chart, &option);
                        register_brush_handler(&chart, brush_selection);
                        activate_brush(&chart);
                    }
                }

                // Resize all visible charts after DOM has settled
                gloo_timers::future::TimeoutFuture::new(50).await;
                resize_all_charts();
            });
        }
    });

    // Effect-highlight updates run independently of the (expensive) data render:
    // when the selected-effect windows change, merge ONLY the markArea into each
    // already-rendered chart instead of rebuilding the whole option. This is what
    // turns effect toggles — and the post-load window fetch — from a full 4-chart
    // rebuild into a cheap partial update.
    use_effect(move || {
        let windows = effect_windows.read().clone(); // subscribe to window changes
        let show_dps_val = *show_dps.read();
        let show_hps_val = *show_hps.read();
        let show_dtps_val = *show_dtps.read();
        let show_hp_val = *show_hp.read();

        spawn(async move {
            // Wait two frames so charts created by the data effect (which also waits
            // two frames) exist first; that effect is registered earlier, so its
            // chart creation runs before this apply on the same frame.
            next_animation_frame().await;
            next_animation_frame().await;

            // Rate charts: highlight lanes scale to the burst data's max. peek the
            // data signals so this effect only depends on the windows.
            let max_of =
                |d: &[TimeSeriesPoint]| d.iter().map(|p| p.total_value).fold(0.0_f64, f64::max);
            if show_dps_val && let Some(chart) = get_chart("chart-dps") {
                apply_effect_markareas(&chart, &windows, max_of(&dps_data.peek()));
            }
            if show_hps_val && let Some(chart) = get_chart("chart-hps") {
                apply_effect_markareas(&chart, &windows, max_of(&hps_data.peek()));
            }
            if show_dtps_val && let Some(chart) = get_chart("chart-dtps") {
                apply_effect_markareas(&chart, &windows, max_of(&dtps_data.peek()));
            }
            // HP% axis is fixed 0–100.
            if show_hp_val && let Some(chart) = get_chart("chart-hp") {
                apply_effect_markareas(&chart, &windows, 100.0);
            }
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

    // Resize charts when layout changes (sidebar collapse, entity panel collapse, fullscreen)
    // CSS transitions take ~250ms, so delay resize to let the layout settle
    use_effect(move || {
        let _sidebar = *props.sidebar_collapsed.read();
        let _entity = *props.entity_collapsed.read();
        let _fullscreen = *props.overview_fullscreen.read();

        spawn(async move {
            gloo_timers::future::TimeoutFuture::new(300).await;
            resize_all_charts();
        });
    });

    // Cleanup charts on unmount
    use_drop(move || {
        dispose_chart("chart-dps");
        dispose_chart("chart-hps");
        dispose_chart("chart-dtps");
        dispose_chart("chart-hp");
    });

    let current_effects = selected_effects.read().clone();
    let sorted_effects = {
        let active = active_effects.read();
        let passive = passive_effects.read();
        let mut all: Vec<EffectChartData> = active.iter().chain(passive.iter()).cloned().collect();
        let col = *effect_sort_col.read();
        let asc = *effect_sort_asc.read();
        all.sort_by(|a, b| {
            let ord = match col {
                EffectSort::Name => a.effect_name.cmp(&b.effect_name),
                EffectSort::Applications => a.count.cmp(&b.count),
                EffectSort::Uptime => a.total_duration_secs.total_cmp(&b.total_duration_secs),
                EffectSort::Pct => a.uptime_pct.total_cmp(&b.uptime_pct),
            };
            if asc { ord } else { ord.reverse() }
        });
        all
    };

    let is_live = props.encounter_idx.is_none();
    let dps_empty = dps_data.read().is_empty();
    let hps_empty = hps_data.read().is_empty();
    let dtps_empty = dtps_data.read().is_empty();
    let hp_empty = hp_data.read().is_empty();

    let current_sf = source_filter.read().clone();

    rsx! {
        div { class: "charts-layout",
            // Left column: chart toggles + chart canvases
            div { class: "charts-column",
                // Chart visibility toggles (horizontal bar)
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
                    label {
                        input {
                            r#type: "checkbox",
                            checked: *show_hp.read(),
                            onchange: move |e| show_hp.set(e.checked())
                        }
                        span { class: "toggle-hp", "HP%" }
                    }
                }
                // Charts area: the spinner sits in the first visible chart so it
                // never lands in the gap between two
                div { class: "charts-area",
                    {
                        let loading_now = *loading.read();
                        let slots = [
                            ("chart-dps", *show_dps.read(), dps_empty, "No damage dealt in fight"),
                            ("chart-hps", *show_hps.read(), hps_empty, "No healing in fight"),
                            ("chart-dtps", *show_dtps.read(), dtps_empty, "No damage taken in fight"),
                            ("chart-hp", *show_hp.read(), hp_empty, "No HP data in fight"),
                        ];
                        let first_shown = slots.iter().position(|s| s.1);
                        rsx! {
                            for (i, (id, shown, empty, msg)) in slots.into_iter().enumerate() {
                                if shown {
                                    if empty && !loading_now && !is_live {
                                        div { key: "{id}", class: "chart-empty", "{msg}" }
                                    } else {
                                        div { key: "{id}", class: "chart-slot",
                                            div { id: "{id}", class: "chart-container" }
                                            if loading_now && first_shown == Some(i) {
                                                div { class: "charts-loading",
                                                    i { class: "fa-solid fa-spinner fa-spin" }
                                                    " Loading..."
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

            // Right column: effects panel (stacked vertically)
            div { class: "effects-panel",
                // Source filter toggle buttons
                div { class: "history-seg seg-fill effects-source-filter",
                    button {
                        class: if current_sf == "all" { "seg active" } else { "seg" },
                        onclick: move |_| source_filter.set("all".to_string()),
                        "All"
                    }
                    button {
                        class: if current_sf == "self" { "seg active" } else { "seg" },
                        onclick: move |_| source_filter.set("self".to_string()),
                        "Self"
                    }
                    button {
                        class: if current_sf == "other_players" { "seg active" } else { "seg" },
                        onclick: move |_| source_filter.set("other_players".to_string()),
                        "Player"
                    }
                    button {
                        class: if current_sf == "npcs" { "seg active" } else { "seg" },
                        onclick: move |_| source_filter.set("npcs".to_string()),
                        "NPC"
                    }
                }

                // Combined effects table (active abilities + passive effects)
                div { class: "effects-section",
                    h4 { "Effects" }
                    if sorted_effects.is_empty() {
                        div { class: "effects-empty", "No effects" }
                    } else {
                        div { class: "effect-table-wrapper",
                            table { class: "effect-table",
                                thead {
                                    {
                                        let cur = *effect_sort_col.read();
                                        let asc = *effect_sort_asc.read();
                                        let arrow = |col: EffectSort| -> &'static str {
                                            if cur == col { if asc { " \u{25B2}" } else { " \u{25BC}" } } else { "" }
                                        };
                                        let sort_click = move |col: EffectSort| {
                                            move |_: MouseEvent| {
                                                if *effect_sort_col.read() == col {
                                                    let was_asc = *effect_sort_asc.peek();
                                                    effect_sort_asc.set(!was_asc);
                                                } else {
                                                    effect_sort_col.set(col);
                                                    effect_sort_asc.set(col == EffectSort::Name);
                                                }
                                            }
                                        };
                                        rsx! {
                                            tr {
                                                th { class: "sortable", onclick: sort_click(EffectSort::Name), "Effect{arrow(EffectSort::Name)}" }
                                                th { class: "num sortable col-apps", title: "Applications", onclick: sort_click(EffectSort::Applications), "Apps{arrow(EffectSort::Applications)}" }
                                                th { class: "num sortable col-uptime", onclick: sort_click(EffectSort::Uptime), "Uptime{arrow(EffectSort::Uptime)}" }
                                                th { class: "num sortable col-pct", onclick: sort_click(EffectSort::Pct), "%{arrow(EffectSort::Pct)}" }
                                            }
                                        }
                                    }
                                }
                                tbody {
                                    for effect in sorted_effects.iter() {
                                        {
                                            let eid = effect.effect_id;
                                            let icon_id = effect.ability_id.unwrap_or(effect.effect_id);
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
                                                    td { class: "effect-name-cell", title: "{effect.effect_name}",
                                                        div { class: "effect-name-inner",
                                                            AbilityIcon { key: "{icon_id}", ability_id: icon_id, size: 16 }
                                                            if effect.effect_name.is_empty() {
                                                                span { class: "effect-id-fallback", "{eid}" }
                                                            } else {
                                                                span { class: "effect-name-text", "{effect.effect_name}" }
                                                            }
                                                        }
                                                    }
                                                    td { class: "num", "{effect.count}" }
                                                    td { class: "num", "{formatting::format_duration_f32(effect.total_duration_secs)}" }
                                                    td { class: "num", "{formatting::format_pct_f32(effect.uptime_pct, props.european)}" }
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
