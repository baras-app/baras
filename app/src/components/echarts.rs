//! ECharts interop shared by every chart in the app: instance lookup on a DOM
//! element, option application, resize and disposal. Options can be built as
//! `serde_json::Value`, converted with `json_to_js` and applied with
//! `set_option_replace`.

use dioxus::prelude::*;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::utils::js_set;

#[wasm_bindgen]
extern "C" {
    // `catch`: when the CDN script failed to load, `echarts` is undefined and
    // the ReferenceError would otherwise unwind through wasm and wedge the UI
    #[wasm_bindgen(catch, js_namespace = echarts, js_name = init)]
    fn echarts_init(dom: &web_sys::Element, theme: &JsValue, opts: &JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(catch, js_namespace = echarts, js_name = getInstanceByDom)]
    fn echarts_get_instance(dom: &web_sys::Element) -> Result<JsValue, JsValue>;
}

/// The instance bound to `element_id`, creating one if needed. None when the
/// element is not in the DOM (yet).
pub fn init_chart(element_id: &str) -> Option<JsValue> {
    init_with_renderer(element_id, "canvas")
}

/// `init_chart`, drawn as SVG: text stays crisp at fractional display scales
/// and partial redraws leave no artifacts. Suits charts with light data.
pub fn init_svg_chart(element_id: &str) -> Option<JsValue> {
    init_with_renderer(element_id, "svg")
}

fn init_with_renderer(element_id: &str, renderer: &str) -> Option<JsValue> {
    let element = web_sys::window()?.document()?.get_element_by_id(element_id)?;
    let existing = echarts_get_instance(&element).ok()?;
    if !existing.is_null() && !existing.is_undefined() {
        return Some(existing);
    }
    let opts = json_to_js(&serde_json::json!({ "renderer": renderer }))?;
    echarts_init(&element, &JsValue::NULL, &opts).ok()
}

/// An already-initialized instance for `element_id`; never creates one.
pub fn get_chart(element_id: &str) -> Option<JsValue> {
    let element = web_sys::window()?.document()?.get_element_by_id(element_id)?;
    let existing = echarts_get_instance(&element).ok()?;
    (!existing.is_null() && !existing.is_undefined()).then_some(existing)
}

/// `target[name]` when it is a function
fn method(target: &JsValue, name: &str) -> Option<js_sys::Function> {
    js_sys::Reflect::get(target, &JsValue::from_str(name)).ok()?.dyn_into().ok()
}

fn call0(target: &JsValue, name: &str) {
    if let Some(func) = method(target, name) {
        let _ = func.call0(target);
    }
}

pub fn set_chart_option(chart: &JsValue, option: &JsValue) {
    if let Some(func) = method(chart, "setOption") {
        let _ = func.call1(chart, option);
    }
}

/// JSON → plain JS objects (not Maps), ready for `setOption`.
pub fn json_to_js(option: &serde_json::Value) -> Option<JsValue> {
    option.serialize(&serde_wasm_bindgen::Serializer::json_compatible()).ok()
}

/// Put a JS function at `path` (e.g. a label formatter), which JSON cannot
/// carry. Missing intermediate objects are created.
pub fn set_js_fn(obj: &JsValue, path: &[&str], args: &str, body: &str) {
    let mut cur = obj.clone();
    for key in &path[..path.len() - 1] {
        let next = js_sys::Reflect::get(&cur, &JsValue::from_str(key)).ok().filter(|v| v.is_object());
        cur = match next {
            Some(v) => v,
            None => {
                let v = js_sys::Object::new().into();
                let _ = js_sys::Reflect::set(&cur, &JsValue::from_str(key), &v);
                v
            }
        };
    }
    let func = js_sys::Function::new_with_args(args, body);
    let _ = js_sys::Reflect::set(&cur, &JsValue::from_str(path[path.len() - 1]), &func);
}

/// Apply an option (`notMerge`, so a smaller series set replaces the previous
/// one instead of stacking).
pub fn set_option_replace(chart: &JsValue, option: &JsValue) {
    if let Some(func) = method(chart, "setOption") {
        let _ = func.call2(chart, option, &JsValue::TRUE);
    }
}

pub fn resize_chart(chart: &JsValue) {
    call0(chart, "resize");
}

pub fn dispose_chart(element_id: &str) {
    if let Some(chart) = get_chart(element_id) {
        call0(&chart, "dispose");
    }
}

