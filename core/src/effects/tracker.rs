//! Effect tracking handler
//!
//! Tracks active effects on entities by matching game signals against
//! configured effect definitions. Produces `ActiveEffect` instances
//! that can be fed to overlay renderers.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::NaiveDateTime;

use crate::combat_log::EntityType;
use crate::context::IStr;
use crate::dsl::EntityFilterMatching;
use crate::signal_processor::{GameSignal, SignalHandler};

use super::{ActiveEffect, EffectDefinition, EffectTriggerMode};

/// Combined set of effect definitions
#[derive(Debug, Clone, Default)]
pub struct DefinitionSet {
    /// All effect definitions, keyed by ID
    pub effects: HashMap<String, EffectDefinition>,
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
            // Warn about effects that will never match anything
            if def.effects.is_empty() && def.refresh_abilities.is_empty() {
                eprintln!(
                    "[EFFECT WARNING] Effect '{}' has no effects or refresh_abilities - it will never match anything!",
                    def.id
                );
            }

            if self.effects.contains_key(&def.id) {
                duplicates.push(def.id.clone());
                if !overwrite {
                    continue; // Skip duplicate - keep the first definition
                }
                // Overwrite mode: user definitions replace bundled
            }
            self.effects.insert(def.id.clone(), def);
        }
        duplicates
    }

    /// Get an effect definition by ID
    pub fn get(&self, id: &str) -> Option<&EffectDefinition> {
        self.effects.get(id)
    }

    /// Find effect definitions that match a game effect ID/name
    pub fn find_matching(&self, effect_id: u64, effect_name: Option<&str>) -> Vec<&EffectDefinition> {
        self.effects
            .values()
            .filter(|def| def.enabled && def.matches_effect(effect_id, effect_name))
            .collect()
    }

    /// Get all enabled effect definitions
    pub fn enabled(&self) -> impl Iterator<Item = &EffectDefinition> {
        self.effects.values().filter(|def| def.enabled)
    }
}

/// Key for identifying unique effect instances
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct EffectInstanceKey {
    definition_id: String,
    target_entity_id: i64,
}

/// Entity info for filter matching
#[derive(Debug, Clone, Copy)]
struct EntityInfo {
    id: i64,
    /// NPC class/template ID (0 for players/companions)
    npc_id: i64,
    entity_type: EntityType,
    name: IStr,
}

/// Info about a newly registered target (for raid frame registration)
#[derive(Debug, Clone)]
pub struct NewTargetInfo {
    pub entity_id: i64,
    pub name: IStr,
}

/// Cached target info from TargetSet events
#[derive(Debug, Clone)]
struct TrackedTarget {
    entity_id: i64,
    name: IStr,
    entity_type: EntityType,
}

/// Tracks active effects for overlay display.
///
/// Matches game signals against effect definitions and maintains
/// a collection of active effects that can be queried for rendering.
#[derive(Debug)]
pub struct EffectTracker {
    /// Effect definitions to match against
    definitions: DefinitionSet,

    /// Currently active effects
    active_effects: HashMap<EffectInstanceKey, ActiveEffect>,

    /// Local player entity ID (for source/target filtering)
    local_player_id: Option<i64>,

    /// Current game time (latest timestamp from signals)
    current_game_time: Option<NaiveDateTime>,

    /// Whether we're in live mode (tracking effects) vs historical mode (skip)
    /// Defaults to false - must be enabled after initial file load
    live_mode: bool,

    /// Queue of targets that received effects from local player.
    /// Drained by the service to attempt registration in the raid registry.
    /// The registry itself handles duplicate rejection.
    new_targets: Vec<NewTargetInfo>,

    /// Current target for each entity (source_id -> target info)
    /// Used to resolve target when AbilityActivated has empty/self target
    current_targets: HashMap<i64, TrackedTarget>,
}

impl Default for EffectTracker {
    fn default() -> Self {
        Self::new(DefinitionSet::new())
    }
}

