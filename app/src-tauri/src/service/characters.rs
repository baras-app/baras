//! Characters tab: the roster query and edits to the character store.

use std::collections::HashMap;

use baras_core::characters::CharacterStore;
use baras_types::characters::{CharacterRecord, CharacterRoster, ManualLock};

use super::handler::ServiceHandle;

impl ServiceHandle {
    /// Every character the logs name, merged with the store, lockouts included.
    pub async fn character_roster(&self) -> CharacterRoster {
        let catalog = self.area_catalog();
        let shared = self.shared.clone();
        // Newest event per log, so a character's last session counts even
        // when that log recorded no pulls
        let last_events: HashMap<String, i64> = self
            .shared
            .directory_index
            .read()
            .await
            .entries()
            .into_iter()
            .map(|e| {
                let t = e.last_event_time.unwrap_or(e.created_at);
                (e.filename.clone(), t.and_utc().timestamp_millis())
            })
            .collect();
        self.with_pull_index(move |index| {
            let store = shared.character_store.lock().unwrap_or_else(|p| p.into_inner());
            baras_core::characters::roster(index, &store, &catalog, &last_events)
        })
        .await
        .unwrap_or_default()
    }

    /// Apply one edit and persist; false when the file could not be written.
    fn edit_characters(&self, edit: impl FnOnce(&mut CharacterStore)) -> bool {
        let mut store = self.shared.character_store.lock().unwrap_or_else(|p| p.into_inner());
        edit(&mut store);
        match store.save() {
            Ok(()) => true,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to save characters.json");
                false
            }
        }
    }

    /// Returns the stored key; None for a blank name or a failed write.
    pub fn save_character(&self, record: CharacterRecord, previous_key: Option<String>) -> Option<String> {
        let mut key = None;
        self.edit_characters(|s| key = s.upsert(record, previous_key.as_deref()))
            .then_some(key)
            .flatten()
    }

    pub fn delete_character(&self, key: &str) -> bool {
        self.edit_characters(|s| s.remove(key))
    }

    pub fn restore_character(&self, key: &str) -> bool {
        self.edit_characters(|s| {
            s.restore(key);
        })
    }

    pub fn lockout_reset_hour(&self) -> u8 {
        self.shared.character_store.lock().unwrap_or_else(|p| p.into_inner()).reset_hour_utc
    }

    pub fn set_lockout_reset_hour(&self, hour: u8) -> bool {
        self.edit_characters(|s| s.set_reset_hour(hour))
    }

    pub fn add_manual_lock(&self, key: &str, lock: ManualLock) -> bool {
        self.edit_characters(|s| s.add_manual_lock(key, lock))
    }

    pub fn clear_manual_lock(&self, key: &str, operation: &str, tier: &str) -> bool {
        self.edit_characters(|s| s.clear_manual_lock(key, operation, tier))
    }
}
