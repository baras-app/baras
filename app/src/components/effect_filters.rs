//! Effects editor filter bar, pinned above the list in one row: the name
//! search, an origin toggle, overlay and discipline dropdowns, then Clear. Counts apply the search and the other filters, so they show what
//! picking that value would leave.

use dioxus::prelude::*;

use super::editor_common::{ORIGINS, Origin, OriginFilter};
use super::discipline_selector::{ALL_DISCIPLINES, ALL_ROLES};
use super::effect_editor::effect_origin;
use crate::types::{DisplayTarget, EffectFilters, EffectListItem};

const NO_OVERLAY: &str = "No overlay";
const UNRESTRICTED: &str = "Unrestricted";

#[derive(Debug, Clone, Copy, PartialEq)]
enum Section {
    Origin,
    Overlay,
    Discipline,
}

const SECTIONS: [Section; 3] = [Section::Origin, Section::Overlay, Section::Discipline];

/// (sub-group label, values) in display order
type Groups = Vec<(Option<&'static str>, Vec<String>)>;

impl Section {
    fn selected(self, f: &EffectFilters) -> Option<&str> {
        match self {
            Self::Origin => f.origin.as_deref(),
            Self::Overlay => f.overlay.as_deref(),
            Self::Discipline => f.discipline.as_deref(),
        }
    }

    fn set(self, f: &mut EffectFilters, value: Option<String>) {
        match self {
            Self::Origin => f.origin = value,
            Self::Overlay => f.overlay = value,
            Self::Discipline => f.discipline = value,
        }
    }

    /// The section values one effect falls under
    fn values(self, e: &EffectListItem) -> Vec<&str> {
        match self {
            Self::Origin => vec![effect_origin(false, e.is_bundled, e.is_user_override).label()],
            Self::Overlay => {
                let v: Vec<&str> = e
                    .display_targets
                    .iter()
                    .filter(|t| **t != DisplayTarget::None)
                    .map(|t| t.label())
                    .collect();
                if v.is_empty() { vec![NO_OVERLAY] } else { v }
            }
            Self::Discipline if e.disciplines.is_empty() => vec![UNRESTRICTED],
            Self::Discipline => e.disciplines.iter().map(String::as_str).collect(),
        }
    }

    /// Values present in `effects`, in display order; unknown ones go last
    fn groups(self, effects: &[EffectListItem]) -> Groups {
        let mut present: Vec<&str> = effects.iter().flat_map(|e| self.values(e)).collect();
        present.sort_unstable();
        present.dedup();

        let known: Vec<(Option<&'static str>, Vec<&'static str>)> = match self {
            Self::Origin => vec![(None, ORIGINS.map(|o| o.label()).to_vec())],
            Self::Overlay => {
                let mut v: Vec<&str> = DisplayTarget::all()
                    .iter()
                    .chain([&DisplayTarget::EffectsOverlay])
                    .filter(|t| **t != DisplayTarget::None)
                    .map(|t| t.label())
                    .collect();
                v.push(NO_OVERLAY);
                vec![(None, v)]
            }
            Self::Discipline => {
                let mut g = vec![(None, vec![UNRESTRICTED]), (Some("Roles"), ALL_ROLES.to_vec())];
                g.extend(ALL_DISCIPLINES.iter().map(|(class, d)| (Some(*class), d.to_vec())));
                g
            }
        };

        let extra: Vec<String> = present
            .iter()
            .filter(|p| !known.iter().any(|(_, vs)| vs.contains(p)))
            .map(|p| p.to_string())
            .collect();
        let mut out: Groups = known
            .into_iter()
            .map(|(g, vs)| (g, vs.into_iter().filter(|v| present.contains(v)).map(String::from).collect::<Vec<_>>()))
            .filter(|(_, vs)| !vs.is_empty())
            .collect();
        if !extra.is_empty() {
            out.push((Some("Other"), extra));
        }
        out
    }
}

/// Whether `e` passes every section's filter except `skip`
fn passes(f: &EffectFilters, e: &EffectListItem, skip: Option<Section>) -> bool {
    SECTIONS
        .iter()
        .filter(|s| Some(**s) != skip)
        .all(|s| s.selected(f).is_none_or(|v| s.values(e).contains(&v)))
}

pub fn matches_filters(f: &EffectFilters, e: &EffectListItem) -> bool {
    passes(f, e, None)
}

/// How many effects in `effects` picking `value` in `section` would leave
fn count(f: &EffectFilters, effects: &[EffectListItem], section: Section, value: &str) -> usize {
    effects.iter().filter(|e| passes(f, e, Some(section)) && section.values(e).contains(&value)).count()
}

/// `effects` is the list after the name search and hide-disabled toggle
#[component]
pub fn EffectFilterBar(effects: Memo<Vec<EffectListItem>>, filters: Signal<EffectFilters>, search: Signal<String>) -> Element {
    let f = filters.read().clone();
    let list = effects.read();
    let active = SECTIONS.iter().filter(|s| s.selected(&f).is_some()).count();
    let origin = f.origin.clone();

    rsx! {
        div { class: "history-toolbar effect-filter-bar",
            input {
                class: "search-input",
                r#type: "text",
                placeholder: "Search by name...",
                value: "{search}",
                oninput: move |e| search.set(e.value()),
            }
            OriginFilter {
                selected: ORIGINS.into_iter().find(|o| origin.as_deref() == Some(o.label())),
                counts: ORIGINS.map(|o| count(&f, &list, Section::Origin, o.label())),
                kind: "effects",
                on_pick: move |o: Option<Origin>| filters.write().origin = o.map(|o| o.label().to_string()),
            }
            FilterSelect { section: Section::Overlay, any: "Any overlay", effects, filters }
            FilterSelect { section: Section::Discipline, any: "Any discipline", effects, filters }
            button {
                class: "btn btn-xs history-clear",
                class: if active > 0 { "active" },
                disabled: active == 0,
                title: "Show every effect: any origin, overlay and discipline",
                onclick: move |_| filters.set(EffectFilters::default()),
                i { class: "fa-solid fa-filter-circle-xmark" }
                " Clear filters"
                if active > 0 { span { class: "history-more-count", "{active}" } }
            }
        }
    }
}

/// Dropdown for one section; the empty option is "any", groups become optgroups
#[component]
fn FilterSelect(section: Section, any: &'static str, effects: Memo<Vec<EffectListItem>>, filters: Signal<EffectFilters>) -> Element {
    let f = filters.read().clone();
    let list = effects.read();
    let selected = section.selected(&f).unwrap_or_default().to_string();

    rsx! {
        select {
            class: if !selected.is_empty() { "history-tag-active" },
            value: "{selected}",
            onchange: move |e| {
                let v = e.value();
                section.set(&mut filters.write(), (!v.is_empty()).then_some(v));
            },
            option { value: "", "{any}" }
            for (group, values) in section.groups(&list) {
                match group {
                    Some(g) => rsx! {
                        optgroup { label: "{g}",
                            for v in values {
                                option { value: "{v}", selected: v == selected, "{v} ({count(&f, &list, section, &v)})" }
                            }
                        }
                    },
                    None => rsx! {
                        for v in values {
                            option { value: "{v}", selected: v == selected, "{v} ({count(&f, &list, section, &v)})" }
                        }
                    },
                }
            }
        }
    }
}
