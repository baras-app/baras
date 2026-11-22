//! The Parsely.io section of the Settings tab: credentials and the guild list.

use dioxus::prelude::*;

use super::helpers::{add_parsely_guild, save_parsely_guild_fields, save_setting};

#[component]
pub(super) fn ParselySettings(
    mut username: Signal<String>,
    mut password: Signal<String>,
    mut guilds: Signal<Vec<String>>,
    mut selected_guild: Signal<Option<String>>,
) -> Element {
    let mut guild_input = use_signal(String::new);

    rsx! {
        div { class: "settings-section",
            h4 { "Parsely.io" }
            p { class: "hint", "Upload logs to parsely.io for leaderboards and detailed analysis." }
            div { class: "setting-row",
                label { "Username" }
                input {
                    r#type: "text",
                    placeholder: "Optional",
                    value: username,
                    oninput: move |e| username.set(e.value()),
                    // Saved when the field is left
                    onchange: move |e| {
                        let v = e.value();
                        save_setting("Parsely username", move |cfg| cfg.parsely.username = v);
                    },
                }
            }
            div { class: "setting-row",
                label { "Password" }
                input {
                    r#type: "password",
                    placeholder: "Optional",
                    value: password,
                    oninput: move |e| password.set(e.value()),
                    onchange: move |e| {
                        let v = e.value();
                        save_setting("Parsely password", move |cfg| cfg.parsely.password = v);
                    },
                }
            }
            div { class: "setting-row",
                label { "Guilds" }
                div { class: "parsely-guilds-field",
                    if !guilds.read().is_empty() {
                        div { class: "parsely-guilds-chips",
                            for (idx, guild) in guilds.read().iter().enumerate() {
                                {
                                    let name = guild.clone();
                                    rsx! {
                                        span {
                                            key: "{name}-{idx}",
                                            class: "chip",
                                            "{name}"
                                            button {
                                                class: "chip-remove",
                                                title: "Remove guild",
                                                onclick: move |_| {
                                                    let (guilds_snapshot, selected_snapshot) = {
                                                        let mut list = guilds.write();
                                                        list.retain(|g| g != &name);
                                                        let still_has_selected = selected_guild
                                                            .read()
                                                            .as_deref()
                                                            .map(|s| list.iter().any(|g| g == s))
                                                            .unwrap_or(false);
                                                        if !still_has_selected {
                                                            selected_guild.set(list.first().cloned());
                                                        }
                                                        (list.clone(), selected_guild.read().clone())
                                                    };
                                                    save_parsely_guild_fields(
                                                        guilds_snapshot,
                                                        selected_snapshot,
                                                    );
                                                },
                                                "×"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "parsely-guilds-add",
                        input {
                            r#type: "text",
                            placeholder: "Add a guild...",
                            value: guild_input,
                            oninput: move |e| guild_input.set(e.value()),
                            onkeydown: move |e| {
                                if e.key() == Key::Enter {
                                    add_parsely_guild(
                                        guild_input,
                                        guilds,
                                        selected_guild,
                                    );
                                }
                            }
                        }
                        button {
                            class: "btn btn-secondary btn-sm",
                            onclick: move |_| {
                                add_parsely_guild(
                                    guild_input,
                                    guilds,
                                    selected_guild,
                                );
                            },
                            "Add"
                        }
                    }
                }
            }
        }
    }
}
