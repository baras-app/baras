//! Replay tab: the fight played back on a canvas, with a card per player and
//! per enemy showing health, effects, target and cast at the playback time.

mod canvas;
mod card_view;
mod controls;
mod entity_card;
mod loaded;
mod map_tabs;

use std::rc::Rc;

use dioxus::prelude::*;

use baras_types::{MapDefinition, PhaseSegment, ReplayEntityKind};

use crate::api;
use crate::components::class_icons::get_class_icon;
use crate::components::enemy_views::{LoadFailed, loading};
use canvas::{DotMeta, Player};
use controls::Controls;
use entity_card::EntityCard;
use loaded::Loaded;
use map_tabs::MapTabs;

const CANVAS_ID: &str = "replay-canvas";

/// Shared by the canvas, the controls and the cards of one mounted replay
#[derive(Clone)]
struct ReplayState {
    loaded: Rc<Loaded>,
    time: Signal<f32>,
    playing: Signal<bool>,
    highlight: Signal<Option<i64>>,
    player: Signal<Option<Rc<Player>>>,
    maps: Signal<Vec<MapDefinition>>,
    /// Map entry on the canvas right now
    shown_map: Signal<Option<usize>>,
    /// Map entry the user pinned; None follows the raid
    pinned_map: Signal<Option<usize>>,
    phases: Rc<Vec<PhaseSegment>>,
    european: bool,
}

impl ReplayState {
    /// Highlights an entity, or clears the highlight when it already is
    fn toggle_highlight(&self, id: Option<i64>) {
        let mut highlight = self.highlight;
        let next = id.filter(|id| *highlight.peek() != Some(*id));
        highlight.set(next);
    }
}

/// The loaded data as a prop: equal only to itself, so it is never compared deeply
#[derive(Clone)]
struct LoadedProp(Rc<Loaded>);

impl PartialEq for LoadedProp {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

#[component]
pub fn ReplayView(
    encounter_idx: Option<u32>,
    phases: Vec<PhaseSegment>,
    maps: Vec<MapDefinition>,
    european: bool,
) -> Element {
    let mut fight = use_resource(use_reactive!(|encounter_idx| async move {
        api::query_replay(encounter_idx)
            .await
            .map(|data| LoadedProp(Rc::new(Loaded::new(data))))
    }));
    let loaded = fight.read();
    match loaded.as_ref() {
        Some(Some(loaded)) => rsx! {
            ReplayStage { loaded: loaded.clone(), phases, maps, european }
        },
        Some(None) => rsx! { LoadFailed { on_retry: move |_| fight.restart() } },
        None => loading(),
    }
}

#[component]
fn ReplayStage(loaded: LoadedProp, phases: Vec<PhaseSegment>, maps: Vec<MapDefinition>, european: bool) -> Element {
    let state = use_hook(|| ReplayState {
        loaded: loaded.0.clone(),
        time: Signal::new(0.0),
        playing: Signal::new(false),
        highlight: Signal::new(None),
        player: Signal::new(None),
        maps: Signal::new(Vec::new()),
        shown_map: Signal::new(None),
        pinned_map: Signal::new(None),
        phases: Rc::new(phases),
        european,
    });
    use_context_provider(|| state.clone());
    let ReplayState {
        time,
        playing,
        highlight,
        mut player,
        shown_map,
        ..
    } = state.clone();

    // The canvas exists once this has rendered
    let mount_state = state.clone();
    use_effect(move || {
        if player.peek().is_some() {
            return;
        }
        let data = &mount_state.loaded.data;
        let mut meta = DotMeta::default();
        for e in &data.entities {
            if let Some(icon) = e.class_icon.as_deref().and_then(get_class_icon) {
                meta.icons.insert(e.id.to_string(), icon.to_string());
            }
            if let Some(n) = mount_state.loaded.numbers.get(&e.id) {
                meta.numbers.insert(e.id.to_string(), *n);
            }
        }
        let pick_state = mount_state.clone();
        let on_pick = move |id: Option<i64>| {
            pick_state.toggle_highlight(id);
            if let Some(id) = id {
                scroll_to_card(id);
            }
        };
        if let Some(mounted) = Player::mount(CANVAS_ID, data, &meta, time, playing, on_pick, shown_map) {
            player.set(Some(Rc::new(mounted)));
        }
    });
    use_effect(move || {
        let id = highlight();
        if let Some(player) = player.read().as_ref() {
            player.set_highlight(id);
        }
    });
    // Maps arrive after the fight (and change with the boss definition): hand
    // them to the canvas, then each image as it loads
    let mut state_maps = state.maps;
    let map_phases = state.phases.clone();
    use_effect(use_reactive!(|maps| {
        let Some(player) = player.read().clone() else { return };
        state_maps.set(maps.clone());
        let mut urls: Vec<Option<String>> = vec![None; maps.len()];
        player.set_maps(&maps, &urls, &map_phases);
        let phases = map_phases.clone();
        spawn(async move {
            for (i, map) in maps.iter().enumerate() {
                urls[i] = api::get_replay_map_image(&map.image).await;
                if urls[i].is_some() {
                    player.set_maps(&maps, &urls, &phases);
                }
            }
        });
    }));
    use_drop(move || {
        if let Some(player) = player.peek().as_ref() {
            player.destroy();
        }
    });

    let data = &state.loaded.data;
    // NPCs come and go; players are always listed
    let present = use_memo({
        let loaded = state.loaded.clone();
        move || {
            let t = time();
            loaded
                .data
                .entities
                .iter()
                .enumerate()
                .filter(|(_, e)| e.kind == ReplayEntityKind::Npc && loaded.is_present(e, t))
                .map(|(i, _)| i)
                .collect::<Vec<usize>>()
        }
    });
    let players = data
        .entities
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind != ReplayEntityKind::Npc);

    rsx! {
        div { class: "replay-layout",
            div { class: "replay-cards",
                div { class: "replay-cards-title", "Players" }
                for (index, entity) in players {
                    EntityCard { key: "{entity.id}", index }
                }
            }
            div { class: "replay-stage",
                MapTabs {}
                canvas { id: CANVAS_ID, class: "replay-canvas" }
                Controls {}
            }
            div { class: "replay-cards",
                div { class: "replay-cards-title", "Enemies" }
                for index in present() {
                    EntityCard { key: "{data.entities[index].id}", index }
                }
            }
        }
    }
}

fn card_id(entity_id: i64) -> String {
    format!("replay-card-{entity_id}")
}

fn scroll_to_card(entity_id: i64) {
    if let Some(card) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(&card_id(entity_id)))
    {
        card.scroll_into_view_with_bool(false);
    }
}
