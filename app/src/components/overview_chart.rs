//! Overview group chart: every player's DPS / EHPS / DTPS / TPS over time,
//! as lines or stacked so each band is that player's share. The metric
//! follows the table's sort column (the tabs just set it), and hovering a
//! table row highlights that player's line. The Alive tab swaps in one row
//! per player, green while alive and red while dead.

use dioxus::prelude::*;
use serde_json::{Value, json};
use wasm_bindgen::JsCast;

use crate::api::{self, GroupMetric, GroupTimeSeries, PlayerDeath, RaidOverviewRow, TimeRange};
use crate::components::echarts::{
    TOOLTIP_ROWS_JS, activate_brush, apply_brush_config, register_span_click, dispatch_action, dispose_chart, get_chart, init_svg_chart, json_to_js, next_animation_frame, register_brush_handler, set_chart_option, set_js_fn, set_option_replace,
};
use crate::components::overview_table::OverviewSort;
use crate::utils::{LightTheme, css_var};

const CHART_ID: &str = "overview-group-chart";
/// Categorical slots (validated order); past sixteen players the rest fold
/// into grey "Others"
const OTHERS: &str = "Others";
/// `--series-1..=16` in styles.css: enough for a full 16-player raid
const SERIES_COLORS: usize = 16;
/// Series id of the death markers (no data of its own; skipped in the tooltip)
const DEATHS_ID: &str = "deaths";
const GRID_LEFT: f64 = 52.0;
const GRID_RIGHT: f64 = 16.0;
/// Death label metrics (10px font): rough glyph width, padding, row height
const LABEL_CHAR_PX: f64 = 6.0;
const LABEL_PAD_PX: f64 = 10.0;
const LABEL_ROW_PX: f64 = 16.0;

const TABS: [(&str, OverviewSort); 4] = [
    ("DPS", OverviewSort::DPS),
    ("EHPS", OverviewSort::EHPS),
    ("DTPS", OverviewSort::DTPS),
    ("Threat", OverviewSort::ThreatTotal),
];

fn metric_of(sort: OverviewSort) -> GroupMetric {
    use OverviewSort::*;
    match sort {
        ThreatTotal | TPS => GroupMetric::Threat,
        DamageTakenTotal | DTPS | APS => GroupMetric::DamageTaken,
        HealingTotal | HPS | HealingPct | EHPS | ShieldingTotal | SPS => GroupMetric::Healing,
        DPS | DamageTotal | APM | Interrupts => GroupMetric::Damage,
    }
}

/// Threat plots its running total, so a drop shows as the fall it is; the
/// rest plot a running per-second average (total so far / elapsed), like the
/// Charts tab and the overlays: the last point equals the table's value
fn running(values: &[f64], bucket_secs: f64, metric: GroupMetric) -> Vec<f64> {
    let mut total = 0.0;
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            total += v;
            if metric == GroupMetric::Threat { total } else { total / ((i + 1) as f64 * bucket_secs) }
        })
        .collect()
}

fn chart_note(m: GroupMetric) -> &'static str {
    match m {
        GroupMetric::Damage => "DPS · running average",
        GroupMetric::Healing => "EHPS · running average",
        GroupMetric::DamageTaken => "DTPS · running average",
        GroupMetric::Threat => "Threat · running total",
    }
}

/// JS: compact number (12.3K / 1.25M), honouring the European decimal comma
fn compact_js(eu: bool) -> String {
    let sep = if eu { "s = s.replace('.', ',');" } else { "" };
    format!(
        "var a = Math.abs(v), s = a >= 1e6 ? (v / 1e6).toFixed(2) + 'M' : a >= 1e3 ? (v / 1e3).toFixed(1) + 'K' : Math.round(v).toString(); {sep} return s;"
    )
}

const MMSS: &str = "var m = Math.floor(v / 60), s = Math.floor(v % 60); return m + ':' + (s < 10 ? '0' : '') + s;";

