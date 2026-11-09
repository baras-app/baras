//! Import preview: what importing an encounter TOML will change, per boss,
//! before anything is written.

use dioxus::prelude::*;

use crate::types::{ImportItemDiff, ImportPreview};

/// Items grouped by type: (type, names), types in name order
fn group_by_type(items: &[ImportItemDiff]) -> Vec<(String, Vec<String>)> {
    let mut map: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    for item in items {
        map.entry(item.item_type.clone()).or_default().push(item.name.clone());
    }
    map.into_iter().collect()
}

/// "1 timer" / "3 timers"
fn count_label(item_type: &str, count: usize) -> String {
    if count == 1 { format!("1 {item_type}") } else { format!("{count} {item_type}s") }
}

/// One change line: badge (Replace / Add), count, then the names
#[component]
fn ChangeRow(kind: &'static str, item_type: String, names: Vec<String>) -> Element {
    let verb = if kind == "add" { "Add" } else { "Replace" };
    rsx! {
        div { class: "import-change",
            span { class: "import-badge {kind}", "{verb}" }
            div { class: "import-change-text",
                span { class: "import-count", {count_label(&item_type, names.len())} }
                span { class: "import-names", "{names.join(\", \")}" }
            }
        }
    }
}

#[component]
pub fn ImportPreviewModal(
    preview: ImportPreview,
    target_area_name: Option<String>,
    on_confirm: EventHandler<()>,
    on_cancel: EventHandler<()>,
) -> Element {
    let has_errors = !preview.errors.is_empty();

    rsx! {
        div { class: "modal-overlay", onclick: move |_| on_cancel.call(()),
            div { class: "modal-content import-preview", onclick: move |e| e.stop_propagation(),
                div { class: "modal-header",
                    h3 { "Import Encounter Definition" }
                }

                div { class: "import-meta",
                    if let Some(ref name) = preview.source_area_name {
                        span { class: "import-meta-key", "From" }
                        span { "{name}" }
                    }
                    span { class: "import-meta-key", "Into" }
                    if preview.is_new_area {
                        span { class: "text-warning", "New area" }
                    } else {
                        span { {target_area_name.clone().unwrap_or_default()} }
                    }
                }

                if has_errors {
                    div { class: "import-errors",
                        for err in &preview.errors {
                            div { class: "import-error",
                                i { class: "fa-solid fa-triangle-exclamation" }
                                span { "{err}" }
                            }
                        }
                    }
                }

                div { class: "import-body",
                    for boss in &preview.bosses {
                        section { class: "import-boss",
                            div { class: "import-boss-head",
                                span { class: "import-boss-name", "{boss.boss_name}" }
                                if boss.is_new_boss {
                                    span { class: "import-badge new", "New boss" }
                                }
                            }
                            if boss.is_new_boss {
                                div { class: "import-summary",
                                    {group_by_type(&boss.items_to_add).iter().map(|(t, n)| count_label(t, n.len())).collect::<Vec<_>>().join(" · ")}
                                }
                            } else {
                                for (item_type, names) in group_by_type(&boss.items_to_replace) {
                                    ChangeRow { kind: "replace", item_type, names }
                                }
                                for (item_type, names) in group_by_type(&boss.items_to_add) {
                                    ChangeRow { kind: "add", item_type, names }
                                }
                                if boss.items_unchanged > 0 {
                                    div { class: "import-summary", "{boss.items_unchanged} unchanged" }
                                }
                            }
                        }
                    }
                }

                div { class: "modal-footer",
                    button { class: "btn", onclick: move |_| on_cancel.call(()), "Cancel" }
                    button { class: "btn btn-success", disabled: has_errors, onclick: move |_| on_confirm.call(()), "Import" }
                }
            }
        }
    }
}
