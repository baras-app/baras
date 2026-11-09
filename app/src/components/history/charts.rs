//! Per-boss charts for the Overview: your DPS and EHPS on every pull, and
//! pulls per raid session. Options are built as JSON and applied through the
//! shared ECharts helpers; the rows come from the same resource as the
//! table, so the charts follow the selection and filters for free. Clicking
//! a point or bar publishes a `ChartPick` the table narrows itself to.

use std::cell::RefCell;
use crate::utils::css_var;
use std::rc::Rc;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use baras_types::formatting;
use baras_types::history::PullRow;

use crate::components::class_icons::{get_class_icon, get_role_icon};

use crate::components::echarts::{
    dispose_chart, init_chart, json_to_js, next_animation_frame, on_click, set_js_fn, set_option_replace,
    use_resize_charts,
};

/// What a chart click selects. Carried on every data item as `pick`, so the
/// click handler only has to deserialize what ECharts hands back.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ChartPick {
    Pull { filename: String, start_ms: i64 },
    Night { character: String, date: String },
}

impl ChartPick {
    pub fn matches(&self, row: &PullRow) -> bool {
        match self {
            ChartPick::Pull { filename, start_ms } => row.filename == *filename && row.start_ms == *start_ms,
            ChartPick::Night { character, date } => {
                let (c, d) = night_key(row);
                c == *character && d == *date
            }
        }
    }

    pub fn label(&self, rows: &[PullRow]) -> String {
        match self {
            ChartPick::Pull { .. } => rows
                .iter()
                .find(|r| self.matches(r))
                .map(|r| format!("pull {}", pull_label(&r.timestamp)))
                .unwrap_or_else(|| "pull".into()),
            ChartPick::Night { character, date } => format!("{character} · {date}"),
        }
    }
}

const DPS_ID: &str = "hist-chart-dps";
const EHPS_ID: &str = "hist-chart-ehps";
const NIGHTS_ID: &str = "hist-chart-nights";
const CHART_IDS: [&str; 3] = [DPS_ID, EHPS_ID, NIGHTS_ID];
/// Kill / wipe colours for the hover dot and the tooltip's result line
const KILL_COLOR: &str = "#2ecc71";
const WIPE_COLOR: &str = "#e74c3c";
const NO_KILL_COLOR: &str = "#6c7a89";

/// `Sep 12 19:15:03` from an ISO timestamp. Each pull gets its own category
/// (seconds keep same-minute pulls apart); the axis label formatter shows
/// only the day, the tooltip shows all of it.
fn pull_label(iso: &str) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let month = iso.get(5..7).and_then(|m| m.parse::<usize>().ok()).and_then(|m| MONTHS.get(m.wrapping_sub(1)));
    let day = iso.get(8..10).and_then(|d| d.parse::<u32>().ok());
    let time = iso.get(11..19).unwrap_or("");
    match (month, day) {
        (Some(m), Some(d)) => format!("{m} {d} {time}"),
        _ => iso.to_string(),
    }
}