/// Tooltip: time, the group total (stacked), then every player at that
/// second, highest first. Stacked points carry the value as `raw`.
fn tooltip_js(eu: bool, stacked: bool) -> String {
    format!(
        "{TOOLTIP_ROWS_JS}\
         params = params.filter(function (p) {{ return p.seriesId !== '{DEATHS_ID}'; }});\
         if (!params.length) return '';\
         var fmt = function (v) {{ {c} }}, mmss = function (v) {{ {MMSS} }};\
         var val = function (p) {{ return p.data && p.data.raw !== undefined ? p.data.raw : p.value; }};\
         var ps = params.slice().sort(function (a, b) {{ return val(b) - val(a); }});\
         var total = ps.reduce(function (t, p) {{ return t + val(p); }}, 0);\
         var r = tipHead(mmss(params[0].axisValue));\
         if ({stacked}) r += tipRow('Group', fmt(total), 'margin-bottom:2px');\
         ps.forEach(function (p) {{\
           var pct = {stacked} ? ' <span style=\"opacity:.6\">' + Math.round(p.data.value) + '%</span>' : '';\
           r += tipRow(p.marker + p.seriesName, fmt(val(p)) + pct);\
         }});\
         return r;",
        c = compact_js(eu),
    )
}

/// Players in the table, coloured in a stable (alphabetical) order. Past
/// sixteen, the top fifteen by total keep a colour and the rest share grey.
fn build_option(data: &GroupTimeSeries, metric: GroupMetric, roster: &[String], stacked: bool, deaths: &[PlayerDeath]) -> Option<Value> {
    let mut players: Vec<(&str, Vec<f64>, f64)> = data
        .series
        .iter()
        .filter(|(n, _)| roster.contains(n))
        .map(|(n, v)| (n.as_str(), running(v, data.bucket_secs, metric), v.iter().sum::<f64>()))
        .filter(|(_, _, total)| *total != 0.0)
        .collect();
    if players.is_empty() {
        return None;
    }
    // Theme palette (--series-N in styles.css)
    let palette: Vec<String> = (1..=SERIES_COLORS).map(|i| css_var(&format!("--series-{i}"))).collect();
    players.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    let keep = if players.len() > palette.len() { palette.len() - 1 } else { players.len() };
    let rest = players.split_off(keep);
    players.sort_by(|a, b| a.0.cmp(b.0));

    let grey = css_var("--text-disabled");
    // Stacked = share of the group: each point is { value: % of group, raw }, and
    // the bands always fill the height (negative threat counts as none)
    let group: Vec<f64> = stacked
        .then(|| {
            let len = players[0].1.len();
            (0..len).map(|i| players.iter().chain(&rest).map(|p| p.1[i].max(0.0)).sum()).collect()
        })
        .unwrap_or_default();
    let line = |name: &str, values: &[f64], color: &str| {
        let points: Vec<Value> = values
            .iter()
            .enumerate()
            .map(|(i, v)| match group.get(i) {
                Some(g) => json!({ "value": if *g > 0.0 { v.max(0.0) * 100.0 / g } else { 0.0 }, "raw": v }),
                None => json!(v),
            })
            .collect();
        if stacked {
            json!({
                "type": "line", "name": name, "showSymbol": false, "stack": "group", "data": points, "silent": true,
                "itemStyle": { "color": color },
                "lineStyle": { "width": 0 },
                "areaStyle": { "color": color, "opacity": 0.9 },
                "emphasis": { "focus": "series" },
            })
        } else {
            json!({
                "type": "line", "name": name, "showSymbol": false, "sampling": "lttb", "data": points, "silent": true,
                "itemStyle": { "color": color },
                "lineStyle": { "color": color, "width": 1.5 },
                "emphasis": { "focus": "series", "lineStyle": { "width": 2.5 } },
            })
        }
    };

    let mut series: Vec<Value> = players.iter().zip(&palette).map(|((n, v, _), c)| line(n, v, c)).collect();
    if !rest.is_empty() {
        if stacked {
            let sum: Vec<f64> = (0..group.len()).map(|i| rest.iter().map(|r| r.1[i]).sum()).collect();
            series.push(line(OTHERS, &sum, &grey));
        } else {
            series.extend(rest.iter().map(|(n, v, _)| line(n, v, &grey)));
        }
    }
    let mut legend: Vec<&str> = players.iter().map(|p| p.0).collect();
    if !rest.is_empty() && stacked {
        legend.push(OTHERS);
    }

    let times: Vec<f64> = (0..players[0].1.len()).map(|i| data.start_secs + i as f64 * data.bucket_secs).collect();
    let muted = css_var("--text-muted");
    let rule = css_var("--border-light");

    // Death markers: a dashed rule per roster death in range, snapped to its
    // bucket, named just inside the top of the plot (rows staggered once the
    // width is known)
    let last = times.len() as f64 - 1.0;
    let marks: Vec<Value> = deaths
        .iter()
        .filter(|d| roster.contains(&d.name))
        .filter_map(|d| {
            let i = ((d.death_time_secs as f64 - data.start_secs) / data.bucket_secs).round();
            (0.0..=last).contains(&i).then(|| json!({ "xAxis": i, "name": d.name }))
        })
        .collect();
    if !marks.is_empty() {
        let red = Some(css_var("--lt-red")).filter(|c| !c.is_empty()).unwrap_or_else(|| "#e74c3c".into());
        series.push(json!({
            "type": "line", "id": DEATHS_ID, "data": [],
            "markLine": {
                "symbol": ["none", "none"], "animation": false, "silent": true, "data": marks,
                "lineStyle": { "color": red, "type": "dashed", "width": 1, "opacity": 0.7 },
                "label": { "position": "end", "formatter": "{b}", "color": red, "fontSize": 10, "distance": 2, "offset": [0.0, LABEL_ROW_PX],
                           "backgroundColor": css_var("--bg-elevated"), "padding": [1, 4], "borderRadius": 2 },
                // Highlighting a player's line blurs the other series; keep the markers as they are
                "blur": { "lineStyle": { "opacity": 0.7 }, "label": { "opacity": 1 } },
            },
        }));
    }
    Some(json!({
        "animation": false,
        "grid": { "left": GRID_LEFT, "right": GRID_RIGHT, "top": 30, "bottom": 24 },
        "legend": { "type": "scroll", "top": 0, "data": legend, "selectedMode": false, "itemWidth": 14, "itemHeight": 8,
                    "textStyle": { "color": muted, "fontSize": 10 }, "pageTextStyle": { "color": muted } },
        "tooltip": { "trigger": "axis", "backgroundColor": css_var("--bg-elevated"), "borderColor": css_var("--border-strong"),
                     "textStyle": { "color": css_var("--text-primary"), "fontSize": 11 } },
        // Hovering shouldn't bold every line at the cursor (row hover still highlights one).
        // Top level: ECharts ignores this under tooltip.axisPointer
        "axisPointer": { "triggerEmphasis": false },
        // Category (one per second), not value: ECharts stacks the x values
        // on a value/value grid, which tore the stacked bands
        "xAxis": { "type": "category", "boundaryGap": false, "data": times,
                   "axisLabel": { "color": muted }, "axisTick": { "show": false }, "splitLine": { "show": false } },
        "yAxis": { "type": "value", "max": if stacked { json!(100) } else { json!(null) },
                   "axisLabel": { "color": muted }, "splitLine": { "lineStyle": { "color": rule } } },
        "series": series,
    }))
}

