//! In-app keyboard shortcuts: one document-level keydown listener.
//!
//! Escape is DOM-driven so dialogs need no registration: it clicks the
//! topmost click-to-dismiss backdrop, else the last open drawer's
//! `data-esc-close` button. Letter keys click the visible element tagged
//! `data-shortcut="<key>"`, so buttons own their actions. Up/Down step through
//! the Analysis encounter list and digits pick its view tab. Shortcuts are
//! ignored while typing, with a modifier held, or inside a
//! `data-no-shortcuts` element (hotkey capture). The exception is Ctrl+S,
//! which clicks the open editor form's `data-shortcut-save` button from
//! anywhere, including its own fields.

use dioxus::prelude::*;
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{Document, HtmlElement, KeyboardEvent};

use crate::types::{MainTab, UiSessionState};

/// Click-to-dismiss layers, which always sit above drawers
const BACKDROPS: &str = ".modal-backdrop, .modal-overlay, .nav-menu-backdrop, .context-menu-backdrop";
const DRAWER_CLOSE: &str = "[data-esc-close]";

const SAVE: &str = "[data-shortcut-save]";
const ENCOUNTERS: &str = ".data-explorer .sidebar-encounter-item:not(.live-query-item)";
const VIEW_TABS: &str = ".explorer-views .data-tab";

/// `[` / `]` cycle order, matching the header
const TAB_ORDER: [MainTab; 10] = [
    MainTab::DataExplorer,
    MainTab::Overlays,
    MainTab::EncounterBuilder,
    MainTab::Effects,
    MainTab::BossStats,
    MainTab::RaidSummaries,
    MainTab::PvpStats,
    MainTab::LogManagement,
    MainTab::Characters,
    MainTab::Settings,
];

pub fn use_app_shortcuts(ui_state: Signal<UiSessionState>) {
    use_hook(move || {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            return;
        };
        let doc = document.clone();
        let closure = Closure::<dyn Fn(KeyboardEvent)>::new(move |e: KeyboardEvent| {
            if e.default_prevented() || e.meta_key() || e.alt_key() {
                return;
            }
            let target = e.target().and_then(|t| t.dyn_into::<HtmlElement>().ok());
            if e.ctrl_key() {
                if e.key().eq_ignore_ascii_case("s")
                    && let Some(save) = last_visible(&doc, SAVE)
                {
                    // Leaving the field first commits what was typed into it
                    if let Some(t) = &target {
                        let _ = t.blur();
                    }
                    save.click();
                    e.prevent_default();
                }
                return;
            }
            if let Some(t) = &target
                && t.closest("[data-no-shortcuts]").ok().flatten().is_some()
            {
                return;
            }
            let editing = target.as_ref().is_some_and(is_editable);
            let modal_open = last_visible(&doc, BACKDROPS).is_some();

            match e.key().as_str() {
                // First Escape leaves the field, the next one closes
                "Escape" if editing => {
                    if let Some(t) = &target {
                        let _ = t.blur();
                    }
                }
                "Escape" => {
                    let Some(el) = last_visible(&doc, BACKDROPS).or_else(|| last_visible(&doc, DRAWER_CLOSE)) else {
                        return;
                    };
                    el.click();
                }
                _ if editing || modal_open => return,
                key @ ("[" | "]") => {
                    let step = if key == "]" { 1 } else { TAB_ORDER.len() - 1 };
                    let mut ui_state = ui_state; // Copy handle: closure is `Fn`
                    let mut state = ui_state.write();
                    let idx = TAB_ORDER.iter().position(|t| *t == state.active_tab).unwrap_or(0);
                    state.active_tab = TAB_ORDER[(idx + step) % TAB_ORDER.len()];
                }
                key @ ("ArrowUp" | "ArrowDown") => {
                    let rows = visible_all(&doc, ENCOUNTERS);
                    let selected = rows
                        .iter()
                        .position(|el| el.class_name().split_whitespace().any(|c| c == "selected"));
                    let next = match (selected, key) {
                        (Some(i), "ArrowUp") => i.checked_sub(1),
                        (Some(i), _) => Some(i + 1),
                        (None, _) => Some(0),
                    };
                    let Some(el) = next.and_then(|i| rows.get(i)) else {
                        return;
                    };
                    el.click();
                    scroll_into_view_nearest(el);
                }
                key if key.len() == 1 && key.chars().all(|c| c.is_ascii_digit()) => {
                    // 1–9 then 0 for the tenth view
                    let n = key.parse::<usize>().unwrap_or(0);
                    let tabs = visible_all(&doc, VIEW_TABS);
                    let Some(el) = tabs.get(if n == 0 { 9 } else { n - 1 }) else {
                        return;
                    };
                    el.click();
                }
                key if key.len() == 1 && key.chars().all(|c| c.is_ascii_alphabetic()) => {
                    let selector = format!("[data-shortcut=\"{}\"]", key.to_ascii_lowercase());
                    let Some(el) = last_visible(&doc, &selector) else {
                        return;
                    };
                    el.click();
                }
                _ => return,
            }
            e.prevent_default();
        });
        let _ = document.add_event_listener_with_callback("keydown", closure.as_ref().unchecked_ref());
        // App root lives for the whole session
        closure.forget();
    });
}

fn is_editable(el: &HtmlElement) -> bool {
    matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT") || el.is_content_editable()
}

/// Every visible match, in DOM order
fn visible_all(doc: &Document, selector: &str) -> Vec<HtmlElement> {
    let Ok(nodes) = doc.query_selector_all(selector) else {
        return Vec::new();
    };
    (0..nodes.length())
        .filter_map(|i| nodes.item(i)?.dyn_into::<HtmlElement>().ok())
        .filter(|el| el.offset_width() > 0 || el.offset_height() > 0)
        .collect()
}

/// `scrollIntoView({ block: "nearest" })`: only scrolls when the row is off-screen
pub(crate) fn scroll_into_view_nearest(el: &HtmlElement) {
    let opts = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&opts, &"block".into(), &"nearest".into());
    if let Ok(f) = js_sys::Reflect::get(el, &"scrollIntoView".into())
        && let Ok(f) = f.dyn_into::<js_sys::Function>()
    {
        let _ = f.call1(el, &opts);
    }
}

/// Last rendered (DOM order ≈ stacking order) visible match
fn last_visible(doc: &Document, selector: &str) -> Option<HtmlElement> {
    let nodes = doc.query_selector_all(selector).ok()?;
    (0..nodes.length())
        .rev()
        .filter_map(|i| nodes.item(i)?.dyn_into::<HtmlElement>().ok())
        .find(|el| el.offset_width() > 0 || el.offset_height() > 0)
}
