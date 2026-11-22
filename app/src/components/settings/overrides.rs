//! One overlay's overrides of the Global tab: window style for every overlay,
//! plus the shared metric options for metric overlays.

use dioxus::prelude::*;

use super::draft::{SettingsDraft, appearance_for};
use super::frame_style::FrameOverrideRows;
use super::metrics_global::MetricOverrideRows;

#[component]
pub fn OverridesSection(
    draft: SettingsDraft,
    tab_key: Signal<String>,
    is_metric: bool,
    on_open_global: EventHandler<()>,
) -> Element {
    let s = draft.current();
    let key = tab_key();
    let appearance = appearance_for(&s, &key);
    let customized = s.frame_overrides.contains_key(&key)
        || (is_metric
            && (appearance.bar_gap.is_some()
                || appearance.bar_radius.is_some()
                || !appearance.overrides.is_empty()));

    rsx! {
        div { class: "settings-section",
            details { class: "collapsible",
                summary { class: "collapsible-summary",
                    "Override Global Settings"
                    span { class: "settings-check-note", if customized { "customized" } else { "following Global" } }
                }
                p { class: "hint",
                    "Ticked options apply to this overlay only. The rest show and follow the "
                    button { class: "link-btn", onclick: move |_| on_open_global.call(()), "Global tab" }
                    "."
                }
                h5 { class: "override-group", "Window Style" }
                FrameOverrideRows { draft, tab_key }
                if is_metric {
                    h5 { class: "override-group", "Metric Overlays" }
                    MetricOverrideRows { draft, tab_key }
                }
            }
        }
    }
}
