//! The panel's working copy of `OverlaySettings`.

use dioxus::prelude::*;
use gloo_timers::future::TimeoutFuture;

use crate::api;
use crate::types::{OverlayAppearanceConfig, OverlaySettings};

/// Draft of the overlay settings being edited. Edits mark the draft dirty and
/// schedule a debounced live preview; Save in the panel footer persists them.
/// Copyable so every tab component takes it as a plain prop.
#[derive(Clone, Copy, PartialEq)]
pub struct SettingsDraft {
    settings: Signal<OverlaySettings>,
    has_changes: Signal<bool>,
    status: Signal<String>,
    /// Debounce token: each edit bumps it and only the latest preview fires
    preview_request_id: Signal<u32>,
}

pub fn use_settings_draft(settings: Signal<OverlaySettings>) -> SettingsDraft {
    SettingsDraft {
        #[allow(clippy::redundant_closure)]
        settings: use_signal(|| settings()),
        has_changes: use_signal(|| false),
        status: use_signal(String::new),
        preview_request_id: use_signal(|| 0),
    }
}

impl SettingsDraft {
    pub fn current(&self) -> OverlaySettings {
        (self.settings)()
    }

    pub fn has_changes(&self) -> bool {
        (self.has_changes)()
    }

    pub fn status(&self) -> String {
        (self.status)()
    }

    pub fn set_status(&self, msg: impl Into<String>) {
        let mut status = self.status;
        status.set(msg.into());
    }

    /// Apply an edit: dirty the draft and preview it on the overlays after 300ms of quiet
    pub fn update(&self, new_settings: OverlaySettings) {
        let (mut settings, mut has_changes, mut request_id) =
            (self.settings, self.has_changes, self.preview_request_id);
        settings.set(new_settings.clone());
        has_changes.set(true);
        self.set_status("");

        let id = request_id() + 1;
        request_id.set(id);
        spawn(async move {
            TimeoutFuture::new(300).await;
            if request_id() == id {
                api::preview_overlay_settings(&new_settings).await;
            }
        });
    }

    /// Change the draft in place: `draft.edit(|s| s.alerts_opacity = v)`
    pub fn edit(&self, f: impl FnOnce(&mut OverlaySettings)) {
        let mut s = self.current();
        f(&mut s);
        self.update(s);
    }

    /// Replace the draft with persisted settings (after a save, profile load or reset)
    pub fn replace(&self, settings: OverlaySettings) {
        let (mut current, mut has_changes) = (self.settings, self.has_changes);
        current.set(settings);
        has_changes.set(false);
    }

    /// Unmount cleanup (leaving the Overlays page): stop any pending preview;
    /// true when unsaved edits were previewed and need reverting. try_: the
    /// panel's signals may already be torn down
    pub fn abandon(&self) -> bool {
        let mut request_id = self.preview_request_id;
        if let Ok(mut id) = request_id.try_write() {
            *id += 1;
        }
        self.has_changes.try_peek().is_ok_and(|c| *c)
    }

}

/// Appearance for an overlay key, falling back to its bundled default
pub fn appearance_for(settings: &OverlaySettings, key: &str) -> OverlayAppearanceConfig {
    settings
        .appearances
        .get(key)
        .cloned()
        .or_else(|| settings.default_appearances.get(key).cloned())
        .unwrap_or_default()
}
