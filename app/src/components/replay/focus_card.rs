//! Strip over the top of the map naming the highlighted entity: who it is,
//! its health, target and what it is casting, without looking away to its card.

use dioxus::prelude::*;

use baras_types::{ReplayEntityKind, formatting};

use super::ReplayState;
use super::loaded::Role;
use crate::components::ability_icon::AbilityIcon;
use crate::components::class_icons::get_class_icon;

#[component]
pub(super) fn FocusCard() -> Element {
    let state = use_context::<ReplayState>();
    let (time, highlight, maps) = (state.time, state.highlight, state.maps);
    // Recomputed every tick, re-rendered only when something shown changed
    let focus = use_memo({
        let state = state.clone();
        move || {
            let id = highlight()?;
            let index = state.loaded.data.entities.iter().position(|e| e.id == id)?;
            let entity = &state.loaded.data.entities[index];
            let view = state.loaded.card(
                entity,
                time(),
                Some(id),
                &maps.read(),
                (state.shown_map)(),
                &state.phases,
            );
            Some((index, view))
        }
    });
    let Some((index, view)) = focus() else {
        return rsx! {};
    };
    let entities = &state.loaded.data.entities;
    let entity = &entities[index];
    let icon = |name: Option<&str>| name.and_then(get_class_icon).copied();
    let tag = if entity.is_boss {
        "BOSS"
    } else if entity.kind == ReplayEntityKind::Npc {
        "NPC"
    } else {
        Role::of(entity).label()
    };
    let pct = if view.max_hp > 0 {
        view.hp as f32 * 100.0 / view.max_hp as f32
    } else {
        0.0
    };
    let european = state.european;
    let compact = move |v: f32| formatting::format_compact(v as i64, european);
    let target = view.target.map(|i| &entities[i]);

    rsx! {
        div { class: "replay-focus", class: if view.dead { "dead" },
            div { class: "replay-focus-who",
                if let Some(src) = icon(entity.class_icon.as_deref()) {
                    img { class: "class-icon", src, alt: "" }
                }
                span { class: "replay-focus-name", "{entity.name}" }
                span { class: "replay-card-tag", "{tag}" }
                div { class: "replay-hp replay-focus-hp",
                    div { class: "replay-hp-fill", style: "width: {pct:.1}%" }
                }
                span { class: "replay-focus-value",
                    if view.dead { "Dead" } else { "{pct:.0}%" }
                }
            }
            div { class: "replay-focus-doing",
                if let Some(target) = target {
                    span { class: "replay-focus-item",
                        "on"
                        if let Some(src) = icon(target.class_icon.as_deref()) {
                            img { class: "class-icon", src, alt: "" }
                        }
                        span { class: "replay-focus-name", "{target.name}" }
                        if let Some(metres) = view.target_distance {
                            span { class: "replay-focus-value", "{metres} m" }
                        }
                    }
                }
                if let Some(cast) = &view.cast {
                    span { class: "replay-focus-item",
                        AbilityIcon { ability_id: cast.ability_id, size: 16 }
                        span { class: "replay-focus-name", "{cast.name}" }
                        span { class: "replay-focus-value",
                            if cast.interrupted { "interrupted" } else { "{cast.pct}%" }
                        }
                    }
                } else if let Some((ability_id, name)) = &view.last_ability {
                    span { class: "replay-focus-item replay-cast-last",
                        AbilityIcon { ability_id: *ability_id, size: 16 }
                        span { class: "replay-focus-name", "{name}" }
                    }
                }
                if let Some((dps, hps)) = view.rates.filter(|_| entity.kind == ReplayEntityKind::Player) {
                    span { class: "replay-focus-item replay-focus-value", "DPS {compact(dps)}" }
                    span { class: "replay-focus-item replay-focus-value", "HPS {compact(hps)}" }
                }
            }
        }
    }
}
