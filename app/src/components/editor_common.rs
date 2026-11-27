//! Pieces shared by the encounter and effect editors: where a definition came
//! from, and the Save / Duplicate / Delete-or-Reset action bar.

use baras_types::preview::{OverlayPreviewRequest, PreviewAudio};
use dioxus::prelude::*;

use crate::api;
use crate::components::toast::{ToastSeverity, use_toast};
use crate::types::AudioConfig;

/// Where an editable definition came from
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Draft not yet saved
    New,
    /// Ships with the app, unedited
    Builtin,
    /// Ships with the app, edited by the user
    Modified,
    /// Created by the user
    Custom,
}

impl Origin {
    /// `is_builtin` = unedited built-in, `is_modified` = edited built-in
    pub fn of(is_builtin: bool, is_modified: bool) -> Self {
        if is_builtin {
            Self::Builtin
        } else if is_modified {
            Self::Modified
        } else {
            Self::Custom
        }
    }

    /// Modifier class shared by the badge and the origin filter toggle
    pub fn css_class(&self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Builtin => "builtin",
            Self::Modified => "modified",
            Self::Custom => "custom",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::New => "New",
            Self::Builtin => "Built-in",
            Self::Modified => "Modified",
            Self::Custom => "Custom",
        }
    }
}

/// Origin dot for a list row; an unedited built-in is the norm and shows
/// nothing (the blank keeps the column). `kind` names the item in the
/// tooltip ("timer")
#[component]
pub fn OriginBadge(origin: Origin, kind: &'static str) -> Element {
    let title = match origin {
        Origin::New => "New: not yet saved".to_string(),
        Origin::Builtin => String::new(),
        Origin::Modified => format!("Modified: built-in {kind} you have edited"),
        Origin::Custom => "Custom: created by you".to_string(),
    };
    rsx! { span { class: "origin-badge {origin.css_class()}", title: "{title}" } }
}

/// Origins in filter order: the user's own first
pub const ORIGINS: [Origin; 3] = [Origin::Custom, Origin::Modified, Origin::Builtin];

/// Encounter builder lists: show only this origin (session-wide, all tabs)
pub static ENCOUNTER_ORIGIN: GlobalSignal<Option<Origin>> = Signal::global(|| None);

/// How many of `origins` fall under each [`ORIGINS`] entry
pub fn origin_counts(origins: impl Iterator<Item = Origin>) -> [usize; 3] {
    let mut n = [0; 3];
    for o in origins {
        if let Some(i) = ORIGINS.iter().position(|x| *x == o) {
            n[i] += 1;
        }
    }
    n
}

/// Custom / Modified / Built-in segmented toggle. Nothing lit = any origin;
/// clicking the lit one clears it. `kind` names the items ("timers")
#[component]
pub fn OriginFilter(
    selected: Option<Origin>,
    counts: [usize; 3],
    kind: &'static str,
    on_pick: EventHandler<Option<Origin>>,
) -> Element {
    rsx! {
        div { class: "filter-pills", title: "Where the {kind} come from",
            for (o, n) in ORIGINS.into_iter().zip(counts) {
                {
                    let lit = selected == Some(o);
                    rsx! {
                        button {
                            class: "seg origin-seg {o.css_class()}",
                            class: if lit { "active" },
                            title: "{n} {kind}",
                            onclick: move |_| on_pick.call((!lit).then_some(o)),
                            OriginBadge { origin: o, kind }
                            "{o.label()}"
                        }
                    }
                }
            }
        }
    }
}

/// An edit form's draft of `stored`, and whether it has unsaved edits. A save
/// clears them once it reaches `stored`, so a failed one stays unsaved. Without
/// edits the draft follows `stored`: the refetch after a save changes origin
/// flags, which must not read as an edit.
pub fn use_item_draft<T: Clone + PartialEq + 'static>(stored: T) -> (Signal<T>, Memo<bool>) {
    let mut draft = use_signal(|| stored.clone());
    let mut baseline = use_signal(|| stored.clone());
    use_effect(use_reactive!(|stored| {
        if *baseline.peek() == stored {
            return;
        }
        if *draft.peek() == *baseline.peek() {
            draft.set(stored.clone());
        }
        baseline.set(stored);
    }));
    let has_changes = use_memo(move || *draft.read() != *baseline.read());
    (draft, has_changes)
}

