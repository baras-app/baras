//! Pieces shared by the encounter and effect editors: where a definition came
//! from, and the Save / Duplicate / Delete-or-Reset action bar.

use dioxus::prelude::*;

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

/// One-letter origin marker; `kind` names the item in the tooltip ("timer")
#[component]
pub fn OriginBadge(origin: Origin, kind: &'static str) -> Element {
    let (letter, title) = match origin {
        Origin::New => ("N", "New: not yet saved".to_string()),
        Origin::Builtin => ("B", "Built-in: ships with the app".to_string()),
        Origin::Modified => ("M", format!("Modified: built-in {kind} you have edited")),
        Origin::Custom => ("C", "Custom: created by you".to_string()),
    };
    rsx! { span { class: "origin-badge {origin.css_class()}", title: "{title}", "{letter}" } }
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
pub fn OriginFilter(selected: Option<Origin>, counts: [usize; 3], kind: &'static str, on_pick: EventHandler<Option<Origin>>) -> Element {
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
                            " {o.label()}"
                        }
                    }
                }
            }
        }
    }
}

/// Save, optional Duplicate, any extra buttons (`children`), then the removal
/// action that fits the origin: none for an unedited built-in, Reset for an
/// edited one, Delete for a custom one, Cancel for an unsaved draft.
#[component]
pub fn ItemActions(
    has_changes: bool,
    origin: Origin,
    on_save: EventHandler<()>,
    on_delete: EventHandler<()>,
    #[props(default)] on_duplicate: Option<EventHandler<()>>,
    #[props(default)] children: Element,
) -> Element {
    let mut confirm = use_signal(|| false);
    let (label, prompt, tone) = match origin {
        Origin::Modified => ("Reset to Built-in", "Reset to built-in?", "btn-warning"),
        _ => ("Delete", "Delete?", "btn-danger"),
    };

    rsx! {
        div { class: "form-actions",
            button {
                class: if has_changes { "btn btn-success btn-sm" } else { "btn btn-sm" },
                disabled: !has_changes,
                onclick: move |_| on_save.call(()),
                "Save"
            }
            if let Some(dup) = on_duplicate {
                button { class: "btn btn-primary btn-sm", onclick: move |_| dup.call(()), "Duplicate" }
            }
            {children}
            match origin {
                Origin::Builtin => rsx! {},
                Origin::New => rsx! {
                    button { class: "btn btn-sm ml-auto", onclick: move |_| on_delete.call(()), "Cancel" }
                },
                _ if confirm() => rsx! {
                    span { class: "flex items-center gap-xs ml-auto",
                        "{prompt}"
                        button {
                            class: "btn {tone} btn-sm",
                            onclick: move |_| {
                                confirm.set(false);
                                on_delete.call(());
                            },
                            "Yes"
                        }
                        button { class: "btn btn-sm", onclick: move |_| confirm.set(false), "No" }
                    }
                },
                _ => rsx! {
                    button { class: "btn {tone} btn-sm ml-auto", onclick: move |_| confirm.set(true), "{label}" }
                },
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
    #[props(default = VNode::empty())] action: Element,
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
