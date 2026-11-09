//! Edit forms of the encounter and effect editors open in a drawer at the
//! right of the editor panel, pushing the list over (CSS on `.editor-layout`).
//! One drawer per panel; the guard asks before unsaved edits are thrown away.

use dioxus::prelude::*;

/// Shared by a panel's lists and its open drawer
#[derive(Clone, Copy)]
pub struct DrawerGuard {
    /// The open form has unsaved edits
    dirty: Signal<bool>,
    /// Action waiting on the discard prompt
    pending: Signal<Option<Callback<()>>>,
}

impl DrawerGuard {
    /// Run `action` now, or after the user agrees to discard unsaved edits
    pub fn confirm(&self, action: impl FnMut(()) + 'static) {
        let action = Callback::new(action);
        if *self.dirty.peek() {
            let mut pending = self.pending;
            pending.set(Some(action));
        } else {
            action.call(());
        }
    }

    /// Point `target` (a list's open item) at `value`, guarded
    pub fn switch(&self, mut target: Signal<Option<String>>, value: Option<String>) {
        self.confirm(move |_| target.set(value.clone()));
    }
}

/// Call once at the panel root, and render `DiscardPrompt` inside it
pub fn provide_drawer_guard() -> DrawerGuard {
    use_context_provider(|| DrawerGuard { dirty: Signal::new(false), pending: Signal::new(None) })
}

pub fn use_drawer_guard() -> DrawerGuard {
    use_context()
}

#[component]
pub fn DiscardPrompt() -> Element {
    let guard = use_drawer_guard();
    let Some(action) = *guard.pending.read() else {
        return rsx! {};
    };
    let (mut dirty, mut pending) = (guard.dirty, guard.pending);
    rsx! {
        div { class: "modal-overlay",
            div { class: "modal-content discard-prompt",
                div { class: "modal-header", h3 { "Discard unsaved changes?" } }
                p { class: "hint", "The open item has edits that have not been saved." }
                div { class: "modal-footer",
                    button { class: "btn btn-sm", onclick: move |_| pending.set(None), "Keep editing" }
                    button {
                        class: "btn btn-danger btn-sm",
                        onclick: move |_| {
                            pending.set(None);
                            dirty.set(false);
                            action.call(());
                        },
                        "Discard"
                    }
                }
            }
        }
    }
}

/// The drawer around one item's edit form; `on_close` (none = no close
/// button) runs through the guard; `actions` sit at the header's right
#[component]
pub fn EditorDrawer(
    title: String,
    dirty: bool,
    #[props(default)] on_close: Option<EventHandler<()>>,
    #[props(default = VNode::empty())] actions: Element,
    children: Element,
) -> Element {
    let guard = use_drawer_guard();
    let mut flag = guard.dirty;
    use_effect(use_reactive!(|dirty| flag.set(dirty)));
    // Unmounted with edits (boss or tab switch): nothing is left to guard
    // (try_: the panel owning the flag may already be gone)
    use_drop(move || {
        if let Ok(mut w) = flag.try_write() {
            *w = false;
        }
    });
    let close = move || {
        if let Some(on_close) = on_close {
            guard.confirm(move |_| on_close.call(()));
        }
    };

    rsx! {
        aside {
            class: "editor-drawer",
            tabindex: "-1",
            onkeydown: move |e| {
                if e.key() == Key::Escape {
                    close();
                }
            },
            div { class: "editor-drawer-header",
                h3 { title: "{title}", "{title}" }
                if dirty {
                    span { class: "unsaved-indicator", title: "Unsaved changes" }
                }
                span { class: "editor-drawer-actions",
                    {actions}
                    if on_close.is_some() {
                        button { class: "btn btn-xs btn-ghost", title: "Close (Esc)", onclick: move |_| close(),
                            i { class: "fa-solid fa-xmark" }
                        }
                    }
                }
            }
            // `list-item-body` keeps the form input rules the inline forms had
            div { class: "editor-drawer-body list-item-body", {children} }
        }
    }
}

/// Holds the drawer's place while the list has nothing open, so switching
/// between lists does not resize the panel
#[component]
pub fn EmptyDrawer(hint: String) -> Element {
    rsx! {
        aside { class: "editor-drawer empty",
            div { class: "empty-state-guidance",
                i { class: "fa-solid fa-pen-to-square" }
                p { class: "hint", "{hint}" }
            }
        }
    }
}
