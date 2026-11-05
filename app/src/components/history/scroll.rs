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
