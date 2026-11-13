//! Effects editor filter bar, pinned above the list in one row: the name
//! search, an origin toggle, overlay and discipline multi-selects, then Clear. Counts apply the search and the other filters, so they show what
//! picking that value would leave.

use dioxus::prelude::*;

use super::editor_common::{ORIGINS, Origin, OriginFilter};
use super::discipline_selector::{ALL_DISCIPLINES, ALL_ROLES};
use super::effect_editor::effect_origin;
use super::multi_select::{Entry, MultiSelect};
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
    /// Picked values; empty = no filter
    fn picked(self, f: &EffectFilters) -> &[String] {
        match self {
            Self::Origin => f.origin.as_slice(),
            Self::Overlay => &f.overlays,
            Self::Discipline => &f.disciplines,
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
        .all(|s| {
            let picked = s.picked(f);
            picked.is_empty() || s.values(e).iter().any(|v| picked.iter().any(|p| p == v))
        })
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
    let active = SECTIONS.iter().filter(|s| !s.picked(&f).is_empty()).count();
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
            FilterSelect { section: Section::Overlay, any: "Any overlay", noun: "overlays", effects, filters }
            FilterSelect { section: Section::Discipline, any: "Any discipline", noun: "disciplines", effects, filters }
            button {
                class: "btn btn-xs history-clear",
                class: if active > 0 { "active" },
                disabled: active == 0,
                title: "Show every effect: any origin, overlay and discipline (X)",
                "data-shortcut": "x",
                onclick: move |_| filters.set(EffectFilters::default()),
                i { class: "fa-solid fa-filter-circle-xmark" }
                " Clear filters"
                if active > 0 { span { class: "history-more-count", "{active}" } }
            }
        }
    }
}

/// Multi-select for one section: each value with the count picking it would
/// leave, under its group heading
#[component]
fn FilterSelect(
    section: Section,
    any: &'static str,
    noun: &'static str,
    effects: Memo<Vec<EffectListItem>>,
    filters: Signal<EffectFilters>,
) -> Element {
    let f = filters.read().clone();
    let list = effects.read();
    let mut entries: Vec<Entry> = Vec::new();
    for (group, values) in section.groups(&list) {
        if let Some(g) = group {
            entries.push((g.to_string(), Vec::new()));
        }
        entries.extend(values.into_iter().map(|v| (format!("{v} ({})", count(&f, &list, section, &v)), vec![v])));
    }

    rsx! {
        MultiSelect {
            any_label: any,
            noun,
            entries,
            picked: section.picked(&f).to_vec(),
            on_change: move |v: Vec<String>| {
                let mut w = filters.write();
                match section {
                    Section::Overlay => w.overlays = v,
                    Section::Discipline => w.disciplines = v,
                    Section::Origin => w.origin = v.into_iter().next(),
                }
            },
        }
    }
}
