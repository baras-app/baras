//! Charts of the dummy parse's global cooldown use: how long each press
//! took until the next (red when well past the typical GCD or a channel's
//! end, amber for a clipped channel; hover for the
//! abilities, click to pick it in the press table) and the density of those
//! times.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use dioxus::prelude::*;
use serde_json::{Value, json};
use wasm_bindgen::prelude::*;

use baras_types::formatting;
use baras_types::{DummyParse, GcdAnalysis};

use crate::components::ability_icon::fetch_icon_url;
use crate::components::echarts::{
    TOOLTIP_ROWS_JS, dispose_chart, escape, get_chart, init_chart, json_to_js, next_animation_frame, set_chart_option,
    set_js_fn, set_option_replace,
};
use crate::components::mirror_names::{display_id, display_name};
use crate::utils::{LightTheme, css_var};

const TIMELINE_ID: &str = "dummy-gcd-timeline";
const DENSITY_ID: &str = "dummy-gcd-density";
/// Points the density curve is drawn with
const DENSITY_STEPS: usize = 160;
/// Kernel width bounds (s): narrow enough to keep the GCD and channel peaks
/// apart, wide enough not to draw one spike per press
const BANDWIDTH_RANGE: (f32, f32) = (0.03, 0.15);

const MMSS_JS: &str = "var m = Math.floor(v / 60), s = Math.floor(v % 60); return m + ':' + (s < 10 ? '0' : '') + s;";

fn axis_style() -> (Value, Value) {
    (
        json!({ "color": css_var("--text-muted"), "fontSize": 10 }),
        json!({ "lineStyle": { "color": css_var("--border-light") } }),
    )
}

