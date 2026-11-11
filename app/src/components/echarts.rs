//! ECharts interop shared by every chart in the app: instance lookup on a DOM
//! element, option application, resize and disposal. Options can be built as
//! `serde_json::Value` and applied with `set_json_option`.

use dioxus::prelude::*;
use serde::Serialize;
use wasm_bindgen::prelude::*;

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
