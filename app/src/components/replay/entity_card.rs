//! One entity's card beside the replay canvas

use dioxus::prelude::*;

use baras_types::{PvpFaction, ReplayEntityKind, formatting};

use super::card_view::IconView;
use super::loaded::Role;
use super::{ReplayState, card_id};
use crate::components::ability_icon::AbilityIcon;
use crate::components::class_icons::{get_class_icon, get_role_icon};

fn icon_row(title: &'static str, icons: &[IconView], more: usize) -> Element {
    rsx! {
        div { class: "replay-effects",
            span { class: "replay-effects-title", "{title}" }
            div { class: "replay-effects-icons",
                for icon in icons {
                    span {
                        key: "{icon.effect_id}",
                        class: "replay-effect",
                        class: if icon.from_highlight { "from-highlight" },
                        title: "{icon.name}",
                        AbilityIcon {
                            ability_id: icon.effect_id,
                            fallback_id: Some(icon.ability_id),
                            fallback: icon.name.clone(),
                            size: 18,
                        }
                        if icon.stacks > 1 {
                            span { class: "replay-effect-stacks", "{icon.stacks}" }
                        }
                    }
                }
                if more > 0 {
                    span { class: "replay-effects-more", "+{more}" }
                }
            }
        }
    }
}

#[component]
pub(super) fn EntityCard(index: usize) -> Element {
    let state = use_context::<ReplayState>();
    let entity = &state.loaded.data.entities[index];
    let id = entity.id;
    let (time, highlight, maps) = (state.time, state.highlight, state.maps);
    // Recomputed every tick, re-rendered only when something on the card changed
    let view = use_memo({
        let state = state.clone();
        move || {
            let entity = &state.loaded.data.entities[index];
            state.loaded.card(
                entity,
                time(),
                highlight(),
                &maps.read(),
                (state.shown_map)(),
                &state.phases,
            )
        }
    });
    let view = view();
    let selected = highlight() == Some(id);
    let is_npc = entity.kind == ReplayEntityKind::Npc;
    let tag = if entity.is_boss {
        "BOSS"
    } else if is_npc {
        "NPC"
    } else if entity.kind == ReplayEntityKind::Companion {
        "COMPANION"
    } else {
        Role::of(entity).label()
    };
    let class_icon = entity.class_icon.as_deref().and_then(get_class_icon).copied();
    let role_icon = entity.role_icon.as_deref().and_then(get_role_icon).copied();
    let pct = if view.max_hp > 0 {
        view.hp as f32 * 100.0 / view.max_hp as f32
    } else {
        0.0
    };
    // On a map other than the one shown: not on the canvas
    let elsewhere = view.off_map;
    let elsewhere_label = view
        .map
        .filter(|_| elsewhere)
        .and_then(|on| maps.read().get(on).map(map_label));
    // Compact: health, target and cast only; the highlighted card stays full
    let is_compact = (state.compact)() && !selected;
    let hp_text = if view.dead {
        "Dead".to_string()
    } else if view.max_hp > 0 {
        format!(
            "{} / {} ({pct:.0}%)",
            formatting::format_thousands(view.hp),
            formatting::format_thousands(view.max_hp)
        )
    } else {
        "HP unavailable".to_string()
    };
    let on_recap = state.on_recap;
    let name = entity.name.clone();
    let european = state.european;
    let compact = move |v: f32| formatting::format_compact(v as i64, european);
    let toggle = {
        let state = state.clone();
        move |_| state.toggle_highlight(Some(id))
    };

    rsx! {
        div {
            id: card_id(id),
            class: "replay-card",
            class: if selected { "selected" },
            class: if view.dead { "dead" },
            class: if elsewhere { "elsewhere" },
            class: if entity.faction == Some(PvpFaction::Friendly) { "team-friendly" },
            class: if entity.faction == Some(PvpFaction::Enemy) { "team-enemy" },
            onclick: toggle,
            div { class: "replay-card-head",
                if let Some(src) = class_icon {
                    img { class: "class-icon", src, alt: "" }
                }
                span { class: "replay-card-name", title: "{entity.name}", "{entity.name}" }
                if let Some(n) = state.loaded.numbers.get(&id) {
                    span { class: "replay-card-number", "{n}" }
                }
                if let Some(src) = role_icon {
                    img { class: "class-icon", src, alt: "" }
                }
                span { class: "replay-card-tag", "{tag}" }
            }
            div { class: "replay-hp", title: "{hp_text}",
                div { class: "replay-hp-fill", style: "width: {pct:.1}%" }
            }
            if let Some(at) = view.died_at.filter(|_| entity.kind == ReplayEntityKind::Player) {
                div { class: "replay-card-line",
                    button {
                        class: "btn btn-xs",
                        title: "What killed them: open the death recap",
                        onclick: move |e| {
                            e.stop_propagation();
                            on_recap.call((name.clone(), at));
                        },
                        i { class: "fa-solid fa-skull" }
                        " Recap"
                    }
                }
            } else if !is_compact {
                div { class: "replay-card-line", "{hp_text}" }
            }
            if !is_compact {
                {icon_row("Buffs", &view.buffs, view.more_buffs)}
                {icon_row("Debuffs", &view.debuffs, view.more_debuffs)}
            }
            div { class: "replay-card-line",
                if let Some(label) = elsewhere_label {
                    "On {label}"
                } else if elsewhere {
                    "Off the map"
                } else if let Some(target) = view.target.map(|i| &state.loaded.data.entities[i]) {
                    span { class: "replay-card-target",
                        "Target:"
                        // The target's discipline icon, else its role
                        if let Some(src) = target
                            .class_icon
                            .as_deref()
                            .and_then(get_class_icon)
                            .or_else(|| target.role_icon.as_deref().and_then(get_role_icon))
                            .copied()
                        {
                            img { class: "class-icon", src, alt: "" }
                        }
                        span { class: "replay-card-target-name", "{target.name}" }
                        if let Some(metres) = view.target_distance {
                            span { class: "replay-card-distance", "{metres} m" }
                        }
                    }
                } else {
                    "No target"
                }
            }
            div {
                class: "replay-cast",
                class: if is_compact { "compact" },
                class: if view.cast.as_ref().is_some_and(|c| c.interrupted) { "interrupted" },
                if let Some(cast) = &view.cast {
                    // Compact cards name the cast without a progress bar
                    if !is_compact {
                        div { class: "replay-cast-fill", style: "width: {cast.pct}%" }
                    }
                    span { class: "replay-cast-name",
                        AbilityIcon { ability_id: cast.ability_id, size: 14 }
                        if cast.interrupted { "{cast.name} (interrupted)" } else { "{cast.name}" }
                    }
                } else if let Some((ability_id, name)) = &view.last_ability {
                    // Nothing in progress: the last ability used, with no bar
                    span { class: "replay-cast-name replay-cast-last",
                        AbilityIcon { ability_id: *ability_id, size: 14 }
                        "{name}"
                    }
                }
            }
            if let Some((dps, hps)) = view.rates && !is_npc {
                div { class: "replay-card-line replay-card-rates",
                    span { "DPS {compact(dps)}" }
                    span { "HPS {compact(hps)}" }
                }
            }
        }
    }
}

/// A map entry's name on tabs and cards: its label, else its image file without the extension
pub(super) fn map_label(map: &baras_types::MapDefinition) -> String {
    map.label.clone().unwrap_or_else(|| {
        map.image
            .rsplit_once('.')
            .map_or(map.image.as_str(), |(stem, _)| stem)
            .to_string()
    })
}
