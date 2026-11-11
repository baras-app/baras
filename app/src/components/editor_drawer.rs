//! Edit forms of the encounter and effect editors open in a drawer at the
//! right of the editor panel, pushing the list over (CSS on `.editor-layout`).
//! One drawer per panel; the guard asks before unsaved edits are thrown away.

use dioxus::prelude::*;

// ─────────────────────────────────────────────────────────────────────────
// Size: dragged width and folded state, shared by both editors for the session
// ─────────────────────────────────────────────────────────────────────────

/// Width (px) the user dragged the drawer to; None = the CSS default
static DRAWER_WIDTH: GlobalSignal<Option<f64>> = Signal::global(|| None);
static DRAWER_COLLAPSED: GlobalSignal<bool> = Signal::global(|| false);

const MIN_WIDTH: f64 = 360.0;

pub fn window_width() -> f64 {
    web_sys::window()
        .and_then(|w| w.inner_width().ok())
        .and_then(|v| v.as_f64())
        .unwrap_or(1600.0)
}

/// The CSS default width clamp(440px, 46vw, 860px)
fn default_width() -> f64 {
    (window_width() * 0.46).clamp(440.0, 860.0)
}

/// `style` for `.editor-layout`: the dragged width, which both the drawer and
/// the push padding read
pub fn layout_style() -> String {
    DRAWER_WIDTH().map(|w| format!("--editor-drawer-w: {w:.0}px")).unwrap_or_default()
}

/// Left-edge grip of a right-docked drawer: drag to resize, double-click for
/// the default. `width` holds the dragged width (None = CSS default, which
/// `fallback` approximates for the drag start)
#[component]
pub fn ResizeHandle(width: Signal<Option<f64>>, fallback: f64) -> Element {
    // (pointer x, width) when the drag started
    let mut drag = use_signal(|| None::<(f64, f64)>);
    rsx! {
        div {
            class: "editor-drawer-resize",
            title: "Drag to resize · double-click to reset",
            onmousedown: move |e| {
                e.prevent_default();
                drag.set(Some((e.client_coordinates().x, width.peek().unwrap_or(fallback))));
            },
            ondoubleclick: move |_| width.set(None),
        }
        // Full-window catcher while dragging, so the pointer can leave the grip
        if let Some((x0, w0)) = drag() {
            div {
                class: "editor-drawer-drag-capture",
                onmousemove: move |e| {
                    let w = (w0 + x0 - e.client_coordinates().x).clamp(MIN_WIDTH, window_width() * 0.7);
                    width.set(Some(w));
                },
                onmouseup: move |_| drag.set(None),
            }
        }
    }
}

/// Fold / unfold button for the drawer header
#[component]
fn CollapseButton() -> Element {
    let collapsed = DRAWER_COLLAPSED();
    rsx! {
        button {
            class: "btn btn-xs btn-ghost editor-drawer-fold",
            title: if collapsed { "Expand the editor" } else { "Collapse the editor" },
            onclick: move |_| *DRAWER_COLLAPSED.write() = !collapsed,
            i { class: if collapsed { "fa-solid fa-angles-left" } else { "fa-solid fa-angles-right" } }
        }
    }
}

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
                p { "The open item has edits that have not been saved. Discard them and continue?" }
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

    let collapsed = DRAWER_COLLAPSED();

    rsx! {
        aside {
            // Folded, the form stays mounted (hidden) so its edits survive
            class: if collapsed { "editor-drawer collapsed" } else { "editor-drawer" },
            tabindex: "-1",
            onkeydown: move |e| {
                if e.key() == Key::Escape {
                    close();
                }
            },
            if !collapsed {
                ResizeHandle { width: DRAWER_WIDTH.signal(), fallback: default_width() }
            }
            div { class: "editor-drawer-header",
                CollapseButton {}
                h3 { title: "{title}", onclick: move |_| if collapsed { *DRAWER_COLLAPSED.write() = false }, "{title}" }
                if dirty {
                    span { class: "unsaved-indicator", title: "Unsaved changes" }
                }
                span { class: "editor-drawer-actions",
                    if !collapsed {
                        {actions}
                    }
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
    let collapsed = DRAWER_COLLAPSED();
    rsx! {
        aside { class: if collapsed { "editor-drawer empty collapsed" } else { "editor-drawer empty" },
            if collapsed {
                div { class: "editor-drawer-header", CollapseButton {} }
            } else {
                ResizeHandle { width: DRAWER_WIDTH.signal(), fallback: default_width() }
                div { class: "empty-state-guidance",
                    i { class: "fa-solid fa-pen-to-square" }
                    p { class: "hint", "{hint}" }
                }
            }
        }
    }
}
