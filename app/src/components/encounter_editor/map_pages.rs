//! Maps tab table: the pages the Replay tab draws this boss on, whether each
//! image is on this machine, and a preview of it.

use dioxus::prelude::*;

use baras_types::MapDefinition;

use crate::api;

#[component]
pub fn MapPagesTable(
    file_path: String,
    maps: Vec<MapDefinition>,
    /// The boss's `map_pages`: the area's pages it is fought on
    boss_pages: Vec<String>,
) -> Element {
    let narrowed = !boss_pages.is_empty();
    let pages = use_resource(use_reactive!(|(file_path, maps, boss_pages)| async move {
        api::get_boss_map_pages(&file_path, &maps, &boss_pages).await
    }));
    // (title, image as a data URL; None while it loads or when it is missing)
    let mut preview = use_signal(|| None::<(String, Option<String>)>);

    let Some(pages) = pages() else {
        return rsx! {};
    };
    let source = if pages.from_area && narrowed {
        "From the map repository: the pages of this area the boss is fought on"
    } else if pages.from_area {
        "From the map repository: every page the game has for this area"
    } else {
        "From this boss's own entries"
    };

    rsx! {
        div { class: "map-pages",
            if pages.pages.is_empty() {
                div { class: "text-sm text-muted",
                    if pages.from_area && !pages.has_index {
                        "The map repository's index has not been downloaded yet. Open this fight in the Replay tab to fetch it."
                    } else {
                        "No maps for this boss. Replay is not offered for it until it has one."
                    }
                }
            } else {
                div { class: "text-sm text-muted", "{source}" }
                table { class: "pull-history-table map-pages-table",
                    thead {
                        tr {
                            th { "Map" }
                            th { "Image" }
                            th { "Phases" }
                            th { "Status" }
                            th {}
                        }
                    }
                    tbody {
                        for (i, page) in pages.pages.into_iter().enumerate() {
                            {
                                let title = page.map.label.clone().unwrap_or_else(|| page.map.image.clone());
                                let image = page.map.image.clone();
                                let phases = match page.map.phases.is_empty() {
                                    true => "Any".to_string(),
                                    false => page.map.phases.join(", "),
                                };
                                rsx! {
                                    tr { key: "{i}",
                                        td { "{title}" }
                                        td { class: "text-muted", "{image}" }
                                        td { "{phases}" }
                                        td {
                                            if page.on_disk {
                                                span { class: "tag tag-success", "Downloaded" }
                                            } else {
                                                span { class: "tag tag-muted", "Not downloaded" }
                                            }
                                        }
                                        td {
                                            button {
                                                class: "btn btn-xs",
                                                disabled: !page.on_disk,
                                                onclick: move |_| {
                                                    preview.set(Some((title.clone(), None)));
                                                    let image = image.clone();
                                                    spawn(async move {
                                                        let url = api::get_replay_map_image(&image).await;
                                                        if let Some(p) = preview.write().as_mut() {
                                                            p.1 = url;
                                                        }
                                                    });
                                                },
                                                "View"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some((title, url)) = preview() {
            div { class: "modal-overlay", onclick: move |_| preview.set(None),
                div { class: "modal-content map-preview", onclick: move |e| e.stop_propagation(),
                    div { class: "modal-header",
                        h3 { "{title}" }
                        button {
                            class: "btn btn-xs btn-ghost",
                            "data-esc-close": true,
                            onclick: move |_| preview.set(None),
                            i { class: "fa-solid fa-xmark" }
                        }
                    }
                    match url {
                        Some(src) => rsx! { img { src, alt: "{title}" } },
                        None => rsx! { div { class: "text-sm text-muted", "Loading..." } },
                    }
                }
            }
        }
    }
}