fn tooltip() -> Value {
    json!({
        "trigger": "axis",
        "backgroundColor": css_var("--bg-elevated"),
        "borderColor": css_var("--border-strong"),
        "textStyle": { "color": css_var("--text-primary"), "fontSize": 12 },
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Delay timeline
// ─────────────────────────────────────────────────────────────────────────

/// One block per press, spanning the time until the next one and as tall
/// as that time, so the blocks tile the parse; red when it runs well past
/// the typical GCD (drawn as a line) or a channel's end, amber when a
/// channel was clipped.
fn timeline_option(parse: &DummyParse, selected: Option<usize>, eu: bool) -> Value {
    let (label, split) = axis_style();
    let gcd = &parse.gcd;
    let data: Vec<Value> = gcd
        .presses
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let color = match (selected == Some(i), p.is_clipped(), p.is_long()) {
                (true, ..) => css_var("--text-accent"),
                (_, true, _) => css_var("--color-warning"),
                (_, _, true) => css_var("--color-error"),
                _ => css_var("--text-tertiary"),
            };
            let channel = p.channel.map_or(String::new(), |c| {
                let ticks = match c.full_ticks {
                    0 => format!("{} ticks", c.ticks),
                    full => format!("{}/{full} ticks", c.ticks),
                };
                if c.clipped {
                    format!("{ticks} · clipped")
                } else {
                    ticks
                }
            });
            json!({
                // The id rides along for `PRESS_BLOCK_JS`; it is not drawn
                "value": [p.time_secs, p.interval_secs, p.ability_id],
                "itemStyle": { "color": color },
                "took": format!("{} s", formatting::format_decimal(p.interval_secs, 3, eu)),
                "channel": channel,
                "at": formatting::format_duration_f32(p.time_secs),
                "ability": escape(&display_name(p.ability_id, &p.ability)),
                "next": escape(&display_name(p.next_id, &p.next)),
                "abilityId": display_id(p.ability_id).to_string(),
                "nextId": display_id(p.next_id).to_string(),
            })
        })
        .collect();
    let typical = json!({
        "symbol": "none", "silent": true, "label": { "show": false },
        "lineStyle": { "color": css_var("--text-accent"), "type": "dashed", "width": 1 },
        "data": [{ "yAxis": gcd.typical_gcd_secs }],
    });
    json!({
        "animation": false,
        "grid": { "left": 44, "right": 16, "top": 12, "bottom": 22 },
        "tooltip": tooltip(),
        // Wheel zooms and drag pans, to pick single presses out of a long parse
        "dataZoom": [{ "type": "inside", "xAxisIndex": 0, "filterMode": "none" }],
        "xAxis": {
            "type": "value", "min": 0, "max": parse.duration_secs.max(1),
            "axisLabel": label, "splitLine": { "show": false },
            "axisPointer": { "snap": true },
        },
        "yAxis": { "type": "value", "splitNumber": 2, "axisLabel": label, "splitLine": split },
        "series": [{
            "type": "custom", "name": "To next press", "encode": { "x": 0, "y": 1 },
            "data": data, "markLine": typical,
        }],
    })
}

/// Global holding the ability id hovered in the by-ability table, if any
const HOVER_GLOBAL: &str = "__barasDummyHover";

/// Block from the press to the next one, a pixel short so neighbours read
/// apart; faded while another ability is hovered in the by-ability table
const PRESS_BLOCK_JS: &str = "var t = api.value(0), len = api.value(1), hover = window.__barasDummyHover;\
    var a = api.coord([t, len]), b = api.coord([t + len, 0]);\
    return { type: 'rect', shape: { x: a[0], y: a[1], width: Math.max(b[0] - a[0] - 1, 1), height: b[1] - a[1] },\
             style: { fill: api.visual('color'), opacity: hover == null || hover === api.value(2) ? 1 : 0.12 } };";

/// Global the tooltip reads icon data URLs from, by ability id: carrying them
/// on every press would repeat each a hundred times over
const ICONS_GLOBAL: &str = "__barasDummyIcons";

const TIMELINE_TOOLTIP: &str = "var d = (params[0] || {}).data || {}, icons = window.__barasDummyIcons || {};\
    var named = function (id, name) {\
      return (icons[id] ? '<img src=\"' + icons[id] + '\" width=\"16\" height=\"16\" style=\"vertical-align:-3px;margin-right:4px\">' : '') + name;\
    };\
    return tipHead(d.at) + tipRow('Pressed', named(d.abilityId, d.ability)) + tipRow('Next press', named(d.nextId, d.next))\
      + tipRow('Time between', d.took) + (d.channel ? tipRow('Channel', d.channel) : '');";

/// A click anywhere in the plot hands `cb` the index of the nearest press,
/// i.e. the one the tooltip is showing. Replaces the previous handler.
const NEAREST_CLICK_JS: &str = "var zr = chart.getZr();\
    if (chart.__nearest) zr.off('click', chart.__nearest);\
    chart.__nearest = function (e) {\
      if (!chart.containPixel('grid', [e.offsetX, e.offsetY])) return;\
      var x = chart.convertFromPixel({ xAxisIndex: 0 }, e.offsetX);\
      var d = (chart.getOption().series[0] || {}).data || [], best = -1, gap = Infinity;\
      for (var i = 0; i < d.length; i++) {\
        var g = Math.abs(d[i].value[0] - x);\
        if (g < gap) { gap = g; best = i; }\
      }\
      if (best >= 0) cb(best);\
    };\
    zr.on('click', chart.__nearest);";

/// Time to the next press, for every press. `selected` is an index into
/// `gcd.presses`, shared with the press table: a click here picks the row, a
/// picked row is highlighted here. While `hovered` names an ability, the
/// presses of every other ability fade.
#[component]
pub(super) fn PressTimeline(
    parse: Resource<Option<DummyParse>>,
    selected: Signal<Option<usize>>,
    hovered: ReadSignal<Option<i64>>,
    european: bool,
) -> Element {
    let handler: Rc<RefCell<Option<Closure<dyn FnMut(JsValue)>>>> = use_hook(|| Rc::new(RefCell::new(None)));
    let light_theme = try_use_context::<LightTheme>();
    // Icons for the tooltip, looked up once per parse and naming preference
    use_effect(move || {
        let ids: BTreeSet<i64> = parse
            .read()
            .as_ref()
            .and_then(|p| p.as_ref())
            .map(|p| {
                let ids = p.gcd.presses.iter().flat_map(|p| [p.ability_id, p.next_id]);
                ids.map(display_id).collect()
            })
            .unwrap_or_default();
        spawn(async move {
            let mut icons = HashMap::new();
            for id in ids {
                if let Some(url) = fetch_icon_url(id as u64).await {
                    icons.insert(id.to_string(), url);
                }
            }
            if let (Some(window), Some(js)) = (web_sys::window(), json_to_js(&json!(icons))) {
                let _ = js_sys::Reflect::set(&window, &JsValue::from_str(ICONS_GLOBAL), &js);
            }
        });
    });
    use_effect(move || {
        if let Some(LightTheme(t)) = light_theme {
            t.read();
        }
        let Some(option) = parse
            .read()
            .as_ref()
            .and_then(|p| p.as_ref())
            .map(|p| timeline_option(p, selected(), european))
        else {
            return;
        };
        let handler = handler.clone();
        spawn(async move {
            next_animation_frame().await;
            next_animation_frame().await;
            let (Some(chart), Some(js)) = (init_chart(TIMELINE_ID), json_to_js(&option)) else {
                return;
            };
            set_js_fn(&js, &["xAxis", "axisLabel", "formatter"], "v", MMSS_JS);
            set_js_fn(&js, &["yAxis", "axisLabel", "formatter"], "v", "return v + ' s';");
            set_js_fn(&js, &["series", "0", "renderItem"], "params, api", PRESS_BLOCK_JS);
            set_js_fn(
                &js,
                &["tooltip", "formatter"],
                "params",
                &format!("{TOOLTIP_ROWS_JS}{TIMELINE_TOOLTIP}"),
            );
            set_option_replace(&chart, &js);
            let pick = Closure::new(move |index: JsValue| {
                let hit = index.as_f64().map(|i| i as usize);
                // Outside the Dioxus runtime (JS callback), so never panic on a lock
                let _ = selected
                    .try_write()
                    .map(|mut w| *w = if *w == hit { None } else { hit });
            });
            let install = js_sys::Function::new_with_args("chart, cb", NEAREST_CLICK_JS);
            let _ = install.call2(&JsValue::NULL, &chart, pick.as_ref().unchecked_ref());
            *handler.borrow_mut() = Some(pick);
        });
    });
    // An empty merge redraws the blocks without resetting the zoom
    use_effect(move || {
        let id = hovered().map_or(JsValue::NULL, |id| JsValue::from_f64(id as f64));
        if let Some(window) = web_sys::window() {
            let _ = js_sys::Reflect::set(&window, &JsValue::from_str(HOVER_GLOBAL), &id);
        }
        if let Some(chart) = get_chart(TIMELINE_ID) {
            set_chart_option(&chart, &js_sys::Object::new());
        }
    });
    use_drop(|| dispose_chart(TIMELINE_ID));

    rsx! {
        div { class: "overview-chart-box",
            div { class: "overview-chart-header",
                span { class: "overview-chart-note",
                    "Time to the next press · red: late · amber: clipped channel · scroll to zoom, drag to pan · click one to find it in Presses"
                }
            }
            div { id: TIMELINE_ID, class: "overview-chart", style: "height: 170px;" }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Interval density
// ─────────────────────────────────────────────────────────────────────────

/// Value at quantile `q` (0-1) of an ascending slice
fn quantile(sorted: &[f32], q: f32) -> f32 {
    sorted[((sorted.len() - 1) as f32 * q) as usize]
}

fn mean(values: &[f32]) -> f32 {
    values.iter().sum::<f32>() / values.len().max(1) as f32
}

/// Gaussian kernel density of `sorted` as `[x, density]` points. The long
/// tail (the odd multi-second pause) is cut at the 98th percentile so the
/// peaks keep the width of the chart.
fn density(sorted: &[f32]) -> Vec<[f32; 2]> {
    let n = sorted.len() as f32;
    let m = mean(sorted);
    let sd = (sorted.iter().map(|v| (v - m).powi(2)).sum::<f32>() / n).sqrt();
    let iqr = quantile(sorted, 0.75) - quantile(sorted, 0.25);
    // Silverman's rule, bounded: the intervals are multi-modal (GCD, casts,
    // channels), where the plain rule smears the peaks together
    let spread = if iqr > 0.0 { sd.min(iqr / 1.34) } else { sd };
    let h = (0.9 * spread * n.powf(-0.2)).clamp(BANDWIDTH_RANGE.0, BANDWIDTH_RANGE.1);
    let lo = (sorted[0] - 3.0 * h).max(0.0);
    let hi = quantile(sorted, 0.98) + 3.0 * h;
    let norm = 1.0 / (n * h * (2.0 * std::f32::consts::PI).sqrt());
    (0..=DENSITY_STEPS)
        .map(|i| {
            let x = lo + (hi - lo) * i as f32 / DENSITY_STEPS as f32;
            let sum: f32 = sorted.iter().map(|v| (-0.5 * ((x - v) / h).powi(2)).exp()).sum();
            [x, sum * norm]
        })
        .collect()
}

fn density_option(gcd: &GcdAnalysis) -> Option<Value> {
    let gaps = &gcd.intervals_secs;
    if gaps.len() < 2 {
        return None;
    }
    let (label, _) = axis_style();
    let color = css_var("--text-accent");
    let line = |x: f32, kind: &str| json!({ "xAxis": x, "lineStyle": { "color": css_var("--text-primary"), "type": kind, "width": 1 } });
    Some(json!({
        "animation": false,
        "grid": { "left": 12, "right": 16, "top": 12, "bottom": 22 },
        "tooltip": tooltip(),
        "xAxis": { "type": "value", "scale": true, "axisLabel": label, "splitLine": { "show": false } },
        "yAxis": { "type": "value", "show": false },
        "series": [{
            "type": "line", "name": "Presses", "data": density(gaps), "showSymbol": false, "smooth": true,
            "lineStyle": { "color": color, "width": 1.5 },
            "areaStyle": { "color": color, "opacity": 0.15 },
            "markLine": {
                "symbol": "none", "silent": true, "label": { "show": false },
                "data": [line(quantile(gaps, 0.5), "solid"), line(mean(gaps), "dashed")],
            },
        }],
    }))
}

/// How the time between two on-GCD presses is distributed, with the median
/// (solid) and mean (dashed) marked.
#[component]
pub(super) fn IntervalDensity(parse: Resource<Option<DummyParse>>, european: bool) -> Element {
    let light_theme = try_use_context::<LightTheme>();
    use_effect(move || {
        if let Some(LightTheme(t)) = light_theme {
            t.read();
        }
        let option = parse
            .read()
            .as_ref()
            .and_then(|p| p.as_ref())
            .and_then(|p| density_option(&p.gcd));
        spawn(async move {
            next_animation_frame().await;
            next_animation_frame().await;
            let Some(option) = option else {
                return dispose_chart(DENSITY_ID);
            };
            if let (Some(chart), Some(js)) = (init_chart(DENSITY_ID), json_to_js(&option)) {
                set_js_fn(&js, &["xAxis", "axisLabel", "formatter"], "v", "return v + ' s';");
                set_js_fn(
                    &js,
                    &["tooltip", "formatter"],
                    "params",
                    "return params[0].value[0].toFixed(3) + ' s between presses';",
                );
                set_option_replace(&chart, &js);
            }
        });
    });
    use_drop(|| dispose_chart(DENSITY_ID));

    let read = parse.read();
    let gaps = read
        .as_ref()
        .and_then(|p| p.as_ref())
        .map(|p| p.gcd.intervals_secs.as_slice())
        .unwrap_or_default();
    let secs = |v: f32| format!("{} s", formatting::format_decimal(v, 3, european));
    rsx! {
        div { class: "overview-chart-box",
            div { class: "overview-chart-header",
                if gaps.len() >= 2 {
                    span { class: "overview-chart-note",
                        "Time between presses · median {secs(quantile(gaps, 0.5))} (solid) · mean {secs(mean(gaps))} (dashed)"
                    }
                }
            }
            div { id: DENSITY_ID, class: "overview-chart", style: "height: 150px;" }
        }
    }
}