#[component]
pub fn OverviewGroupChart(
    encounter_idx: Option<u32>,
    time_range: TimeRange,
    /// Table rows, in table order: the players to plot (and a refresh signal while live)
    rows: Vec<RaidOverviewRow>,
    sort_col: Signal<OverviewSort>,
    sort_asc: Signal<bool>,
    /// Player whose table row is hovered
    hovered: ReadSignal<Option<String>>,
    /// View toggles, owned by the parent so they persist across encounters
    mut alive_view: Signal<bool>,
    mut stacked: Signal<bool>,
    mut show_deaths: Signal<bool>,
    european: bool,
    /// Encounter deaths; roster players' deaths are marked on the chart
    deaths: Vec<PlayerDeath>,
    /// Drag across the chart to narrow the timeline to that span
    on_time_range_change: EventHandler<TimeRange>,
) -> Element {
    let metric = metric_of(sort_col());
    let roster: Vec<String> = rows.iter().map(|r| r.name.clone()).collect();
    // Colours resolve from CSS when the option is built; reading the theme
    // subscribes this render, so a theme flip rebuilds the option
    if let Some(LightTheme(t)) = try_use_context::<LightTheme>() {
        let _ = t();
    }

    let data = use_resource(use_reactive!(|(encounter_idx, time_range, metric, rows)| async move {
        let _ = rows;
        let tr = (time_range.end > 0.0).then_some(time_range);
        api::query_group_over_time(encounter_idx, 1000, metric, tr.as_ref()).await
    }));

    // Brush span in x-axis slots (the axis is a category, one per bucket),
    // turned back into combat seconds for the timeline
    let brush: Signal<Option<(f64, f64)>> = use_signal(|| None);
    use_effect(move || {
        let Some((a, b)) = brush() else { return };
        if let Some(Some(d)) = data.peek().as_ref() {
            // The Alive view's x axis is already in seconds
            let at = |i: f64| if *alive_view.peek() { i as f32 } else { (d.start_secs + i * d.bucket_secs) as f32 };
            on_time_range_change.call(TimeRange { start: at(a), end: at(b) });
        }
    });

    let option = data.read().as_ref().and_then(|d| d.as_ref()).and_then(|d| {
        if alive_view() {
            build_alive_option(d, &roster, &deaths)
        } else {
            build_option(d, metric, &roster, stacked(), if show_deaths() { &deaths } else { &[] })
        }
    });
    let has_data = option.is_some();

    // Death markers as last drawn (labels staggered), restyled on row hover
    let mut drawn_marks: Signal<Vec<Value>> = use_signal(Vec::new);

    use_effect(use_reactive!(|option, european| {
        let stacked = stacked();
        let alive = alive_view();
        spawn(async move {
            next_animation_frame().await;
            next_animation_frame().await;
            let (Some(mut option), Some(chart)) = (option, init_svg_chart(CHART_ID)) else { return };
            if let Some(el) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id(CHART_ID)) {
                stagger_death_labels(&mut option, el.client_width() as f64);
            }
            drawn_marks.set(death_marks(&mut option).map(|m| m.clone()).unwrap_or_default());
            let Some(js) = json_to_js(&option) else { return };
            set_js_fn(&js, &["xAxis", "axisLabel", "formatter"], "v", MMSS);
            if alive {
                set_js_fn(&js, &["tooltip", "formatter"], "p", &alive_tooltip_js());
            } else {
                set_js_fn(&js, &["tooltip", "formatter"], "params", &tooltip_js(european, stacked));
                let y_fmt = if stacked { "return v + '%';".to_string() } else { compact_js(european) };
                set_js_fn(&js, &["yAxis", "axisLabel", "formatter"], "v", &y_fmt);
            }
            apply_brush_config(js.unchecked_ref());
            set_option_replace(&chart, &js);
            register_brush_handler(&chart, brush);
            activate_brush(&chart);
            // Alive view: clicking a span narrows the timeline to it (same path as a brush drag)
            if alive {
                register_span_click(&chart, brush);
            }
        });
    }));

    // Table row hover → emphasise that player's series and death markers,
    // dim the rest
    use_effect(move || {
        let name = hovered();
        let marks = drawn_marks();
        if let Some(chart) = get_chart(CHART_ID) {
            if !marks.is_empty() {
                let styled: Vec<Value> = marks
                    .into_iter()
                    .map(|mut m| {
                        // Hovered player's deaths go full opacity (bold label); everyone else's fade.
                        // The row's line highlight puts this series in its blur state,
                        // so each marker's blur style must match too.
                        let (line, label) = match name.as_deref() {
                            Some(n) if m["name"] == n => (
                                json!({ "opacity": 1 }),
                                json!({ "fontWeight": "bold", "opacity": 1 }),
                            ),
                            Some(_) => (json!({ "opacity": 0.15 }), json!({ "opacity": 0.25 })),
                            None => return m,
                        };
                        m["lineStyle"] = line.clone();
                        m["blur"] = json!({ "lineStyle": line, "label": label.clone() });
                        for (k, v) in label.as_object().into_iter().flatten() {
                            m["label"][k] = v.clone();
                        }
                        m
                    })
                    .collect();
                let patch = json!({ "series": [{ "id": DEATHS_ID, "markLine": { "data": styled } }] });
                if let Some(js) = json_to_js(&patch) {
                    set_chart_option(&chart, &js);
                }
            }
            dispatch_action(&chart, &json!({ "type": "downplay" }));
            if let Some(name) = name {
                // Alive view: the player is a y category (data name) in every series
                let target = if alive_view() { "name" } else { "seriesName" };
                dispatch_action(&chart, &json!({ "type": "highlight", target: name }));
            }
        }
    });
    use_drop(|| dispose_chart(CHART_ID));

    rsx! {
        div { class: "overview-chart-box",
            div { class: "overview-chart-header",
                div { class: "filter-pills", title: "Metric (follows the table's sort column)",
                    for (label, sort) in TABS {
                        button {
                            class: if !alive_view() && metric_of(sort) == metric { "seg active" } else { "seg" },
                            onclick: move |_| {
                                alive_view.set(false);
                                sort_col.set(sort);
                                sort_asc.set(false);
                            },
                            "{label}"
                        }
                    }
                    button {
                        class: if alive_view() { "seg active" } else { "seg" },
                        title: "When each player was alive (green) or dead (red)",
                        onclick: move |_| alive_view.set(true),
                        "Alive"
                    }
                }
                if alive_view() {
                    span { class: "overview-chart-note", "Alive / dead over time" }
                } else {
                    span { class: "overview-chart-note", "{chart_note(metric)}" }
                    div { class: "filter-pills ml-auto",
                        button {
                            class: if show_deaths() { "seg active" } else { "seg" },
                            title: "Mark player deaths on the chart",
                            onclick: move |_| show_deaths.toggle(),
                            i { class: "fa-solid fa-skull" }
                            " Deaths"
                        }
                    }
                    div { class: "filter-pills",
                        button { class: if !stacked() { "seg active" } else { "seg" }, onclick: move |_| stacked.set(false), "Lines" }
                        button { class: if stacked() { "seg active" } else { "seg" }, onclick: move |_| stacked.set(true), "Stacked" }
                    }
                }
            }
            if has_data {
                div { id: CHART_ID, class: "overview-chart" }
            } else {
                div { class: "overview-chart chart-empty", "No data for this metric" }
            }
        }
    }
}

