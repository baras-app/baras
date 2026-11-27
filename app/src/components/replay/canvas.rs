//! Bindings to the canvas player in `assets/replay.js`, which owns the
//! playback clock and all drawing.

use dioxus::prelude::*;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use baras_types::{MapDefinition, PhaseSegment, ReplayData};

const _: Asset = asset!(
    "/assets/replay.js",
    AssetOptions::js().with_static_head(true).with_minify(false)
);

#[wasm_bindgen]
extern "C" {
    type Handle;

    #[wasm_bindgen(catch, js_namespace = BarasReplay, js_name = create)]
    fn create(canvas: &web_sys::Element, data: &JsValue, meta: &JsValue) -> Result<Handle, JsValue>;
    #[wasm_bindgen(method)]
    fn play(this: &Handle);
    #[wasm_bindgen(method)]
    fn pause(this: &Handle);
    #[wasm_bindgen(method)]
    fn seek(this: &Handle, secs: f64);
    #[wasm_bindgen(method, js_name = setSpeed)]
    fn set_speed(this: &Handle, speed: f64);
    #[wasm_bindgen(method, js_name = setScale)]
    fn set_scale(this: &Handle, scale: f64);
    #[wasm_bindgen(method, js_name = resetView)]
    fn reset_view(this: &Handle);
    #[wasm_bindgen(method, js_name = setHighlight)]
    fn set_highlight(this: &Handle, id: JsValue);
    #[wasm_bindgen(method, js_name = setMaps)]
    fn set_maps(this: &Handle, entries: &JsValue, urls: &JsValue, phases: &JsValue);
    #[wasm_bindgen(method, js_name = setPinnedMap)]
    fn set_pinned_map(this: &Handle, index: JsValue);
    #[wasm_bindgen(method, js_name = onTick)]
    fn on_tick(this: &Handle, cb: &Closure<dyn FnMut(f64, bool)>);
    #[wasm_bindgen(method, js_name = onPick)]
    fn on_pick(this: &Handle, cb: &Closure<dyn FnMut(JsValue)>);
    #[wasm_bindgen(method, js_name = onScale)]
    fn on_scale(this: &Handle, cb: &Closure<dyn FnMut(f64)>);
    #[wasm_bindgen(method, js_name = onMapChange)]
    fn on_map_change(this: &Handle, cb: &Closure<dyn FnMut(f64)>);
    #[wasm_bindgen(method)]
    fn destroy(this: &Handle);
}

#[derive(Serialize)]
struct CanvasData<'a> {
    duration_secs: f32,
    entities: &'a [baras_types::ReplayEntity],
}

/// Icons and numbers the canvas draws on player dots, by entity id
#[derive(Serialize, Default)]
pub struct DotMeta {
    pub icons: std::collections::HashMap<String, String>,
    pub numbers: std::collections::HashMap<String, u8>,
}

/// A mounted canvas player. Dropping it does not stop it: call `destroy`.
pub struct Player {
    handle: Handle,
    _tick: Closure<dyn FnMut(f64, bool)>,
    _pick: Closure<dyn FnMut(JsValue)>,
    _map: Closure<dyn FnMut(f64)>,
    _scale: Closure<dyn FnMut(f64)>,
}

/// Maps and flattened structs become plain objects, which is what the script reads
fn to_js<T: Serialize>(value: &T) -> JsValue {
    value
        .serialize(&serde_wasm_bindgen::Serializer::json_compatible())
        .unwrap_or(JsValue::NULL)
}

impl Player {
    /// Starts a player on the canvas `element_id`. None when the element or
    /// the script is missing.
    pub fn mount(
        element_id: &str,
        data: &ReplayData,
        meta: &DotMeta,
        mut time: Signal<f32>,
        mut playing: Signal<bool>,
        on_pick: impl FnMut(Option<i64>) + 'static,
        mut shown_map: Signal<Option<usize>>,
        mut scale: Signal<f64>,
    ) -> Option<Self> {
        let canvas = web_sys::window()?.document()?.get_element_by_id(element_id)?;
        // The script draws tracks only: casts, effects and ability uses stay in Rust
        let tracks = CanvasData {
            duration_secs: data.duration_secs,
            entities: &data.entities,
        };
        let handle = create(&canvas, &to_js(&tracks), &to_js(meta)).ok()?;
        let tick = Closure::<dyn FnMut(f64, bool)>::new(move |secs: f64, is_playing: bool| {
            time.set(secs as f32);
            if *playing.peek() != is_playing {
                playing.set(is_playing);
            }
        });
        let mut on_pick = on_pick;
        let pick = Closure::<dyn FnMut(JsValue)>::new(move |id: JsValue| on_pick(id.as_f64().map(|id| id as i64)));
        let map = Closure::<dyn FnMut(f64)>::new(move |index: f64| shown_map.set(Some(index as usize)));
        handle.on_tick(&tick);
        handle.on_pick(&pick);
        let zoom = Closure::<dyn FnMut(f64)>::new(move |value: f64| scale.set(value));
        handle.on_map_change(&map);
        handle.on_scale(&zoom);
        Some(Self {
            handle,
            _tick: tick,
            _pick: pick,
            _map: map,
            _scale: zoom,
        })
    }

    pub fn play(&self) {
        self.handle.play();
    }

    pub fn pause(&self) {
        self.handle.pause();
    }

    pub fn seek(&self, secs: f32) {
        self.handle.seek(secs as f64);
    }

    pub fn set_speed(&self, speed: f64) {
        self.handle.set_speed(speed);
    }

    pub fn set_scale(&self, scale: f64) {
        self.handle.set_scale(scale);
    }

    /// Default zoom, recentred
    pub fn reset_view(&self) {
        self.handle.reset_view();
    }

    pub fn set_highlight(&self, id: Option<i64>) {
        self.handle
            .set_highlight(id.map_or(JsValue::NULL, |id| JsValue::from_f64(id as f64)));
    }

    /// Map entries, one image URL per entry (None until it is loaded) and the
    /// fight's phases
    pub fn set_maps(&self, maps: &[MapDefinition], urls: &[Option<String>], phases: &[PhaseSegment]) {
        self.handle.set_maps(&to_js(&maps), &to_js(&urls), &to_js(&phases));
    }

    /// Pin a map entry; None follows the raid
    pub fn set_pinned_map(&self, index: Option<usize>) {
        self.handle
            .set_pinned_map(index.map_or(JsValue::NULL, |i| JsValue::from_f64(i as f64)));
    }

    pub fn destroy(&self) {
        self.handle.destroy();
    }
}
