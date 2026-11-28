//! Replay tab: the fight played back on a canvas, with a card per player and
//! per enemy showing health, effects, target and cast at the playback time.

mod canvas;
mod card_view;
mod controls;
mod entity_card;
mod focus_card;
mod loaded;
mod map_prompt;
mod map_tabs;
mod timeline;

use std::rc::Rc;

use dioxus::prelude::*;

use baras_types::{MapDefinition, PhaseSegment, PvpFaction, ReplayEntityKind};

use crate::api;
use crate::components::class_icons::get_class_icon;
use crate::components::enemy_views::{LoadFailed, loading};
use canvas::{DotMeta, Player};
use controls::Controls;
use entity_card::EntityCard;
use focus_card::FocusCard;
use loaded::{Loaded, Role};
pub use map_prompt::MapPrompt;
use map_tabs::MapTabs;

const CANVAS_ID: &str = "replay-canvas";
/// Wait before loading a newly selected encounter's fight
const LOAD_DEBOUNCE_MS: u32 = 150;
/// A jump to a death lands this long before it
const DEATH_LEAD_SECS: f32 = 5.0;

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
    /// Zoom, set by the slider or the mouse wheel
    scale: Signal<f64>,
    /// Cards show only health, target and cast, except the highlighted one
    compact: Signal<bool>,
    /// Whether NPCs with no name (invisible mechanic helpers) are shown
    show_unnamed: Signal<bool>,
    /// The explorer's selected time range, played on a loop; None plays the whole fight
    range: Signal<Option<(f32, f32)>>,
    /// Opens the death recap of a player's death at a time
    on_recap: EventHandler<(String, f32)>,
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
    /// A player and a moment to open on (from a death recap)
    jump: Option<(String, f32)>,
    /// The explorer's selected time range, when it is not the whole fight
    range: Option<(f32, f32)>,
    on_recap: EventHandler<(String, f32)>,
) -> Element {
    let mut fight = use_resource(use_reactive!(|encounter_idx| async move {
        // Stepping through encounters remounts this view each time: only the
        // one that is stayed on asks for its fight
        gloo_timers::future::TimeoutFuture::new(LOAD_DEBOUNCE_MS).await;
        api::query_replay(encounter_idx)
            .await
            .map(|data| LoadedProp(Rc::new(Loaded::new(data))))
    }));
    let loaded = fight.read();
    match loaded.as_ref() {
        Some(Some(loaded)) => rsx! {
            ReplayStage { loaded: loaded.clone(), phases, maps, european, jump, range, on_recap }
        },
        Some(None) => rsx! { LoadFailed { on_retry: move |_| fight.restart() } },
        None => loading(),
    }
}

#[component]
fn ReplayStage(
    loaded: LoadedProp,
    phases: Vec<PhaseSegment>,
    maps: Vec<MapDefinition>,
    european: bool,
    jump: Option<(String, f32)>,
    range: Option<(f32, f32)>,
    on_recap: EventHandler<(String, f32)>,
) -> Element {
    let state = use_hook(|| ReplayState {
        loaded: loaded.0.clone(),
        time: Signal::new(0.0),
        playing: Signal::new(false),
        highlight: Signal::new(None),
        player: Signal::new(None),
        maps: Signal::new(Vec::new()),
        shown_map: Signal::new(None),
        pinned_map: Signal::new(None),
        scale: Signal::new(1.0),
        compact: Signal::new(false),
        show_unnamed: Signal::new(false),
        range: Signal::new(None),
        on_recap,
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
            if let Some(role) = Role::of(e).key() {
                meta.roles.insert(e.id.to_string(), role);
            }
        }
        let pick_state = mount_state.clone();
        let on_pick = move |id: Option<i64>| {
            pick_state.toggle_highlight(id);
            if let Some(id) = id {
                scroll_to_card(id);
            }
        };
        if let Some(mounted) = Player::mount(
            CANVAS_ID,
            data,
            &meta,
            time,
            playing,
            on_pick,
            shown_map,
            mount_state.scale,
        ) {
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
    let map_loaded = state.loaded.clone();
    use_effect(use_reactive!(|maps| {
        let Some(player) = player.read().clone() else { return };
        // An area lists every room of the instance: keep the ones this fight used
        let maps = map_loaded.maps_in_use(&maps);
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
    let show_unnamed = state.show_unnamed;
    use_effect(move || {
        let on = show_unnamed();
        if let Some(player) = player.read().as_ref() {
            player.set_show_unnamed(on);
        }
    });
    let mut state_range = state.range;
    use_effect(use_reactive!(|range| {
        state_range.set(range);
        if let Some(player) = player.read().as_ref() {
            player.set_loop(range);
        }
    }));
    // Opened from a death recap: land shortly before the death with the player selected
    let jump_loaded = state.loaded.clone();
    let mut jump_highlight = state.highlight;
    use_effect(use_reactive!(|jump| {
        let Some(player) = player.read().clone() else { return };
        if let Some((name, at)) = jump {
            player.pause();
            player.seek((at - DEATH_LEAD_SECS).max(0.0));
            if let Some(e) = jump_loaded.data.entities.iter().find(|e| e.name == name) {
                jump_highlight.set(Some(e.id));
                scroll_to_card(e.id);
            }
        }
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
            let unnamed = show_unnamed();
            loaded
                .data
                .entities
                .iter()
                .enumerate()
                .filter(|(_, e)| e.kind == ReplayEntityKind::Npc && loaded.is_present(e, t))
                .filter(|(_, e)| unnamed || !e.name.is_empty())
                .map(|(i, _)| i)
                .collect::<Vec<usize>>()
        }
    });
    // Tanks, healers, damage, then companions; by name within each (the data's order)
    let mut players: Vec<_> = data
        .entities
        .iter()
        .enumerate()
        .filter(|(_, e)| e.kind != ReplayEntityKind::Npc)
        .collect();
    players.sort_by_key(|(_, e)| (e.kind != ReplayEntityKind::Player, Role::of(e)));
    // A PvP match puts the enemy team in the enemies column, above its NPCs;
    // players whose side is not known yet stay on the left, after the friends
    let is_pvp = players.iter().any(|(_, e)| e.faction.is_some());
    let (enemy_players, mut players): (Vec<_>, Vec<_>) = players
        .into_iter()
        .partition(|(_, e)| e.faction == Some(PvpFaction::Enemy));
    players.sort_by_key(|(_, e)| is_pvp && e.faction.is_none());
    let players_title = if is_pvp { "Friends" } else { "Players" };

    rsx! {
        // Shown instead of the stage when the tab is too narrow for it (CSS)
        div { class: "replay-too-narrow uptime-empty", "Widen the window to watch the replay" }
        div { class: "replay-layout fade-in",
            div { class: "replay-cards",
                div { class: "replay-cards-title", "{players_title}" }
                for (index, entity) in players {
                    EntityCard { key: "{entity.id}", index }
                }
            }
            div { class: "replay-stage",
                MapTabs {}
                div { class: "replay-canvas-wrap",
                    canvas { id: CANVAS_ID, class: "replay-canvas" }
                    FocusCard {}
                }
                Controls {}
            }
            div { class: "replay-cards",
                div { class: "replay-cards-title", "Enemies" }
                for (index, entity) in enemy_players {
                    EntityCard { key: "{entity.id}", index }
                }
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
