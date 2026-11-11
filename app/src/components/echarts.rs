//! ECharts interop shared by every chart in the app: instance lookup on a DOM
//! element, option application, resize and disposal. Options can be built as
//! `serde_json::Value` and applied with `set_json_option`.

use dioxus::prelude::*;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::utils::js_set;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = echarts, js_name = init)]
    fn echarts_init(dom: &web_sys::Element) -> JsValue;

    #[wasm_bindgen(js_namespace = echarts, js_name = getInstanceByDom)]
    pub fn echarts_get_instance(dom: &web_sys::Element) -> JsValue;
}

/// The instance bound to `element_id`, creating one if needed. None when the
/// element is not in the DOM (yet).
pub fn init_chart(element_id: &str) -> Option<JsValue> {
    let element = web_sys::window()?.document()?.get_element_by_id(element_id)?;
    let existing = echarts_get_instance(&element);
    if !existing.is_null() && !existing.is_undefined() {
        return Some(existing);
    }
    Some(echarts_init(&element))
}

/// An already-initialized instance for `element_id`; never creates one.
pub fn get_chart(element_id: &str) -> Option<JsValue> {
    let element = web_sys::window()?.document()?.get_element_by_id(element_id)?;
    let existing = echarts_get_instance(&element);
    (!existing.is_null() && !existing.is_undefined()).then_some(existing)
}

fn call0(target: &JsValue, method: &str) {
    if let Some(func) = js_sys::Reflect::get(target, &JsValue::from_str(method))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
    {
        let _ = func.call0(target);
    }
}

pub fn set_chart_option(chart: &JsValue, option: &JsValue) {
    if let Some(func) = js_sys::Reflect::get(chart, &JsValue::from_str("setOption"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
    {
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
    if let Some(func) = js_sys::Reflect::get(chart, &JsValue::from_str("setOption"))
        .ok()
        .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
    {
        let _ = func.call2(chart, option, &JsValue::TRUE);
    }
}

/// Apply an option built as JSON.
pub fn set_json_option(chart: &JsValue, option: &serde_json::Value) {
    if let Some(option) = json_to_js(option) {
        set_option_replace(chart, &option);
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

/// Replace the click handler on a chart. Params come back as the ECharts
/// event object; `data` on it is the series item that was hit.
pub fn on_click(chart: &JsValue, handler: &Closure<dyn FnMut(JsValue)>) {
    let get = |name: &str| {
        js_sys::Reflect::get(chart, &JsValue::from_str(name))
            .ok()
            .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
    };
    if let Some(off) = get("off") {
        let _ = off.call1(chart, &JsValue::from_str("click"));
    }
    if let Some(on) = get("on") {
        let _ = on.call2(chart, &JsValue::from_str("click"), handler.as_ref().unchecked_ref());
    }
}

/// `chart.dispatchAction(action)`, e.g. highlight / downplay a series
pub fn dispatch_action(chart: &JsValue, action: &serde_json::Value) {
    if let Some(action) = json_to_js(action)
        && let Some(func) = js_sys::Reflect::get(chart, &JsValue::from_str("dispatchAction"))
            .ok()
            .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
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
pub fn register_brush_handler(chart: &JsValue, mut time_range_sink: Signal<Option<(f64, f64)>>) {
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

    // Replace any earlier listener: charts that re-apply options on the same
    // instance would otherwise fire once per registration
    if let Ok(off_fn) = js_sys::Reflect::get(chart, &JsValue::from_str("off"))
        && let Some(func) = off_fn.dyn_ref::<js_sys::Function>()
    {
        let _ = func.call1(chart, &JsValue::from_str("brushEnd"));
    }

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
