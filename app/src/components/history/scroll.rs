//! Scroll offsets of the History panes, remembered for the app session: a
//! tab switch unmounts the pane, so each one saves on scroll and puts the
//! offset back once its data has rendered.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use dioxus::prelude::*;

use crate::components::echarts::next_animation_frame;

thread_local! {
    static OFFSETS: RefCell<HashMap<&'static str, i32>> = RefCell::new(HashMap::new());
}

fn element(id: &str) -> Option<web_sys::Element> {
    web_sys::window()?.document()?.get_element_by_id(id)
}

/// Scroll the container with `id` back to its first row
pub fn scroll_to_top(id: &'static str) {
    if let Some(el) = element(id) {
        el.set_scroll_top(0);
        OFFSETS.with(|o| o.borrow_mut().insert(id, 0));
    }
}

/// `onscroll` handler body for the element with `id`
pub fn save_scroll(id: &'static str) {
    if let Some(el) = element(id) {
        OFFSETS.with(|o| o.borrow_mut().insert(id, el.scroll_top()));
    }
}

/// Put the saved offsets of `ids` back once `ready` first turns true after
/// mount; two frames later, so the content has its height by then.
pub fn use_restore_scroll(ids: &'static [&'static str], ready: bool) {
    let done = use_hook(|| Rc::new(Cell::new(false)));
    if ready && !done.get() {
        done.set(true);
        spawn(async move {
            next_animation_frame().await;
            next_animation_frame().await;
            for id in ids {
                if let Some(top) = OFFSETS.with(|o| o.borrow().get(id).copied())
                    && let Some(el) = element(id)
                {
                    el.set_scroll_top(top);
                }
            }
        });
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Incremental rendering
// ─────────────────────────────────────────────────────────────────────────

/// Rows a long list renders at first, and adds per step
pub const PAGE: usize = 100;

/// Cap on a long list in the scroll container `id`: `PAGE` rows, then
/// another page each time the user nears the bottom. `key` names what the
/// list was built from (filter, selection, sort); a new key starts over.
#[derive(Clone, Copy, PartialEq)]
pub struct Paged {
    state: Signal<(String, usize)>,
}

pub fn use_paged(key: &str) -> Paged {
    Paged { state: use_signal(|| (key.to_string(), PAGE)) }
}

impl Paged {
    pub fn limit(&self, key: &str) -> usize {
        let s = self.state.read();
        if s.0 == key { s.1 } else { PAGE }
    }

    /// `onscroll` body: remember the offset and grow the cap near the bottom
    pub fn on_scroll(mut self, id: &'static str, key: &str, total: usize) {
        save_scroll(id);
        let limit = self.limit(key);
        if limit < total
            && element(id).is_some_and(|el| el.scroll_top() + el.client_height() >= el.scroll_height() - 400)
        {
            self.state.set((key.to_string(), limit + PAGE));
        }
    }

    pub fn show_all(mut self, key: &str) {
        self.state.set((key.to_string(), usize::MAX));
    }
}
