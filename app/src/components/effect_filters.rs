//! Effects editor filter bar, pinned above the list in one row: the search,
//! the class picker with its class / general scope, the overlay picker, an
//! origin toggle, hide-disabled, then Clear.
//! Counts apply the search and the other filters, so they show what picking
//! that value would leave.

use dioxus::prelude::*;

use super::def_list::HideDisabled;
use super::discipline_selector::{ALL_DISCIPLINES, class_of};
use super::editor_common::{ORIGINS, Origin, OriginFilter};
use super::effect_editor::effect_origin;
use super::multi_select::{Entry, MultiSelect};
use crate::types::{DisplayTarget, EffectFilters, EffectListItem, EffectScope};

const NO_OVERLAY: &str = "No overlay";
const ALL_CLASSES: &str = "All classes";
/// (scope, label, tooltip) in toggle order
const SCOPES: [(EffectScope, &str, &str); 3] = [
    (
        EffectScope::Both,
        "Class + General",
        "Effects tied to disciplines, plus the general ones",
    ),
    (
        EffectScope::ClassOnly,
        "Class only",
        "Only effects tied to specific disciplines",
    ),
    (
        EffectScope::GeneralOnly,
        "General only",
        "Only general effects: no discipline set, so they are live on every class",
    ),
];

#[derive(Debug, Clone, Copy, PartialEq)]
enum Section {
    Origin,
    Overlay,
    Class,
}

const SECTIONS: [Section; 3] = [Section::Origin, Section::Overlay, Section::Class];

impl Section {
    /// Picked values; empty = no filter
    fn picked(self, f: &EffectFilters) -> &[String] {
        match self {
            Self::Origin => f.origin.as_slice(),
            Self::Overlay => &f.overlays,
            Self::Class => f.class.as_slice(),
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
            // A general effect is live on every class
            Self::Class if e.disciplines.is_empty() => ALL_DISCIPLINES.iter().map(|(class, _)| *class).collect(),
            Self::Class => e.disciplines.iter().filter_map(|d| class_of(d)).collect(),
        }
    }
}

/// `f` with the class settled: no pick is the character's class, "All
/// classes" is no filter
pub fn resolved(f: &EffectFilters, player_class: Option<&'static str>) -> EffectFilters {
    let class = match f.class.as_deref() {
        None => player_class.map(String::from),
        Some(ALL_CLASSES) => None,
        Some(c) => Some(c.to_string()),
    };
    EffectFilters { class, ..f.clone() }
}

/// Whether `e` passes every section's filter except `skip`
fn passes(f: &EffectFilters, e: &EffectListItem, skip: Option<Section>) -> bool {
    let in_scope = match f.scope {
        EffectScope::Both => true,
        EffectScope::ClassOnly => !e.disciplines.is_empty(),
        EffectScope::GeneralOnly => e.disciplines.is_empty(),
    };
    in_scope
        && SECTIONS.iter().filter(|s| Some(**s) != skip).all(|s| {
            let picked = s.picked(f);
            picked.is_empty() || s.values(e).iter().any(|v| picked.iter().any(|p| p == v))
        })
}

/// `f` is a [`resolved`] filter
pub fn matches_filters(f: &EffectFilters, e: &EffectListItem) -> bool {
    passes(f, e, None)
}

/// How many effects in `effects` picking `value` in `section` would leave
fn count(f: &EffectFilters, effects: &[EffectListItem], section: Section, value: &str) -> usize {
    effects
        .iter()
        .filter(|e| passes(f, e, Some(section)) && section.values(e).contains(&value))
        .count()
}

/// `effects` is the list after the search and hide-disabled toggle
#[component]
pub fn EffectFilterBar(
    effects: Memo<Vec<EffectListItem>>,
    filters: Signal<EffectFilters>,
    search: Signal<String>,
    hide_disabled: Signal<bool>,
    /// Disabled effects in the whole list
    disabled_count: usize,
    /// Class of the logged-in character, the class filter's starting point
    player_class: Option<&'static str>,
) -> Element {
    let f = resolved(&filters.read(), player_class);
    let list = effects.read();
    let active =
        SECTIONS.iter().filter(|s| !s.picked(&f).is_empty()).count() + usize::from(f.scope != EffectScope::Both);
    let origin = f.origin.clone();

    let overlays: Vec<Entry> = DisplayTarget::all()
        .iter()
        .chain([&DisplayTarget::EffectsOverlay])
        .filter(|t| **t != DisplayTarget::None)
        .map(|t| t.label())
        .chain([NO_OVERLAY])
        .map(|v| (v, count(&f, &list, Section::Overlay, v)))
        .filter(|(v, n)| *n > 0 || f.overlays.iter().any(|p| p == v))
        .map(|(v, n)| (format!("{v} ({n})"), vec![v.to_string()]))
        .collect();

    let class = f.class.clone();
    let any_class = list.iter().filter(|e| passes(&f, e, Some(Section::Class))).count();
    let classes = ALL_DISCIPLINES.iter().map(|(c, _)| *c);

    rsx! {
        div { class: "history-toolbar effect-filter-bar",
            input {
                class: "search-input",
                r#type: "text",
                placeholder: "Search name or ID...",
                value: "{search}",
                oninput: move |e| search.set(e.value()),
            }
            select {
                class: if class.is_some() { "active" },
                title: "Effects live on this class",
                onchange: move |e| filters.write().class = Some(e.value()),
                option { value: ALL_CLASSES, selected: class.is_none(), "{ALL_CLASSES} ({any_class})" }
                for c in classes {
                    option {
                        value: c,
                        selected: class.as_deref() == Some(c),
                        "{c} ({count(&f, &list, Section::Class, c)})"
                    }
                }
            }
            div { class: "filter-pills", title: "General effects have no discipline set and are live on every class",
                for (scope, label, title) in SCOPES {
                    button {
                        class: "seg",
                        class: if f.scope == scope { "active" },
                        title,
                        onclick: move |_| filters.write().scope = scope,
                        "{label}"
                    }
                }
            }
            MultiSelect {
                any_label: "Any overlay",
                noun: "overlays",
                entries: overlays,
                picked: f.overlays.clone(),
                on_change: move |v: Vec<String>| filters.write().overlays = v,
            }
            OriginFilter {
                selected: ORIGINS.into_iter().find(|o| origin.as_deref() == Some(o.label())),
                counts: ORIGINS.map(|o| count(&f, &list, Section::Origin, o.label())),
                kind: "effects",
                on_pick: move |o: Option<Origin>| filters.write().origin = o.map(|o| o.label().to_string()),
            }
            HideDisabled { hidden: hide_disabled, count: disabled_count, noun: "effects" }
            button {
                class: "btn btn-xs history-clear",
                class: if active > 0 { "active" },
                disabled: active == 0,
                title: "Show every effect: any class, overlay and origin (X)",
                "data-shortcut": "x",
                onclick: move |_| {
                    filters.set(EffectFilters { class: Some(ALL_CLASSES.to_string()), ..Default::default() });
                },
                i { class: "fa-solid fa-filter-circle-xmark" }
                " Clear filters"
                if active > 0 { span { class: "history-more-count", "{active}" } }
            }
        }
    }
}
