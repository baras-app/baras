//! Pickers shared by the trigger, counter, challenge, effect and modifier
//! forms. Given a label each one is a form row (`Field`); with an empty label
//! it is the bare control, for a caller that supplies its own row.

use dioxus::prelude::*;

use crate::components::form_fields::Field;
use crate::types::{AbilitySelector, EffectSelector, EntityFilter, EntitySelector, MitigationType};

/// A form row when labelled, else just the control stack
#[component]
fn Row(
    label: &'static str,
    #[props(default)] help: &'static str,
    /// A list (chips + add control): boxed so it reads as one unit
    #[props(default)]
    boxed: bool,
    children: Element,
) -> Element {
    let stack = rsx! { div { class: if boxed { "field-stack chip-box" } else { "field-stack" }, {children} } };
    if label.is_empty() {
        stack
    } else {
        rsx! { Field { label, help, top: true, {stack} } }
    }
}

#[component]
fn Chips(labels: Vec<String>, on_remove: EventHandler<usize>) -> Element {
    rsx! {
        if !labels.is_empty() {
            div { class: "field-chips",
                for (idx, label) in labels.into_iter().enumerate() {
                    span { class: "chip",
                        "{label}"
                        button { class: "chip-remove", onclick: move |_| on_remove.call(idx), "×" }
                    }
                }
            }
        }
    }
}

/// Picked values as chips over an "ID or Name" box that adds one
#[component]
fn ChipList(
    label: &'static str,
    chips: Vec<String>,
    on_remove: EventHandler<usize>,
    on_add: EventHandler<String>,
) -> Element {
    let mut text = use_signal(String::new);
    let mut add = move || {
        let value = text.peek().trim().to_string();
        if !value.is_empty() {
            on_add.call(value);
            text.set(String::new());
        }
    };

    rsx! {
        Row { label, boxed: true,
            Chips { labels: chips, on_remove }
            div { class: "field-stack-row",
                input {
                    r#type: "text",
                    placeholder: "ID or Name (Enter)",
                    value: "{text}",
                    oninput: move |e| text.set(e.value()),
                    onkeydown: move |e| if e.key() == Key::Enter { add() },
                }
                button { class: "btn btn-sm", onclick: move |_| add(), "Add" }
            }
        }
    }
}

/// `list` without entry `idx`
fn without<T: Clone>(list: &[T], idx: usize) -> Vec<T> {
    let mut next = list.to_vec();
    next.remove(idx);
    next
}

/// `list` with `item` appended, unless it is already in
fn with<T: Clone + PartialEq>(list: &[T], item: T) -> Option<Vec<T>> {
    (!list.contains(&item)).then(|| {
        let mut next = list.to_vec();
        next.push(item);
        next
    })
}

