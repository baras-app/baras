//! Overview group chart: every player's DPS / EHPS / DTPS / TPS over time,
//! as lines or stacked so each band is that player's share. The metric
//! follows the table's sort column (the tabs just set it), and hovering a
//! table row highlights that player's line.

use dioxus::prelude::*;
use serde_json::{Value, json};

use crate::api::{self, GroupMetric, GroupTimeSeries, RaidOverviewRow, TimeRange};
use crate::components::echarts::{
    dispatch_action, dispose_chart, get_chart, init_chart, json_to_js, next_animation_frame, set_js_fn,
    set_option_replace, use_resize_charts,
};
use crate::components::overview_table::OverviewSort;
use crate::utils::{LightTheme, css_var};

const CHART_ID: &str = "overview-group-chart";
/// Categorical slots (validated order); past eight players the rest fold
/// into grey "Others"
const SERIES_LIGHT: [&str; 8] = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"];
const SERIES_DARK: [&str; 8] = ["#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#008300", "#9085e9", "#e66767"];
const OTHERS: &str = "Others";

const TABS: [(&str, OverviewSort); 4] = [
    ("DPS", OverviewSort::DPS),
    ("EHPS", OverviewSort::EHPS),
    ("DTPS", OverviewSort::DTPS),
    ("TPS", OverviewSort::TPS),
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

fn metric_label(m: GroupMetric) -> &'static str {
    match m {
        GroupMetric::Damage => "DPS",
        GroupMetric::Healing => "EHPS",
        GroupMetric::DamageTaken => "DTPS",
        GroupMetric::Threat => "TPS",
    }
}

/// Running per-second average (total so far / elapsed), like the Charts
/// tab and the overlays: the last point equals the table's value
fn running_rate(values: &[f64], bucket_secs: f64) -> Vec<f64> {
    let mut total = 0.0;
    values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            total += v;
            total / ((i + 1) as f64 * bucket_secs)
        })
        .collect()
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
        "var fmt = function (v) {{ {c} }}, mmss = function (v) {{ {MMSS} }};\
         var val = function (p) {{ return p.data && p.data.raw !== undefined ? p.data.raw : p.value; }};\
         var ps = params.slice().sort(function (a, b) {{ return val(b) - val(a); }});\
         var total = ps.reduce(function (t, p) {{ return t + val(p); }}, 0);\
         var r = '<div style=\"opacity:.7;margin-bottom:2px\">' + mmss(params[0].axisValue) + '</div>';\
         if ({stacked}) r += '<div style=\"margin-bottom:2px\">Group <b style=\"float:right;margin-left:16px\">' + fmt(total) + '</b></div>';\
         ps.forEach(function (p) {{\
           var pct = {stacked} ? ' <span style=\"opacity:.6\">' + Math.round(p.data.value) + '%</span>' : '';\
           r += '<div>' + p.marker + p.seriesName + '<b style=\"float:right;margin-left:16px\">' + fmt(val(p)) + pct + '</b></div>';\
         }});\
         return r;",
        c = compact_js(eu),
    )
}

