//! Effects editor grouping, purely visual: a left rail of General, then per
//! base class its shared effects and both advanced classes; the list shows the
//! picked entry flat, its disciplines as dividers. An effect's category comes
//! from its disciplines; nothing about tracking changes.

use std::collections::HashMap;

use dioxus::prelude::*;

use super::class_icons::{ClassGlyph, get_class_icon};
use super::discipline_selector::ALL_DISCIPLINES;
use baras_types::history::discipline_mirror;

/// Base classes; `ALL_DISCIPLINES` lists their two advanced classes back to back
const BASE_CLASSES: [&str; 4] = [
    "Inquisitor / Consular",
    "Warrior / Knight",
    "Bounty Hunter / Trooper",
    "Agent / Smuggler",
];

#[derive(Debug, Clone, Copy, PartialEq)]
enum Category {
    General,
    /// Spans both advanced classes of a base class
    Base(usize),
    /// Several disciplines of one advanced class (`ALL_DISCIPLINES` index)
    Class(usize),
    /// One discipline mirror pair, by its Imperial name
    Discipline(usize, &'static str),
}

fn base_key(b: usize) -> String {
    format!("base:{}", BASE_CLASSES[b])
}
fn class_key(c: usize) -> String {
    format!("class:{}", ALL_DISCIPLINES[c].0)
}
pub const GENERAL_KEY: &str = "general";

/// (advanced class index, Imperial name) of a discipline display name
fn locate(name: &str) -> Option<(usize, &'static str)> {
    let class = ALL_DISCIPLINES.iter().position(|(_, ns)| ns.contains(&name))?;
    Some((class, discipline_mirror(name)?.0))
}

fn categorize(disciplines: &[String]) -> Category {
    let Some(spots) = disciplines.iter().map(|d| locate(d)).collect::<Option<Vec<_>>>() else {
        return Category::General; // roles or unknown names
    };
    let Some(&(c, d)) = spots.first() else {
        return Category::General;
    };
    if spots.iter().all(|s| s.0 == c) {
        if spots.iter().all(|s| s.1 == d) { Category::Discipline(c, d) } else { Category::Class(c) }
    } else if spots.iter().all(|s| s.0 / 2 == c / 2) {
        Category::Base(c / 2)
    } else {
        Category::General
    }
}

/// Rail entry key holding an effect with `disciplines`
pub fn rail_key(disciplines: &[String]) -> String {
    match categorize(disciplines) {
        Category::General => GENERAL_KEY.to_string(),
        Category::Base(b) => base_key(b),
        Category::Class(c) | Category::Discipline(c, _) => class_key(c),
    }
}

/// One rail entry's list: its own items, then one section per discipline pair
pub struct Group<T> {
    pub key: String,
    pub label: String,
    /// Icon files (Imperial then Republic) for class and discipline labels
    pub icons: Vec<String>,
    /// Font Awesome icon for labels without game icons (General, shared)
    pub fa_icon: Option<&'static str>,
    pub items: Vec<T>,
    pub children: Vec<Group<T>>,
}

impl<T> Group<T> {
    pub fn total(&self) -> usize {
        self.count(|_| true)
    }
    /// Items here and in sections matching `f`
    pub fn count(&self, f: impl Fn(&T) -> bool + Copy) -> usize {
        self.items.iter().filter(|i| f(i)).count() + self.children.iter().map(|c| c.count(f)).sum::<usize>()
    }
    fn new(key: String, label: String) -> Self {
        Self { key, label, icons: Vec::new(), fa_icon: None, items: Vec::new(), children: Vec::new() }
    }
}

/// Icon files for a "Imperial / Republic" label ("Kinetic Combat" → kinetic-combat.png)
fn pair_icons(label: &str) -> Vec<String> {
    label.split(" / ").map(|n| format!("{}.png", n.to_lowercase().replace(' ', "-"))).collect()
}

/// `items` by rail entry, in rail order (keeps their order within a group);
/// empty groups and sections are left out
pub fn group<T>(items: Vec<T>, disciplines: impl Fn(&T) -> &[String]) -> Vec<Group<T>> {
    let mut general = Group { fa_icon: Some("fa-globe"), ..Group::new(GENERAL_KEY.to_string(), "General".to_string()) };
    let mut bases: Vec<Group<T>> = (0..BASE_CLASSES.len())
        .map(|b| Group {
            fa_icon: Some("fa-code-branch"),
            ..Group::new(base_key(b), format!("{} (shared)", BASE_CLASSES[b]))
        })
        .collect();
    let mut classes: Vec<Group<T>> = ALL_DISCIPLINES
        .iter()
        .enumerate()
        .map(|(c, (label, names))| {
            let mut g = Group { icons: pair_icons(label), ..Group::new(class_key(c), label.to_string()) };
            g.children = names
                .iter()
                .filter_map(|n| discipline_mirror(n).filter(|(imp, _)| imp == n))
                .map(|(imp, rep)| {
                    let label = format!("{imp} / {rep}");
                    Group { icons: pair_icons(&label), ..Group::new(imp.to_string(), label) }
                })
                .collect();
            g
        })
        .collect();

    for item in items {
        match categorize(disciplines(&item)) {
            Category::General => general.items.push(item),
            Category::Base(b) => bases[b].items.push(item),
            Category::Class(c) => classes[c].items.push(item),
            Category::Discipline(c, d) => {
                if let Some(g) = classes[c].children.iter_mut().find(|g| g.key == d) {
                    g.items.push(item);
                }
            }
        }
    }

    let mut out = vec![general];
    let mut classes = classes.into_iter();
    for base in bases {
        out.push(base);
        out.extend(classes.by_ref().take(2));
    }
    for g in &mut out {
        g.children.retain(|c| c.total() > 0);
    }
    out.retain(|g| g.total() > 0);
    out
}

/// Effects per rail key
pub type RailCounts = HashMap<String, usize>;

pub fn rail_counts<'a>(items: impl Iterator<Item = &'a [String]>) -> RailCounts {
    let mut out = RailCounts::new();
    for disciplines in items {
        *out.entry(rail_key(disciplines)).or_default() += 1;
    }
    out
}

