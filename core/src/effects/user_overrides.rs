//! User effect overrides (`definitions/effects.toml`).
//!
//! An entry whose `id` matches a bundled effect stores only the top-level keys
//! that differ from bundled (each replaces the bundled value whole), plus
//! `reset = [..]` for bundled keys the user cleared. Any other entry is a full
//! custom definition. v1 files held full copies and are migrated on load.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use toml::{Table, Value};

use super::{DefinitionSet, EFFECTS_DSL_VERSION, EffectDefinition};

const RESET_KEY: &str = "reset";

#[derive(Debug, Error)]
pub enum UserEffectsError {
    #[error("IO error at {0}")]
    Io(PathBuf, #[source] std::io::Error),
    #[error("parse error in {0}")]
    Parse(PathBuf, #[source] toml::de::Error),
    #[error("serialization error")]
    Serialize(#[from] toml::ser::Error),
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct UserEffectsFile {
    #[serde(default)]
    version: u32,
    #[serde(default, rename = "effect")]
    entries: Vec<Table>,
}

/// Raw user entries, resolved against bundled definitions on demand
#[derive(Debug, Clone, Default)]
pub struct UserEffects {
    entries: Vec<Table>,
}

impl UserEffects {
    /// Default location: `{config}/definitions/effects.toml`
    pub fn default_path() -> Option<PathBuf> {
        crate::paths::config_dir().map(|p| p.join("definitions").join("effects.toml"))
    }

    /// Load the user file, migrating a v1 file in place (backup kept at `.v1.bak`).
    /// Unknown versions are backed up to `.bak` and ignored, as before.
    pub fn load(path: &Path, bundled: &DefinitionSet) -> Result<Self, UserEffectsError> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path).map_err(|e| UserEffectsError::Io(path.to_path_buf(), e))?;
        let file: UserEffectsFile = match toml::from_str(&content) {
            Ok(f) => f,
            Err(e) => {
                let _ = std::fs::rename(path, path.with_extension("toml.bak"));
                return Err(UserEffectsError::Parse(path.to_path_buf(), e));
            }
        };

        match file.version {
            EFFECTS_DSL_VERSION => Ok(Self { entries: file.entries }),
            1 => {
                let migrated = Self::migrate_v1(file.entries, bundled);
                std::fs::copy(path, path.with_extension("toml.v1.bak"))
                    .map_err(|e| UserEffectsError::Io(path.to_path_buf(), e))?;
                migrated.save(path)?;
                tracing::info!(
                    count = migrated.entries.len(),
                    "Migrated user effects to sparse overrides"
                );
                Ok(migrated)
            }
            version => {
                tracing::warn!(
                    version,
                    expected = EFFECTS_DSL_VERSION,
                    "User effects version mismatch, backing up"
                );
                let _ = std::fs::rename(path, path.with_extension("toml.bak"));
                Ok(Self::default())
            }
        }
    }

    /// Write entries; removes the file when there are none
    pub fn save(&self, path: &Path) -> Result<(), UserEffectsError> {
        if self.entries.is_empty() {
            if path.exists() {
                std::fs::remove_file(path).map_err(|e| UserEffectsError::Io(path.to_path_buf(), e))?;
            }
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| UserEffectsError::Io(path.to_path_buf(), e))?;
        }
        let file = UserEffectsFile {
            version: EFFECTS_DSL_VERSION,
            entries: self.entries.clone(),
        };
        let content = toml::to_string_pretty(&file)?;
        std::fs::write(path, content).map_err(|e| UserEffectsError::Io(path.to_path_buf(), e))
    }

    /// v1 entries were full definitions: diff the bundled ones (missing keys become `reset`)
    fn migrate_v1(entries: Vec<Table>, bundled: &DefinitionSet) -> Self {
        let mut out = Self::default();
        for table in entries {
            match Value::Table(table.clone()).try_into::<EffectDefinition>() {
                Ok(def) => out.upsert(&def, bundled.get(&def.id)),
                Err(e) => {
                    tracing::warn!(error = %e, "Keeping unparseable v1 user effect as-is");
                    out.entries.push(table);
                }
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.position(id).is_some()
    }

    fn position(&self, id: &str) -> Option<usize> {
        self.entries.iter().position(|t| entry_id(t) == Some(id))
    }

    /// Store `def`: a diff when it overrides `bundled`, else the full definition.
    /// An override equal to bundled removes the entry.
    pub fn upsert(&mut self, def: &EffectDefinition, bundled: Option<&EffectDefinition>) {
        let entry = match bundled {
            Some(b) => diff(def, b),
            None => to_table(def),
        };
        match (self.position(&def.id), entry) {
            (Some(i), Some(e)) => self.entries[i] = e,
            (None, Some(e)) => self.entries.push(e),
            (Some(i), None) => {
                self.entries.remove(i);
            }
            (None, None) => {}
        }
    }

    /// Remove an entry; returns whether one existed
    pub fn remove(&mut self, id: &str) -> bool {
        self.position(id).map(|i| self.entries.remove(i)).is_some()
    }

    /// Resolve every entry to a full definition. Orphaned overrides (bundled
    /// id gone) and entries that fail to parse are skipped with a warning.
    pub fn resolve(&self, bundled: &DefinitionSet) -> Vec<EffectDefinition> {
        self.entries
            .iter()
            .filter_map(|entry| {
                let id = entry_id(entry)?;
                let base = bundled.get(id);
                if base.is_none() && entry.contains_key(RESET_KEY) {
                    tracing::warn!(id, "Skipping user override of a bundled effect that no longer exists");
                    return None;
                }
                merge(entry, base)
                    .inspect_err(|e| tracing::warn!(id, error = %e, "Skipping invalid user effect"))
                    .ok()
            })
            .collect()
    }

    /// Bundled definitions with user entries applied
    pub fn apply(&self, bundled: &DefinitionSet) -> DefinitionSet {
        let mut set = bundled.clone();
        set.add_definitions(self.resolve(bundled), true);
        set
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Diff / merge
// ═══════════════════════════════════════════════════════════════════════════

fn entry_id(t: &Table) -> Option<&str> {
    t.get("id").and_then(Value::as_str)
}

fn to_table(def: &EffectDefinition) -> Option<Table> {
    Table::try_from(def)
        .inspect_err(|e| tracing::error!(id = def.id, error = %e, "Failed to serialize effect"))
        .ok()
}

/// Top-level keys of `def` that differ from `bundled`; `None` when identical
fn diff(def: &EffectDefinition, bundled: &EffectDefinition) -> Option<Table> {
    let (user, base) = (to_table(def)?, to_table(bundled)?);
    let mut out = Table::new();
    out.insert("id".into(), Value::String(def.id.clone()));
    for (k, v) in user.iter().filter(|(k, _)| *k != "id") {
        if base.get(k) != Some(v) {
            out.insert(k.clone(), v.clone());
        }
    }
    let reset: Vec<Value> = base
        .keys()
        .filter(|k| !user.contains_key(*k))
        .map(|k| Value::String(k.clone()))
        .collect();
    if out.len() == 1 && reset.is_empty() {
        return None;
    }
    // Every bundled override carries `reset` (possibly empty) so it stays
    // recognizable as an override if the bundled effect disappears
    out.insert(RESET_KEY.into(), Value::Array(reset));
    Some(out)
}

fn merge(entry: &Table, bundled: Option<&EffectDefinition>) -> Result<EffectDefinition, toml::de::Error> {
    let mut table = bundled.and_then(to_table).unwrap_or_default();
    if let Some(reset) = entry.get(RESET_KEY).and_then(Value::as_array) {
        for key in reset.iter().filter_map(Value::as_str) {
            table.remove(key);
        }
    }
    for (k, v) in entry.iter().filter(|(k, _)| *k != RESET_KEY) {
        table.insert(k.clone(), v.clone());
    }
    Value::Table(table).try_into()
}

/// Whether two definitions differ only in user toggles (`enabled`, `audio.enabled`)
pub fn eq_ignoring_toggles(a: &EffectDefinition, b: &EffectDefinition) -> bool {
    let mut a = a.clone();
    a.enabled = b.enabled;
    a.audio.enabled = b.audio.enabled;
    to_table(&a) == to_table(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundled() -> DefinitionSet {
        let src = r#"
            version = 1
            [[effect]]
            id = "probe"
            name = "Probe"
            color = [1, 2, 3, 255]
            duration_secs = 20.0
            [effect.trigger]
            type = "effect_applied"
            effects = [42]
        "#;
        let cfg: super::super::DefinitionConfig = toml::from_str(src).unwrap();
        let mut set = DefinitionSet::new();
        set.add_definitions(cfg.effects, false);
        set
    }

    #[test]
    fn toggle_and_reset_round_trip() {
        let set = bundled();
        let mut def = set.get("probe").unwrap().clone();
        def.enabled = false;
        def.color = None;

        let mut user = UserEffects::default();
        user.upsert(&def, set.get("probe"));
        let entry = &user.entries[0];
        assert_eq!(entry.len(), 3, "{entry:?}"); // id, enabled, reset
        assert_eq!(entry["reset"].as_array().unwrap(), &vec![Value::from("color")]);

        let resolved = &user.resolve(&set)[0];
        assert!(!resolved.enabled && resolved.color.is_none());
        assert_eq!(resolved.duration_secs, Some(20.0));

        let mut back = resolved.clone();
        back.enabled = true;
        back.color = Some([1, 2, 3, 255]);
        user.upsert(&back, set.get("probe"));
        assert!(user.is_empty());
    }

    #[test]
    fn toggles_are_not_material() {
        let set = bundled();
        let base = set.get("probe").unwrap();
        let mut def = base.clone();
        def.enabled = false;
        def.audio.enabled = true;
        assert!(eq_ignoring_toggles(&def, base));
        def.duration_secs = Some(5.0);
        assert!(!eq_ignoring_toggles(&def, base));
    }
}