/// Minimal HTML escape for names placed in the tooltip markup
fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A pull point: its value plus what the tooltip shows (character with role
/// and discipline icons, the value as text, kill or wipe) and the click pick
fn pull_point(r: &PullRow, value: i64, shown: String) -> Value {
    let icon = |a: Option<&'static manganis::Asset>| a.map(|a| a.to_string()).unwrap_or_default();
    let (result, result_color) = match r.boss_hp_pct {
        _ if r.success => ("Kill".to_string(), KILL_COLOR),
        Some(pct) => (format!("Wipe · {pct:.1} %"), WIPE_COLOR),
        None => ("Wipe".to_string(), WIPE_COLOR),
    };
    json!({
        "value": value,
        "pick": ChartPick::Pull { filename: r.filename.clone(), start_ms: r.start_ms },
        // Invisible until hovered (the series' emphasis), but still a click target
        "itemStyle": { "color": result_color, "opacity": 0 },
        "who": escape(&r.character),
        "role": icon(r.role_icon.as_deref().and_then(get_role_icon)),
        "disc": icon(r.discipline_icon.as_deref().and_then(get_class_icon)),
        "shown": shown,
        "result": result,
        "rc": result_color,
    })
}

/// Tooltip body for pull charts (axis trigger); reads the `pull_point` fields
const PULL_TOOLTIP: &str = "var p = params[0], d = p.data || {};\
    var img = function (u) { return u ? '<img src=\"' + u + '\" style=\"width:16px;height:16px;vertical-align:-3px;margin-right:4px\">' : ''; };\
    return '<div style=\"font-size:11px;opacity:.7;margin-bottom:2px\">' + p.name + '</div>'\
      + '<div>' + img(d.role) + img(d.disc) + '<b>' + (d.who || '') + '</b></div>'\
      + '<div>' + p.marker + p.seriesName + ': <b>' + (d.shown || p.value) + '</b></div>'\
      + '<div style=\"color:' + d.rc + ';font-weight:600\">' + (d.result || '') + '</div>';";

fn base_option(title: &str) -> Value {
    json!({
        "title": { "text": title, "left": "center", "textStyle": { "color": css_var("--text-primary"), "fontSize": 12 } },
        "grid": { "left": 52, "right": 16, "top": 32, "bottom": 28 },
        "animation": false,
        "tooltip": { "trigger": "axis", "backgroundColor": css_var("--bg-elevated"), "borderColor": css_var("--border-strong"), "textStyle": { "color": css_var("--text-primary") } },
    })
}

/// Pulls evenly spaced in order, labelled by day; ECharts thins the labels
/// to what fits, and the tooltip carries the full time.
fn pull_axis(labels: Vec<String>) -> Value {
    json!({
        "type": "category",
        "data": labels,
        "axisLabel": { "color": css_var("--text-muted"), "fontSize": 10, "hideOverlap": true },
        "axisTick": { "alignWithLabel": true },
        "splitLine": { "show": false }
    })
}

/// Axis labels at period boundaries only, keyed by point index: months
/// while they fit, then quarters, then years. The points themselves stay
/// evenly spaced and unlabelled; the tooltip has their time.
fn period_ticks(timestamps: &[&str]) -> Value {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let ym = |ts: &str| -> Option<(u32, u32)> {
        Some((ts.get(..4)?.parse().ok()?, ts.get(5..7)?.parse().ok()?))
    };
    let month = |(y, m): (u32, u32)| format!("{} {y}", MONTHS[(m as usize).saturating_sub(1).min(11)]);
    let quarter = |(y, m): (u32, u32)| format!("Q{} {y}", (m + 2) / 3);
    let year = |(y, _): (u32, u32)| y.to_string();
    let ticks = |label: &dyn Fn((u32, u32)) -> String| -> Vec<(usize, String)> {
        let mut out: Vec<(usize, String)> = Vec::new();
        for (i, ts) in timestamps.iter().enumerate() {
            if let Some(p) = ym(ts) {
                let l = label(p);
                if out.last().is_none_or(|(_, prev)| *prev != l) {
                    out.push((i, l));
                }
            }
        }
        out
    };
    let mut t = ticks(&month);
    if t.len() > 6 {
        t = ticks(&quarter);
    }
    if t.len() > 8 {
        t = ticks(&year);
    }
    Value::Object(t.into_iter().map(|(i, l)| (i.to_string(), Value::String(l))).collect())
}

/// One point per pull where the logging character has the metric, in
/// chronological order; the tooltip says who played and whether it was a kill.
fn metric_option(rows: &[PullRow], title: &str, color: &str, pick: impl Fn(&PullRow) -> Option<i64>) -> Option<Value> {
    let points: Vec<(&PullRow, i64)> = rows.iter().filter_map(|r| pick(r).filter(|v| *v > 0).map(|v| (r, v))).collect();
    if points.is_empty() {
        return None;
    }
    let labels: Vec<String> = points.iter().map(|(r, _)| pull_label(&r.timestamp)).collect();
    let ticks = period_ticks(&points.iter().map(|(r, _)| r.timestamp.as_str()).collect::<Vec<_>>());
    let data: Vec<Value> = points.iter().map(|(r, v)| pull_point(r, *v, formatting::format_compact(*v, false))).collect();
    let mut opt = base_option(title);
    opt["_pull_tooltip"] = json!(true);
    opt["xAxis"] = pull_axis(labels);
    // Read by the effect to install the boundary-only label/tick/gridline functions
    opt["_ticks"] = ticks;
    // A boundary label reads as "from here on", so it hangs to the right of its tick
    opt["xAxis"]["axisLabel"]["align"] = json!("left");
    opt["xAxis"]["splitLine"] = json!({ "show": true, "lineStyle": { "color": css_var("--border-light"), "type": "dashed" } });
    opt["yAxis"] = json!({ "type": "value", "axisLabel": { "color": css_var("--text-muted") }, "splitLine": { "lineStyle": { "color": css_var("--border-light") } }, "scale": true });
    opt["dataZoom"] = json!([{ "type": "inside", "zoomOnMouseWheel": true, "moveOnMouseMove": true }]);
    opt["series"] = json!([{
        // No markers; the hover dot takes the pull's kill / wipe colour
        "type": "line", "name": title, "data": data, "symbol": "circle", "symbolSize": 9, "showAllSymbol": true,
        "emphasis": { "itemStyle": { "opacity": 1 } },
        "itemStyle": { "color": color },
        "lineStyle": { "color": color, "width": 1.5 }, "areaStyle": { "color": color, "opacity": 0.08 },
    }]);
    Some(opt)
}

/// Kills only: time to kill per clear, in order, with the same tooltip
/// as the metric charts; y is m:ss via `_mmss`.
fn ttk_option(rows: &[PullRow]) -> Option<Value> {
    let kills: Vec<&PullRow> = rows.iter().filter(|r| r.success).collect();
    if kills.is_empty() {
        return None;
    }
    let labels: Vec<String> = kills.iter().map(|r| pull_label(&r.timestamp)).collect();
    let ticks = period_ticks(&kills.iter().map(|r| r.timestamp.as_str()).collect::<Vec<_>>());
    let data: Vec<Value> = kills
        .iter()
        .map(|r| pull_point(r, r.duration_seconds, formatting::format_duration(r.duration_seconds)))
        .collect();
    let mut opt = base_option("Time to kill");
    opt["_pull_tooltip"] = json!(true);
    opt["xAxis"] = pull_axis(labels);
    opt["_ticks"] = ticks;
    opt["_mmss"] = json!(true);
    opt["xAxis"]["axisLabel"]["align"] = json!("left");
    opt["xAxis"]["splitLine"] = json!({ "show": true, "lineStyle": { "color": css_var("--border-light"), "type": "dashed" } });
    opt["yAxis"] = json!({ "type": "value", "axisLabel": { "color": css_var("--text-muted") }, "splitLine": { "lineStyle": { "color": css_var("--border-light") } }, "scale": true });
    opt["dataZoom"] = json!([{ "type": "inside", "zoomOnMouseWheel": true, "moveOnMouseMove": true }]);
    opt["series"] = json!([{
        "type": "line", "name": "Time to kill", "data": data, "symbol": "circle", "symbolSize": 9, "showAllSymbol": true,
        "emphasis": { "itemStyle": { "opacity": 1 } },
        "itemStyle": { "color": KILL_COLOR },
        "lineStyle": { "color": KILL_COLOR, "width": 1.5 }, "areaStyle": { "color": KILL_COLOR, "opacity": 0.08 },
    }]);
    Some(opt)
}

/// Raid-night key: the character and the calendar day the night started on.
/// Pulls before 06:00 belong to the previous evening, and a relog mid-night
/// (a new log file) stays in the same bar.
pub fn night_key(r: &PullRow) -> (String, String) {
    let date = r.timestamp.get(..10).unwrap_or("").to_string();
    let hour: u32 = r.timestamp.get(11..13).and_then(|h| h.parse().ok()).unwrap_or(12);
    let date = if hour < 6 { previous_day(&date) } else { date };
    (r.character.clone(), date)
}

/// `YYYY-MM-DD` minus one day, without pulling in a date crate
fn previous_day(date: &str) -> String {
    let mut parts = date.split('-').filter_map(|p| p.parse::<u32>().ok());
    let (Some(y), Some(m), Some(d)) = (parts.next(), parts.next(), parts.next()) else {
        return date.to_string();
    };
    let days_in = |y: u32, m: u32| match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) => 29,
        _ => 28,
    };
    let (y, m, d) = match (m, d) {
        (1, 1) => (y - 1, 12, 31),
        (m, 1) => (y, m - 1, days_in(y, m - 1)),
        (m, d) => (y, m, d - 1),
    };
    format!("{y:04}-{m:02}-{d:02}")
}