/// Resolves on the next paint; two in a row let a fresh layout settle.
pub async fn next_animation_frame() {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        if let Some(win) = web_sys::window() {
            let cb = wasm_bindgen::closure::Closure::once_into_js(move || {
                let _ = resolve.call0(&JsValue::NULL);
            });
            let _ = win.request_animation_frame(cb.unchecked_ref());
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// Keep the charts bound to `ids` sized to their hosts while the component
/// is mounted: ECharts does not watch its container, so a window resize
/// would otherwise leave the canvas at the old size until the next option.
pub fn use_resize_charts(ids: &'static [&'static str]) {
    // Rc: hooks hand out clones, and the listener must outlive this render
    let closure: std::rc::Rc<Closure<dyn FnMut()>> = use_hook(|| {
        let closure = Closure::new(move || {
            for id in ids {
                if let Some(chart) = get_chart(id) {
                    resize_chart(&chart);
                }
            }
        });
        if let Some(win) = web_sys::window() {
            let _ = win.add_event_listener_with_callback("resize", closure.as_ref().unchecked_ref());
        }
        std::rc::Rc::new(closure)
    });
    use_drop(move || {
        if let Some(win) = web_sys::window() {
            let _ = win.remove_event_listener_with_callback("resize", (*closure).as_ref().unchecked_ref());
        }
    });
}

/// Click anywhere in the plot area (not just on a drawn item) → `handler`
/// gets the first non-null data item of the x-axis column under the cursor,
/// i.e. what an axis-trigger tooltip shows there. Needs a category x-axis
/// with series data aligned to it. Replaces the chart's previous handler.
pub fn on_column_click(chart: &JsValue, handler: &Closure<dyn FnMut(JsValue)>) {
    const INSTALL: &str = "var zr = chart.getZr();\
        if (chart.__columnClick) zr.off('click', chart.__columnClick);\
        chart.__columnClick = function (e) {\
          var pt = [e.offsetX, e.offsetY];\
          if (!chart.containPixel('grid', pt)) return;\
          var i = Math.round(chart.convertFromPixel({ xAxisIndex: 0 }, e.offsetX));\
          var series = chart.getOption().series || [];\
          for (var k = 0; k < series.length; k++) {\
            var d = (series[k].data || [])[i];\
            if (d !== null && d !== undefined) { cb(d); return; }\
          }\
        };\
        zr.on('click', chart.__columnClick);";
    let install = js_sys::Function::new_with_args("chart, cb", INSTALL);
    let _ = install.call2(&JsValue::NULL, chart, handler.as_ref().unchecked_ref());
}

/// `chart.dispatchAction(action)`, e.g. highlight / downplay a series
pub fn dispatch_action(chart: &JsValue, action: &serde_json::Value) {
    if let Some(action) = json_to_js(action)
        && let Some(func) = method(chart, "dispatchAction")
    {
        let _ = func.call1(chart, &action);
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Drag-to-select time range (brush)
// ─────────────────────────────────────────────────────────────────────────

/// Add brush configuration to an ECharts option for drag-to-select time range.
/// Always active — no toolbox icon needed. Reset via the phase timeline bar.
pub fn apply_brush_config(obj: &js_sys::Object) {
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
pub fn activate_brush(chart: &JsValue) {
    dispatch_action(
        chart,
        &serde_json::json!({ "type": "takeGlobalCursor", "key": "brush", "brushOption": { "brushType": "lineX" } }),
    );
}

/// Register a brush-end handler on an ECharts instance that writes the
/// selected x-axis span `(start, end)` to `sink`, then clears the selection
/// from the chart. Replaces any earlier handler, so charts that re-apply
/// options on the same instance don't fire once per registration.
pub fn register_brush_handler(chart: &JsValue, mut sink: Signal<Option<(f64, f64)>>) {
    use wasm_bindgen::closure::Closure;

    let target = chart.clone();
    let closure = Closure::wrap(Box::new(move |params: JsValue| {
        // params.areas[0].coordRange = [start, end] in x-axis coordinates
        let Some(area) = js_sys::Reflect::get(&params, &JsValue::from_str("areas"))
            .ok()
            .and_then(|a| a.dyn_into::<js_sys::Array>().ok())
            .filter(|a| a.length() > 0)
            .map(|a| a.get(0))
        else {
            return;
        };
        if let Ok(range) = js_sys::Reflect::get(&area, &JsValue::from_str("coordRange"))
            && let Some(range) = range.dyn_ref::<js_sys::Array>()
            && let (Some(start), Some(end)) = (range.get(0).as_f64(), range.get(1).as_f64())
            && end > start
        {
            sink.set(Some((start, end)));
        }
        dispatch_action(&target, &serde_json::json!({ "type": "brush", "areas": [] }));
    }) as Box<dyn FnMut(JsValue)>);

    if let Some(off) = method(chart, "off") {
        let _ = off.call1(chart, &JsValue::from_str("brushEnd"));
    }
    if let Some(on) = method(chart, "on") {
        let _ = on.call2(chart, &JsValue::from_str("brushEnd"), closure.as_ref().unchecked_ref());
    }
    closure.forget();
}