macro_rules! selector_editor {
    ($(#[$doc:meta])* $name:ident, $selector:ty) => {
        $(#[$doc])*
        #[component]
        pub fn $name(
            label: &'static str,
            selectors: Vec<$selector>,
            on_change: EventHandler<Vec<$selector>>,
        ) -> Element {
            let (removing, adding) = (selectors.clone(), selectors.clone());
            rsx! {
                ChipList {
                    label,
                    chips: selectors.iter().map(|s| s.display()).collect::<Vec<_>>(),
                    on_remove: move |idx| on_change.call(without(&removing, idx)),
                    on_add: move |text: String| {
                        if let Some(next) = with(&adding, <$selector>::from_input(&text)) {
                            on_change.call(next);
                        }
                    },
                }
            }
        }
    };
}

selector_editor!(
    /// Effects by ID or name
    EffectSelectorEditor,
    EffectSelector
);
selector_editor!(
    /// Abilities by ID or name
    AbilitySelectorEditor,
    AbilitySelector
);
selector_editor!(
    /// Entities by NPC ID, roster alias or name
    EntitySelectorEditor,
    EntitySelector
);

/// One of the encounter's timers, phases or counters; `available` is (id, label)
#[component]
pub(super) fn IdSelector(
    label: &'static str,
    value: String,
    available: Vec<(String, String)>,
    on_change: EventHandler<String>,
) -> Element {
    let known = available.iter().any(|(id, _)| *id == value);
    rsx! {
        Field { label,
            select {
                value: "{value}",
                onchange: move |e| on_change.call(e.value()),
                if value.is_empty() {
                    option { value: "", selected: true, "(select)" }
                }
                for (id, text) in &available {
                    option { value: "{id}", selected: *id == value, "{text}" }
                }
                // Keep a value that is no longer in the list visible
                if !value.is_empty() && !known {
                    option { value: "{value}", selected: true, "{value} (not found)" }
                }
            }
        }
    }
}

const SPECIFIC: &str = "Specific (ID or Name)";

/// Source/target filter: the standard options, or specific entities
#[component]
pub fn EntityFilterDropdown(
    label: &'static str,
    value: EntityFilter,
    options: &'static [EntityFilter],
    on_change: EventHandler<EntityFilter>,
) -> Element {
    let selectors = if let EntityFilter::Selector(s) = &value {
        Some(s.clone())
    } else {
        None
    };

    let is_specific = selectors.is_some();

    rsx! {
        Row { label,
            select {
                onchange: move |e| {
                    let picked = e.value();
                    if picked == SPECIFIC {
                        on_change.call(EntityFilter::Selector(vec![]));
                    } else if let Some(opt) = options.iter().find(|o| o.label() == picked) {
                        on_change.call(opt.clone());
                    }
                },
                for opt in options.iter() {
                    option { value: "{opt.label()}", selected: *opt == value, "{opt.label()}" }
                }
                option { value: SPECIFIC, selected: is_specific, "{SPECIFIC}" }
            }
            if let Some(selectors) = selectors {
                EntitySelectorEditor {
                    label: "",
                    selectors,
                    on_change: move |sels| on_change.call(EntityFilter::Selector(sels)),
                }
            }
        }
    }
}

/// Boss entities for HP triggers; none picked = any boss
#[component]
pub fn BossSelector(
    selected: Vec<EntitySelector>,
    available_bosses: Vec<String>,
    on_change: EventHandler<Vec<EntitySelector>>,
) -> Element {
    let (removing, adding) = (selected.clone(), selected.clone());
    rsx! {
        Row { label: "Boss", boxed: true,
            Chips {
                labels: selected.iter().map(|s| s.display()).collect::<Vec<_>>(),
                on_remove: move |idx| on_change.call(without(&removing, idx)),
            }
            select {
                onchange: move |e| {
                    let name = e.value();
                    if !name.is_empty()
                        && !adding.iter().any(|s| s.display() == name)
                        && let Some(next) = with(&adding, EntitySelector::Name(name))
                    {
                        on_change.call(next);
                    }
                },
                option { value: "", if selected.is_empty() { "Any boss" } else { "(add boss...)" } }
                for boss in &available_bosses {
                    {
                        let picked = selected.iter().any(|s| s.display() == *boss);
                        rsx! {
                            option { value: "{boss}", disabled: picked,
                                "{boss}"
                                if picked { " ✓" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Mitigation types a `DamageTaken` trigger is limited to; none = any hit result
#[component]
pub fn MitigationTypeEditor(selected: Vec<MitigationType>, on_change: EventHandler<Vec<MitigationType>>) -> Element {
    let (removing, adding) = (selected.clone(), selected.clone());
    rsx! {
        Row { label: "Mitigation", help: "Only fire for these hit results. Leave empty for any.", boxed: true,
            Chips {
                labels: selected.iter().map(|m| m.display_name().to_string()).collect::<Vec<_>>(),
                on_remove: move |idx| on_change.call(without(&removing, idx)),
            }
            select {
                onchange: move |e| {
                    let name = e.value();
                    if let Some(m) = MitigationType::ALL.iter().find(|m| format!("{m:?}") == name)
                        && let Some(next) = with(&adding, *m)
                    {
                        on_change.call(next);
                    }
                },
                option { value: "", selected: true, if selected.is_empty() { "Any" } else { "(add type...)" } }
                for m in MitigationType::ALL.iter().filter(|m| !selected.contains(m)) {
                    option { value: "{m:?}", "{m.display_name()}" }
                }
            }
        }
    }
}
