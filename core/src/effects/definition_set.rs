//! Effect definitions with lookup indexes by trigger and refresh ability.

use std::collections::{HashMap, HashSet};

use baras_types::{AbilitySelector, EffectSelector};

use super::{DefId, DefinitionConfig, EffectDefinition};
use crate::dsl::Trigger;

/// ID and lowercase-name indexes from a game ID to the definitions it hits
#[derive(Debug, Clone, Default)]
struct Index {
    by_id: HashMap<u64, Vec<DefId>>,
    by_name: HashMap<String, Vec<DefId>>,
}

impl Index {
    fn add_effect(&mut self, selector: &EffectSelector, def: DefId) {
        match selector {
            EffectSelector::Id(id) => self.by_id.entry(*id).or_default().push(def),
            EffectSelector::Name(name) => self.by_name.entry(name.to_lowercase()).or_default().push(def),
        }
    }

    fn add_ability(&mut self, selector: &AbilitySelector, def: DefId) {
        match selector {
            AbilitySelector::Id(id) => self.by_id.entry(*id).or_default().push(def),
            AbilitySelector::Name(name) => self.by_name.entry(name.to_lowercase()).or_default().push(def),
        }
    }
}

/// Combined set of effect definitions with indexes for fast lookup
#[derive(Debug, Clone, Default)]
pub struct DefinitionSet {
    defs: Vec<EffectDefinition>,
    /// Definition ID string -> index into `defs`
    ids: HashMap<String, DefId>,
    /// EffectApplied / EffectRemoved triggers
    effects: Index,
    /// AbilityCast / DamageTaken / HealingTaken triggers
    abilities: Index,
    /// `refresh_abilities` entries
    refresh: Index,
    /// Refresh abilities (IDs, lowercase names) that use AoE damage correlation (`is_aoe_refresh`)
    aoe_refresh_ability_ids: HashSet<u64>,
    aoe_refresh_ability_names: HashSet<String>,
    /// Any enabled definition has duration modifiers
    has_modifiers: bool,
}