/// An edit form's buttons, for its drawer's header: Save, any extra buttons
/// (`children`), then a "more" menu with Duplicate, the form's own entries
/// (`menu`) and the removal that fits the origin: none for an unedited
/// built-in, Reset for an edited one, Delete for a custom one. An unsaved
/// draft gets Cancel instead of the menu.
#[component]
pub fn ItemActions(
    has_changes: bool,
    origin: Origin,
    on_save: EventHandler<()>,
    on_delete: EventHandler<()>,
    #[props(default)] on_duplicate: Option<EventHandler<()>>,
    /// Extra `nav-menu-item` buttons for the "more" menu
    #[props(default)]
    menu: Option<Element>,
    #[props(default)] children: Element,
) -> Element {
    let mut open = use_signal(|| false);
    let mut confirm = use_signal(|| false);
    // (icon, label, question, consequence, confirm button tone)
    let removal = match origin {
        Origin::Modified => Some((
            "fa-rotate-left",
            "Reset to built-in",
            "Reset to built-in?",
            "Your edits are dropped and the built-in definition comes back.",
            "btn-warning",
        )),
        Origin::Custom => Some((
            "fa-trash",
            "Delete",
            "Delete this entry?",
            "This cannot be undone.",
            "btn-danger",
        )),
        _ => None,
    };
    let has_menu = on_duplicate.is_some() || menu.is_some() || removal.is_some();

    rsx! {
        span { class: "item-actions",
            button {
                class: if has_changes { "btn btn-success btn-sm" } else { "btn btn-sm" },
                disabled: !has_changes,
                title: "Save (Ctrl+S)",
                "data-shortcut-save": "true",
                onclick: move |_| on_save.call(()),
                "Save"
            }
            {children}
            if origin == Origin::New {
                button { class: "btn btn-sm", onclick: move |_| on_delete.call(()), "Cancel" }
            }
            if has_menu {
                div { class: "nav-menu",
                    button {
                        class: "btn btn-sm btn-ghost",
                        title: "More actions",
                        onclick: move |_| open.toggle(),
                        i { class: "fa-solid fa-ellipsis" }
                    }
                    if open() {
                        div { class: "nav-menu-backdrop", onclick: move |_| open.set(false) }
                        // Any pick closes the list
                        div { class: "nav-menu-list align-right", onclick: move |_| open.set(false),
                            if let Some(dup) = on_duplicate {
                                button { class: "nav-menu-item", onclick: move |_| dup.call(()),
                                    i { class: "fa-solid fa-copy" }
                                    "Duplicate"
                                }
                            }
                            if let Some(extra) = menu {
                                {extra}
                            }
                            if let Some((icon, label, ..)) = removal {
                                div { class: "nav-menu-divider" }
                                button { class: "nav-menu-item danger", onclick: move |_| confirm.set(true),
                                    i { class: "fa-solid {icon}" }
                                    "{label}"
                                }
                            }
                        }
                    }
                }
            }
            if confirm() && let Some((_, label, question, consequence, tone)) = removal {
                div { class: "modal-overlay", onclick: move |_| confirm.set(false),
                    div { class: "modal-content discard-prompt", onclick: move |e| e.stop_propagation(),
                        div { class: "modal-header", h3 { "{question}" } }
                        p { "{consequence}" }
                        div { class: "modal-footer",
                            button { class: "btn btn-sm", onclick: move |_| confirm.set(false), "Cancel" }
                            button {
                                class: "btn {tone} btn-sm",
                                onclick: move |_| {
                                    confirm.set(false);
                                    on_delete.call(());
                                },
                                "{label}"
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Titled section of an edit form; `icon` is a Font Awesome solid name ("fa-tag")
#[component]
pub fn FormCard(
    icon: &'static str,
    title: &'static str,
    /// Right side of the header (e.g. an "Add" button)
    #[props(default = VNode::empty())]
    action: Element,
    children: Element,
) -> Element {
    rsx! {
        div { class: "form-card",
            div { class: "form-card-header",
                i { class: "fa-solid {icon}" }
                span { "{title}" }
                span { class: "form-card-action", {action} }
            }
            div { class: "form-card-content", {children} }
        }
    }
}

/// Shows the form's unsaved draft on its overlays for a few seconds, with its audio cues
/// at their real timing.
#[component]
pub fn PreviewButton(request: OverlayPreviewRequest) -> Element {
    let mut toast = use_toast();
    rsx! {
        button {
            class: "btn btn-sm",
            title: "Show this entry on its overlays for a few seconds (unsaved changes included)",
            onclick: move |_| {
                let request = request.clone();
                spawn(async move {
                    if request.targets.is_empty() && request.audio.is_none() {
                        toast.show("Nothing to preview: no supported overlay selected", ToastSeverity::Info);
                        return;
                    }
                    match api::preview_overlay_entry(&request).await {
                        Ok(missing) if !missing.is_empty() => toast.show(
                            format!("Overlay not enabled: {}", missing.join(", ")),
                            ToastSeverity::Info,
                        ),
                        Ok(_) => {}
                        Err(e) => toast.show(e, ToastSeverity::Normal),
                    }
                });
            },
            i { class: "fa-solid fa-eye" }
            " Preview"
        }
    }
}

/// A draft's audio settings as preview cues (`None` when audio is off)
pub fn preview_audio(audio: &AudioConfig, instant: bool) -> Option<PreviewAudio> {
    audio.enabled.then(|| PreviewAudio {
        file: audio.file.clone(),
        offset: audio.offset,
        countdown_start: audio.countdown_start,
        countdown_voice: audio.countdown_voice.clone(),
        instant,
    })
}