/// One bar per raid night and character: pulls that night, grey when the
/// boss stayed up, green when it went down. Grey bars
/// piling up before a green one are the progression cost.
fn nights_option(rows: &[PullRow]) -> Option<Value> {
    let mut nights: Vec<((String, String), &str, u32, bool)> = Vec::new(); // (key, first timestamp, pulls, killed)
    for r in rows {
        let key = night_key(r);
        match nights.iter_mut().find(|n| n.0 == key) {
            Some(n) => {
                n.2 += 1;
                n.3 |= r.success;
            }
            None => nights.push((key, &r.timestamp, 1, r.success)),
        }
    }
    if nights.is_empty() {
        return None;
    }
    let labels: Vec<String> = nights.iter().map(|n| pull_label(n.1)).collect();
    let data: Vec<Value> = nights
        .iter()
        .map(|n| {
            let pick = ChartPick::Night { character: n.0.0.clone(), date: n.0.1.clone() };
            if n.3 {
                json!({ "value": n.2, "pick": pick, "itemStyle": { "color": KILL_COLOR, "borderRadius": 2 } })
            } else {
                json!({ "value": n.2, "pick": pick, "itemStyle": { "color": NO_KILL_COLOR, "borderRadius": 2 } })
            }
        })
        .collect();
    let mut opt = base_option("Pulls per session");
    opt["xAxis"] = pull_axis(labels);
    opt["yAxis"] = json!({ "type": "value", "minInterval": 1, "axisLabel": { "color": css_var("--text-muted") }, "splitLine": { "lineStyle": { "color": css_var("--border-light") } } });
    opt["dataZoom"] = json!([{ "type": "inside", "zoomOnMouseWheel": true, "moveOnMouseMove": true }]);
    opt["series"] = json!([{
        "type": "bar", "name": "Pulls", "data": data, "barMaxWidth": 28,
    }]);
    Some(opt)
}

