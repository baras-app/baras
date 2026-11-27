//! Import preview: what a TOML file would replace and add, before it is applied

use dioxus::prelude::*;

use crate::types::{EffectImportDiff, EffectImportPreview};

#[component]
pub(crate) fn EffectImportPreviewModal(
    preview: EffectImportPreview,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let has_errors = !preview.errors.is_empty();
    let replace_count = preview.effects_to_replace.len();
    let add_count = preview.effects_to_add.len();
    let unchanged = preview.effects_unchanged;

    // Replacements first, then additions
    let rows: Vec<(bool, &EffectImportDiff)> = preview
        .effects_to_replace
        .iter()
        .map(|e| (true, e))
        .chain(preview.effects_to_add.iter().map(|e| (false, e)))
        .collect();

    rsx! {
        div { class: "modal-overlay", onclick: move |_| on_cancel.call(()),
            div { class: "modal-content effect-import",
                onclick: move |e| e.stop_propagation(),

                div { class: "modal-header",
                    h3 { "Import Effect Definitions" }
                }

                div { class: "modal-body",
                    for error in &preview.errors {
                        div { class: "effect-import-error", "{error}" }
                    }

                    if !rows.is_empty() {
                        table { class: "effect-import-table",
                            thead {
                                tr {
                                    th {}
                                    th { "Name" }
                                    th { "ID" }
                                    th { "Overlays" }
                                }
                            }
                            tbody {
                                for (replaces, effect) in &rows {
                                    tr {
                                        td {
                                            if *replaces {
                                                span { class: "text-warning", title: "Replaces an existing effect", "~" }
                                            } else {
                                                span { class: "text-success", title: "New effect", "+" }
                                            }
                                        }
                                        td { "{effect.name}" }
                                        td { code { class: "text-xs", "{effect.id}" } }
                                        td {
                                            for t in &effect.display_targets {
                                                span { class: "tag", "{t.label()}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                div { class: "modal-footer",
                    div { class: "text-muted text-sm effect-import-summary",
                        "{replace_count} replace, {add_count} new"
                        if unchanged > 0 { ", {unchanged} unchanged" }
                    }
                    button { class: "btn", onclick: move |_| on_cancel.call(()), "Cancel" }
                    button {
                        class: "btn btn-success",
                        disabled: has_errors,
                        onclick: move |_| on_confirm.call(()),
                        "Import"
                    }
                }
            }
        }
    }
}