/// Players in the table, coloured in a stable (alphabetical) order. Past
/// eight, the top seven by total keep a colour and the rest share grey.
fn build_option(data: &GroupTimeSeries, roster: &[String], stacked: bool, light: bool) -> Option<Value> {
    let mut players: Vec<(&str, Vec<f64>, f64)> = data
        .series
        .iter()
        .filter(|(n, _)| roster.contains(n))
        .map(|(n, v)| (n.as_str(), running_rate(v, data.bucket_secs), v.iter().sum::<f64>()))
        .filter(|(_, _, total)| *total != 0.0)
        .collect();
    if players.is_empty() {
        return None;
    }
    let palette = if light { SERIES_LIGHT } else { SERIES_DARK };
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
                "type": "line", "name": name, "showSymbol": false, "stack": "group", "data": points,
                "itemStyle": { "color": color },
                "lineStyle": { "width": 0 },
                "areaStyle": { "color": color, "opacity": 0.9 },
                "emphasis": { "focus": "series" },
            })
        } else {
            json!({
                "type": "line", "name": name, "showSymbol": false, "sampling": "lttb", "data": points,
                "itemStyle": { "color": color },
                "lineStyle": { "color": color, "width": 1.5 },
                "emphasis": { "focus": "series", "lineStyle": { "width": 2.5 } },
            })
        }
    };

    let mut series: Vec<Value> = players.iter().zip(palette).map(|((n, v, _), c)| line(n, v, c)).collect();
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
    Some(json!({
        "animation": false,
        "grid": { "left": 52, "right": 16, "top": 30, "bottom": 24 },
        "legend": { "type": "scroll", "top": 0, "data": legend, "itemWidth": 14, "itemHeight": 8,
                    "textStyle": { "color": muted, "fontSize": 10 }, "pageTextStyle": { "color": muted } },
        "tooltip": { "trigger": "axis", "backgroundColor": css_var("--bg-elevated"), "borderColor": css_var("--border-strong"),
                     "textStyle": { "color": css_var("--text-primary"), "fontSize": 11 } },
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
    european: bool,
) -> Element {
    let metric = metric_of(sort_col());
    let mut stacked = use_signal(|| false);
    let roster: Vec<String> = rows.iter().map(|r| r.name.clone()).collect();
    // Colours resolve from CSS when the option is built: rebuild on a theme flip
    let light = try_use_context::<LightTheme>().is_some_and(|LightTheme(t)| t());

    let data = use_resource(use_reactive!(|(encounter_idx, time_range, metric, rows)| async move {
        let _ = rows;
        let tr = (time_range.end > 0.0).then_some(time_range);
        api::query_group_over_time(encounter_idx, 1000, metric, tr.as_ref()).await
    }));

    let option = data.read().as_ref().and_then(|d| d.as_ref()).and_then(|d| build_option(d, &roster, stacked(), light));
    let has_data = option.is_some();

    use_resize_charts(&[CHART_ID]);
    use_effect(use_reactive!(|option, european| {
        let stacked = stacked();
        spawn(async move {
            next_animation_frame().await;
            next_animation_frame().await;
            let (Some(option), Some(chart)) = (option, init_chart(CHART_ID)) else { return };
            let Some(js) = json_to_js(&option) else { return };
            set_js_fn(&js, &["tooltip", "formatter"], "params", &tooltip_js(european, stacked));
            set_js_fn(&js, &["xAxis", "axisLabel", "formatter"], "v", MMSS);
            let y_fmt = if stacked { "return v + '%';".to_string() } else { compact_js(european) };
            set_js_fn(&js, &["yAxis", "axisLabel", "formatter"], "v", &y_fmt);
            set_option_replace(&chart, &js);
        });
    }));

    // Table row hover → emphasise that player's series, dim the rest
    use_effect(move || {
        let name = hovered();
        if let Some(chart) = get_chart(CHART_ID) {
            dispatch_action(&chart, &json!({ "type": "downplay" }));
            if let Some(name) = name {
                dispatch_action(&chart, &json!({ "type": "highlight", "seriesName": name }));
            }
        }
    });
    use_drop(|| dispose_chart(CHART_ID));

    rsx! {
        div { class: "overview-chart-box",
            div { class: "overview-chart-header",
                div { class: "history-seg", title: "Metric (follows the table's sort column)",
                    for (label, sort) in TABS {
                        button {
                            class: if metric_of(sort) == metric { "seg active" } else { "seg" },
                            onclick: move |_| {
                                sort_col.set(sort);
                                sort_asc.set(false);
                            },
                            "{label}"
                        }
                    }
                }
                span { class: "overview-chart-note", "{metric_label(metric)} · running average" }
                div { class: "history-seg ml-auto",
                    button { class: if !stacked() { "seg active" } else { "seg" }, onclick: move |_| stacked.set(false), "Lines" }
                    button { class: if stacked() { "seg active" } else { "seg" }, onclick: move |_| stacked.set(true), "Stacked" }
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