/// Chronological copy of the resource; the table sorts its own view
fn chronological(pulls: &Resource<Option<Vec<PullRow>>>) -> Vec<PullRow> {
    let read = pulls.read();
    let mut v = read.as_ref().and_then(|p| p.clone()).unwrap_or_default();
    v.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
    v
}

/// With "Kills only" on, every row is a kill and pulls-per-night is
/// meaningless, so the third slot shows time to kill instead.
fn options(rows: &[PullRow], kills_only: bool) -> [(&'static str, Option<Value>); 3] {
    [
        (DPS_ID, metric_option(rows, "DPS", "#e74c3c", |r| r.dps)),
        (EHPS_ID, metric_option(rows, "EHPS", "#2ecc71", |r| r.ehps)),
        (NIGHTS_ID, if kills_only { ttk_option(rows) } else { nights_option(rows) }),
    ]
}

#[component]
pub fn PullCharts(pulls: Resource<Option<Vec<PullRow>>>, pick: Signal<Option<ChartPick>>, kills_only: bool) -> Element {
    let [(_, dps), (_, ehps), (_, nights)] = options(&chronological(&pulls), kills_only);
    use_resize_charts(&CHART_IDS);
    // Click handlers live as long as the component; replaced on every refresh
    let handlers: Rc<RefCell<Vec<Closure<dyn FnMut(JsValue)>>>> = use_hook(|| Rc::new(RefCell::new(Vec::new())));

    // The effect keeps its first closure, so it rebuilds from the resource
    // itself rather than capturing this render's options. The chart divs
    // exist after this render; two frames give them a size.
    use_effect(move || {
        let charts = options(&chronological(&pulls), kills_only);
        let handlers = handlers.clone();
        spawn(async move {
            next_animation_frame().await;
            next_animation_frame().await;
            let mut fresh = Vec::new();
            for (id, option) in &charts {
                match option {
                    Some(option) => {
                        if let (Some(chart), Some(js)) = (init_chart(id), json_to_js(option)) {
                            if let Some(ticks) = option.get("_ticks") {
                                // Period boundaries only: label, tick and gridline share one index set
                                let show = format!("var t = {ticks}; return t[i] !== undefined;");
                                set_js_fn(&js, &["xAxis", "axisLabel", "formatter"], "v, i", &format!("var t = {ticks}; return t[i] || '';"));
                                set_js_fn(&js, &["xAxis", "axisLabel", "interval"], "i, v", &show);
                                set_js_fn(&js, &["xAxis", "axisTick", "interval"], "i, v", &show);
                                set_js_fn(&js, &["xAxis", "splitLine", "interval"], "i, v", &show);
                            } else {
                                // Day only on the axis ("Sep 12"); the category keeps the time for the tooltip
                                set_js_fn(&js, &["xAxis", "axisLabel", "formatter"], "v", "var p = v.split(' '); return p.length > 2 ? p[0] + ' ' + p[1] : v;");
                            }
                            if option.get("_mmss").is_some() {
                                let mmss = "var m = Math.floor(v / 60), s = Math.floor(v % 60); return m + ':' + (s < 10 ? '0' : '') + s;";
                                set_js_fn(&js, &["yAxis", "axisLabel", "formatter"], "v", mmss);
                                set_js_fn(&js, &["tooltip", "valueFormatter"], "v", mmss);
                            }
                            if option.get("_pull_tooltip").is_some() {
                                set_js_fn(&js, &["tooltip", "formatter"], "params", PULL_TOOLTIP);
                            }
                            set_option_replace(&chart, &js);
                            let handler = Closure::new(move |params: JsValue| {
                                let data = js_sys::Reflect::get(&params, &JsValue::from_str("data")).unwrap_or(JsValue::NULL);
                                let hit = js_sys::Reflect::get(&data, &JsValue::from_str("pick")).unwrap_or(JsValue::NULL);
                                if let Ok(hit) = serde_wasm_bindgen::from_value::<ChartPick>(hit) {
                                    // Outside the Dioxus runtime (JS callback), so never panic on a lock
                                    let _ = pick.try_write().map(|mut w| {
                                        *w = if w.as_ref() == Some(&hit) { None } else { Some(hit) };
                                    });
                                }
                            });
                            on_click(&chart, &handler);
                            fresh.push(handler);
                        }
                    }
                    None => dispose_chart(id),
                }
            }
            *handlers.borrow_mut() = fresh;
        });
    });
    use_drop(|| {
        for id in CHART_IDS {
            dispose_chart(id);
        }
    });

    rsx! {
        div { class: "history-charts",
            ChartSlot { id: DPS_ID, has_data: dps.is_some(), empty: "No DPS recorded for you on these pulls" }
            ChartSlot { id: EHPS_ID, has_data: ehps.is_some(), empty: "No healing recorded for you on these pulls" }
            ChartSlot { id: NIGHTS_ID, has_data: nights.is_some(), empty: "No pulls" }
        }
    }
}

/// The chart's DOM host, or the empty-state text. Keyed on `has_data` so the
/// host element is recreated (and the ECharts instance re-bound) on change.
#[component]
fn ChartSlot(id: &'static str, has_data: bool, empty: &'static str) -> Element {
    rsx! {
        if has_data {
            div { id: "{id}", class: "history-chart" }
        } else {
            div { class: "history-chart chart-empty", "{empty}" }
        }
    }
}
