//! Map switcher above the replay canvas, for bosses whose fight spans several maps

use dioxus::prelude::*;

use super::ReplayState;
use super::entity_card::map_label;

#[component]
pub(super) fn MapTabs() -> Element {
    let ReplayState {
        maps,
        shown_map,
        mut pinned_map,
        player,
        ..
    } = use_context::<ReplayState>();
    let maps = maps.read();
    if maps.len() < 2 {
        return rsx! {};
    }
    let mut pin = move |index: Option<usize>| {
        pinned_map.set(index);
        if let Some(player) = player.read().as_ref() {
            player.set_pinned_map(index);
        }
    };
    let following = pinned_map().is_none();

    rsx! {
        div { class: "data-tab-selector replay-map-tabs",
            button {
                class: if following { "data-tab active" } else { "data-tab" },
                title: "Follow the raid from map to map",
                onclick: move |_| pin(None),
                "Auto"
            }
            for (index, map) in maps.iter().enumerate() {
                button {
                    key: "{index}",
                    // In Auto, the map the raid is on is marked without being pinned
                    class: if pinned_map() == Some(index) { "data-tab active" } else { "data-tab" },
                    class: if following && shown_map() == Some(index) { "replay-map-shown" },
                    onclick: move |_| pin(Some(index)),
                    "{map_label(map)}"
                }
            }
        }
    }
}
