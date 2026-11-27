//! One row of the effects list (name, discipline, source → target, overlays,
//! toggles) and its columns

use dioxus::prelude::*;
use manganis::Asset;

use super::discipline_selector::ALL_DISCIPLINES;
use super::effect_editor::effect_origin;
use super::effect_form::EffectEditForm;
use super::effect_trigger::{effect_icon_id, get_trigger_filters};
use crate::components::ability_icon::ColorIcon;
use crate::components::class_icons::get_class_icon;
use crate::components::def_list::{Column, NoteMark};
use crate::components::editor_common::OriginBadge;
use crate::types::{DisplayTarget, EffectListItem, EntityFilter};

/// Most discipline icons a row shows before "+n"
const MAX_ICONS: usize = 4;

/// Icon of a discipline or advanced class, by display name
fn icon_of(name: &str) -> Option<Asset> {
    get_class_icon(&format!("{}.png", name.to_lowercase().replace(' ', "-"))).copied()
}

/// Disciplines as the row shows them: (icon names, text, how many more).
/// "All" when none is set, a class when all of its specs are listed, a lone
/// discipline by name, else icons only with the names in the tooltip
fn discipline_cell(disciplines: &[String]) -> (Vec<&str>, Option<String>, usize) {
    if disciplines.is_empty() {
        return (vec![], Some("All".to_string()), 0);
    }
    let has =
        |names: &[&str]| names.len() == disciplines.len() && names.iter().all(|n| disciplines.iter().any(|d| d == n));
    for (class, names) in ALL_DISCIPLINES {
        // Each class lists its Imperial specs, then the Republic mirrors
        let (imp, rep) = names.split_at(names.len() / 2);
        let (imp_class, rep_class) = class.split_once(" / ").unwrap_or((class, class));
        if has(names) {
            return (vec![imp_class, rep_class], Some(class.to_string()), 0);
        } else if has(imp) {
            return (vec![imp_class], Some(imp_class.to_string()), 0);
        } else if has(rep) {
            return (vec![rep_class], Some(rep_class.to_string()), 0);
        }
    }
    let icons: Vec<&str> = disciplines.iter().take(MAX_ICONS).map(String::as_str).collect();
    let label = (disciplines.len() == 1).then(|| disciplines[0].clone());
    (icons, label, disciplines.len().saturating_sub(MAX_ICONS))
}

/// Short name of a trigger's source or target for the row
fn party(filter: &EntityFilter) -> &'static str {
    match filter {
        EntityFilter::LocalPlayer => "You",
        EntityFilter::OtherPlayers => "Others",
        EntityFilter::AnyPlayer | EntityFilter::AnyPlayerOrCompanion => "Players",
        EntityFilter::AnyFriendlyPlayer => "Allies",
        EntityFilter::AnyEnemyPlayer => "Enemy players",
        EntityFilter::AnyCompanion => "Companions",
        EntityFilter::AnyExceptLocal => "Not you",
        EntityFilter::CurrentTarget => "Target",
        EntityFilter::Boss => "Boss",
        EntityFilter::NpcExceptBoss => "Adds",
        EntityFilter::AnyNpc => "NPCs",
        EntityFilter::Selector(_) => "Specific",
        EntityFilter::Any => "Anyone",
    }
}

/// Column names, in the order of the rows' grid
pub(crate) const COLUMNS: [Column; 7] = [
    Column::BLANK,
    Column::new("Name", "", "Sort by name"),
    Column::new(
        "Discipline",
        "col-discipline col-drop-2",
        "Your disciplines the effect is live on",
    ),
    Column::new(
        "Source → Target",
        "col-dim col-drop-1",
        "Who must cast or apply it, and who it lands on",
    ),
    Column::new("Overlays", "", "Overlays the effect is shown on"),
    Column::BLANK,
    Column::BLANK,
];

/// What `e` sorts by in column `col` of [`COLUMNS`]
pub(crate) fn sort_text(col: usize, e: &EffectListItem) -> String {
    match col {
        2 => e.disciplines.join(","),
        3 => {
            let (source, target) = get_trigger_filters(&e.trigger);
            format!("{} {}", party(&source), party(&target))
        }
        4 => e
            .display_targets
            .iter()
            .map(|t| t.label())
            .collect::<Vec<_>>()
            .join(","),
        _ => e.name.clone(),
    }
}

