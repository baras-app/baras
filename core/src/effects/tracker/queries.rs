//! Read-only views of tracked effects for the overlays and uptime placeholders.

use super::*;

impl EffectTracker {
    /// Check if there are any active effects (cheap check before full iteration)
    pub fn has_active_effects(&self) -> bool {
        !self.active_effects.is_empty()
    }

    /// Get all active effects for rendering
    pub fn active_effects(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects.values()
    }

    /// Get mutable references to all active effects (for audio processing)
    pub fn active_effects_mut(&mut self) -> impl Iterator<Item = &mut ActiveEffect> {
        self.active_effects.values_mut()
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // Categorized Output Methods (by DisplayTarget)
    // ─────────────────────────────────────────────────────────────────────────────

    /// Displayed effects destined for one overlay
    pub fn effects_on(&self, target: DisplayTarget) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects
            .values()
            .filter(move |e| e.is_displayed() && e.display_targets.contains(&target))
    }

    /// Definitions flagged for uptime tracking on the given overlay target.
    /// Fail-closed: empty until the local player's discipline is known, so
    /// placeholders never show for the wrong class before detection.
    pub fn uptime_definitions(&self, target: DisplayTarget) -> Vec<&EffectDefinition> {
        let Some(discipline) = self.local_player_discipline.as_ref() else {
            return Vec::new();
        };
        let mut defs: Vec<_> = self
            .definitions
            .enabled()
            .filter(|def| {
                def.track_uptime
                    && def.displays_on(target)
                    && (def.disciplines.is_empty() || def.disciplines.contains(discipline))
                    && (!def.track_uptime_after_first || self.uptime_last_icons.contains_key(&def.id))
            })
            .collect();
        // Stable pin order regardless of HashMap iteration
        defs.sort_by(|a, b| a.id.cmp(&b.id));
        defs
    }

    /// Snapshot the icons of currently-active uptime effects. Entries are
    /// retained after the effect ends, so multi-effect definitions keep
    /// their placeholder on the last-used variant.
    pub fn remember_uptime_icons(&mut self) {
        for (key, effect) in &self.active_effects {
            if effect.is_removed() || !self.definitions.def(key.def).track_uptime {
                continue;
            }
            match self.uptime_last_icons.get_mut(&effect.definition_id) {
                Some(icon) => *icon = effect.icon_ability_id,
                None => {
                    self.uptime_last_icons
                        .insert(effect.definition_id.clone(), effect.icon_ability_id);
                }
            }
        }
    }

    /// Forget uptime-tracking state on player logout (new blank log file or
    /// game process exit): clears the detected discipline so "show when
    /// absent" placeholders stop rendering, and the first-appearance memory
    /// so `track_uptime_after_first` re-arms for the next login.
    pub fn clear_uptime_tracking(&mut self) {
        self.local_player_discipline = None;
        self.uptime_last_icons.clear();
    }

    /// Look up a definition by ID (e.g. from an active effect's definition_id)
    pub fn definition(&self, id: &str) -> Option<&EffectDefinition> {
        self.definitions.get(id)
    }

    /// Icon for an uptime placeholder: last-activated icon if seen this
    /// session, else the definition's static fallback.
    pub fn uptime_placeholder_icon(&self, def: &EffectDefinition) -> u64 {
        self.uptime_last_icons
            .get(&def.id)
            .copied()
            .unwrap_or_else(|| def.uptime_icon_id())
    }

    /// Get effects destined for DOT tracker, grouped by target entity
    pub fn dot_tracker_effects(&self) -> std::collections::HashMap<i64, Vec<&ActiveEffect>> {
        let mut by_target: std::collections::HashMap<i64, Vec<&ActiveEffect>> = std::collections::HashMap::new();
        for effect in self.active_effects.values() {
            if effect.is_displayed() && effect.display_targets.contains(&DisplayTarget::DotTracker) {
                by_target.entry(effect.target_entity_id).or_default().push(effect);
            }
        }
        by_target
    }

    /// Get effects destined for the boss HP overlay, grouped by target entity id.
    /// Keyed by entity id (not name) so two NPCs sharing a name don't share icons.
    pub fn boss_health_effects_by_target_id(&self) -> std::collections::HashMap<i64, Vec<&ActiveEffect>> {
        let mut by_target: std::collections::HashMap<i64, Vec<&ActiveEffect>> = std::collections::HashMap::new();
        for effect in self.active_effects.values() {
            if effect.is_displayed() && effect.display_targets.contains(&DisplayTarget::BossHealth) {
                by_target.entry(effect.target_entity_id).or_default().push(effect);
            }
        }
        by_target
    }
}