impl EffectTracker {
    /// Create a new effect tracker with the given definitions
    pub fn new(definitions: DefinitionSet) -> Self {
        Self {
            definitions,
            active_effects: HashMap::new(),
            local_player_id: None,
            current_game_time: None,
            live_mode: false, // Start in historical mode
            new_targets: Vec::new(),
            current_targets: HashMap::new(),
        }
    }

    /// Enable live mode (start tracking effects)
    /// Call this after initial file load is complete
    pub fn set_live_mode(&mut self, enabled: bool) {
        self.live_mode = enabled;
    }

    /// Set the local player entity ID (needed for filtering)
    pub fn set_local_player(&mut self, entity_id: i64) {
        self.local_player_id = Some(entity_id);
    }

    /// Update definitions (e.g., after config reload)
    /// Also updates display properties on any active effects that match
    pub fn set_definitions(&mut self, definitions: DefinitionSet) {
        // Update active effects with new display properties from their definitions
        for effect in self.active_effects.values_mut() {
            if let Some(def) = definitions.effects.get(&effect.definition_id) {
                effect.show_on_raid_frames = def.show_on_raid_frames;
                effect.show_on_effects_overlay = def.show_on_effects_overlay;
                effect.color = def.effective_color();
                effect.category = def.category;
            }
        }
        self.definitions = definitions;
    }

    /// Check if there are any active effects (cheap check before full iteration)
    pub fn has_active_effects(&self) -> bool {
        !self.active_effects.is_empty()
    }

    /// Check if there are effects still ticking (not yet removed/expired)
    /// Use this for early-out checks - effects with removed_at set are just fading out
    pub fn has_ticking_effects(&self) -> bool {
        self.active_effects.values().any(|e| e.removed_at.is_none())
    }

    /// Check if there's any work to do (effects to render or new targets to register)
    pub fn has_pending_work(&self) -> bool {
        self.has_ticking_effects() || !self.new_targets.is_empty()
    }

    /// Get the current game time (latest timestamp from combat log)
    pub fn current_game_time(&self) -> Option<NaiveDateTime> {
        self.current_game_time
    }

    /// Get all active effects for rendering
    pub fn active_effects(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects.values()
    }

    /// Get mutable references to all active effects (for audio processing)
    pub fn active_effects_mut(&mut self) -> impl Iterator<Item = &mut ActiveEffect> {
        self.active_effects.values_mut()
    }

