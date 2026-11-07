//! `~/.config/baras/characters.json`: the user-owned half of every character.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use baras_types::characters::{
    CHARACTER_NOTE_MAX, CREW_SKILLS, CharacterRecord, DEFAULT_RESET_HOUR_UTC, MAX_CLASSES, MAX_CREW_SKILLS,
    ManualLock, character_key,
};

use crate::game_data::Class;

pub const CHARACTERS_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterStore {
    #[serde(default)]
    pub version: u32,
    /// Hour (UTC) of the daily and weekly reset
    #[serde(default = "default_reset_hour")]
    pub reset_hour_utc: u8,
    /// Keyed by `name@server` (see `character_key`)
    #[serde(default)]
    pub characters: BTreeMap<String, CharacterRecord>,
    /// Characters the user removed; discovery never brings them back
    #[serde(default)]
    pub hidden: BTreeSet<String>,
}

fn default_reset_hour() -> u8 {
    DEFAULT_RESET_HOUR_UTC
}

impl Default for CharacterStore {
    fn default() -> Self {
        Self {
            version: CHARACTERS_VERSION,
            reset_hour_utc: DEFAULT_RESET_HOUR_UTC,
            characters: BTreeMap::new(),
            hidden: BTreeSet::new(),
        }
    }
}

/// `~/.config/baras/characters.json`
pub fn store_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("baras")
        .join("characters.json")
}

impl CharacterStore {
    /// Missing or unreadable file → empty store (and a warning, never a panic).
    pub fn load() -> Self {
        let path = store_path();
        match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                tracing::warn!(path = %path.display(), error = %e, "Unreadable characters.json; starting empty");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Atomic write (tmp + rename)
    pub fn save(&mut self) -> std::io::Result<()> {
        self.version = CHARACTERS_VERSION;
        let path = store_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(&tmp, &path)
    }

    /// Normalize and store a record; `previous_key` is the entry it replaces
    /// when the name or server was edited. Returns the stored key, or None
    /// for a blank name.
    pub fn upsert(&mut self, mut record: CharacterRecord, previous_key: Option<&str>) -> Option<String> {
        record.name = record.name.trim().to_string();
        record.server = record.server.trim().to_string();
        if record.name.is_empty() {
            return None;
        }
        record.classes.retain(|c| Class::from_name(c).is_some());
        record.classes.dedup();
        record.classes.truncate(MAX_CLASSES);
        record.crew_skills.retain(|s| CREW_SKILLS.contains(&s.as_str()));
        record.crew_skills.dedup();
        record.crew_skills.truncate(MAX_CREW_SKILLS);
        record.guild = record.guild.trim().to_string();
        record.note = record.note.trim().chars().take(CHARACTER_NOTE_MAX).collect();

        let key = character_key(&record.name, &record.server);
        if let Some(prev) = previous_key.filter(|p| *p != key) {
            // Carry manual locks across a rename unless the edit set its own
            if let Some(old) = self.characters.remove(prev) && record.manual_locks.is_empty() {
                record.manual_locks = old.manual_locks;
            }
        }
        self.hidden.remove(&key);
        self.characters.insert(key.clone(), record);
        Some(key)
    }

    /// Drop the record and keep the character out of discovery
    pub fn remove(&mut self, key: &str) {
        self.characters.remove(key);
        self.hidden.insert(key.to_string());
    }

    pub fn restore(&mut self, key: &str) -> bool {
        self.hidden.remove(key)
    }

    pub fn set_reset_hour(&mut self, hour: u8) {
        self.reset_hour_utc = hour.min(23);
    }

    /// A character with no record yet gets one holding just the lock
    pub fn add_manual_lock(&mut self, key: &str, lock: ManualLock) {
        let record = self.characters.entry(key.to_string()).or_insert_with(|| {
            let (name, server) = key.rsplit_once('@').unwrap_or((key, ""));
            CharacterRecord { name: name.to_string(), server: server.to_string(), ..Default::default() }
        });
        record.manual_locks.retain(|l| !(l.operation == lock.operation && l.tier == lock.tier));
        record.manual_locks.push(lock);
    }

    pub fn clear_manual_lock(&mut self, key: &str, operation: &str, tier: &str) {
        if let Some(record) = self.characters.get_mut(key) {
            record.manual_locks.retain(|l| !(l.operation == operation && l.tier == tier));
        }
    }
}