/// Left rail: General, then each base class's shared effects and advanced
/// classes. Advanced classes always show so the rail never shifts; General and
/// shared entries only when they hold effects
#[component]
pub fn CategoryRail(counts: RailCounts, selected: String, on_select: EventHandler<String>) -> Element {
    let entry = |key: String, label: String, icon: Element| {
        let total = counts.get(&key).copied().unwrap_or_default();
        let is_sel = key == selected;
        let pick = key.clone();
        rsx! {
            div {
                key: "{key}",
                class: "sidebar-boss-item effect-rail-item",
                class: if is_sel { "selected" },
                class: if total == 0 { "empty" },
                onclick: move |_| on_select.call(pick.clone()),
                span { class: "effect-rail-icon", {icon} }
                span { class: "truncate", "{label}" }
                span { class: "effect-rail-count", title: "Effects", "{total}" }
            }
        }
    };
    let has = |key: &str| counts.get(key).is_some_and(|c| *c > 0);

    rsx! {
        if has(GENERAL_KEY) {
            {entry(GENERAL_KEY.to_string(), "General".to_string(), rsx! { i { class: "fa-solid fa-globe" } })}
        }
        for b in 0..BASE_CLASSES.len() {
            div { key: "{BASE_CLASSES[b]}", class: "effect-rail-heading", "{BASE_CLASSES[b]}" }
            if has(&base_key(b)) {
                {entry(base_key(b), "Shared".to_string(), rsx! { i { class: "fa-solid fa-code-branch" } })}
            }
            for c in [2 * b, 2 * b + 1] {
                {
                    let label = ALL_DISCIPLINES[c].0;
                    let icons = pair_icons(label);
                    let first = label.split(" / ").next().unwrap_or(label).to_string();
                    entry(class_key(c), label.to_string(), rsx! {
                        ClassGlyph { name: first, icon: icons[0].clone() }
                    })
                }
            }
        }
    }
}

/// Section title above a group's rows (`sub` for discipline dividers)
#[component]
pub fn GroupTitle(
    label: String,
    icons: Vec<String>,
    fa_icon: Option<&'static str>,
    sub: bool,
) -> Element {
    rsx! {
        div { class: if sub { "effect-divider" } else { "effect-group-title" },
            span { class: "effect-category-icons",
                if let Some(fa) = fa_icon {
                    i { class: "fa-solid {fa}" }
                }
                if sub {
                    for asset in icons.iter().filter_map(|i| get_class_icon(i)) {
                        img { src: *asset }
                    }
                } else {
                    for (name, icon) in label.split(" / ").zip(icons.iter()) {
                        ClassGlyph { name: name.to_string(), icon: icon.clone() }
                    }
                }
            }
            span { "{label}" }
        }
    }
}
