//! The lockout reset hour, shown on the application Settings page. It lives
//! in characters.json with the rest of the character data, not in AppConfig.

use dioxus::prelude::*;

use crate::api;
use crate::components::{ToastSeverity, use_toast};

#[component]
pub fn LockoutResetSetting() -> Element {
    let mut toast = use_toast();
    let mut hour = use_resource(|| async move { api::get_lockout_reset_hour().await.unwrap_or(12) });
    let value = hour.read().unwrap_or(12);
    rsx! {
        div { class: "setting-row",
            label { "Operation reset hour (UTC)" }
            input {
                r#type: "number",
                min: "0",
                max: "23",
                step: "1",
                value: "{value}",
                onchange: move |e| {
                    let Ok(h) = e.value().parse::<u8>() else { return };
                    spawn(async move {
                        match api::set_lockout_reset_hour(h.min(23)).await {
                            Ok(true) => hour.restart(),
                            _ => toast.show("Failed to save the reset hour", ToastSeverity::Normal),
                        }
                    });
                },
            }
        }
        p { class: "hint", "When the game clears operation lockouts: weekly on Tuesday for Veteran and Master, daily for Story. 12 is 6 AM Central standard time." }
    }
}