#[component]
pub(crate) fn EffectRow(
    effect: EffectListItem,
    expanded: bool,
    #[props(default = false)] is_draft: bool,
    on_toggle: EventHandler<()>,
    on_save: EventHandler<EffectListItem>,
    on_delete: EventHandler<()>,
    on_duplicate: EventHandler<()>,
    #[props(default)] on_export: EventHandler<()>,
) -> Element {
    let mut is_dirty = use_signal(|| false);
    let color = effect.color.unwrap_or([128, 128, 128, 255]);
    let color_hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);
    let (source, target) = get_trigger_filters(&effect.trigger);
    let (icons, label, more) = discipline_cell(&effect.disciplines);
    let overlays: Vec<&'static str> = effect
        .display_targets
        .iter()
        .filter(|t| **t != DisplayTarget::None)
        .map(|t| t.label())
        .collect();

    let effect_for_enable = effect.clone();
    let effect_for_audio = effect.clone();

    rsx! {
        div {
            id: "effect-{effect.id}",
            class: "list-item",
            class: if expanded { "expanded" },
            class: if !effect.enabled { "item-disabled" },
            div {
                class: "list-item-header def-row effect-cols",
                onclick: move |_| on_toggle.call(()),

                ColorIcon {
                    icon_id: effect.show_icon.then(|| effect_icon_id(effect.icon_ability_id, &effect.trigger)).flatten(),
                    color: color_hex.clone(),
                }

                span { class: "col-name", title: if is_draft { "" } else { "{effect.id}" },
                    span { class: "effect-name def-name truncate", "{effect.name}" }
                    NoteMark { note: effect.note.clone() }
                    if expanded && is_dirty() {
                        span { class: "unsaved-indicator", title: "Unsaved changes" }
                    }
                }

                span {
                    class: "col-discipline col-drop-2",
                    class: if effect.disciplines.is_empty() { "text-muted" },
                    title: if effect.disciplines.is_empty() { "Live on every discipline".to_string() } else { effect.disciplines.join(", ") },
                    for name in icons {
                        if let Some(src) = icon_of(name) {
                            img { class: "discipline-icon", src, alt: "{name}" }
                        }
                    }
                    if let Some(label) = label {
                        span { class: "truncate", "{label}" }
                    }
                    if more > 0 {
                        span { class: "text-muted", "+{more}" }
                    }
                }

                span { class: "col-dim col-drop-1 truncate", title: "Source: {source.label()} · Target: {target.label()}",
                    "{party(&source)} → {party(&target)}"
                }

                span { class: "col-tags",
                    if effect.is_alert {
                        span { class: "tag tag-alert", "Alert" }
                    } else if overlays.is_empty() {
                        span { class: "text-muted", title: "Not shown on any overlay", "—" }
                    } else {
                        for o in overlays {
                            span { class: "tag", "{o}" }
                        }
                    }
                }

                OriginBadge { origin: effect_origin(is_draft, effect.is_bundled, effect.is_user_override), kind: "effect" }

                // Toggles save straight away, without opening the effect; a draft
                // has nothing saved to toggle (the cell stays for the grid)
                div { class: "def-row-toggles",
                    if !is_draft {
                        span {
                            class: "row-toggle",
                            class: if !effect.audio.enabled { "row-toggle-quiet" },
                            title: if effect.audio.enabled { "Disable audio" } else { "Enable audio" },
                            onclick: move |e| {
                                e.stop_propagation();
                                let mut updated = effect_for_audio.clone();
                                updated.audio.enabled = !updated.audio.enabled;
                                on_save.call(updated);
                            },
                            i { class: if effect.audio.enabled { "fa-solid fa-volume-high text-primary" } else { "fa-solid fa-volume-xmark text-muted" } }
                        }
                        span {
                            class: "row-toggle",
                            title: if effect.enabled { "Disable effect" } else { "Enable effect" },
                            onclick: move |e| {
                                e.stop_propagation();
                                let mut updated = effect_for_enable.clone();
                                updated.enabled = !updated.enabled;
                                on_save.call(updated);
                            },
                            i { class: if effect.enabled { "fa-solid fa-power-off text-success" } else { "fa-solid fa-power-off text-muted" } }
                        }
                    }
                }
            }

            if expanded {
                EffectEditForm {
                    effect: effect.clone(),
                    is_draft,
                    is_bundled: effect.is_bundled,
                    is_user_override: effect.is_user_override,
                    on_save,
                    on_delete,
                    on_duplicate,
                    on_export,
                    on_dirty: move |dirty: bool| is_dirty.set(dirty),
                    on_close: on_toggle,
                }
            }
        }
    }
}