    /// Get active effects for a specific target entity
    pub fn effects_for_target(&self, target_id: i64) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects
            .values()
            .filter(move |e| e.target_entity_id == target_id)
    }

    /// Drain the queue of targets for raid frame registration attempts.
    /// Called by the service - the registry handles duplicate rejection.
    pub fn take_new_targets(&mut self) -> Vec<NewTargetInfo> {
        std::mem::take(&mut self.new_targets)
    }

    /// Tick the tracker - removes expired effects and updates state
    pub fn tick(&mut self) {
        let Some(current_time) = self.current_game_time else {
            return;
        };

        // Mark duration-expired effects as removed
        for effect in self.active_effects.values_mut() {
            if effect.is_active(current_time) && effect.has_duration_expired(current_time) {
                effect.mark_removed();
            }
        }

        // Remove effects that have finished fading
        self.active_effects.retain(|_, effect| !effect.should_remove());
    }

    /// Handle effect application signal
    fn handle_effect_applied(
        &mut self,
        effect_id: i64,
        effect_name: IStr,
        action_id: i64,
        action_name: IStr,
        source_id: i64,
        source_name: IStr,
        source_entity_type: EntityType,
        source_npc_id: i64,
        target_id: i64,
        target_name: IStr,
        target_entity_type: EntityType,
        target_npc_id: i64,
        timestamp: NaiveDateTime,
        charges: Option<u8>,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
        self.current_game_time = Some(timestamp);

        // Garbage collect dead effects before processing new ones.
        self.active_effects.retain(|_, effect| effect.is_active(timestamp));

        // Skip effect tracking when processing historical data (initial file load)
        if !self.live_mode {
            return;
        }

        // Build entity info for filter matching
        let source_info = EntityInfo {
            id: source_id,
            npc_id: source_npc_id,
            entity_type: source_entity_type,
            name: source_name,
        };
        let target_info = EntityInfo {
            id: target_id,
            npc_id: target_npc_id,
            entity_type: target_entity_type,
            name: target_name,
        };

        // Resolve effect name for matching
        let effect_name_str = crate::context::resolve(effect_name);

        // Find matching definitions (only those that trigger on EffectApplied)
        let matching_defs: Vec<_> = self
            .definitions
            .find_matching(effect_id as u64, Some(&effect_name_str))
            .into_iter()
            .filter(|def| def.trigger == EffectTriggerMode::EffectApplied)
            .filter(|def| self.matches_filters(def, source_info, target_info, encounter))
            .collect();

        let is_from_local = self.local_player_id == Some(source_id);
        let mut should_register = false;

        for def in matching_defs {
            let key = EffectInstanceKey {
                definition_id: def.id.clone(),
                target_entity_id: target_id,
            };

            let duration = def.duration_secs.map(Duration::from_secs_f32);

            if let Some(existing) = self.active_effects.get_mut(&key) {
                // Refresh existing effect if this action is in refresh_abilities
                let action_name_str = crate::context::resolve(action_name);
                let should_refresh = if def.refresh_abilities.is_empty() {
                    def.can_be_refreshed
                } else {
                    def.can_refresh_with(action_id as u64, Some(&action_name_str))
                };

                if should_refresh {
                    existing.refresh(timestamp, duration);
                    if let Some(c) = charges {
                        existing.set_stacks(c);
                    }
                    should_register = true;
                }
            } else {
                // Create new effect
                let display_text = def.display_text.clone().unwrap_or_else(|| def.name.clone());
                let mut effect = ActiveEffect::new(
                    def.id.clone(),
                    effect_id as u64,
                    def.name.clone(),
                    display_text,
                    source_id,
                    target_id,
                    target_name,
                    is_from_local,
                    timestamp,
                    duration,
                    def.effective_color(),
                    def.category,
                    def.show_on_raid_frames,
                    def.show_on_effects_overlay,
                    &def.audio,
                );

                if let Some(c) = charges {
                    effect.set_stacks(c);
                }

                self.active_effects.insert(key, effect);
                should_register = true;
            }
        }

        // Queue target for raid frame registration only when effect was created or refreshed.
        if should_register && is_from_local && matches!(target_entity_type, EntityType::Player | EntityType::Companion) {
            self.new_targets.push(NewTargetInfo {
                entity_id: target_id,
                name: target_name,
            });
        }
    }

    /// Refresh any tracked effects on this target that have this action in their refresh_abilities.
    fn refresh_effects_by_action(
        &mut self,
        action_id: i64,
        action_name: IStr,
        target_id: i64,
        target_name: IStr,
        target_entity_type: &EntityType,
        timestamp: NaiveDateTime,
    ) {
        // Find all definitions that have this action in their refresh_abilities
        let action_name_str = crate::context::resolve(action_name);
        let refreshable_defs: Vec<_> = self.definitions
            .enabled()
            .filter(|def| def.can_refresh_with(action_id as u64, Some(&action_name_str)))
            .map(|def| (def.id.clone(), def.duration_secs.map(Duration::from_secs_f32)))
            .collect();

        let mut did_refresh = false;
        for (def_id, duration) in refreshable_defs {
            let key = EffectInstanceKey {
                definition_id: def_id,
                target_entity_id: target_id,
            };

            if let Some(effect) = self.active_effects.get_mut(&key) {
                effect.refresh(timestamp, duration);
                did_refresh = true;
            }
        }

        // Push to new_targets if we actually refreshed an effect
        if did_refresh && matches!(target_entity_type, EntityType::Player | EntityType::Companion) {
            self.new_targets.push(NewTargetInfo {
                entity_id: target_id,
                name: target_name,
            });
        }
    }

    /// Handle effect removal signal
    fn handle_effect_removed(
        &mut self,
        effect_id: i64,
        effect_name: IStr,
        source_id: i64,
        target_id: i64,
        target_name: IStr,
        timestamp: NaiveDateTime,
    ) {
        self.current_game_time = Some(timestamp);

        // Skip when processing historical data
        if !self.live_mode {
            return;
        }

        // Resolve effect name for matching
        let effect_name_str = crate::context::resolve(effect_name);

        let matching_defs: Vec<_> = self
            .definitions
            .find_matching(effect_id as u64, Some(&effect_name_str))
            .into_iter()
            .collect();

        let is_from_local = self.local_player_id == Some(source_id);

        for def in matching_defs {
            let key = EffectInstanceKey {
                definition_id: def.id.clone(),
                target_entity_id: target_id,
            };

            match def.trigger {
                EffectTriggerMode::EffectApplied => {
                    // Mark existing effect as removed (normal behavior)
                    if let Some(effect) = self.active_effects.get_mut(&key) {
                        effect.mark_removed();
                    }
                }
                EffectTriggerMode::EffectRemoved => {
                    // Create new effect when the game effect is removed (cooldown tracking)
                    let duration = def.duration_secs.map(Duration::from_secs_f32);
                    let display_text = def.display_text.clone().unwrap_or_else(|| def.name.clone());
                    let effect = ActiveEffect::new(
                        def.id.clone(),
                        effect_id as u64,
                        def.name.clone(),
                        display_text,
                        source_id,
                        target_id,
                        target_name,
                        is_from_local,
                        timestamp,
                        duration,
                        def.effective_color(),
                        def.category,
                        def.show_on_raid_frames,
                        def.show_on_effects_overlay,
                        &def.audio,
                    );
                    self.active_effects.insert(key, effect);
                }
            }
        }
    }

    /// Handle charges changed signal
    fn handle_charges_changed(
        &mut self,
        effect_id: i64,
        effect_name: IStr,
        action_id: i64,
        action_name: IStr,
        target_id: i64,
        timestamp: NaiveDateTime,
        charges: u8,
    ) {
        self.current_game_time = Some(timestamp);

        // Find matching definitions (by ID or name)
        let effect_name_str = crate::context::resolve(effect_name);
        let matching_defs: Vec<_> = self
            .definitions
            .find_matching(effect_id as u64, Some(&effect_name_str))
            .into_iter()
            .collect();

        let action_name_str = crate::context::resolve(action_name);

        for def in matching_defs {
            let key = EffectInstanceKey {
                definition_id: def.id.clone(),
                target_entity_id: target_id,
            };

            if let Some(effect) = self.active_effects.get_mut(&key) {
                effect.set_stacks(charges);

                // Check if this action should refresh the effect
                let should_refresh = if def.refresh_abilities.is_empty() {
                    def.can_be_refreshed
                } else {
                    def.can_refresh_with(action_id as u64, Some(&action_name_str))
                };

                if should_refresh {
                    let duration = def.duration_secs.map(Duration::from_secs_f32);
                    effect.refresh(timestamp, duration);
                }
            }
        }
    }

    /// Handle entity death - clear effects unless persist_past_death
    fn handle_entity_death(&mut self, entity_id: i64) {
        // Get definition IDs that should persist past death
        let persist_ids: std::collections::HashSet<_> = self
            .definitions
            .enabled()
            .filter(|def| def.persist_past_death)
            .map(|def| def.id.as_str())
            .collect();

        // Mark non-persisting effects on dead entity as removed
        for (key, effect) in self.active_effects.iter_mut() {
            if effect.target_entity_id == entity_id && !persist_ids.contains(key.definition_id.as_str()) {
                effect.mark_removed();
            }
        }
    }

    /// Handle combat end - optionally clear combat-only effects
    fn handle_combat_ended(&mut self) {
        // Mark effects that don't track outside combat as removed
        let outside_combat_ids: std::collections::HashSet<_> = self
            .definitions
            .enabled()
            .filter(|def| def.track_outside_combat)
            .map(|def| def.id.as_str())
            .collect();

        for (key, effect) in self.active_effects.iter_mut() {
            if !outside_combat_ids.contains(key.definition_id.as_str()) {
                effect.mark_removed();
            }
        }
    }

    /// Handle area change (zone transition) - clear all active effects
    fn handle_area_change(&mut self) {
        for (_key, effect) in self.active_effects.iter_mut() {
            effect.mark_removed();
        }
        self.current_targets.clear();
    }

    /// Check if an effect matches source/target filters
    fn matches_filters(
        &self,
        def: &EffectDefinition,
        source: EntityInfo,
        target: EntityInfo,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) -> bool {
        // Get boss entity IDs from encounter's HP tracking (entities with tracked HP are bosses)
        let boss_ids: HashSet<i64> = encounter
            .map(|e| e.hp_by_entity.keys().copied().collect())
            .unwrap_or_default();

        def.source.matches(source.id, source.entity_type, source.name, source.npc_id, self.local_player_id, &boss_ids)
            && def.target.matches(target.id, target.entity_type, target.name, target.npc_id, self.local_player_id, &boss_ids)
    }
}