/// Alive tab: one bar per player on a seconds axis, stacked spans that
/// alternate alive (even series) / dead (odd), in table order. Spans start at
/// 0 and run to the end of the shown window; the axis clips to the window.
fn build_alive_option(data: &GroupTimeSeries, roster: &[String], deaths: &[PlayerDeath]) -> Option<Value> {
    let len = data.series.first().map(|s| s.1.len()).filter(|&n| n > 0 && !roster.is_empty())?;
    let start = data.start_secs;
    let end = start + (len - 1) as f64 * data.bucket_secs;

    let spans: Vec<Vec<(f64, f64)>> = roster
        .iter()
        .map(|name| {
            let mut out = Vec::new();
            let mut t = 0.0;
            for d in deaths.iter().filter(|d| &d.name == name) {
                let died = (d.death_time_secs as f64).max(t);
                if died >= end {
                    break;
                }
                let back = d.revive_time_secs.map_or(end, |r| (r as f64).clamp(died, end));
                out.extend([(t, died), (died, back)]);
                t = back;
                if t >= end {
                    break;
                }
            }
            if t < end {
                out.push((t, end));
            }
            out
        })
        .collect();

    let green = css_var("--lt-green");
    let red = css_var("--lt-red");
    let highlight = css_var("--text-primary");
    let max_spans = spans.iter().map(Vec::len).max().unwrap_or(0);
    let series: Vec<Value> = (0..max_spans)
        .map(|k| {
            let alive = k % 2 == 0;
            let points: Vec<Value> = spans
                .iter()
                .map(|p| p.get(k).map_or(json!(0), |&(a, b)| json!({ "value": b - a, "from": a, "to": b })))
                .collect();
            json!({
                "type": "bar", "name": if alive { "Alive" } else { "Dead" }, "stack": "life", "barWidth": "60%",
                "data": points,
                "itemStyle": { "color": if alive { &green } else { &red }, "opacity": if alive { 0.8 } else { 1.0 } },
                "emphasis": { "itemStyle": { "opacity": 1, "borderColor": highlight, "borderWidth": 1 } },
            })
        })
        .collect();

    let muted = css_var("--text-muted");
    Some(json!({
        "animation": false,
        "grid": { "left": 8, "right": GRID_RIGHT, "top": 8, "bottom": 24, "containLabel": true },
        "tooltip": { "trigger": "item", "backgroundColor": css_var("--bg-elevated"), "borderColor": css_var("--border-strong"),
                     "textStyle": { "color": css_var("--text-primary"), "fontSize": 11 } },
        "xAxis": { "type": "value", "min": start, "max": end,
                   "axisLabel": { "color": muted }, "splitLine": { "lineStyle": { "color": css_var("--border-light") } } },
        "yAxis": { "type": "category", "inverse": true, "data": roster,
                   "axisLabel": { "color": muted, "fontSize": 10, "width": 110, "overflow": "truncate" }, "axisTick": { "show": false } },
        "series": series,
    }))
}

