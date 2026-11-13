//! In-app keyboard shortcuts: one document-level keydown listener.
//!
//! Escape is DOM-driven so dialogs need no registration: it clicks the
//! topmost click-to-dismiss backdrop, else the last open drawer's
//! `data-esc-close` button. Letter keys click the visible element tagged
//! `data-shortcut="<key>"`, so buttons own their actions. Shortcuts are
//! ignored while typing, with a modifier held, or inside a
//! `data-no-shortcuts` element (hotkey capture).

use dioxus::prelude::*;
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{Document, HtmlElement, KeyboardEvent};

use crate::types::{MainTab, UiSessionState};

/// Click-to-dismiss layers, which always sit above drawers
const BACKDROPS: &str = ".modal-backdrop, .modal-overlay, .nav-menu-backdrop";
const DRAWER_CLOSE: &str = "[data-esc-close]";

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
            if e.default_prevented() || e.ctrl_key() || e.meta_key() || e.alt_key() {
                return;
            }
            let target = e.target().and_then(|t| t.dyn_into::<HtmlElement>().ok());
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

/// Last rendered (DOM order ≈ stacking order) visible match
fn last_visible(doc: &Document, selector: &str) -> Option<HtmlElement> {
    let nodes = doc.query_selector_all(selector).ok()?;
    (0..nodes.length())
        .rev()
        .filter_map(|i| nodes.item(i)?.dyn_into::<HtmlElement>().ok())
        .find(|el| el.offset_width() > 0 || el.offset_height() > 0)
}
