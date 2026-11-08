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
}

/// One-letter origin marker; `kind` names the item in the tooltip ("timer")
#[component]
pub fn OriginBadge(origin: Origin, kind: &'static str) -> Element {
    let (letter, class, title) = match origin {
        Origin::New => ("N", "new", "New: not yet saved".to_string()),
        Origin::Builtin => ("B", "builtin", "Built-in: ships with the app".to_string()),
        Origin::Modified => ("M", "modified", format!("Modified: built-in {kind} you have edited")),
        Origin::Custom => ("C", "custom", "Custom: created by you".to_string()),
    };
    rsx! { span { class: "origin-badge {class}", title: "{title}", "{letter}" } }
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
