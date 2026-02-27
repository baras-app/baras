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
use crate::dsl::{EntityFilter, EntityFilterMatching};
use crate::encounter::CombatEncounter;
use crate::game_data::Discipline;
use crate::signal_processor::{GameSignal, SignalHandler};

use crate::timers::FiredAlert;

use super::{ActiveEffect, AlertTrigger, DisplayTarget, EffectDefinition, EffectKey, RefreshTrigger};

/// Grace period (ms) after the app's duration timer expires before hard-removing
/// an effect from the tracker. During this window, `refresh_abilities` can still
/// revive the effect — the timer is a heuristic and the in-game buff may outlast it.
/// An authoritative `EffectRemoved` signal always removes immediately, bypassing this.
const TIMER_EXPIRY_GRACE_MS: i64 = 8000;

/// Get the entity roster from the current encounter, or empty slice if none.
fn get_entities(encounter: Option<&CombatEncounter>) -> &[EntityDefinition] {
    static EMPTY: &[EntityDefinition] = &[];
    let Some(enc) = encounter else {
        return EMPTY;
    };
    let Some(idx) = enc.active_boss_idx() else {
        return EMPTY;
    };
    // Use get() to avoid panic if index is stale after boss definitions reload
    enc.boss_definitions()
        .get(idx)
        .map(|def| def.entities.as_slice())
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

/// Combined set of effect definitions with indexes for fast lookup
#[derive(Debug, Clone, Default)]
pub struct DefinitionSet {
    /// All effect definitions, keyed by definition ID
    pub effects: HashMap<String, EffectDefinition>,

    // ─── Indexes for O(1) lookup ─────────────────────────────────────────────
    /// Effect ID -> definition IDs (for EffectApplied/EffectRemoved triggers)
    effect_id_index: HashMap<u64, Vec<String>>,
    /// Ability ID -> definition IDs (for AbilityCast triggers)
    ability_id_index: HashMap<u64, Vec<String>>,
    /// Lowercase effect name -> definition IDs (for name-based effect matchers)
    effect_name_index: HashMap<String, Vec<String>>,
    /// Lowercase ability name -> definition IDs (for name-based ability matchers)
    ability_name_index: HashMap<String, Vec<String>>,
    /// Refresh ability ID -> definition IDs (for refresh_abilities matching)
    refresh_ability_id_index: HashMap<u64, Vec<String>>,
    /// Refresh ability name -> definition IDs (for refresh_abilities matching)
    refresh_ability_name_index: HashMap<String, Vec<String>>,
    /// Ability IDs that use AoE damage correlation for refresh detection.
    /// Derived from definitions where `is_aoe_refresh = true`.
    aoe_refresh_ability_ids: HashSet<u64>,
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
            if self.effects.contains_key(&def.id) {
                duplicates.push(def.id.clone());
                if !overwrite {
                    continue; // Skip duplicate - keep the first definition
                }
                // Overwrite mode: remove old index entries before replacing
                self.remove_from_indexes(&def.id);
            }
            self.add_to_indexes(&def);
            self.effects.insert(def.id.clone(), def);
        }
        duplicates
    }

    fn add_to_indexes(&mut self, def: &EffectDefinition) {
        use crate::dsl::Trigger;
        use baras_types::{AbilitySelector, EffectSelector};

        match &def.trigger {
            Trigger::EffectApplied { effects, .. } | Trigger::EffectRemoved { effects, .. } => {
                for selector in effects {
                    match selector {
                        EffectSelector::Id(id) => {
                            self.effect_id_index.entry(*id).or_default().push(def.id.clone());
                        }
                        EffectSelector::Name(name) => {
                            self.effect_name_index
                                .entry(name.to_lowercase())
                                .or_default()
                                .push(def.id.clone());
                        }
                    }
                }
            }
            Trigger::AbilityCast { abilities, .. }
            | Trigger::DamageTaken { abilities, .. }
            | Trigger::HealingTaken { abilities, .. } => {
                for selector in abilities {
                    match selector {
                        AbilitySelector::Id(id) => {
                            self.ability_id_index.entry(*id).or_default().push(def.id.clone());
                        }
                        AbilitySelector::Name(name) => {
                            self.ability_name_index
                                .entry(name.to_lowercase())
                                .or_default()
                                .push(def.id.clone());
                        }
                    }
                }
            }
            _ => {}
        }

        // Index refresh_abilities
        for refresh in &def.refresh_abilities {
            match refresh.ability() {
                AbilitySelector::Id(id) => {
                    self.refresh_ability_id_index.entry(*id).or_default().push(def.id.clone());
                    if def.is_aoe_refresh {
                        self.aoe_refresh_ability_ids.insert(*id);
                    }
                }
                AbilitySelector::Name(name) => {
                    self.refresh_ability_name_index
                        .entry(name.to_lowercase())
                        .or_default()
                        .push(def.id.clone());
                }
            }
        }
    }

    fn remove_from_indexes(&mut self, def_id: &str) {
        for entries in self.effect_id_index.values_mut() {
            entries.retain(|id| id != def_id);
        }
        for entries in self.ability_id_index.values_mut() {
            entries.retain(|id| id != def_id);
        }
        for entries in self.effect_name_index.values_mut() {
            entries.retain(|id| id != def_id);
        }
        for entries in self.ability_name_index.values_mut() {
            entries.retain(|id| id != def_id);
        }
        for entries in self.refresh_ability_id_index.values_mut() {
            entries.retain(|id| id != def_id);
        }
        for entries in self.refresh_ability_name_index.values_mut() {
            entries.retain(|id| id != def_id);
        }
        // Rebuild AoE set from remaining definitions
        self.aoe_refresh_ability_ids.clear();
        for def in self.effects.values() {
            if def.is_aoe_refresh {
                for refresh in &def.refresh_abilities {
                    if let baras_types::AbilitySelector::Id(id) = refresh.ability() {
                        self.aoe_refresh_ability_ids.insert(*id);
                    }
                }
            }
        }
    }

    /// Get an effect definition by ID
    pub fn get(&self, id: &str) -> Option<&EffectDefinition> {
        self.effects.get(id)
    }

    /// Find effect definitions matching a game effect ID or name (O(1) indexed lookup)
    pub fn find_matching(
        &self,
        effect_id: u64,
        effect_name: Option<&str>,
    ) -> Vec<&EffectDefinition> {
        let mut results = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();

        if let Some(def_ids) = self.effect_id_index.get(&effect_id) {
            for def_id in def_ids {
                if let Some(def) = self.effects.get(def_id) {
                    if def.enabled {
                        seen.insert(def_id);
                        results.push(def);
                    }
                }
            }
        }

        if let Some(name) = effect_name {
            if let Some(def_ids) = self.effect_name_index.get(&name.to_lowercase()) {
                for def_id in def_ids {
                    if seen.contains(def_id.as_str()) {
                        continue;
                    }
                    if let Some(def) = self.effects.get(def_id) {
                        if def.enabled {
                            seen.insert(def_id);
                            results.push(def);
                        }
                    }
                }
            }
        }

        results
    }

    /// Find effect definitions matching an ability cast trigger (O(1) indexed lookup)
    pub fn find_ability_cast_matching(
        &self,
        ability_id: u64,
        ability_name: Option<&str>,
    ) -> Vec<&EffectDefinition> {
        let mut results = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();

        if let Some(def_ids) = self.ability_id_index.get(&ability_id) {
            for def_id in def_ids {
                if let Some(def) = self.effects.get(def_id) {
                    if def.enabled {
                        seen.insert(def_id);
                        results.push(def);
                    }
                }
            }
        }

        if let Some(name) = ability_name {
            if let Some(def_ids) = self.ability_name_index.get(&name.to_lowercase()) {
                for def_id in def_ids {
                    if seen.contains(def_id.as_str()) {
                        continue;
                    }
                    if let Some(def) = self.effects.get(def_id) {
                        if def.enabled {
                            seen.insert(def_id);
                            results.push(def);
                        }
                    }
                }
            }
        }

        results
    }

    /// Find definitions that can be refreshed by an ability (O(1) indexed lookup)
    pub fn find_refreshable_by(&self, ability_id: u64, ability_name: Option<&str>) -> Vec<&EffectDefinition> {
        let mut results = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();

        if let Some(def_ids) = self.refresh_ability_id_index.get(&ability_id) {
            for def_id in def_ids {
                if let Some(def) = self.effects.get(def_id) {
                    if def.enabled {
                        seen.insert(def_id);
                        results.push(def);
                    }
                }
            }
        }

        if let Some(name) = ability_name {
            if let Some(def_ids) = self.refresh_ability_name_index.get(&name.to_lowercase()) {
                for def_id in def_ids {
                    if seen.contains(def_id.as_str()) {
                        continue;
                    }
                    if let Some(def) = self.effects.get(def_id) {
                        if def.enabled {
                            results.push(def);
                        }
                    }
                }
            }
        }

        results
    }

    /// Check if any definitions can be refreshed by an ability (O(1) indexed lookup)
    pub fn has_refreshable_by(&self, ability_id: u64) -> bool {
        self.refresh_ability_id_index
            .get(&ability_id)
            .map(|ids| ids.iter().any(|id| {
                self.effects.get(id).map(|def| def.enabled).unwrap_or(false)
            }))
            .unwrap_or(false)
    }

    /// Check if an ability ID uses AoE damage correlation for refresh detection
    pub fn is_aoe_refresh(&self, ability_id: u64) -> bool {
        self.aoe_refresh_ability_ids.contains(&ability_id)
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
    /// The primary target (resolved at cast time)
    primary_target: i64,
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

    /// Local player ID (set from session cache during signal dispatch)
    local_player_id: Option<i64>,

    /// Local player's current discipline (for discipline-scoped effects)
    local_player_discipline: Option<Discipline>,

    /// Player's alacrity percentage (e.g., 15.4 for 15.4%)
    /// Used to adjust durations for effects with is_affected_by_alacrity = true
    alacrity_percent: f32,

    /// Player's network latency in milliseconds
    /// Added to effect durations to compensate for network delay
    latency_ms: u16,

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

    /// Alerts fired by effect start/end triggers
    fired_alerts: Vec<FiredAlert>,

    /// Count of active (non-removed) effects for O(1) has_ticking_effects() check
    ticking_count: usize,

    /// Current target for each entity (source_id -> (target_id, target_name, entity_type))
    /// Used as fallback when encounter doesn't have target info (e.g., outside combat)
    current_targets: HashMap<i64, (i64, IStr, EntityType)>,

    /// Recent ability casts by local player: (ability_id, target_id) -> timestamp
    /// Used to validate DotTracker ApplyEffect signals and reject lingering effects
    recent_casts: HashMap<(u64, i64), NaiveDateTime>,
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
            local_player_id: None,
            local_player_discipline: None,
            alacrity_percent: 0.0,
            latency_ms: 0,
            new_targets: Vec::new(),
            pending_aoe_refresh: None,
            aoe_collecting: None,
            fired_alerts: Vec::new(),
            ticking_count: 0,
            current_targets: HashMap::new(),
            recent_casts: HashMap::new(),
        }
    }

    /// Take any fired alerts (drains the queue)
    pub fn take_fired_alerts(&mut self) -> Vec<FiredAlert> {
        std::mem::take(&mut self.fired_alerts)
    }

    /// Build a `FiredAlert` for an instant alert (no active effect created).
    ///
    /// If `alert_text` is set, the text overlay fires with that text.
    /// If `alert_text` is `None`, only audio fires (no text on screen) — the
    /// `text` field is still populated (with the definition name) for the audio
    /// TTS fallback, but `alert_text_enabled` is `false` so nothing is shown.
    fn build_instant_alert(def: &EffectDefinition, timestamp: NaiveDateTime) -> FiredAlert {
        let has_text = def.alert_text.is_some();
        let text = def
            .alert_text
            .clone()
            .unwrap_or_else(|| def.name.clone());
        FiredAlert {
            id: def.id.clone(),
            name: def.name.clone(),
            text,
            color: def.color,
            timestamp,
            alert_text_enabled: has_text,
            audio_enabled: def.audio.enabled,
            audio_file: def.audio.file.clone(),
        }
    }

    /// Set the player's alacrity percentage for duration calculations
    pub fn set_player_context(&mut self, player_id: i64, discipline_id: i64) {
        self.local_player_id = Some(player_id);
        self.local_player_discipline = Discipline::from_guid(discipline_id);
    }

    pub fn set_alacrity(&mut self, alacrity_percent: f32) {
        self.alacrity_percent = alacrity_percent;
    }

    /// Set the player's network latency for duration calculations
    pub fn set_latency(&mut self, latency_ms: u16) {
        self.latency_ms = latency_ms;
    }

    /// Calculate effective duration for a definition, applying alacrity and latency if configured
    /// For cooldowns with cooldown_ready_secs, adds the ready period to the total duration
    ///
    /// Formula: (base_duration / (1 + alacrity)) + latency + cooldown_ready_secs
    fn effective_duration(&self, def: &super::EffectDefinition) -> Option<Duration> {
        def.duration_secs.map(|base_secs| {
            // Apply alacrity reduction if enabled for this effect
            let adjusted = if def.is_affected_by_alacrity && self.alacrity_percent > 0.0 {
                base_secs / (1.0 + self.alacrity_percent / 100.0)
            } else {
                base_secs
            };
            // Add latency compensation for effects affected by alacrity (network-sensitive)
            let with_latency = if def.is_affected_by_alacrity && self.latency_ms > 0 {
                adjusted + (self.latency_ms as f32 / 1000.0)
            } else {
                adjusted
            };
            // Add cooldown_ready_secs to extend the total duration for the ready state
            let total = with_latency + def.cooldown_ready_secs;
            Duration::from_secs_f32(total)
        })
    }

    /// Check if any of the definition's refresh abilities were recently cast.
    ///
    /// For AoE definitions (`is_aoe_refresh`), checks if the ability was cast
    /// at ANY target recently (since we only track the primary target).
    /// For single-target abilities, checks the specific target.
    fn has_recent_refresh_cast(
        &self,
        def: &super::EffectDefinition,
        target_id: i64,
        timestamp: NaiveDateTime,
    ) -> bool {
        const RECENT_CAST_WINDOW_MS: i64 = 1500;

        def.refresh_abilities.iter().any(|refresh| {
            if let baras_types::AbilitySelector::Id(ability_id) = refresh.ability() {
                if def.is_aoe_refresh {
                    // AoE: check if ability was cast at ANY target recently
                    self.recent_casts.iter().any(|(&(aid, _), &ts)| {
                        aid == *ability_id && {
                            let elapsed = (timestamp - ts).num_milliseconds();
                            elapsed >= 0 && elapsed <= RECENT_CAST_WINDOW_MS
                        }
                    })
                } else {
                    // Single-target: check specific target
                    self.recent_casts
                        .get(&(*ability_id, target_id))
                        .is_some_and(|&ts| {
                            let elapsed = (timestamp - ts).num_milliseconds();
                            elapsed >= 0 && elapsed <= RECENT_CAST_WINDOW_MS
                        })
                }
            } else {
                false
            }
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

    /// Update definitions (e.g., after config reload)
    /// Also updates display properties on any active effects that match.
    /// Removes active effects whose definitions are now disabled or deleted.
    pub fn set_definitions(&mut self, definitions: DefinitionSet) {
        // Remove active effects whose definitions are now disabled or deleted
        self.active_effects.retain(|_, effect| {
            definitions
                .effects
                .get(&effect.definition_id)
                .map(|def| def.enabled)
                .unwrap_or(false) // Remove if definition doesn't exist
        });

        // Update active effects with new display properties from their definitions
        for effect in self.active_effects.values_mut() {
            if let Some(def) = definitions.effects.get(&effect.definition_id) {
                // Track if alert_on_expire is changing to true (to prevent unexpected alerts)
                let old_alert_on_expire = effect.alert_on_expire;
                let new_alert_on_expire = matches!(def.alert_on, AlertTrigger::OnExpire);

                // Display properties
                effect.name = def.name.clone();
                effect.display_text = def.display_text.clone().unwrap_or_else(|| def.name.clone());
                effect.color = def.effective_color();
                effect.display_target = def.display_target;
                effect.icon_ability_id = def.icon_ability_id.unwrap_or(effect.game_effect_id);
                effect.show_at_secs = def.show_at_secs;
                effect.show_icon = def.show_icon;
                effect.display_source = def.display_source;
                effect.cooldown_ready_secs = def.cooldown_ready_secs;

                // Alert properties
                effect.alert_text = def.alert_text.clone();
                effect.alert_on_expire = new_alert_on_expire;

                // If alert_on_expire just became true, mark as already fired to prevent
                // unexpected alerts on already-active effects
                if new_alert_on_expire && !old_alert_on_expire {
                    effect.on_end_alert_fired = true;
                }

                // Audio properties
                effect.countdown_start = def.audio.countdown_start;
                effect.countdown_voice =
                    def.audio.countdown_voice.clone().unwrap_or_default();
                effect.audio_file = def.audio.file.clone();
                effect.audio_offset = def.audio.offset;
                effect.audio_enabled = def.audio.enabled;
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
    /// O(1) using the ticking_count counter
    pub fn has_ticking_effects(&self) -> bool {
        self.ticking_count > 0
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
    pub fn raid_frame_effects(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects
            .values()
            .filter(|e| e.display_target == DisplayTarget::RaidFrames && e.removed_at.is_none() && !e.timer_expired)
    }

    /// Get effects destined for Effects A overlay
    pub fn effects_a(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects
            .values()
            .filter(|e| e.display_target == DisplayTarget::EffectsA && e.removed_at.is_none() && !e.timer_expired)
    }

    /// Get effects destined for Effects B overlay
    pub fn effects_b(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects
            .values()
            .filter(|e| e.display_target == DisplayTarget::EffectsB && e.removed_at.is_none() && !e.timer_expired)
    }

    /// Get effects destined for cooldown tracker
    pub fn cooldown_effects(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects
            .values()
            .filter(|e| e.display_target == DisplayTarget::Cooldowns && e.removed_at.is_none() && !e.timer_expired)
    }

    /// Get effects destined for DOT tracker, grouped by target entity
    pub fn dot_tracker_effects(&self) -> std::collections::HashMap<i64, Vec<&ActiveEffect>> {
        let mut by_target: std::collections::HashMap<i64, Vec<&ActiveEffect>> =
            std::collections::HashMap::new();
        for effect in self.active_effects.values() {
            if effect.removed_at.is_none() && !effect.timer_expired && effect.display_target == DisplayTarget::DotTracker {
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
            .filter(|e| e.display_target == DisplayTarget::EffectsOverlay && e.removed_at.is_none() && !e.timer_expired)
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

        // Collect effects that just ended (duration expired or removed by signal).
        // Include audio info so alerts fire reliably before GC.
        let mut ended_effects: Vec<(String, Option<String>, bool)> = Vec::new();

        for effect in self.active_effects.values_mut() {
            // Handle duration-expired effects.
            // Timer expiry is a heuristic — the in-game effect may outlast our estimate.
            // Instead of immediately removing, mark as timer_expired so the effect stays
            // in active_effects and can be revived by refresh_abilities.
            // After the grace period, hard-remove for GC.
            if effect.removed_at.is_none() && effect.has_duration_expired(current_time) {
                if !effect.timer_expired {
                    // First tick after timer expiry — mark as timer-expired
                    effect.timer_expired = true;
                    self.ticking_count = self.ticking_count.saturating_sub(1);
                }

                // After grace period, hard-remove (GC on next retain pass)
                if let Some(expires_at) = effect.expires_at {
                    let since_expiry_ms = current_time
                        .signed_duration_since(expires_at)
                        .num_milliseconds();
                    if since_expiry_ms > TIMER_EXPIRY_GRACE_MS {
                        effect.mark_removed();
                    }
                }
            }

            // Collect alert info for effects that just ended (any reason)
            if !effect.on_end_alert_fired
                && (effect.has_base_duration_ended() || effect.removed_at.is_some())
            {
                effect.on_end_alert_fired = true;
                let should_play_audio = effect.audio_enabled
                    && !effect.audio_played
                    && effect.audio_offset == 0
                    && effect.audio_file.is_some();
                if should_play_audio {
                    effect.audio_played = true;
                }
                ended_effects.push((
                    effect.definition_id.clone(),
                    effect.audio_file.clone(),
                    should_play_audio,
                ));
            }
        }

        // Fire OnExpire alerts (with audio for early removals)
        for (def_id, audio_file, audio_enabled) in ended_effects {
            if let Some(def) = self.definitions.effects.get(&def_id)
                && def.alert_on == AlertTrigger::OnExpire
                && let Some(text) = &def.alert_text
            {
                self.fired_alerts.push(FiredAlert {
                    id: def_id,
                    name: def.name.clone(),
                    text: text.clone(),
                    color: def.color,
                    timestamp: current_time,
                    alert_text_enabled: true,
                    audio_enabled,
                    audio_file,
                });
            }
        }

        // Remove effects that have been marked removed (immediate, no fade delay)
        self.active_effects
            .retain(|_, effect| effect.removed_at.is_none());

        // Clean up old recent_casts entries (older than 5 seconds)
        self.recent_casts
            .retain(|_, ts| (current_time - *ts).num_milliseconds() < 5000);
    }

    /// Handle effect application signal
    fn handle_effect_applied(
        &mut self,
        effect_id: i64,
        effect_name: IStr,
        _action_id: i64,
        _action_name: IStr,
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

        // Note: GC is handled by tick() - don't duplicate here to reduce work per signal

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
        let mut pending_alerts: Vec<FiredAlert> = Vec::new();

        for def in matching_defs {
            // Instant alerts: fire and skip — no ActiveEffect created
            if def.is_alert {
                pending_alerts.push(Self::build_instant_alert(def, timestamp));
                continue;
            }

            let key = EffectKey::new(&def.id, target_id);

            let duration = self.effective_duration(def);

            // Hard-coded exclusivity: when another player refreshes the same ability
            // (e.g., Kolto Shell, Trauma Probe), the game refreshes the original
            // caster's effect rather than creating a new one. If the local player's
            // version is already active, refresh it instead of creating a phantom
            // "_others" variant.
            let dominant_def_id = match def.id.as_str() {
                "kolto_shell_others" => Some("kolto_shell"),
                "trauma_probe_others" => Some("trauma_probe"),
                _ => None,
            };
            if let Some(dominant_id) = dominant_def_id {
                let dominant_key = EffectKey::new(dominant_id, target_id);
                if let Some(dominant) = self.active_effects.get_mut(&dominant_key) {
                    if dominant.removed_at.is_none() {
                        // The local player's effect exists — the other player just refreshed it.
                        // Update our effect's duration instead of creating a phantom.
                        dominant.refresh(timestamp, duration);
                        if let Some(c) = charges {
                            dominant.set_stacks(c);
                        }
                        // Register the target for raid frames even though the signal
                        // came from another player — the target is a known group member.
                        if target_entity_type == EntityType::Player {
                            self.new_targets.push(NewTargetInfo {
                                entity_id: target_id,
                                name: target_name,
                            });
                        }
                        continue;
                    }
                }
            }

            // Pre-compute DotTracker validation before mutable borrow of active_effects
            let dot_tracker_valid = def.display_target != DisplayTarget::DotTracker
                || self.has_recent_refresh_cast(def, target_id, timestamp);

            if let Some(existing) = self.active_effects.get_mut(&key) {
                // Skip duplicate log lines (same timestamp) to avoid corrupting timing
                if existing.last_refreshed_at == timestamp {
                    if let Some(c) = charges {
                        existing.set_stacks(c);
                    }
                    continue;
                }

                // For DotTracker, validate that a refresh_ability was cast within the
                // recent window. This prevents lingering effects (which reapply with the
                // same effect_id ~5 seconds after DOT expires) from incorrectly refreshing.
                if !dot_tracker_valid {
                    continue;
                }

                existing.refresh(timestamp, duration);
                if let Some(c) = charges {
                    existing.set_stacks(c);
                }
                should_register = true;

                // Collect alert for effect refresh if configured
                if def.alert_on == AlertTrigger::OnApply
                    && let Some(text) = &def.alert_text
                {
                    pending_alerts.push(FiredAlert {
                        id: def.id.clone(),
                        name: def.name.clone(),
                        text: text.clone(),
                        color: def.color,
                        timestamp,
                        alert_text_enabled: true,
                        audio_enabled: false,
                        audio_file: None,
                    });
                }
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
                    def.display_target,
                    icon_ability_id,
                    def.show_at_secs,
                    def.show_icon,
                    def.display_source,
                    def.cooldown_ready_secs,
                    &def.audio,
                    def.alert_text.clone(),
                    def.alert_on == AlertTrigger::OnExpire,
                );

                if let Some(c) = charges {
                    effect.set_stacks(c);
                }

                self.active_effects.insert(key, effect);
                self.ticking_count += 1;
                should_register = true;

                // Collect alert for effect start if configured
                if def.alert_on == AlertTrigger::OnApply
                    && let Some(text) = &def.alert_text
                {
                    pending_alerts.push(FiredAlert {
                        id: def.id.clone(),
                        name: def.name.clone(),
                        text: text.clone(),
                        color: def.color,
                        timestamp,
                        alert_text_enabled: true,
                        audio_enabled: false,
                        audio_file: None,
                    });
                }
            }
        }

        // Queue collected alerts
        self.fired_alerts.extend(pending_alerts);

        // Queue target for raid frame registration only when effect was created or refreshed.
        // Only players belong on raid frames (not companions or NPCs)
        if should_register
            && is_from_local
            && target_entity_type == EntityType::Player
        {
            self.new_targets.push(NewTargetInfo {
                entity_id: target_id,
                name: target_name,
            });
        }
    }

    /// Refresh any tracked effects that have this action in their refresh_abilities.
    /// For raid frame effects, also creates the effect if it doesn't exist yet
    /// (handles late registration when initial application was missed).
    ///
    /// The `trigger_type` parameter specifies what kind of event triggered this refresh:
    /// - `Activation`: AbilityActivated signal (instant refresh)
    /// - `Heal`: HealingDone signal (refresh after heal lands, for cast-time abilities)
    fn refresh_effects_by_action(
        &mut self,
        action_id: i64,
        action_name: IStr,
        source_id: i64,
        source_name: IStr,
        target_id: i64,
        target_name: IStr,
        target_entity_type: EntityType,
        timestamp: NaiveDateTime,
        encounter: Option<&crate::encounter::CombatEncounter>,
        trigger_type: RefreshTrigger,
    ) {
        // For AoE abilities (target_id == 0), we can't reliably detect which targets
        // were actually hit. Damage events from ongoing DOTs on other targets look
        // identical to first ticks from the new cast. Rather than risk false refreshes
        // on targets that weren't in the AoE, we skip refresh detection entirely.
        // New applications are still tracked via ApplyEffect signals.
        if target_id == 0 {
            return;
        }

        // Use the entity type from the combat log signal rather than the encounter
        // roster, which may be incomplete (players who haven't generated combat events
        // yet won't appear in encounter.players)
        let is_player = target_entity_type == EntityType::Player;

        // Single-target case: refresh effect on specific target
        let action_name_str = crate::context::resolve(action_name);

        // Collect matching definitions with all info needed for creation
        struct RefreshableEffect {
            id: String,
            name: String,
            display_text: String,
            duration: Option<Duration>,
            color: [u8; 4],
            display_target: DisplayTarget,
            icon_ability_id: u64,
            show_at_secs: f32,
            show_icon: bool,
            display_source: bool,
            cooldown_ready_secs: f32,
            audio: crate::dsl::AudioConfig,
            alert_text: Option<String>,
            alert_on_expire: bool,
            default_charges: Option<u8>,
            /// Minimum stacks required for this refresh (None = any)
            min_stacks: Option<u8>,
        }

        let local_discipline = self.local_player_discipline;
        let refreshable_defs: Vec<_> = self
            .definitions
            .find_refreshable_by(action_id as u64, Some(action_name_str))
            .into_iter()
            .filter(|def| {
                matches!(
                    def.source_filter(),
                    EntityFilter::LocalPlayer | EntityFilter::Any
                )
            })
            .filter(|def| def.matches_discipline(local_discipline.as_ref()))
            .filter_map(|def| {
                // Find the matching RefreshAbility entry to get conditions
                let refresh_ability = def.find_refresh_ability(action_id as u64, Some(action_name_str))?;

                // Check if trigger type matches
                if refresh_ability.trigger() != trigger_type {
                    return None;
                }

                Some(RefreshableEffect {
                    id: def.id.clone(),
                    name: def.name.clone(),
                    display_text: def.display_text().to_string(),
                    duration: self.effective_duration(def),
                    color: def.effective_color(),
                    display_target: def.display_target,
                    icon_ability_id: def.icon_ability_id.unwrap_or(action_id as u64),
                    show_at_secs: def.show_at_secs,
                    show_icon: def.show_icon,
                    display_source: def.display_source,
                    cooldown_ready_secs: def.cooldown_ready_secs,
                    audio: def.audio.clone(),
                    alert_text: def.alert_text.clone(),
                    alert_on_expire: def.alert_on == AlertTrigger::OnExpire,
                    default_charges: def.default_charges,
                    min_stacks: refresh_ability.min_stacks(),
                })
            })
            .collect();

        for def in refreshable_defs {
            let key = EffectKey::new(&def.id, target_id);
            // Fallback: if the resolved target is an NPC, also try source_id.
            // Handles self-cast abilities (e.g. Dark Ward) where target resolution
            // resolves to the combat target but the effect is keyed to the caster.
            // Only for NPC targets — player targets are intentional (e.g. heals).
            let fallback_key = if target_id != source_id
                && target_entity_type != EntityType::Player
            {
                Some(EffectKey::new(&def.id, source_id))
            } else {
                None
            };

            let matched_key = if self.active_effects.contains_key(&key) {
                Some(key.clone())
            } else {
                fallback_key.filter(|k| self.active_effects.contains_key(k))
            };

            if let Some(effect) = matched_key.and_then(|k| self.active_effects.get_mut(&k)) {
                // Don't resurrect effects that have been authoritatively removed.
                // EffectRemoved is the source of truth — if the game says the effect
                // is gone, refresh abilities cannot bring it back.
                // (timer_expired effects CAN still be refreshed — that's the grace period
                // for when our duration estimate expires before the in-game effect.)
                if effect.removed_at.is_some() {
                    continue;
                }

                // Check min_stacks condition if specified
                if let Some(min_stacks) = def.min_stacks {
                    if effect.stacks < min_stacks {
                        continue; // Skip refresh - not enough stacks
                    }
                }

                // Existing effect - refresh duration
                effect.refresh(timestamp, def.duration);

                // Re-register for raid frames (in case user cleared the slot)
                if def.display_target == DisplayTarget::RaidFrames && is_player {
                    self.new_targets.push(NewTargetInfo {
                        entity_id: target_id,
                        name: target_name,
                    });
                }
            } else if def.display_target == DisplayTarget::RaidFrames {
                // Don't late-register if min_stacks is required — no existing effect
                // means 0 stacks, which can't satisfy the minimum. Only unconditional
                // refresh abilities (Simple variant, no min_stacks) should late-register.
                if def.min_stacks.is_some() {
                    continue;
                }
                // Raid frame effect doesn't exist - create it (late registration)
                let mut effect = ActiveEffect::new(
                    def.id.clone(),
                    action_id as u64,
                    def.name,
                    def.display_text,
                    source_id,
                    source_name,
                    target_id,
                    target_name,
                    true, // is_from_local - this function is only called for local player
                    timestamp,
                    def.duration,
                    def.color,
                    def.display_target,
                    def.icon_ability_id,
                    def.show_at_secs,
                    def.show_icon,
                    def.display_source,
                    def.cooldown_ready_secs,
                    &def.audio,
                    def.alert_text,
                    def.alert_on_expire,
                );

                if let Some(charges) = def.default_charges {
                    effect.set_stacks(charges);
                }

                self.active_effects.insert(key, effect);
                self.ticking_count += 1;

                // Queue target for raid frame registration (only players)
                if def.display_target == DisplayTarget::RaidFrames && is_player {
                    self.new_targets.push(NewTargetInfo {
                        entity_id: target_id,
                        name: target_name,
                    });
                }
            }
        }
    }

    /// Set up pending AoE refresh state when AbilityActivate has [=] target.
    /// Only sets up state for AoE refresh abilities (definitions with `is_aoe_refresh = true`)
    /// that use damage correlation instead of individual ApplyEffect signals.
    fn setup_pending_aoe_refresh(
        &mut self,
        ability_id: i64,
        timestamp: NaiveDateTime,
        primary_target: i64,
    ) {
        if self.definitions.is_aoe_refresh(ability_id as u64) {
            self.pending_aoe_refresh = Some(PendingAoeRefresh {
                ability_id,
                timestamp,
                primary_target,
            });
            self.aoe_collecting = None;
        }
    }

    /// Handle damage event for AoE refresh correlation.
    fn handle_damage_for_aoe_refresh(
        &mut self,
        ability_id: i64,
        target_id: i64,
        timestamp: NaiveDateTime,
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

        // Check if this damage is on the primary target (stored at cast time)
        if target_id == pending.primary_target {
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

        let refreshable_def_ids: Vec<_> = self
            .definitions
            .find_refreshable_by(collecting.ability_id as u64, None)
            .into_iter()
            .map(|def| (def.id.clone(), self.effective_duration(def)))
            .collect();

        // Refresh effects on all collected targets
        for target_id in collecting.targets {
            for (def_id, duration) in &refreshable_def_ids {
                let key = EffectKey::new(def_id, target_id);
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
        target_entity_type: EntityType,
        timestamp: NaiveDateTime,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
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
        let current_target_id =
            local_player_id.and_then(|id| self.current_targets.get(&id).map(|(tid, _, _)| *tid));
        for def in matching_defs {
            // Only process AbilityCast triggers here (index also contains DamageTaken/HealingTaken)
            if !def.is_ability_cast_trigger() {
                continue;
            }

            // Check discipline filter
            if !def.matches_discipline(self.local_player_discipline.as_ref()) {
                continue;
            }

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
                    current_target_id,
                    &boss_ids,
                )
            {
                continue;
            }

            // Instant alerts: fire and skip — no ActiveEffect created
            if def.is_alert {
                self.fired_alerts.push(Self::build_instant_alert(def, timestamp));
                continue;
            }

            // For procs, the effect is typically shown on the caster (source)
            // Use target from definition's target filter, or default to source
            let (effect_target_id, effect_target_name, effect_target_type) =
                if def.target_filter().is_local_player() {
                    // Local player is always EntityType::Player
                    (
                        local_player_id.unwrap_or(source_id),
                        source_name,
                        EntityType::Player,
                    )
                } else if target_id == source_id {
                    (source_id, source_name, source_entity_type)
                } else {
                    (target_id, target_name, target_entity_type)
                };

            let key = EffectKey::new(&def.id, effect_target_id);

            let duration = self.effective_duration(def);

            if let Some(existing) = self.active_effects.get_mut(&key) {
                // Refresh existing effect (same trigger ability was cast again)
                existing.refresh(timestamp, duration);

                // Re-register target in raid registry if they were removed
                if existing.is_from_local_player
                    && effect_target_type == EntityType::Player
                {
                    self.new_targets.push(NewTargetInfo {
                        entity_id: effect_target_id,
                        name: effect_target_name,
                    });
                }

                // Fire OnApply alert on refresh
                if def.alert_on == AlertTrigger::OnApply
                    && let Some(text) = &def.alert_text
                {
                    self.fired_alerts.push(FiredAlert {
                        id: def.id.clone(),
                        name: def.name.clone(),
                        text: text.clone(),
                        color: def.color,
                        timestamp,
                        alert_text_enabled: true,
                        audio_enabled: false,
                        audio_file: None,
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
                    def.display_target,
                    icon_ability_id,
                    def.show_at_secs,
                    def.show_icon,
                    def.display_source,
                    def.cooldown_ready_secs,
                    &def.audio,
                    def.alert_text.clone(),
                    def.alert_on == AlertTrigger::OnExpire,
                );
                self.active_effects.insert(key, effect);
                self.ticking_count += 1;

                // Fire OnApply alert for new effect
                if def.alert_on == AlertTrigger::OnApply
                    && let Some(text) = &def.alert_text
                {
                    self.fired_alerts.push(FiredAlert {
                        id: def.id.clone(),
                        name: def.name.clone(),
                        text: text.clone(),
                        color: def.color,
                        timestamp,
                        alert_text_enabled: true,
                        audio_enabled: false,
                        audio_file: None,
                    });
                }
            }
        }
    }

    /// Handle damage/healing taken trigger - creates a simple timed effect.
    /// No refresh or charge logic, just starts (or restarts) the timer on each event.
    fn handle_ability_event_trigger(
        &mut self,
        ability_id: i64,
        ability_name: IStr,
        source_id: i64,
        source_name: IStr,
        source_entity_type: EntityType,
        source_npc_id: i64,
        target_id: i64,
        target_name: IStr,
        target_entity_type: EntityType,
        target_npc_id: i64,
        timestamp: NaiveDateTime,
        encounter: Option<&crate::encounter::CombatEncounter>,
        trigger_check: fn(&EffectDefinition) -> bool,
    ) {
        self.current_game_time = Some(timestamp);
        let ability_name_str = crate::context::resolve(ability_name);

        let matching_defs: Vec<_> = self
            .definitions
            .find_ability_cast_matching(ability_id as u64, Some(ability_name_str))
            .into_iter()
            .collect();

        if matching_defs.is_empty() {
            return;
        }

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

        let is_from_local = self.local_player_id == Some(source_id);

        for def in matching_defs {
            if !trigger_check(def) {
                continue;
            }
            if !self.matches_filters(def, source_info, target_info, encounter) {
                continue;
            }

            // Instant alerts: fire and skip — no ActiveEffect created
            if def.is_alert {
                self.fired_alerts.push(Self::build_instant_alert(def, timestamp));
                continue;
            }

            let key = EffectKey::new(&def.id, target_id);
            let duration = self.effective_duration(def);

            if let Some(existing) = self.active_effects.get_mut(&key) {
                existing.refresh(timestamp, duration);

                // Fire OnApply alert on refresh
                if def.alert_on == AlertTrigger::OnApply
                    && let Some(text) = &def.alert_text
                {
                    self.fired_alerts.push(FiredAlert {
                        id: def.id.clone(),
                        name: def.name.clone(),
                        text: text.clone(),
                        color: def.color,
                        timestamp,
                        alert_text_enabled: true,
                        audio_enabled: false,
                        audio_file: None,
                    });
                }
            } else {
                let display_text = def.display_text().to_string();
                let icon_ability_id = def.icon_ability_id.unwrap_or(ability_id as u64);
                let effect = ActiveEffect::new(
                    def.id.clone(),
                    ability_id as u64,
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
                    def.display_target,
                    icon_ability_id,
                    def.show_at_secs,
                    def.show_icon,
                    def.display_source,
                    def.cooldown_ready_secs,
                    &def.audio,
                    def.alert_text.clone(),
                    def.alert_on == AlertTrigger::OnExpire,
                );
                self.active_effects.insert(key, effect);
                self.ticking_count += 1;

                // Fire OnApply alert for new effect
                if def.alert_on == AlertTrigger::OnApply
                    && let Some(text) = &def.alert_text
                {
                    self.fired_alerts.push(FiredAlert {
                        id: def.id.clone(),
                        name: def.name.clone(),
                        text: text.clone(),
                        color: def.color,
                        timestamp,
                        alert_text_enabled: true,
                        audio_enabled: false,
                        audio_file: None,
                    });
                }
            }
        }
    }

    /// Handle effect removal signal
    fn handle_effect_removed(
        &mut self,
        effect_id: i64,
        effect_name: IStr,
        source_id: i64,
        source_entity_type: EntityType,
        source_name: IStr,
        source_npc_id: i64,
        target_id: i64,
        target_entity_type: EntityType,
        target_name: IStr,
        target_npc_id: i64,
        timestamp: NaiveDateTime,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
        self.current_game_time = Some(timestamp);
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

        let matching_defs: Vec<_> = self
            .definitions
            .find_matching(effect_id as u64, Some(effect_name_str))
            .into_iter()
            .collect();

        let is_from_local = local_player_id == Some(source_id);

        for def in matching_defs {
            let key = EffectKey::new(&def.id, target_id);

            if def.is_effect_applied_trigger() {
                // Mark existing effect as removed (normal behavior)
                // Skip if ignore_effect_removed OR cooldowns (cooldowns always use timer-based expiry)
                let is_cooldown = def.display_target == DisplayTarget::Cooldowns;
                if !def.ignore_effect_removed
                    && !is_cooldown
                    && let Some(effect) = self.active_effects.get_mut(&key)
                {
                    // Only honor removal if it occurred well AFTER the last refresh.
                    // DOT reapplication sends ApplyEffect then RemoveEffect - sometimes
                    // the RemoveEffect arrives up to ~1 second later (for the old DOT instance).
                    // Use a 1 second window to ignore stale RemoveEffect signals.
                    let since_refresh_ms = timestamp
                        .signed_duration_since(effect.last_refreshed_at)
                        .num_milliseconds();
                    if since_refresh_ms > 1000 {
                        if effect.mark_removed() {
                            self.ticking_count = self.ticking_count.saturating_sub(1);
                        }
                    }
                }
            } else if def.is_effect_removed_trigger()
                && self.matches_filters(def, source_info, target_info, encounter)
            {
                // Instant alerts: fire and skip — no ActiveEffect created
                if def.is_alert {
                    self.fired_alerts.push(Self::build_instant_alert(def, timestamp));
                    continue;
                }

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
                    def.display_target,
                    icon_ability_id,
                    def.show_at_secs,
                    def.show_icon,
                    def.display_source,
                    def.cooldown_ready_secs,
                    &def.audio,
                    def.alert_text.clone(),
                    def.alert_on == AlertTrigger::OnExpire,
                );
                self.active_effects.insert(key, effect);
                self.ticking_count += 1;

                // Fire OnApply alert for new EffectRemoved-triggered effect
                if def.alert_on == AlertTrigger::OnApply
                    && let Some(text) = &def.alert_text
                {
                    self.fired_alerts.push(FiredAlert {
                        id: def.id.clone(),
                        name: def.name.clone(),
                        text: text.clone(),
                        color: def.color,
                        timestamp,
                        alert_text_enabled: true,
                        audio_enabled: false,
                        audio_file: None,
                    });
                }
            }
        }
    }

    /// Handle charges changed signal
    fn handle_charges_changed(
        &mut self,
        effect_id: i64,
        effect_name: IStr,
        _action_id: i64,
        _action_name: IStr,
        source_id: i64,
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

        for def in matching_defs {
            let key = EffectKey::new(&def.id, target_id);

            // Calculate duration before borrowing active_effects mutably
            let duration = if def.is_refreshed_on_modify {
                self.effective_duration(def)
            } else {
                None
            };

            if let Some(effect) = self.active_effects.get_mut(&key) {
                // Only update charges if the source matches the effect's source.
                // This prevents another player's ability charges (e.g., Kolto Probes
                // from a second operative healer) from corrupting our tracked stacks.
                if effect.source_entity_id != source_id {
                    continue;
                }

                effect.set_stacks(charges);

                // Refresh duration on ModifyCharges if is_refreshed_on_modify is set.
                // Uses refresh_duration() (not refresh()) to avoid updating last_refreshed_at,
                // which would cause the stale-removal window to swallow a legitimate
                // RemoveEffect that follows shortly after a charge change.
                if let Some(dur) = duration {
                    effect.refresh_duration(timestamp, dur);
                }
            }
        }
    }

    /// Handle entity death - clear effects unless persist_past_death
    fn handle_entity_death(&mut self, entity_id: i64) {
        for (key, effect) in self.active_effects.iter_mut() {
            if effect.target_entity_id != entity_id {
                continue;
            }
            let persist = self
                .definitions
                .effects
                .get(&key.definition_id)
                .map(|def| def.persist_past_death)
                .unwrap_or(false);
            if !persist && effect.mark_removed() {
                self.ticking_count = self.ticking_count.saturating_sub(1);
            }
        }
    }

    /// Handle combat end - optionally clear combat-only effects
    fn handle_combat_ended(&mut self) {
        // Clear pending AoE refresh state
        self.pending_aoe_refresh = None;
        self.aoe_collecting = None;

        // Mark effects that don't track outside combat as removed
        let outside_combat_ids: HashSet<&str> = self
            .definitions
            .enabled()
            .filter(|def| def.track_outside_combat)
            .map(|def| def.id.as_str())
            .collect();

        for (key, effect) in self.active_effects.iter_mut() {
            if !outside_combat_ids.contains(key.definition_id.as_str()) {
                if effect.mark_removed() {
                    self.ticking_count = self.ticking_count.saturating_sub(1);
                }
            }
        }
    }

    /// Handle area change (zone transition) - clear all active effects
    fn handle_area_change(&mut self) {
        // Clear pending AoE refresh state
        self.pending_aoe_refresh = None;
        self.aoe_collecting = None;

        for (_key, effect) in self.active_effects.iter_mut() {
            if effect.mark_removed() {
                self.ticking_count = self.ticking_count.saturating_sub(1);
            }
        }
    }

    /// Check if an effect matches source/target filters and discipline scope
    fn matches_filters(
        &self,
        def: &EffectDefinition,
        source: EntityInfo,
        target: EntityInfo,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) -> bool {
        // Check discipline filter (only relevant for player characters)
        if !def.matches_discipline(self.local_player_discipline.as_ref()) {
            return false;
        }

        // Get local player ID from self, boss entity IDs from encounter
        let local_player_id = self.local_player_id;
        let current_target_id =
            local_player_id.and_then(|id| self.current_targets.get(&id).map(|(tid, _, _)| *tid));
        let boss_ids = get_boss_ids(encounter);

        let entities = get_entities(encounter);

        def.source_filter().matches(
            entities,
            source.id,
            source.entity_type,
            source.name,
            source.npc_id,
            local_player_id,
            current_target_id,
            &boss_ids,
        ) && def.target_filter().matches(
            entities,
            target.id,
            target.entity_type,
            target.name,
            target.npc_id,
            local_player_id,
            current_target_id,
            &boss_ids,
        )
    }
}

impl SignalHandler for EffectTracker {
    fn handle_signals(
        &mut self,
        signals: &[GameSignal],
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
        for signal in signals {
            self.handle_signal(signal, encounter);
        }
        // Only finalize AoE collection if we're past the collection window (10ms).
        // This ensures secondary targets have time to arrive across multiple batches,
        // while still finalizing promptly once the window has elapsed.
        if let Some(ref collecting) = self.aoe_collecting {
            if let Some(current_time) = self.current_game_time {
                let elapsed_ms = (current_time - collecting.anchor_timestamp).num_milliseconds();
                if elapsed_ms > 10 {
                    self.finalize_aoe_refresh();
                }
            }
        }
    }

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
                source_entity_type,
                source_name,
                source_npc_id,
                target_id,
                target_entity_type,
                target_name,
                target_npc_id,
                timestamp,
            } => {
                self.handle_effect_removed(
                    *effect_id,
                    *effect_name,
                    *source_id,
                    *source_entity_type,
                    *source_name,
                    *source_npc_id,
                    *target_id,
                    *target_entity_type,
                    *target_name,
                    *target_npc_id,
                    *timestamp,
                    encounter,
                );
            }
            GameSignal::EffectChargesChanged {
                effect_id,
                effect_name,
                action_id,
                action_name,
                source_id,
                source_entity_type: _,
                target_id,
                timestamp,
                charges,
            } => {
                self.handle_charges_changed(
                    *effect_id,
                    *effect_name,
                    *action_id,
                    *action_name,
                    *source_id,
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
            GameSignal::DisciplineChanged {
                entity_id,
                discipline_id,
                ..
            } => {
                // Track local player's discipline for discipline-scoped effects
                if self.local_player_id == Some(*entity_id) {
                    self.local_player_discipline = Discipline::from_guid(*discipline_id);
                }
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
                // Use explicit target if available, otherwise query encounter or fallback cache
                let local_player_id = self.local_player_id;
                if local_player_id == Some(*source_id) {
                    let is_self_or_empty = *target_id == 0 || *target_id == *source_id;
                    let (resolved_target, resolved_target_name, resolved_entity_type) = if is_self_or_empty {
                        // Query encounter for caster's current target, fall back to cached target,
                        // finally default to self (game casts on caster when no target)
                        if let Some((target, name, etype)) =
                            self.current_targets.get(source_id).copied()
                        {
                            (target, name, etype)
                        } else if let Some(target) =
                            encounter.and_then(|e| e.get_current_target(*source_id))
                        {
                            // Encounter has target - look up name and entity type
                            let player_info = encounter
                                .and_then(|e| e.players.get(&target));
                            let name = player_info.map(|p| p.name).unwrap_or(*source_name);
                            let etype = if player_info.is_some() {
                                EntityType::Player
                            } else {
                                EntityType::Npc
                            };
                            (target, name, etype)
                        } else {
                            // No target info - default to self (always a player)
                            (*source_id, *source_name, EntityType::Player)
                        }
                    } else {
                        (*target_id, *target_name, *target_entity_type)
                    };

                    // Record cast for DotTracker validation (prevents lingering effect issues)
                    self.recent_casts
                        .insert((*ability_id as u64, resolved_target), *timestamp);

                    self.refresh_effects_by_action(
                        *ability_id,
                        *ability_name,
                        *source_id,
                        *source_name,
                        resolved_target,
                        resolved_target_name,
                        resolved_entity_type,
                        *timestamp,
                        encounter,
                        RefreshTrigger::Activation,
                    );

                    // For AoE abilities, set up pending state for damage correlation
                    // This allows us to detect and refresh effects on secondary targets too
                    // Check directly if this is an AoE refresh ability (don't rely on target_id)
                    self.setup_pending_aoe_refresh(*ability_id, *timestamp, resolved_target);
                }
            }
            GameSignal::DamageTaken {
                ability_id,
                ability_name,
                source_id,
                source_entity_type,
                source_name,
                source_npc_id,
                target_id,
                target_entity_type,
                target_name,
                target_npc_id,
                timestamp,
            } => {
                // Existing AoE refresh logic
                if self.local_player_id == Some(*source_id) {
                    self.handle_damage_for_aoe_refresh(*ability_id, *target_id, *timestamp);
                }
                // DamageTaken trigger matching for effects tracker
                self.handle_ability_event_trigger(
                    *ability_id,
                    *ability_name,
                    *source_id,
                    *source_name,
                    *source_entity_type,
                    *source_npc_id,
                    *target_id,
                    *target_name,
                    *target_entity_type,
                    *target_npc_id,
                    *timestamp,
                    encounter,
                    EffectDefinition::is_damage_taken_trigger,
                );
            }
            GameSignal::HealingDone {
                ability_id,
                ability_name,
                source_id,
                source_entity_type,
                source_name,
                source_npc_id,
                target_id,
                target_entity_type,
                target_name,
                target_npc_id,
                timestamp,
            } => {
                // Existing refresh on heal completion logic
                if self.local_player_id == Some(*source_id) {
                    self.refresh_effects_by_action(
                        *ability_id,
                        *ability_name,
                        *source_id,
                        *source_name,
                        *target_id,
                        *target_name,
                        *target_entity_type,
                        *timestamp,
                        encounter,
                        RefreshTrigger::Heal,
                    );
                }
                // HealingTaken trigger matching for effects tracker
                self.handle_ability_event_trigger(
                    *ability_id,
                    *ability_name,
                    *source_id,
                    *source_name,
                    *source_entity_type,
                    *source_npc_id,
                    *target_id,
                    *target_name,
                    *target_entity_type,
                    *target_npc_id,
                    *timestamp,
                    encounter,
                    EffectDefinition::is_healing_taken_trigger,
                );
            }
            GameSignal::TargetChanged {
                source_id,
                target_id,
                target_entity_type,
                target_name,
                ..
            } => {
                // Cache target ID, name, and entity type for fallback
                self.current_targets
                    .insert(*source_id, (*target_id, *target_name, *target_entity_type));
            }
            GameSignal::TargetCleared { source_id, .. } => {
                self.current_targets.remove(source_id);
            }
            // Boss entity IDs are now read from encounter.hp_by_entity in matches_filters
            _ => {}
        }
    }
}
