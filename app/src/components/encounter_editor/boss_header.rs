//! One header row over the encounter editor: area › boss, the open boss's
//! final-boss / enabled toggles and export, and the area's own actions in a menu.

use dioxus::prelude::*;

use crate::api;
use crate::types::BossWithPath;

#[component]
pub fn EditorHeader(
    area_name: String,
    /// The open boss, if any
    boss: Option<BossWithPath>,
    on_refetch: EventHandler<()>,
    on_status: EventHandler<(String, bool)>,
    on_export_area: EventHandler<()>,
    on_import: EventHandler<()>,
) -> Element {
    let mut menu_open = use_signal(|| false);

    rsx! {
        div { class: "editor-header",
            h2 {
                span { class: if boss.is_some() { "editor-header-area" } else { "" }, "{area_name}" }
                if let Some(bwp) = boss.as_ref() {
                    i { class: "fa-solid fa-chevron-right editor-header-sep" }
                    span { title: "{bwp.boss.id}", "{bwp.boss.name}" }
                }
            }
            div { class: "editor-header-actions",
                if let Some(bwp) = boss.clone() {
                    BossActions { bwp, on_refetch, on_status }
                }
                div { class: "nav-menu",
                    button { class: "btn btn-sm btn-ghost", title: "Area actions", onclick: move |_| menu_open.toggle(),
                        i { class: "fa-solid fa-ellipsis-vertical" }
                    }
                    if menu_open() {
                        div { class: "nav-menu-backdrop", onclick: move |_| menu_open.set(false) }
                        div { class: "nav-menu-list",
                            for (icon, label, handler) in [
                                ("fa-solid fa-file-export", "Export area", on_export_area),
                                ("fa-solid fa-file-import", "Import into area", on_import),
                            ] {
                                button {
                                    class: "nav-menu-item",
                                    onclick: move |_| {
                                        menu_open.set(false);
                                        handler.call(());
                                    },
                                    i { class: icon }
                                    " {label}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Final-boss flag, enabled toggle and export for one boss
#[component]
fn BossActions(bwp: BossWithPath, on_refetch: EventHandler<()>, on_status: EventHandler<(String, bool)>) -> Element {
    let (id, path) = (bwp.boss.id.clone(), bwp.file_path.clone());
    let is_final = bwp.boss.is_final_boss;
    let enabled = bwp.boss.enabled;
    let (id_f, path_f, id_e, path_e) = (id.clone(), path.clone(), id.clone(), path.clone());

    rsx! {
        button {
            class: if is_final { "btn btn-sm btn-ghost text-warning" } else { "btn btn-sm btn-ghost text-muted" },
            title: if is_final { "Final boss (auto-stops ops timer) — click to unset" } else { "Mark as final boss (auto-stops ops timer on kill)" },
            onclick: move |_| {
                let (bid, fp) = (id_f.clone(), path_f.clone());
                spawn(async move {
                    match api::update_boss_is_final_boss(&bid, &fp, !is_final).await {
                        Ok(()) => on_refetch.call(()),
                        Err(e) => on_status.call((e, true)),
                    }
                });
            },
            i { class: "fa-solid fa-flag" }
        }
        button {
            class: if enabled { "btn btn-sm btn-ghost text-success" } else { "btn btn-sm btn-ghost text-muted" },
            title: if enabled { "Enabled — click to disable this boss" } else { "Disabled — click to enable this boss" },
            onclick: move |_| {
                let (bid, fp) = (id_e.clone(), path_e.clone());
                spawn(async move {
                    match api::update_boss_enabled(&bid, &fp, !enabled).await {
                        Ok(()) => on_refetch.call(()),
                        Err(e) => on_status.call((e, true)),
                    }
                });
            },
            i { class: if enabled { "fa-solid fa-circle-check" } else { "fa-regular fa-circle" } }
        }
        button {
            class: "btn btn-sm",
            title: "Export this boss as TOML",
            onclick: move |_| {
                let (bid, fp) = (id.clone(), path.clone());
                spawn(async move {
                    match api::export_encounter_toml(Some(&bid), &fp).await {
                        Ok(result) => {
                            let default_name = if result.is_bundled { format!("{bid}_custom.toml") } else { format!("{bid}.toml") };
                            if let Some(save_path) = api::save_file_dialog(&default_name).await {
                                match api::save_export_file(&save_path, &result.toml).await {
                                    Ok(()) => on_status.call(("Boss exported".to_string(), false)),
                                    Err(e) => on_status.call((e, true)),
                                }
                            }
                        }
                        Err(e) => on_status.call((e, true)),
                    }
                });
            },
            "Export"
        }
    }
}
