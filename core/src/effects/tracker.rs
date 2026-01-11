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
use crate::dsl::EntityDefinition;
use crate::dsl::EntityFilterMatching;
use crate::encounter::CombatEncounter;
use crate::signal_processor::{GameSignal, SignalHandler};

use super::{ActiveEffect, DisplayTarget, EffectDefinition, EffectKey};

/// Get the entity roster from the current encounter, or empty slice if none.
fn get_entities(encounter: Option<&CombatEncounter>) -> &[EntityDefinition] {
    static EMPTY: &[EntityDefinition] = &[];
    encounter
        .and_then(|e| e.active_boss_idx())
        .map(|idx| {
            encounter.unwrap().boss_definitions()[idx]
                .entities
                .as_slice()
        })
        .unwrap_or(EMPTY)
}

/// Get the set of boss entity IDs from the current encounter.
fn get_boss_ids(encounter: Option<&CombatEncounter>) -> HashSet<i64> {
    encounter
        .map(|e| {
            e.npcs
                .values()
                .filter_map(|npc| npc.is_boss.then_some(npc.log_id))
                .collect()
        })
        .unwrap_or_default()
}

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
    pub fn add_definitions(
        &mut self,
        definitions: Vec<EffectDefinition>,
        overwrite: bool,
    ) -> Vec<String> {
        let mut duplicates = Vec::new();
        for def in definitions {
            // Warn about effects that will never match anything
            if !def.matches_effect(0, None)
                && !def.is_ability_cast_trigger()
                && def.refresh_abilities.is_empty()
            {
                eprintln!(
                    "[EFFECT WARNING] Effect '{}' has no effect selectors or abilities - it may never match anything!",
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
    pub fn find_matching(
        &self,
        effect_id: u64,
        effect_name: Option<&str>,
    ) -> Vec<&EffectDefinition> {
        self.effects
            .values()
            .filter(|def| def.enabled && def.matches_effect(effect_id, effect_name))
            .collect()
    }

    /// Find effect definitions that match an ability cast trigger
    pub fn find_ability_cast_matching(
        &self,
        ability_id: u64,
        ability_name: Option<&str>,
    ) -> Vec<&EffectDefinition> {
        self.effects
            .values()
            .filter(|def| def.enabled && def.matches_ability_cast(ability_id, ability_name))
            .collect()
    }

    /// Get all enabled effect definitions
    pub fn enabled(&self) -> impl Iterator<Item = &EffectDefinition> {
        self.effects.values().filter(|def| def.enabled)
    }
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

/// Pending AoE refresh waiting for damage correlation
#[derive(Debug, Clone)]
struct PendingAoeRefresh {
    /// The ability that was activated
    ability_id: i64,
    /// When the ability was activated
    timestamp: NaiveDateTime,
}

/// State for collecting AoE damage targets after finding anchor
#[derive(Debug, Clone)]
struct AoeRefreshCollecting {
    /// The ability being tracked
    ability_id: i64,
    /// Anchor timestamp (when primary target was hit)
    anchor_timestamp: NaiveDateTime,
    /// Targets collected so far (within ±10ms window)
    targets: Vec<i64>,
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
    active_effects: HashMap<EffectKey, ActiveEffect>,

    /// Current game time (latest timestamp from signals)
    current_game_time: Option<NaiveDateTime>,

    /// Whether we're in live mode (tracking effects) vs historical mode (skip)
    /// Defaults to false - must be enabled after initial file load
    live_mode: bool,

    /// Local player ID (set from session cache during signal dispatch)
    local_player_id: Option<i64>,

    /// Player's alacrity percentage (e.g., 15.4 for 15.4%)
    /// Used to adjust durations for effects with is_affected_by_alacrity = true
    alacrity_percent: f32,

    /// Queue of targets that received effects from local player.
    /// Drained by the service to attempt registration in the raid registry.
    /// The registry itself handles duplicate rejection.
    new_targets: Vec<NewTargetInfo>,

    /// Pending AoE refresh waiting for damage correlation.
    /// Set when AbilityActivate happens for a refresh ability with [=] target.
    pending_aoe_refresh: Option<PendingAoeRefresh>,

    /// State when we've found the anchor (primary target damage) and are
    /// collecting other targets hit within ±10ms.
    aoe_collecting: Option<AoeRefreshCollecting>,
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
            current_game_time: None,
            live_mode: false, // Start in historical mode
            local_player_id: None,
            alacrity_percent: 0.0,
            new_targets: Vec::new(),
            pending_aoe_refresh: None,
            aoe_collecting: None,
        }
    }

    /// Set the player's alacrity percentage for duration calculations
    pub fn set_alacrity(&mut self, alacrity_percent: f32) {
        self.alacrity_percent = alacrity_percent;
    }

    /// Calculate effective duration for a definition, applying alacrity if configured
    /// For cooldowns with cooldown_ready_secs, adds the ready period to the total duration
    fn effective_duration(&self, def: &super::EffectDefinition) -> Option<Duration> {
        def.duration_secs.map(|base_secs| {
            let adjusted = if def.is_affected_by_alacrity && self.alacrity_percent > 0.0 {
                base_secs / (1.0 + self.alacrity_percent / 100.0)
            } else {
                base_secs
            };
            // Add cooldown_ready_secs to extend the total duration for the ready state
            let total = adjusted + def.cooldown_ready_secs;
            Duration::from_secs_f32(total)
        })
    }

    /// Handle signals with explicit local player ID from session cache
    pub fn handle_signals_with_player(
        &mut self,
        signals: &[GameSignal],
        encounter: Option<&crate::encounter::CombatEncounter>,
        local_player_id: Option<i64>,
    ) {
        self.local_player_id = local_player_id;
        self.handle_signals(signals, encounter);
    }

    /// Enable live mode (start tracking effects)
    /// Call this after initial file load is complete
    pub fn set_live_mode(&mut self, enabled: bool) {
        self.live_mode = enabled;
    }

    /// Update definitions (e.g., after config reload)
    /// Also updates display properties on any active effects that match
    pub fn set_definitions(&mut self, definitions: DefinitionSet) {
        // Update active effects with new display properties from their definitions
        for effect in self.active_effects.values_mut() {
            if let Some(def) = definitions.effects.get(&effect.definition_id) {
                effect.show_on_raid_frames = def.show_on_raid_frames;
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

    // ─────────────────────────────────────────────────────────────────────────────
    // Categorized Output Methods (by DisplayTarget)
    // ─────────────────────────────────────────────────────────────────────────────

    /// Get effects destined for raid frames overlay (HOTs on group members)
    /// Uses the `show_on_raid_frames` flag, not `display_target`
    pub fn raid_frame_effects(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects
            .values()
            .filter(|e| e.show_on_raid_frames && e.removed_at.is_none())
    }

    /// Get effects destined for personal buffs bar
    pub fn personal_buff_effects(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects
            .values()
            .filter(|e| e.display_target == DisplayTarget::PersonalBuffs && e.removed_at.is_none())
    }

    /// Get effects destined for personal debuffs bar
    pub fn personal_debuff_effects(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects.values().filter(|e| {
            e.display_target == DisplayTarget::PersonalDebuffs && e.removed_at.is_none()
        })
    }

    /// Get effects destined for cooldown tracker
    pub fn cooldown_effects(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects
            .values()
            .filter(|e| e.display_target == DisplayTarget::Cooldowns && e.removed_at.is_none())
    }

    /// Get effects destined for DOT tracker, grouped by target entity
    pub fn dot_tracker_effects(&self) -> std::collections::HashMap<i64, Vec<&ActiveEffect>> {
        let mut by_target: std::collections::HashMap<i64, Vec<&ActiveEffect>> =
            std::collections::HashMap::new();
        for effect in self.active_effects.values() {
            if effect.removed_at.is_none() && effect.display_target == DisplayTarget::DotTracker {
                by_target
                    .entry(effect.target_entity_id)
                    .or_default()
                    .push(effect);
            }
        }
        by_target
    }

    /// Get effects destined for generic effects overlay (legacy)
    pub fn effects_overlay_effects(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects
            .values()
            .filter(|e| e.display_target == DisplayTarget::EffectsOverlay && e.removed_at.is_none())
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
        self.active_effects
            .retain(|_, effect| !effect.should_remove());
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
        self.active_effects
            .retain(|_, effect| effect.is_active(timestamp));

        // Skip effect tracking when processing historical data (initial file load)
        if !self.live_mode {
            return;
        }

        let local_player_id = self.local_player_id;

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
        let all_matches = self
            .definitions
            .find_matching(effect_id as u64, Some(effect_name_str));

        let matching_defs: Vec<_> = all_matches
            .into_iter()
            .filter(|def| def.is_effect_applied_trigger())
            .filter(|def| self.matches_filters(def, source_info, target_info, encounter))
            .collect();

        let is_from_local = local_player_id == Some(source_id);
        let mut should_register = false;

        for def in matching_defs {
            let key = EffectKey {
                definition_id: def.id.clone(),
                target_entity_id: target_id,
            };

            let duration = self.effective_duration(def);

            if let Some(existing) = self.active_effects.get_mut(&key) {
                // Always refresh when the same effect is applied again.
                // The game is telling us the effect was (re)applied, so reset the timer.
                // Note: refresh_abilities is for DIFFERENT abilities that can extend
                // an effect (e.g., Surgical Probe extending Kolto Probe), which is
                // handled in refresh_effects_by_action() via AbilityActivated signals.
                existing.refresh(timestamp, duration);
                if let Some(c) = charges {
                    existing.set_stacks(c);
                }
                should_register = true;
            } else {
                // Create new effect
                let display_text = def.display_text().to_string();
                let icon_ability_id = def.icon_ability_id.unwrap_or(effect_id as u64);
                let mut effect = ActiveEffect::new(
                    def.id.clone(),
                    effect_id as u64,
                    def.name.clone(),
                    display_text,
                    source_id,
                    source_name,
                    target_id,
                    target_name,
                    is_from_local,
                    timestamp,
                    duration,
                    def.effective_color(),
                    def.category,
                    def.display_target,
                    icon_ability_id,
                    def.show_on_raid_frames,
                    def.show_at_secs,
                    def.show_icon,
                    def.cooldown_ready_secs,
                    &def.audio,
                );

                if let Some(c) = charges {
                    effect.set_stacks(c);
                }

                // Debug logging for effect creation
                eprintln!(
                    "[DOT_DEBUG] Created effect '{}' display_target={:?} icon_ability_id={}",
                    effect.name, effect.display_target, effect.icon_ability_id
                );

                self.active_effects.insert(key, effect);
                should_register = true;
            }
        }

        // Queue target for raid frame registration only when effect was created or refreshed.
        if should_register
            && is_from_local
            && matches!(
                target_entity_type,
                EntityType::Player | EntityType::Companion
            )
        {
            self.new_targets.push(NewTargetInfo {
                entity_id: target_id,
                name: target_name,
            });
        }
    }

    /// Refresh any tracked effects that have this action in their refresh_abilities.
    fn refresh_effects_by_action(
        &mut self,
        action_id: i64,
        action_name: IStr,
        target_id: i64,
        _target_name: IStr,
        _target_entity_type: &EntityType,
        timestamp: NaiveDateTime,
    ) {
        // For AoE abilities (target_id == 0), we can't reliably detect which targets
        // were actually hit. Damage events from ongoing DOTs on other targets look
        // identical to first ticks from the new cast. Rather than risk false refreshes
        // on targets that weren't in the AoE, we skip refresh detection entirely.
        // New applications are still tracked via ApplyEffect signals.
        if target_id == 0 {
            return;
        }

        // Single-target case: refresh effect on specific target
        let action_name_str = crate::context::resolve(action_name);
        let refreshable_def_ids: Vec<_> = self
            .definitions
            .enabled()
            .filter(|def| def.can_refresh_with(action_id as u64, Some(action_name_str)))
            .map(|def| {
                (
                    def.id.clone(),
                    self.effective_duration(def),
                )
            })
            .collect();

        for (def_id, duration) in refreshable_def_ids {
            let key = EffectKey {
                definition_id: def_id,
                target_entity_id: target_id,
            };
            if let Some(effect) = self.active_effects.get_mut(&key) {
                effect.refresh(timestamp, duration);
            }
        }
    }

    /// Check if an ability has any refresh definitions (used for AoE detection)
    fn has_refresh_definitions(&self, ability_id: i64) -> bool {
        let ability_name_str: Option<&str> = None; // We only check by ID
        self.definitions
            .enabled()
            .any(|def| def.can_refresh_with(ability_id as u64, ability_name_str))
    }

    /// Set up pending AoE refresh state when AbilityActivate has [=] target
    fn setup_pending_aoe_refresh(&mut self, ability_id: i64, timestamp: NaiveDateTime) {
        // Only track if this ability can refresh effects
        if self.has_refresh_definitions(ability_id) {
            self.pending_aoe_refresh = Some(PendingAoeRefresh {
                ability_id,
                timestamp,
            });
            // Clear any stale collecting state
            self.aoe_collecting = None;
        }
    }

    /// Handle damage event for AoE refresh correlation.
    /// Returns true if the damage was processed as part of AoE refresh detection.
    fn handle_damage_for_aoe_refresh(
        &mut self,
        ability_id: i64,
        target_id: i64,
        timestamp: NaiveDateTime,
        current_target_id: Option<i64>,
    ) {
        // Timeout for pending state (2 seconds - longer than any grenade travel time)
        const PENDING_TIMEOUT_MS: i64 = 2000;
        // Window for collecting additional targets after anchor (±10ms)
        const COLLECT_WINDOW_MS: i64 = 10;

        // Check if we're in collecting state and this damage is within window
        if let Some(ref mut collecting) = self.aoe_collecting
            && collecting.ability_id == ability_id
        {
            let diff_ms = (timestamp - collecting.anchor_timestamp)
                .num_milliseconds()
                .abs();
            if diff_ms <= COLLECT_WINDOW_MS {
                // Within window - add target if not already collected
                if !collecting.targets.contains(&target_id) {
                    collecting.targets.push(target_id);
                }
                return;
            } else {
                // Outside window - finalize and refresh all collected targets
                self.finalize_aoe_refresh();
            }
        }

        // Check if we have a pending AoE refresh for this ability
        let Some(ref pending) = self.pending_aoe_refresh else {
            return;
        };

        if pending.ability_id != ability_id {
            return;
        }

        // Check if pending has timed out
        let elapsed_ms = (timestamp - pending.timestamp).num_milliseconds();
        if elapsed_ms > PENDING_TIMEOUT_MS {
            self.pending_aoe_refresh = None;
            return;
        }

        // Check if this damage is on the primary target (current target)
        let Some(primary_target) = current_target_id else {
            return;
        };

        if target_id == primary_target {
            // This is our anchor! Start collecting targets
            self.aoe_collecting = Some(AoeRefreshCollecting {
                ability_id,
                anchor_timestamp: timestamp,
                targets: vec![target_id],
            });
            self.pending_aoe_refresh = None;
        }
    }

    /// Finalize AoE refresh - refresh effects on all collected targets
    fn finalize_aoe_refresh(&mut self) {
        let Some(collecting) = self.aoe_collecting.take() else {
            return;
        };

        // Get refresh definitions for this ability
        let refreshable_def_ids: Vec<_> = self
            .definitions
            .enabled()
            .filter(|def| def.can_refresh_with(collecting.ability_id as u64, None))
            .map(|def| {
                (
                    def.id.clone(),
                    self.effective_duration(def),
                )
            })
            .collect();

        // Refresh effects on all collected targets
        for target_id in collecting.targets {
            for (def_id, duration) in &refreshable_def_ids {
                let key = EffectKey {
                    definition_id: def_id.clone(),
                    target_entity_id: target_id,
                };
                if let Some(effect) = self.active_effects.get_mut(&key) {
                    effect.refresh(collecting.anchor_timestamp, *duration);
                }
            }
        }
    }

    /// Handle ability cast for AbilityCast-triggered effects (procs, cooldowns)
    fn handle_ability_cast(
        &mut self,
        ability_id: i64,
        ability_name: IStr,
        source_id: i64,
        source_name: IStr,
        source_entity_type: EntityType,
        source_npc_id: i64,
        target_id: i64,
        target_name: IStr,
        _target_entity_type: EntityType,
        timestamp: NaiveDateTime,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
        // Skip when not in live mode
        if !self.live_mode {
            return;
        }

        let local_player_id = self.local_player_id;
        let ability_name_str = crate::context::resolve(ability_name);

        // Find definitions with AbilityCast triggers that match this ability
        let matching_defs: Vec<_> = self
            .definitions
            .find_ability_cast_matching(ability_id as u64, Some(ability_name_str))
            .into_iter()
            .collect();

        if matching_defs.is_empty() {
            return;
        }

        // Build entity info for source filter matching
        let source_info = EntityInfo {
            id: source_id,
            npc_id: source_npc_id,
            entity_type: source_entity_type,
            name: source_name,
        };

        // Get boss IDs for filter matching
        let boss_ids = get_boss_ids(encounter);

        let is_from_local = local_player_id == Some(source_id);

        let entities = get_entities(encounter);
        for def in matching_defs {
            // Check source filter from the trigger
            let source_filter = def.source_filter();
            if !source_filter.is_any()
                && !source_filter.matches(
                    entities,
                    source_info.id,
                    source_info.entity_type,
                    source_info.name,
                    source_info.npc_id,
                    local_player_id,
                    &boss_ids,
                )
            {
                continue;
            }

            // For procs, the effect is typically shown on the caster (source)
            // Use target from definition's target filter, or default to source
            let effect_target_id = if def.target_filter().is_local_player() {
                local_player_id.unwrap_or(source_id)
            } else {
                target_id
            };
            let effect_target_name = if effect_target_id == source_id {
                source_name
            } else {
                target_name
            };

            let key = EffectKey {
                definition_id: def.id.clone(),
                target_entity_id: effect_target_id,
            };

            let duration = self.effective_duration(def);

            if let Some(existing) = self.active_effects.get_mut(&key) {
                // Refresh existing effect (same trigger ability was cast again)
                existing.refresh(timestamp, duration);

                // Re-register target in raid registry if they were removed
                if existing.is_from_local_player {
                    self.new_targets.push(NewTargetInfo {
                        entity_id: effect_target_id,
                        name: effect_target_name,
                    });
                }
            } else {
                // Create new effect
                let display_text = def.display_text().to_string();
                let icon_ability_id = def.icon_ability_id.unwrap_or(ability_id as u64);
                let effect = ActiveEffect::new(
                    def.id.clone(),
                    ability_id as u64, // Use ability ID since this is ability-triggered
                    def.name.clone(),
                    display_text,
                    source_id,
                    source_name,
                    effect_target_id,
                    effect_target_name,
                    is_from_local,
                    timestamp,
                    duration,
                    def.effective_color(),
                    def.category,
                    def.display_target,
                    icon_ability_id,
                    def.show_on_raid_frames,
                    def.show_at_secs,
                    def.show_icon,
                    def.cooldown_ready_secs,
                    &def.audio,
                );
                self.active_effects.insert(key, effect);
            }
        }
    }

    /// Handle effect removal signal
    fn handle_effect_removed(
        &mut self,
        effect_id: i64,
        effect_name: IStr,
        source_id: i64,
        source_name: IStr,
        target_id: i64,
        target_name: IStr,
        timestamp: NaiveDateTime,
        _encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
        self.current_game_time = Some(timestamp);
        let local_player_id = self.local_player_id;

        // Skip when processing historical data
        if !self.live_mode {
            return;
        }

        // Resolve effect name for matching
        let effect_name_str = crate::context::resolve(effect_name);

        let matching_defs: Vec<_> = self
            .definitions
            .find_matching(effect_id as u64, Some(effect_name_str))
            .into_iter()
            .collect();

        let is_from_local = local_player_id == Some(source_id);

        for def in matching_defs {
            let key = EffectKey {
                definition_id: def.id.clone(),
                target_entity_id: target_id,
            };

            if def.is_effect_applied_trigger() {
                // Mark existing effect as removed (normal behavior)
                // Skip if ignore_effect_removed OR cooldowns (cooldowns always use timer-based expiry)
                let is_cooldown = def.display_target == DisplayTarget::Cooldowns;
                if !def.ignore_effect_removed
                    && !is_cooldown
                    && let Some(effect) = self.active_effects.get_mut(&key)
                {
                    // Skip removal if the effect was refreshed recently (within 1 second).
                    // When a DoT is reapplied, the game sends ApplyEffect (new) then
                    // RemoveEffect (old) - sometimes in the same batch, sometimes with
                    // a slight delay. We don't want to remove the effect we just refreshed.
                    let since_refresh = timestamp
                        .signed_duration_since(effect.last_refreshed_at)
                        .num_milliseconds();
                    if since_refresh > 1000 {
                        effect.mark_removed();
                    }
                }
            } else if def.is_effect_removed_trigger() {
                // Create new effect when the game effect is removed (cooldown tracking)
                let duration = self.effective_duration(def);
                let display_text = def.display_text().to_string();
                let icon_ability_id = def.icon_ability_id.unwrap_or(effect_id as u64);
                let effect = ActiveEffect::new(
                    def.id.clone(),
                    effect_id as u64,
                    def.name.clone(),
                    display_text,
                    source_id,
                    source_name,
                    target_id,
                    target_name,
                    is_from_local,
                    timestamp,
                    duration,
                    def.effective_color(),
                    def.category,
                    def.display_target,
                    icon_ability_id,
                    def.show_on_raid_frames,
                    def.show_at_secs,
                    def.show_icon,
                    def.cooldown_ready_secs,
                    &def.audio,
                );
                self.active_effects.insert(key, effect);
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
            .find_matching(effect_id as u64, Some(effect_name_str))
            .into_iter()
            .collect();

        let action_name_str = crate::context::resolve(action_name);

        for def in matching_defs {
            let key = EffectKey {
                definition_id: def.id.clone(),
                target_entity_id: target_id,
            };

            // Calculate duration before borrowing active_effects mutably
            let duration = if def.is_refreshed_on_modify {
                self.effective_duration(def)
            } else {
                None
            };

            if let Some(effect) = self.active_effects.get_mut(&key) {
                effect.set_stacks(charges);

                // Refresh duration on ModifyCharges if is_refreshed_on_modify is set
                if let Some(dur) = duration {
                    effect.refresh(timestamp, Some(dur));
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
            if effect.target_entity_id == entity_id
                && !persist_ids.contains(key.definition_id.as_str())
            {
                effect.mark_removed();
            }
        }
    }

    /// Handle combat end - optionally clear combat-only effects
    fn handle_combat_ended(&mut self) {
        // Clear pending AoE refresh state
        self.pending_aoe_refresh = None;
        self.aoe_collecting = None;

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
        // Clear pending AoE refresh state
        self.pending_aoe_refresh = None;
        self.aoe_collecting = None;

        for (_key, effect) in self.active_effects.iter_mut() {
            effect.mark_removed();
        }
    }

    /// Check if an effect matches source/target filters
    fn matches_filters(
        &self,
        def: &EffectDefinition,
        source: EntityInfo,
        target: EntityInfo,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) -> bool {
        // Get local player ID from self, boss entity IDs from encounter
        let local_player_id = self.local_player_id;
        let boss_ids = get_boss_ids(encounter);

        let entities = get_entities(encounter);

        def.source_filter().matches(
            entities,
            source.id,
            source.entity_type,
            source.name,
            source.npc_id,
            local_player_id,
            &boss_ids,
        ) && def.target_filter().matches(
            entities,
            target.id,
            target.entity_type,
            target.name,
            target.npc_id,
            local_player_id,
            &boss_ids,
        )
    }
}

impl SignalHandler for EffectTracker {
    fn handle_signal(
        &mut self,
        signal: &GameSignal,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
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
                source_name,
                target_id,
                target_name,
                timestamp,
                ..
            } => {
                self.handle_effect_removed(
                    *effect_id,
                    *effect_name,
                    *source_id,
                    *source_name,
                    *target_id,
                    *target_name,
                    *timestamp,
                    encounter,
                );
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
                self.handle_charges_changed(
                    *effect_id,
                    *effect_name,
                    *action_id,
                    *action_name,
                    *target_id,
                    *timestamp,
                    *charges,
                );
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
            GameSignal::PlayerInitialized { .. } => {
                // Local player ID is now read from encounter context
            }
            GameSignal::AbilityActivated {
                ability_id,
                ability_name,
                source_id,
                source_name,
                source_entity_type,
                source_npc_id,
                target_id,
                target_name,
                target_entity_type,
                timestamp,
                ..
            } => {
                self.current_game_time = Some(*timestamp);

                // Handle AbilityCast-triggered effects (procs, cooldowns)
                // This works for any source, not just local player
                self.handle_ability_cast(
                    *ability_id,
                    *ability_name,
                    *source_id,
                    *source_name,
                    *source_entity_type,
                    *source_npc_id,
                    *target_id,
                    *target_name,
                    *target_entity_type,
                    *timestamp,
                    encounter,
                );

                // Refresh existing effects (local player only)
                // Use explicit target if available, otherwise query encounter for current target
                let local_player_id = self.local_player_id;
                if local_player_id == Some(*source_id) {
                    let is_self_or_empty = *target_id == 0 || *target_id == *source_id;
                    let resolved_target = if is_self_or_empty {
                        // Query encounter for caster's current target
                        encounter.and_then(|e| e.get_current_target(*source_id))
                    } else {
                        Some(*target_id)
                    };

                    if let Some(refresh_target) = resolved_target {
                        self.refresh_effects_by_action(
                            *ability_id,
                            *ability_name,
                            refresh_target,
                            *target_name,
                            target_entity_type,
                            *timestamp,
                        );
                    }

                    // For AoE abilities ([=] target), set up pending state for damage correlation
                    // This allows us to detect and refresh effects on secondary targets too
                    if is_self_or_empty {
                        self.setup_pending_aoe_refresh(*ability_id, *timestamp);
                    }
                }
            }
            GameSignal::DamageTaken {
                ability_id,
                source_id,
                target_id,
                timestamp,
                ..
            } => {
                // Only process for local player's damage
                if self.local_player_id == Some(*source_id) && self.live_mode {
                    let current_target = encounter.and_then(|e| e.get_current_target(*source_id));
                    self.handle_damage_for_aoe_refresh(
                        *ability_id,
                        *target_id,
                        *timestamp,
                        current_target,
                    );
                }
            }
            // Boss entity IDs are now read from encounter.hp_by_entity in matches_filters
            _ => {}
        }
    }
}
