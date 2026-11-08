//! The lockout reset hour, shown on the application Settings page. It lives
//! in characters.json with the rest of the character data, not in AppConfig.

use dioxus::prelude::*;

use crate::api;
use crate::components::{NumberInput, ToastSeverity, use_toast};

#[component]
pub fn LockoutResetSetting() -> Element {
    let mut toast = use_toast();
    let mut hour = use_resource(|| async move { api::get_lockout_reset_hour().await.unwrap_or(12) });
    let value = hour.read().unwrap_or(12);
    rsx! {
        div { class: "setting-row",
            label { "Operation reset hour (UTC)" }
            NumberInput {
                value: value as f64,
                min: 0.0,
                max: 23.0,
                on_change: move |v: f64| {
                    let h = v as u8;
                    spawn(async move {
                        match api::set_lockout_reset_hour(h).await {
                            Ok(true) => hour.restart(),
                            _ => toast.show("Failed to save the reset hour", ToastSeverity::Normal),
                        }
                    });
                },
            }
        }
        p { class: "hint", "When the game clears operation lockouts: weekly on Tuesday for Veteran and Master, daily for Story." }
    }
}