fn alive_tooltip_js() -> String {
    format!(
        "if (!p.data || p.data.from === undefined) return '';\
         var mmss = function (v) {{ {MMSS} }};\
         return '<b>' + p.name + '</b><br>' + p.marker + p.seriesName + ' ' + mmss(p.data.from) + ' – ' + mmss(p.data.to);"
    )
}

/// Death labels sit level at the top of the plot, over their line (the label
/// anchors above the line's end, so every row shifts down one); one that would overlap a label
/// to its left drops a row (only crowded PvP fights stack). Labels at the
/// plot edges align inward so they don't clip.
fn stagger_death_labels(option: &mut Value, chart_px: f64) {
    let slots = option["xAxis"]["data"].as_array().map_or(0, |a| a.len());
    let plot = chart_px - GRID_LEFT - GRID_RIGHT;
    let Some(marks) = death_marks(option).filter(|_| slots > 1 && plot > 0.0) else { return };
    let step = plot / (slots - 1) as f64;
    // Right edge of the last label in each row; marks arrive in time order
    let mut rows: Vec<f64> = Vec::new();
    for m in marks.iter_mut() {
        let x = m["xAxis"].as_f64().unwrap_or(0.0) * step;
        let w = m["name"].as_str().map_or(0, |n| n.chars().count()) as f64 * LABEL_CHAR_PX + LABEL_PAD_PX;
        let (align, left) = if x + w / 2.0 > plot {
            ("right", x - w)
        } else if x - w / 2.0 < 0.0 {
            ("left", x)
        } else {
            ("center", x - w / 2.0)
        };
        let row = rows.iter().position(|&end| end + 4.0 <= left).unwrap_or(rows.len());
        if row == rows.len() {
            rows.push(0.0);
        }
        rows[row] = left + w;
        m["label"] = json!({ "align": align, "offset": [0.0, (row + 1) as f64 * LABEL_ROW_PX] });
    }
}

/// The death markers' `markLine.data`, if the option has any
fn death_marks(option: &mut Value) -> Option<&mut Vec<Value>> {
    option["series"]
        .as_array_mut()?
        .iter_mut()
        .find(|s| s["id"] == DEATHS_ID)?["markLine"]["data"]
        .as_array_mut()
}
