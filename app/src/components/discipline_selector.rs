//! Discipline / role multi-select used by the effect editor (and its filters)

use dioxus::prelude::*;

use super::multi_select::{Entry, MultiSelect};

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

/// Disciplines grouped by class (roles first with `include_roles`), as the
/// shared multi-select
#[component]
pub fn DisciplineSelector(
    selected: Vec<String>,
    on_change: EventHandler<Vec<String>>,
    /// Add a "Roles" group (Tank/Healer/DPS) above the discipline groups
    #[props(default)] include_roles: bool,
    /// Label shown when nothing is selected
    #[props(default = "All disciplines")] empty_label: &'static str,
) -> Element {
    let roles = include_roles.then_some(("Roles", ALL_ROLES));
    let entries: Vec<Entry> = roles
        .into_iter()
        .chain(ALL_DISCIPLINES.iter().copied())
        .flat_map(|(group, names)| {
            std::iter::once((group.to_string(), Vec::new()))
                .chain(names.iter().map(|n| (n.to_string(), vec![n.to_string()])))
        })
        .collect();

    rsx! {
        MultiSelect { any_label: empty_label, noun: "selected", entries, picked: selected, on_change }
    }
}
