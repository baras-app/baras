//! Dummy Parses trend: DPS on every counted kill, one line per discipline
//! with its mirror ("Lightning / Telekinetics"). Kills are evenly spaced in
//! order, as on the Boss Statistics charts: on a real time axis a session's
//! parses stack into one column.

use dioxus::prelude::*;
use serde_json::{Value, json};

use baras_types::formatting;
use baras_types::history::{DummyRow, discipline_mirror};

use super::charts::{apply_period_ticks, period_ticks};
use super::overview::short_ts;
use crate::components::echarts::{
    dispose_chart, escape, init_chart, json_to_js, next_animation_frame, set_js_fn, set_option_replace,
};
use crate::utils::{LightTheme, css_var};

const CHART_ID: &str = "hist-dummy-trend";

/// Reads the fields `trend_option` puts on every point
const TOOLTIP: &str = "var d = params.data;\
    return '<div style=\"font-size:11px;opacity:.7;margin-bottom:2px\">' + d.when + '</div>'\
      + '<div>' + params.marker + params.seriesName + '</div>'\
      + '<div><b>' + d.shown + '</b> DPS · ' + d.who + '</div>';";

/// Line name of a discipline: both names of its mirror pair
fn line_name(discipline: &str) -> String {
    match discipline_mirror(discipline) {
        Some((imp, rep)) => format!("{imp} / {rep}"),
        None => discipline.to_string(),
    }
}

/// None with fewer than two kills: a single point is not a trend
fn trend_option(rows: &[DummyRow], european: bool) -> Option<Value> {
    let mut rows: Vec<&DummyRow> = rows.iter().filter(|r| r.discipline.is_some()).collect();
    if rows.len() < 2 {
        return None;
    }
    rows.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    let ticks = period_ticks(&rows.iter().map(|r| r.timestamp.as_str()).collect::<Vec<_>>());
    let count = rows.len();
    // A line has a point at its own kills only and bridges the others
    let mut lines: Vec<(String, Vec<Value>)> = Vec::new();
    for (i, r) in rows.into_iter().enumerate() {
        let name = line_name(r.discipline.as_deref().unwrap_or_default());
        let point = json!({
            "value": r.dps,
            "shown": formatting::format_compact(r.dps, european),
            "who": escape(&r.character),
            "when": short_ts(&r.timestamp),
        });
        if !lines.iter().any(|(n, _)| *n == name) {
            lines.push((name.clone(), vec![Value::Null; count]));
        }
        if let Some((_, points)) = lines.iter_mut().find(|(n, _)| *n == name) {
            points[i] = point;
        }
    }
    // Most kills first, as in the discipline table
    lines.sort_by_key(|(_, points)| std::cmp::Reverse(points.iter().filter(|p| !p.is_null()).count()));

    let muted = css_var("--text-muted");
    let series: Vec<Value> = lines
        .into_iter()
        .map(|(name, data)| json!({ "name": name, "type": "line", "data": data, "connectNulls": true, "symbol": "circle", "symbolSize": 7, "showAllSymbol": true, "lineStyle": { "width": 1.5 } }))
        .collect();
    Some(json!({
        "animation": false,
        "grid": { "left": 52, "right": 16, "top": 34, "bottom": 28 },
        "title": { "text": "DPS by Discipline", "left": 10, "top": 6, "textStyle": { "color": css_var("--text-primary"), "fontSize": 12 } },
        "legend": { "type": "scroll", "top": 4, "left": 150, "right": 10, "textStyle": { "color": css_var("--text-secondary"), "fontSize": 11 }, "pageIconColor": muted, "pageTextStyle": { "color": muted } },
        "tooltip": { "trigger": "item", "backgroundColor": css_var("--bg-elevated"), "borderColor": css_var("--border-strong"), "textStyle": { "color": css_var("--text-primary") } },
        "xAxis": {
            "type": "category",
            "data": (0..count).collect::<Vec<_>>(),
            "boundaryGap": false,
            "axisLabel": { "color": muted, "fontSize": 10 },
            "axisTick": { "alignWithLabel": true },
            "splitLine": { "show": true, "lineStyle": { "color": css_var("--border-subtle") } },
        },
        "_ticks": ticks,
        "yAxis": { "type": "value", "scale": true, "axisLabel": { "color": muted, "fontSize": 10 }, "splitLine": { "lineStyle": { "color": css_var("--border-subtle") } } },
        "series": series,
    }))
}

#[component]
pub(super) fn DummyTrend(rows: Vec<DummyRow>, european: bool) -> Element {
    let has_data = rows.iter().filter(|r| r.discipline.is_some()).count() >= 2;
    // Props are plain values and the effect keeps its first closure, so it
    // reads them through a memo
    let data = use_memo(use_reactive!(|(rows, european)| (rows, european)));
    let light_theme = try_use_context::<LightTheme>();
    use_effect(move || {
        // Colours resolve from CSS when the option is built: rebuild on a theme flip
        if let Some(LightTheme(t)) = light_theme {
            t.read();
        }
        let (rows, european) = data();
        let option = trend_option(&rows, european);
        spawn(async move {
            // The host exists after this render; two frames give it a size
            next_animation_frame().await;
            next_animation_frame().await;
            let Some(option) = option else {
                return dispose_chart(CHART_ID);
            };
            if let (Some(chart), Some(js)) = (init_chart(CHART_ID), json_to_js(&option)) {
                set_js_fn(&js, &["tooltip", "formatter"], "params", TOOLTIP);
                if let Some(ticks) = option.get("_ticks") {
                    apply_period_ticks(&js, ticks);
                }
                set_js_fn(&js, &["yAxis", "axisLabel", "formatter"], "v", "return v >= 1000 ? (v / 1000) + 'K' : v;");
                set_option_replace(&chart, &js);
            }
        });
    });
    use_drop(|| dispose_chart(CHART_ID));

    rsx! {
        if has_data {
            div { class: "history-charts",
                div { id: CHART_ID, class: "history-chart" }
            }
        }
    }
}