impl DefinitionSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add definitions. If `overwrite` is true, replaces existing definitions with same ID.
    /// Returns IDs of duplicates that were encountered (skipped if !overwrite, replaced if overwrite).
    pub fn add_definitions(&mut self, definitions: Vec<EffectDefinition>, overwrite: bool) -> Vec<String> {
        let mut duplicates = Vec::new();
        for def in definitions {
            if let Some(&id) = self.ids.get(&def.id) {
                duplicates.push(def.id.clone());
                if overwrite {
                    self.defs[id.0 as usize] = def;
                }
                continue;
            }
            self.ids.insert(def.id.clone(), DefId(self.defs.len() as u32));
            self.defs.push(def);
        }
        self.rebuild_indexes();
        duplicates
    }

    /// Load every bundled `*.toml` in `dir` (skipping the `custom.toml` template),
    /// keeping the first definition on duplicate IDs. Unreadable files are skipped.
    pub fn add_from_dir(&mut self, dir: &std::path::Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            tracing::debug!(path = ?dir, "Failed to read effects directory");
            return;
        };
        // Sorted so first-wins duplicate resolution is deterministic
        let mut files: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension().is_some_and(|ext| ext == "toml") && p.file_name().is_none_or(|n| n != "custom.toml")
            })
            .collect();
        files.sort();

        for path in files {
            if let Ok(contents) = std::fs::read_to_string(&path)
                && let Ok(config) = toml::from_str::<DefinitionConfig>(&contents)
            {
                let count = config.effects.len();
                self.add_definitions(config.effects, false);
                tracing::debug!(file = ?path.file_name().unwrap_or_default(), count, "Loaded effect definitions");
            }
        }
    }

    fn rebuild_indexes(&mut self) {
        let (mut effects, mut abilities, mut refresh) = (Index::default(), Index::default(), Index::default());
        self.aoe_refresh_ability_ids.clear();
        self.aoe_refresh_ability_names.clear();

        for (i, def) in self.defs.iter().enumerate() {
            let id = DefId(i as u32);
            match &def.trigger {
                Trigger::EffectApplied { effects: sel, .. } | Trigger::EffectRemoved { effects: sel, .. } => {
                    sel.iter().for_each(|s| effects.add_effect(s, id));
                }
                Trigger::AbilityCast { abilities: sel, .. }
                | Trigger::DamageTaken { abilities: sel, .. }
                | Trigger::HealingTaken { abilities: sel, .. } => {
                    sel.iter().for_each(|s| abilities.add_ability(s, id));
                }
                _ => {}
            }
            for r in &def.refresh_abilities {
                refresh.add_ability(r.ability(), id);
                if def.is_aoe_refresh {
                    match r.ability() {
                        AbilitySelector::Id(ability_id) => self.aoe_refresh_ability_ids.insert(*ability_id),
                        AbilitySelector::Name(name) => self.aoe_refresh_ability_names.insert(name.to_lowercase()),
                    };
                }
            }
        }
        (self.effects, self.abilities, self.refresh) = (effects, abilities, refresh);
        let has_modifiers = self.enabled().any(|d| !d.modifiers.is_empty());
        self.has_modifiers = has_modifiers;
    }

    /// Enabled definitions an index maps `id` or `name` to, without duplicates
    fn lookup(&self, index: &Index, id: u64, name: Option<&str>) -> Vec<(DefId, &EffectDefinition)> {
        let by_name = name
            .filter(|_| !index.by_name.is_empty())
            .and_then(|n| index.by_name.get(&n.to_lowercase()));
        let mut out: Vec<(DefId, &EffectDefinition)> = Vec::new();
        for &def_id in index.by_id.get(&id).into_iter().chain(by_name).flatten() {
            let def = self.def(def_id);
            if def.enabled && !out.iter().any(|(d, _)| *d == def_id) {
                out.push((def_id, def));
            }
        }
        out
    }

    /// Definition by `DefId` (issued by this set)
    pub fn def(&self, id: DefId) -> &EffectDefinition {
        &self.defs[id.0 as usize]
    }

    /// `DefId` for a definition ID string
    pub fn id_of(&self, id: &str) -> Option<DefId> {
        self.ids.get(id).copied()
    }

    /// Definition by ID string
    pub fn get(&self, id: &str) -> Option<&EffectDefinition> {
        self.id_of(id).map(|d| self.def(d))
    }

    pub fn len(&self) -> usize {
        self.defs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }

    /// Definitions triggered by a game effect ID or name (EffectApplied/EffectRemoved)
    pub fn find_matching(&self, effect_id: u64, effect_name: Option<&str>) -> Vec<(DefId, &EffectDefinition)> {
        self.lookup(&self.effects, effect_id, effect_name)
    }

    /// Definitions triggered by an ability (AbilityCast/DamageTaken/HealingTaken)
    pub fn find_ability_cast_matching(
        &self,
        ability_id: u64,
        ability_name: Option<&str>,
    ) -> Vec<(DefId, &EffectDefinition)> {
        self.lookup(&self.abilities, ability_id, ability_name)
    }

    /// Definitions an ability can refresh
    pub fn find_refreshable_by(&self, ability_id: u64, ability_name: Option<&str>) -> Vec<(DefId, &EffectDefinition)> {
        self.lookup(&self.refresh, ability_id, ability_name)
    }

    /// Whether any enabled definition has duration modifiers (else the
    /// per-batch modifier pass is skipped)
    pub fn has_modifiers(&self) -> bool {
        self.has_modifiers
    }

    /// Check if an ability uses AoE damage correlation for refresh detection
    pub fn is_aoe_refresh(&self, ability_id: u64, ability_name: &str) -> bool {
        self.aoe_refresh_ability_ids.contains(&ability_id)
            || (!self.aoe_refresh_ability_names.is_empty()
                && self.aoe_refresh_ability_names.contains(&ability_name.to_lowercase()))
    }

    /// All definitions, enabled or not
    pub fn iter(&self) -> impl Iterator<Item = &EffectDefinition> {
        self.defs.iter()
    }

    /// Get all enabled effect definitions
    pub fn enabled(&self) -> impl Iterator<Item = &EffectDefinition> {
        self.defs.iter().filter(|def| def.enabled)
    }
}
