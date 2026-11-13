//! Profile list, create/rename/delete, and per-role default profiles.

use dioxus::prelude::*;
use std::collections::HashMap;

use crate::api;
use crate::components::{ToastSeverity, use_toast};
use crate::types::{MAX_PROFILES, MetricType, OverlaySettings};

#[component]
pub fn ProfilesSection(
    mut settings: Signal<OverlaySettings>,
    mut profile_names: Signal<Vec<String>>,
    mut active_profile: Signal<Option<String>>,
    mut profile_dirty: Signal<bool>,
    mut metric_overlays_enabled: Signal<HashMap<MetricType, bool>>,
    mut personal_enabled: Signal<bool>,
    mut raid_enabled: Signal<bool>,
    mut overlays_visible: Signal<bool>,
) -> Element {
    let mut new_profile_name = use_signal(String::new);
    let mut profile_status = use_signal(String::new);
    let mut renaming_profile: Signal<Option<String>> = use_signal(|| None);
    let mut rename_input = use_signal(String::new);
    let mut toast = use_toast();

    let mut role_defaults: Signal<HashMap<String, String>> = use_signal(HashMap::new);
    use_future(move || async move {
        role_defaults.set(api::get_default_profiles_per_role().await);
    });

    rsx! {
        div { class: "settings-section",
            div { class: "collapsible-content",
                // Profile list
                div { class: "profile-list",
                        for name in profile_names().iter() {
                            {
                                let profile_name = name.clone();
                                let is_active = active_profile().as_ref() == Some(&profile_name);
                                let is_renaming = renaming_profile().as_ref() == Some(&profile_name);
                                rsx! {
                                    div {
                                        key: "{profile_name}",
                                        class: if is_active { "profile-item active" } else { "profile-item" },
                                        // Profile name: inline rename input or static text
                                        if is_renaming {
                                            input {
                                                class: "profile-rename-input",
                                                r#type: "text",
                                                maxlength: "32",
                                                value: rename_input,
                                                autofocus: true,
                                                oninput: move |e| rename_input.set(e.value()),
                                                onkeydown: {
                                                    let old_name = profile_name.clone();
                                                    move |e: KeyboardEvent| {
                                                        if e.key() == Key::Enter {
                                                            let new_name = rename_input().trim().to_string();
                                                            let old = old_name.clone();
                                                            if new_name.is_empty() || new_name == old {
                                                                renaming_profile.set(None);
                                                                return;
                                                            }
                                                            spawn(async move {
                                                                if let Err(err) = api::rename_profile(&old, &new_name).await {
                                                                    toast.show(format!("Failed to rename: {}", err), ToastSeverity::Normal);
                                                                } else {
                                                                    profile_names.set(api::get_profile_names().await);
                                                                    active_profile.set(api::get_active_profile().await);
                                                                    profile_status.set(format!("Renamed '{}' to '{}'", old, new_name));
                                                                }
                                                                renaming_profile.set(None);
                                                            });
                                                        } else if e.key() == Key::Escape {
                                                            renaming_profile.set(None);
                                                        }
                                                    }
                                                },
                                            }
                                        } else {
                                            span { class: "profile-name", "{profile_name}" }
                                        }
                                        div { class: "profile-actions",
                                            // Load button
                                            button {
                                                class: "btn btn-small btn-load",
                                                disabled: is_active,
                                                onclick: {
                                                    let pname = profile_name.clone();
                                                    move |_| {
                                                        let pname = pname.clone();
                                                        spawn(async move {
                                                            if let Err(err) = api::load_profile(&pname).await {
                                                                toast.show(format!("Failed to load profile: {}", err), ToastSeverity::Normal);
                                                            } else {
                                                                active_profile.set(Some(pname.clone()));
                                                                profile_dirty.set(false);
                                                                profile_status.set(format!("Loaded '{}'", pname));
                                                                if let Some(config) = api::get_config().await {
                                                                    settings.set(config.overlay_settings);
                                                                }
                                                                if let Some(status) = api::get_overlay_status().await {
                                                                    let new_map: HashMap<MetricType, bool> = MetricType::all()
                                                                        .iter()
                                                                        .map(|ot| (*ot, status.enabled.contains(&ot.config_key().to_string())))
                                                                        .collect();
                                                                    metric_overlays_enabled.set(new_map);
                                                                    personal_enabled.set(status.personal_enabled);
                                                                    raid_enabled.set(status.raid_enabled);
                                                                    overlays_visible.set(status.overlays_visible);
                                                                }
                                                            }
                                                        });
                                                    }
                                                },
                                                if is_active { "Active" } else { "Load" }
                                            }
                                            // Save button
                                            button {
                                                class: "btn btn-small btn-update",
                                                title: "Overwrite profile with current settings",
                                                onclick: {
                                                    let pname = profile_name.clone();
                                                    move |_| {
                                                        let pname = pname.clone();
                                                        spawn(async move {
                                                            if let Err(err) = api::save_profile(&pname).await {
                                                                toast.show(format!("Failed to save profile: {}", err), ToastSeverity::Normal);
                                                            } else {
                                                                active_profile.set(Some(pname.clone()));
                                                                profile_dirty.set(false);
                                                                profile_status.set(format!("Saved '{}'", pname));
                                                            }
                                                        });
                                                    }
                                                },
                                                "Save"
                                            }
                                            // Rename button
                                            button {
                                                class: "btn btn-small btn-rename",
                                                title: "Rename profile",
                                                onclick: {
                                                    let pname = profile_name.clone();
                                                    move |_| {
                                                        rename_input.set(pname.clone());
                                                        renaming_profile.set(Some(pname.clone()));
                                                    }
                                                },
                                                i { class: "fa-solid fa-pen" }
                                            }
                                            // Delete button
                                            button {
                                                class: "btn btn-small btn-delete",
                                                title: "Delete profile",
                                                onclick: {
                                                    let pname = profile_name.clone();
                                                    move |_| {
                                                        let pname = pname.clone();
                                                        spawn(async move {
                                                            if let Err(err) = api::delete_profile(&pname).await {
                                                                toast.show(format!("Failed to delete profile: {}", err), ToastSeverity::Normal);
                                                            } else {
                                                                profile_names.set(api::get_profile_names().await);
                                                                active_profile.set(api::get_active_profile().await);
                                                                profile_status.set(format!("Deleted '{}'", pname));
                                                            }
                                                        });
                                                    }
                                                },
                                                i { class: "fa-solid fa-trash" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                // Create new profile
                {
                    let names = profile_names();
                    let suggested_name = {
                        let mut n = names.len() + 1;
                        loop {
                            let candidate = format!("Profile {n}");
                            if !names.iter().any(|existing| existing == &candidate) {
                                break candidate;
                            }
                            n += 1;
                        }
                    };
                    rsx! {
                        div { class: "profile-create",
                            input {
                                r#type: "text",
                                class: "profile-name-input",
                                placeholder: "{suggested_name}",
                                maxlength: "32",
                                value: new_profile_name,
                                oninput: move |e| new_profile_name.set(e.value()),
                                onfocus: {
                                    let suggested = suggested_name.clone();
                                    move |_| {
                                        if new_profile_name().is_empty() {
                                            new_profile_name.set(suggested.clone());
                                        }
                                    }
                                },
                            }
                            button {
                                class: "btn btn-small btn-save",
                                disabled: new_profile_name().trim().is_empty() || profile_names().len() >= MAX_PROFILES,
                                onclick: move |_| {
                                    let name = new_profile_name().trim().to_string();
                                    if name.is_empty() { return; }
                                    spawn(async move {
                                        if let Err(err) = api::save_profile(&name).await {
                                            toast.show(format!("Failed to create profile: {}", err), ToastSeverity::Normal);
                                        } else {
                                            profile_names.set(api::get_profile_names().await);
                                            active_profile.set(Some(name.clone()));
                                            new_profile_name.set(String::new());
                                            profile_dirty.set(false);
                                            profile_status.set(format!("Created '{}'", name));
                                        }
                                    });
                                },
                                "+ New"
                            }
                        }
                    }
                }

                if profile_names().len() >= MAX_PROFILES {
                    p { class: "hint hint-warning compact", "Maximum {MAX_PROFILES} profiles" }
                }
                if !profile_status().is_empty() {
                    p { class: "profile-status", "{profile_status}" }
                }

                // Role default profiles
                if !profile_names().is_empty() {
                    div { class: "role-defaults-section",
                        div { class: "role-defaults-header",
                            "Role Defaults"
                        }
                        p { class: "hint compact", "Auto-switch profile when your role changes" }
                        for (role, label) in [("Tank", "Tank"), ("Healer", "Healer"), ("Dps", "DPS")] {
                            {
                                let current_default = role_defaults().get(role).cloned().unwrap_or_default();
                                let role_key = role.to_string();
                                rsx! {
                                    div { class: "role-default-row",
                                        label { class: "role-default-label", "{label}" }
                                        select {
                                            class: "role-default-select",
                                            value: "{current_default}",
                                            onchange: {
                                                let role_key = role_key.clone();
                                                move |e: Event<FormData>| {
                                                    let selected = e.value();
                                                    let role = role_key.clone();
                                                    spawn(async move {
                                                        let profile = if selected.is_empty() { None } else { Some(selected.as_str()) };
                                                        if let Err(err) = api::set_default_profile_for_role(&role, profile).await {
                                                            toast.show(format!("Failed to set default: {}", err), ToastSeverity::Normal);
                                                        } else {
                                                            role_defaults.set(api::get_default_profiles_per_role().await);
                                                        }
                                                    });
                                                }
                                            },
                                            option { value: "", "None" }
                                            for pname in profile_names().iter() {
                                                option {
                                                    value: "{pname}",
                                                    selected: current_default == *pname,
                                                    "{pname}"
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
        }
    }
}