impl SignalHandler for EffectTracker {
    fn handle_signal(&mut self, signal: &GameSignal, encounter: Option<&crate::encounter::CombatEncounter>) {
        match signal {
            GameSignal::EffectApplied {
                effect_id,
                effect_name,
                action_id,
                action_name,
                source_id,
                source_name,
                source_entity_type,
                source_npc_id,
                target_id,
                target_name,
                target_entity_type,
                target_npc_id,
                timestamp,
                charges,
            } => {
                self.handle_effect_applied(
                    *effect_id,
                    *effect_name,
                    *action_id,
                    *action_name,
                    *source_id,
                    *source_name,
                    *source_entity_type,
                    *source_npc_id,
                    *target_id,
                    *target_name,
                    *target_entity_type,
                    *target_npc_id,
                    *timestamp,
                    *charges,
                    encounter,
                );
            }
            GameSignal::EffectRemoved {
                effect_id,
                effect_name,
                source_id,
                target_id,
                target_name,
                timestamp,
                ..
            } => {
                self.handle_effect_removed(*effect_id, *effect_name, *source_id, *target_id, *target_name, *timestamp);
            }
            GameSignal::EffectChargesChanged {
                effect_id,
                effect_name,
                action_id,
                action_name,
                target_id,
                timestamp,
                charges,
            } => {
                self.handle_charges_changed(*effect_id, *effect_name, *action_id, *action_name, *target_id, *timestamp, *charges);
            }
            GameSignal::EntityDeath { entity_id, .. } => {
                self.handle_entity_death(*entity_id);
            }
            GameSignal::CombatEnded { .. } => {
                self.handle_combat_ended();
            }
            GameSignal::AreaEntered { .. } => {
                self.handle_area_change();
            }
            GameSignal::PlayerInitialized { entity_id, .. } => {
                self.set_local_player(*entity_id);
            }
            GameSignal::AbilityActivated {
                ability_id,
                ability_name,
                source_id,
                target_id,
                target_name,
                target_entity_type,
                timestamp,
                ..
            } => {
                // Only process abilities from local player
                if self.local_player_id == Some(*source_id) {
                    // Resolve target: if target is self/empty, use tracked target from TargetSet
                    let (resolved_id, resolved_name, resolved_type) =
                        if *target_id == *source_id || *target_id == 0 {
                            if let Some(tracked) = self.current_targets.get(source_id).cloned() {
                                (tracked.entity_id, tracked.name, tracked.entity_type)
                            } else {
                                (*source_id, *target_name, *target_entity_type)
                            }
                        } else {
                            (*target_id, *target_name, *target_entity_type)
                        };

                    self.refresh_effects_by_action(
                        *ability_id,
                        *ability_name,
                        resolved_id,
                        resolved_name,
                        &resolved_type,
                        *timestamp,
                    );
                }
            }
            GameSignal::TargetChanged {
                source_id,
                target_id,
                target_name,
                target_entity_type,
                ..
            } => {
                self.current_targets.insert(*source_id, TrackedTarget {
                    entity_id: *target_id,
                    name: *target_name,
                    entity_type: *target_entity_type,
                });
            }
            GameSignal::TargetCleared { source_id, .. } => {
                self.current_targets.remove(source_id);
            }
            // Boss entity IDs are now read from encounter.hp_by_entity in matches_filters
            _ => {}
        }
    }
}
