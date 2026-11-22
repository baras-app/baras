mod api;
mod app;
mod components;
mod shortcuts;
mod types;
mod utils;

use app::App;
use dioxus::prelude::*;
use dioxus_logger::tracing::Level;

/// Shown once by the panic hook. A panic leaves the wasm instance unusable, so
/// the page would otherwise sit there looking alive but ignoring every click.
const PANIC_BANNER_JS: &str = "if (document.getElementById('panic-banner')) return;
    var bar = document.createElement('div');
    bar.id = 'panic-banner';
    bar.style.cssText = 'position:fixed;left:0;right:0;bottom:0;z-index:2147483647;display:flex;\
        align-items:center;justify-content:center;gap:12px;padding:10px 16px;\
        background:#3a1418;color:#f2d6d6;border-top:1px solid #a33;font:13px sans-serif';
    var text = document.createElement('span');
    text.textContent = 'The interface hit an error and stopped responding.';
    text.title = message;
    var reload = document.createElement('button');
    reload.textContent = 'Reload';
    reload.style.cssText = 'padding:4px 12px;cursor:pointer';
    reload.onclick = function () { location.reload(); };
    bar.append(text, reload);
    document.body.appendChild(bar);";

fn main() {
    // Logging is optional; the app is not
    let _ = dioxus_logger::init(Level::INFO);

    // Dev builds replace this with the dx error overlay when the app launches
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        let message = info.to_string();
        dioxus_logger::tracing::error!("{message}");
        let _ = js_sys::Function::new_with_args("message", PANIC_BANNER_JS)
            .call1(&wasm_bindgen::JsValue::NULL, &wasm_bindgen::JsValue::from_str(&message));
    }));

    launch(App);
}
