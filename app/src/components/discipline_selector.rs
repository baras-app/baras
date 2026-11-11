//! Discipline / role multi-select used by the effect editor (and its filters)

use dioxus::prelude::*;

// All discipline display names grouped by class (Imp / Rep mirror pairs)
pub(crate) const ALL_DISCIPLINES: &[(&str, &[&str])] = &[
    ("Sorcerer / Sage", &["Lightning", "Madness", "Corruption", "Telekinetics", "Balance", "Seer"]),
    ("Assassin / Shadow", &["Hatred", "Darkness", "Deception", "Infiltration", "Kinetic Combat", "Serenity"]),
    ("Juggernaut / Guardian", &["Vengeance", "Immortal", "Rage", "Focus", "Vigilance", "Defense"]),
    ("Marauder / Sentinel", &["Annihilation", "Carnage", "Fury", "Combat", "Watchman", "Concentration"]),
    ("Mercenary / Commando", &["Arsenal", "Innovative Ordnance", "Bodyguard", "Gunnery", "Assault Specialist", "Combat Medic"]),
    ("Powertech / Vanguard", &["Shield Tech", "Pyrotech", "Advanced Prototype", "Plasmatech", "Shield Specialist", "Tactics"]),
    ("Operative / Scoundrel", &["Concealment", "Lethality", "Medicine", "Scrapper", "Ruffian", "Sawbones"]),
    ("Sniper / Gunslinger", &["Marksmanship", "Engineering", "Virulence", "Sharpshooter", "Saboteur", "Dirty Fighting"]),
];

/// Role display names selectable when `include_roles` is set
pub(crate) const ALL_ROLES: &[&str] = &["Tank", "Healer", "DPS"];

#[component]
pub fn DisciplineSelector(
    selected: Vec<String>,
    on_change: EventHandler<Vec<String>>,
    /// Show a "Roles" group (Tank/Healer/DPS) above the discipline groups
    #[props(default)] include_roles: bool,
    /// Label shown when nothing is selected
    #[props(default = "(all disciplines)".to_string())] empty_label: String,
) -> Element {
    let mut dropdown_open = use_signal(|| false);
    let mut dropdown_pos = use_signal(|| (0.0f64, 0.0f64));

    let display = if selected.is_empty() {
        empty_label.clone()
    } else if selected.len() == 1 {
        selected[0].clone()
    } else {
        format!("{} selected", selected.len())
    };

    let groups: Vec<(&'static str, &'static [&'static str])> = if include_roles {
        std::iter::once(("Roles", ALL_ROLES))
            .chain(ALL_DISCIPLINES.iter().copied())
            .collect()
    } else {
        ALL_DISCIPLINES.to_vec()
    };

    rsx! {
        div {
            class: "discipline-selector",
            button {
                class: "select-trigger w-md",
                onclick: move |e| {
                    if !dropdown_open() {
                        let click = e.client_coordinates();
                        let offset = e.element_coordinates();
                        let btn_left = click.x - offset.x;
                        let btn_bottom = click.y - offset.y + 30.0;
                        dropdown_pos.set((btn_left, btn_bottom));
                    }
                    dropdown_open.set(!dropdown_open());
                },
                "{display}"
            }

            if dropdown_open() {
                // Invisible backdrop to catch clicks outside the dropdown
                div {
                    class: "floating-menu-backdrop",
                    onclick: move |_| dropdown_open.set(false),
                }
                div {
                    class: "floating-menu wide",
                    style: "left: {dropdown_pos().0}px; top: {dropdown_pos().1}px;",

                    // "All" option (clears selection)
                    label { class: "flex items-center gap-xs text-sm p-xs cursor-pointer",
                        input {
                            r#type: "checkbox",
                            checked: selected.is_empty(),
                            onchange: move |_| {
                                on_change.call(vec![]);
                                dropdown_open.set(false);
                            }
                        }
                        "{empty_label}"
                    }

                    // Grouped roles/disciplines
                    for (group_name, disciplines) in groups.iter() {
                        {
                            let selected_clone = selected.clone();
                            rsx! {
                                div { class: "text-xs text-muted",
                                    style: "padding: 6px 4px 2px 4px; font-weight: 600; border-top: 1px solid var(--border-subtle); margin-top: 2px;",
                                    "{group_name}"
                                }
                                for disc in disciplines.iter() {
                                    {
                                        let disc_name = disc.to_string();
                                        let is_selected = selected_clone.contains(&disc_name);
                                        let selected_for_change = selected_clone.clone();

                                        rsx! {
                                            label { class: "flex items-center gap-xs text-sm p-xs cursor-pointer",
                                                input {
                                                    r#type: "checkbox",
                                                    checked: is_selected,
                                                    onchange: move |_| {
                                                        let mut new_selected = selected_for_change.clone();
                                                        if is_selected {
                                                            new_selected.retain(|d| d != &disc_name);
                                                        } else {
                                                            new_selected.push(disc_name.clone());
                                                        }
                                                        on_change.call(new_selected);
                                                    }
                                                }
                                                "{disc}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    button {
                        class: "btn btn-sm w-full mt-xs",
                        onclick: move |_| dropdown_open.set(false),
                        "Done"
                    }
                }
            }
        }
    }
}
