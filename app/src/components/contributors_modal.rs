//! Contributors modal — credits popup listing project contributors.

use dioxus::prelude::*;

use crate::api;

/// A project contributor. Edit this list to add/remove entries.
struct Contributor {
    name: &'static str,
    /// What they contributed (e.g. "Core development", "Encounter definitions")
    role: &'static str,
    /// Optional link (GitHub profile, etc.)
    url: Option<&'static str>,
}

const CONTRIBUTORS: &[Contributor] = &[
    Contributor {
        name: "pdubs",
        role: "Core Developer",
        url: Some("https://github.com/apdubs"),
    },
    Contributor {
        name: "Syd-Moonlighter",
        role: "Raid Frame OCR screen capture",
        url: Some("https://github.com/Syd-Moonlighter"),
    },
    Contributor {
        name: "Wolfy",
        role: "Timer Definitions / General Feedback",
        url: None,
    },
    Contributor {
        name: "Keetsune",
        role: "Timer Definitions / General Feedback",
        url: None,
    },
    Contributor {
        name: "Sinrai",
        role: "Timer Definitions / General Feedback",
        url: None,
    },
    Contributor {
        name: "chriseli",
        role: "Timer Definitions / General Feedback",
        url: None,
    },
    Contributor {
        name: "AppalachianMountain",
        role: "Timer Definitions / General Feedback",
        url: None,
    },
    Contributor {
        name: "Zazeg",
        role: "Timer Definitions / General Feedback",
        url: None,
    },
];

struct BundledModel {
    name: &'static str,
    role: &'static str,
    license: &'static str,
    url: &'static str,
}

const MODELS: &[BundledModel] = &[BundledModel {
    name: "ocrs text recognition",
    role: "Raid frame name detection",
    license: "CC BY-SA 4.0",
    url: "https://huggingface.co/robertknight/ocrs",
}];

#[component]
pub fn ContributorsModal(open: Signal<bool>) -> Element {
    let mut open = open;

    if !open() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| open.set(false),
            div {
                class: "changelog-modal contributors-modal",
                onclick: move |e| e.stop_propagation(),
                div { class: "changelog-header",
                    h3 {
                        i { class: "fa-solid fa-heart" }
                        " Contributors"
                    }
                    button {
                        class: "btn btn-close",
                        onclick: move |_| open.set(false),
                        title: "Close",
                        i { class: "fa-solid fa-xmark" }
                    }
                }
                div { class: "changelog-content",
                    p { class: "contributors-intro",
                        "Thank you to the community memebers who contributed their time and energy into providing feedback, timer and effect definitions, or code contributions to this project."
                    }
                    ul { class: "contributors-list",
                        for c in CONTRIBUTORS.iter() {
                            li { key: "{c.name}",
                                if let Some(url) = c.url {
                                    a {
                                        class: "contributor-name",
                                        href: "#",
                                        onclick: move |e| {
                                            e.prevent_default();
                                            spawn(async move {
                                                api::open_url(url).await;
                                            });
                                        },
                                        "{c.name}"
                                    }
                                } else {
                                    span { class: "contributor-name", "{c.name}" }
                                }
                                span { class: "contributor-role", "{c.role}" }
                            }
                        }
                    }
                    h4 { class: "contributors-section", "Third-party model" }
                    ul { class: "contributors-list",
                        for m in MODELS.iter() {
                            li { key: "{m.name}",
                                a {
                                    class: "contributor-name",
                                    href: "#",
                                    onclick: move |e| {
                                        e.prevent_default();
                                        spawn(async move {
                                            api::open_url(m.url).await;
                                        });
                                    },
                                    "{m.name}"
                                }
                                span { class: "contributor-role", "{m.role} · {m.license}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
