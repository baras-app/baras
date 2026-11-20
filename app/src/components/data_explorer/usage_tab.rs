//! Ability Usage tab: cast statistics table and timeline chart.

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

use crate::api::{self, AbilityUsageRow, EncounterTimeline, TimeRange};
use crate::components::ability_icon::AbilityIcon;
use crate::components::ability_naming::AbilityNamingPills;
use crate::components::mirror_names::{display_id, display_name};
use crate::types::{SortDirection, UsageSortColumn};
use crate::utils::{LightTheme, css_var, js_set};
use baras_types::formatting;

// ─────────────────────────────────────────────────────────────────────────────
// Ability Usage Tab
// ─────────────────────────────────────────────────────────────────────────────

/// Theme tokens for selected abilities in the timeline chart (palette lives in
/// styles.css so light mode gets darker, higher-contrast variants)
const USAGE_COLORS: [&str; 6] = [
    "--usage-1",
    "--usage-2",
    "--usage-3",
    "--usage-4",
    "--usage-5",
    "--usage-6",
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
            UsageSortColumn::FirstCast => a
                .first_cast_secs
                .partial_cmp(&b.first_cast_secs)
                .unwrap_or(std::cmp::Ordering::Equal),
            UsageSortColumn::LastCast => a
                .last_cast_secs
                .partial_cmp(&b.last_cast_secs)
                .unwrap_or(std::cmp::Ordering::Equal),
            UsageSortColumn::AvgTime => a
                .avg_time_between
                .partial_cmp(&b.avg_time_between)
                .unwrap_or(std::cmp::Ordering::Equal),
            UsageSortColumn::MedianTime => a
                .median_time_between
                .partial_cmp(&b.median_time_between)
                .unwrap_or(std::cmp::Ordering::Equal),
            UsageSortColumn::MinTime => a
                .min_time_between
                .partial_cmp(&b.min_time_between)
                .unwrap_or(std::cmp::Ordering::Equal),
            UsageSortColumn::MaxTime => a
                .max_time_between
                .partial_cmp(&b.max_time_between)
                .unwrap_or(std::cmp::Ordering::Equal),
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
fn build_usage_timeline_option(rows: &[AbilityUsageRow], selected: &[(i64, &str)], duration_secs: f32) -> JsValue {
    let option = js_sys::Object::new();

    // Tooltip — show ability name + M:SS.mmm timestamp on hover
    let tooltip = js_sys::Object::new();
    js_set(&tooltip, "trigger", &JsValue::from_str("item"));
    js_set(
        &tooltip,
        "backgroundColor",
        &JsValue::from_str(&css_var("--bg-elevated")),
    );
    js_set(&tooltip, "borderColor", &JsValue::from_str(&css_var("--border-light")));
    let tip_text_style = js_sys::Object::new();
    js_set(
        &tip_text_style,
        "color",
        &JsValue::from_str(&css_var("--text-secondary")),
    );
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
    let categories = js_sys::Array::new();
    let series_arr = js_sys::Array::new();

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
            let color = css_var(color);
            // Same color, faded, for the track line
            let track_line_style = js_sys::Object::new();
            js_set(&track_line_style, "color", &JsValue::from_str(&color));
            js_set(&track_line_style, "opacity", &JsValue::from_f64(0.3));
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
            js_set(&item_style, "color", &JsValue::from_str(&color));
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
    js_set(&x_axis_label, "color", &JsValue::from_str(&css_var("--text-tertiary")));
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
    js_set(&x_line_style, "color", &JsValue::from_str(&css_var("--border-medium")));
    js_set(&x_line, "lineStyle", &x_line_style);
    js_set(&x_axis, "axisLine", &x_line);
    let split_line = js_sys::Object::new();
    let split_style = js_sys::Object::new();
    js_set(&split_style, "color", &JsValue::from_str(&css_var("--border-subtle")));
    js_set(&split_line, "lineStyle", &split_style);
    js_set(&x_axis, "splitLine", &split_line);
    js_set(&option, "xAxis", &x_axis);

    // Y-Axis: one category row per selected ability
    let y_axis = js_sys::Object::new();
    js_set(&y_axis, "type", &JsValue::from_str("category"));
    js_set(&y_axis, "data", &categories);
    let y_axis_label = js_sys::Object::new();
    js_set(&y_axis_label, "color", &JsValue::from_str(&css_var("--text-secondary")));
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
///
/// This is a proper component (not a plain function) so that its hooks
/// (`use_resource`, `use_effect`) get their own lifecycle scope and are
/// never conditionally registered in the parent component.
#[component]
pub(super) fn UsageTab(
    selected_encounter: Signal<Option<u32>>,
    selected_source: Signal<Option<String>>,
    time_range: Signal<TimeRange>,
    usage_sort_column: Signal<UsageSortColumn>,
    usage_sort_direction: Signal<SortDirection>,
    selected_abilities: Signal<Vec<(i64, &'static str)>>,
    timeline: Signal<Option<EncounterTimeline>>,
    european: bool,
) -> Element {
    let mut usage_sort_column = usage_sort_column;
    let mut usage_sort_direction = usage_sort_direction;
    let mut selected_abilities = selected_abilities;
    // Fetch usage data reactively
    let usage_data = use_resource(move || {
        let enc = *selected_encounter.read();
        let src = selected_source.read().clone();
        let tr = *time_range.read();
        async move {
            let source = src?;
            let tr_opt = if tr.start != 0.0 || tr.end != 0.0 {
                Some(tr)
            } else {
                None
            };
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

    // Build sorted rows, stored in a signal so the chart effect can reactively track changes
    let mut sorted_rows = use_signal(Vec::<AbilityUsageRow>::new);
    let usage_read = usage_data.read();
    let usage_loading = usage_read.is_none(); // None = resource still loading
    let rows: Vec<AbilityUsageRow> = match &*usage_read {
        Some(Some(data)) => {
            let mut sorted = data.clone();
            for r in &mut sorted {
                r.ability_name = display_name(r.ability_id, &r.ability_name).into_owned();
            }
            sort_usage_rows(&mut sorted, sort_col, sort_dir);
            sorted
        }
        _ => Vec::new(),
    };
    // Update sorted_rows signal when rows change (drives chart effect reactively)
    if *sorted_rows.peek() != rows {
        sorted_rows.set(rows.clone());
    }

    let current_selected = selected_abilities.read().clone();

    let num_selected = current_selected.len();

    // Update timeline chart when selections or data change.
    // All signal reads must be inside the closure so Dioxus tracks dependencies.
    let light_theme = try_use_context::<LightTheme>();
    use_effect(move || {
        // Colors resolve from CSS at build time, so redraw when the theme flips
        if let Some(LightTheme(t)) = light_theme {
            t.read();
        }
        let sel = selected_abilities.read().clone();
        let rows_for_chart = sorted_rows.read().clone();
        // Read timeline duration inside the effect so it re-triggers when timeline loads
        let dur = {
            let tr = *time_range.read();
            if tr.start != 0.0 || tr.end != 0.0 {
                tr.end
            } else {
                timeline.read().as_ref().map(|t| t.duration_secs).unwrap_or(1.0)
            }
        };

        if sel.is_empty() || rows_for_chart.is_empty() {
            return;
        }

        // Defer chart init to ensure the DOM has been updated with visible dimensions
        spawn(async move {
            gloo_timers::future::TimeoutFuture::new(100).await;
            if let Some(chart) = crate::components::echarts::init_chart("usage-timeline-chart") {
                // Resize first so ECharts picks up the current container dimensions
                let resize_fn = js_sys::Reflect::get(&chart, &JsValue::from_str("resize"))
                    .ok()
                    .and_then(|f| f.dyn_into::<js_sys::Function>().ok());
                if let Some(func) = &resize_fn {
                    let _ = func.call0(&chart);
                }

                let option = build_usage_timeline_option(&rows_for_chart, &sel, dur);
                crate::components::echarts::set_option_replace(&chart, &option);

                // Resize again after setOption in case the option changed dimensions
                if let Some(func) = &resize_fn {
                    let _ = func.call0(&chart);
                }
            }
        });
    });

    let eu = european;

    rsx! {
        div { class: "ability-header", AbilityNamingPills {} }
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
                    if rows.is_empty() && !usage_loading {
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
                                format!("--effect-color: var({c});")
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
                                            AbilityIcon { ability_id: display_id(row.ability_id) }
                                            span { class: "ability-name-text", "{row.ability_name}" }
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

        // Timeline Chart — only render once table has data to avoid layout shift
        if !rows.is_empty() {
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
}
