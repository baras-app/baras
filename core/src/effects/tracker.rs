//! Effect tracking handler
//!
//! Tracks active effects on entities by matching game signals against
//! configured effect definitions. Produces `ActiveEffect` instances
//! that can be fed to overlay renderers.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use chrono::NaiveDateTime;

use crate::combat_log::EntityType;
use crate::context::IStr;
use crate::dsl::EntityDefinition;
use crate::dsl::EntityFilterMatching;
use crate::encounter::{CombatEncounter, PvpFaction};
use crate::game_data::{Discipline, DisciplineFilter};
use crate::signal_processor::{GameSignal, SignalHandler};

use crate::timers::FiredAlert;

use super::alacrity::AlacrityBuffTracker;
use super::{ActiveEffect, AlertTrigger, DisplayTarget, EffectDefinition, EffectKey, RefreshScope, RefreshTrigger};

fn format_effect_alert(text: &str, source_name: IStr, target_name: IStr) -> String {
    if !text.contains('{') {
        return text.to_string();
    }
    let src = crate::context::resolve(source_name);
    let tgt = crate::context::resolve(target_name);
    text.replace("{source}", if src.is_empty() { "" } else { src })
        .replace("{target}", if tgt.is_empty() { "" } else { tgt })
}

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
    /// Who cast the ability
    source_id: i64,
    /// When the ability was activated
    timestamp: NaiveDateTime,
    /// The primary target (resolved at cast time)
    primary_target: i64,
}

/// Pending single-target DotTracker refresh waiting for damage confirmation.
/// Set when AbilityActivated fires for a refresh ability on a non-AoE DotTracker effect.
/// Consumed by the next DamageTaken event from the same ability.
#[derive(Debug, Clone)]
struct PendingDotRefresh {
    /// The ability that was activated
    ability_id: i64,
    /// Who cast the ability
    source_id: i64,
    /// When the ability was activated
    timestamp: NaiveDateTime,
    /// Caster's selected target at activation (None when unknown or self).
    /// Only a damage event on this target may confirm the refresh — a bystander's
    /// periodic tick from the same DOT must not claim it.
    target_id: Option<i64>,
}

/// State for collecting AoE damage targets after finding anchor
#[derive(Debug, Clone)]
struct AoeRefreshCollecting {
    /// The ability being tracked
    ability_id: i64,
    /// Who cast the ability
    source_id: i64,
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

    /// Game-time anchor: the highest game time we've seen (monotonic).
    /// Updated via `advance_game_time_anchor()` which ensures this never
    /// moves backward — it takes the max of the new event timestamp and
    /// the current interpolated time.
    current_game_time: Option<NaiveDateTime>,

    /// Monotonic instant when `current_game_time` was last anchored.
    /// Together with `current_game_time`, forms a game-time anchor for interpolation.
    current_game_time_instant: Option<Instant>,

    /// Local player ID (set from session cache during signal dispatch)
    local_player_id: Option<i64>,

    /// Local player's current discipline (for discipline-scoped effects)
    local_player_discipline: Option<Discipline>,

    /// Last known discipline per player entity (for source/target discipline filters)
    player_disciplines: HashMap<i64, Discipline>,

    /// Player's alacrity percentage (e.g., 15.4 for 15.4%)
    /// Used to adjust durations for effects with is_affected_by_alacrity = true
    alacrity_percent: f32,

    /// Temporary alacrity buffs currently active on the local player.
    /// Their bonus is added to `alacrity_percent` when new entries are created.
    alacrity_buffs: AlacrityBuffTracker,

    /// Bias adjustment in milliseconds (may be negative).
    /// Added to alacrity-scaled effect durations to correct a consistent discrepancy.
    latency_ms: i16,

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

    /// Pending single-target DotTracker refresh waiting for next damage event.
    /// Set when AbilityActivated fires for a non-AoE DotTracker refresh ability.
    pending_dot_refresh: Option<PendingDotRefresh>,

    /// Alerts fired by effect start/end triggers
    fired_alerts: Vec<FiredAlert>,

    /// Count of active (non-removed) effects for O(1) has_ticking_effects() check
    ticking_count: usize,

    /// Current target for each entity (source_id -> (target_id, target_name, entity_type))
    /// Used as fallback when encounter doesn't have target info (e.g., outside combat)
    current_targets: HashMap<i64, (i64, IStr, EntityType)>,

    /// Last-seen charge counts for external effects: (effect_id, target_entity_id) -> charges.
    /// Used by ChargesChanged modifiers to determine direction (increased/decreased/refreshed).
    last_seen_charges: HashMap<(i64, i64), u8>,

    /// Last icon each uptime-tracked definition activated with (definition id
    /// -> icon ability id). Retained after the effect ends so multi-effect
    /// definitions keep their placeholder on the variant the player last used.
    uptime_last_icons: HashMap<String, u64>,

    /// Area of the last AreaEntered signal. Instanced areas log AreaEntered
    /// twice (plain, then difficulty-qualified) with the game's effect
    /// restatement in between — wiping on the duplicate would discard the
    /// just-restated effects, so same-area re-entries skip the wipe.
    last_area_id: Option<i64>,
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
            current_game_time_instant: None,
            local_player_id: None,
            local_player_discipline: None,
            player_disciplines: HashMap::new(),
            alacrity_percent: 0.0,
            alacrity_buffs: AlacrityBuffTracker::default(),
            latency_ms: 0,
            new_targets: Vec::new(),
            pending_aoe_refresh: None,
            aoe_collecting: None,
            pending_dot_refresh: None,
            fired_alerts: Vec::new(),
            ticking_count: 0,
            current_targets: HashMap::new(),
            last_seen_charges: HashMap::new(),
            uptime_last_icons: HashMap::new(),
            last_area_id: None,
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
    fn build_instant_alert(
        def: &EffectDefinition,
        timestamp: NaiveDateTime,
        source_name: IStr,
        target_name: IStr,
    ) -> FiredAlert {
        let has_text = def.alert_text.is_some();
        let raw_text = def.alert_text.as_deref().unwrap_or(&def.name);
        let text = format_effect_alert(raw_text, source_name, target_name);
        FiredAlert {
            id: def.id.clone(),
            name: def.name.clone(),
            text,
            color: def.color,
            timestamp,
            alert_text_enabled: has_text,
            audio_enabled: def.audio.enabled,
            audio_file: def.audio.file.clone(),
            icon_ability_id: def.icon_ability_id,
                    remaining_secs: None,
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

    /// Set the duration bias adjustment (ms, may be negative) for duration calculations
    pub fn set_latency(&mut self, latency_ms: i16) {
        self.latency_ms = latency_ms;
    }

    /// Baseline alacrity plus any temporary alacrity buffs active at `at` (log time), in percent.
    fn total_alacrity(&self, at: NaiveDateTime) -> f32 {
        self.alacrity_percent + self.alacrity_buffs.bonus_percent(at)
    }

    /// A modifier's ICD in seconds, alacrity-scaled if the modifier opts in.
    fn effective_icd(&self, modifier: &baras_types::EffectModifier, at: NaiveDateTime) -> Option<f32> {
        let icd = modifier.icd_secs?;
        let alacrity = if modifier.icd_affected_by_alacrity { self.total_alacrity(at) } else { 0.0 };
        Some(if alacrity > 0.0 { icd / (1.0 + alacrity / 100.0) } else { icd })
    }

    /// Calculate effective duration for a definition, applying alacrity and bias if configured
    /// For cooldowns with cooldown_ready_secs, adds the ready period to the total duration
    ///
    /// Formula: (base_duration / (1 + alacrity)) + bias + cooldown_ready_secs
    ///
    /// `at` is the log time of the triggering event — temporary alacrity buffs
    /// are evaluated against it, never the wall-clock-interpolated anchor.
    fn effective_duration(&self, def: &super::EffectDefinition, at: NaiveDateTime) -> Option<Duration> {
        def.duration_secs.map(|base_secs| {
            let total_alacrity = self.total_alacrity(at);
            // Apply alacrity reduction if enabled for this effect
            let adjusted = if def.is_affected_by_alacrity && total_alacrity > 0.0 {
                base_secs / (1.0 + total_alacrity / 100.0)
            } else {
                base_secs
            };
            // Apply bias adjustment (may be negative) for effects affected by alacrity
            let with_bias = if def.is_affected_by_alacrity && self.latency_ms != 0 {
                adjusted + (self.latency_ms as f32 / 1000.0)
            } else {
                adjusted
            };
            // Add cooldown_ready_secs to extend the total duration for the ready state
            let total = with_bias + def.cooldown_ready_secs;
            Duration::from_secs_f32(total.max(0.0))
        })
    }

    /// Source-agnostic lookup for `single_instance_per_target` buffs: find the
    /// tracked instance on `target_id` under any definition in the group
    /// (e.g. `kolto_shell` + `kolto_shell_others`), regardless of source. The
    /// game keeps one instance per target, so at most one entry can match.
    ///
    /// Takes the map directly (not `&mut self`) so callers can hold definition
    /// borrows across the call.
    fn find_single_instance_effect<'a>(
        active_effects: &'a mut HashMap<EffectKey, ActiveEffect>,
        group_ids: &[&str],
        target_id: i64,
    ) -> Option<&'a mut ActiveEffect> {
        active_effects.values_mut().find(|e| {
            e.target_entity_id == target_id && group_ids.iter().any(|id| *id == e.definition_id)
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
                effect.display_targets = def.display_targets.clone();
                effect.icon_ability_id = def.icon_ability_id.unwrap_or(effect.game_effect_id);
                effect.show_at_secs = def.show_at_secs;
                effect.show_icon = def.show_icon;
                effect.display_source = def.display_source;
                effect.show_target = def.show_target;
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

    /// Compute an interpolated game time for smooth display between log events.
    ///
    /// Takes the last game timestamp we received and advances it by the wall time
    /// elapsed since we received it. This stays in SWTOR's clock domain (no cross-clock
    /// comparison) and provides smooth countdown between log events.
    ///
    /// Returns `None` if no game timestamp has been received yet.
    pub fn interpolated_game_time(&self) -> Option<NaiveDateTime> {
        let game_time = self.current_game_time?;
        let received_at = self.current_game_time_instant?;
        let elapsed = received_at.elapsed();
        Some(game_time + chrono::Duration::milliseconds(elapsed.as_millis() as i64))
    }

    /// Advance the game-time anchor to at least `event_timestamp`.
    ///
    /// Uses a monotonic high-water-mark: the new anchor is
    /// `max(event_timestamp, current_interpolated_time)`. This ensures:
    /// - Interpolated game time never jumps backward (no visible "jump" in
    ///   remaining time when a batch of events arrives).
    /// - Processing latency is naturally absorbed: between events the
    ///   interpolation advances past event timestamps by roughly the I/O
    ///   delay, and the `max()` preserves that advancement.
    fn advance_game_time_anchor(&mut self, event_timestamp: NaiveDateTime) {
        let now = Instant::now();
        let anchor_time = match (self.current_game_time, self.current_game_time_instant) {
            (Some(gt), Some(inst)) => {
                let interp = gt + chrono::Duration::milliseconds(inst.elapsed().as_millis() as i64);
                // Never move the anchor backward
                if event_timestamp > interp { event_timestamp } else { interp }
            }
            _ => event_timestamp,
        };
        self.current_game_time = Some(anchor_time);
        self.current_game_time_instant = Some(now);
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
            .filter(|e| e.display_targets.contains(&DisplayTarget::RaidFrames) && e.removed_at.is_none() && !e.timer_expired)
    }

    /// Get effects destined for Effects A overlay
    pub fn effects_a(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects
            .values()
            .filter(|e| e.display_targets.contains(&DisplayTarget::EffectsA) && e.removed_at.is_none() && !e.timer_expired)
    }

    /// Get effects destined for Effects B overlay
    pub fn effects_b(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects
            .values()
            .filter(|e| e.display_targets.contains(&DisplayTarget::EffectsB) && e.removed_at.is_none() && !e.timer_expired)
    }

    /// Get effects destined for Effects C overlay
    pub fn effects_c(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects
            .values()
            .filter(|e| e.display_targets.contains(&DisplayTarget::EffectsC) && e.removed_at.is_none() && !e.timer_expired)
    }

    /// Definitions flagged for uptime tracking on the given overlay target.
    /// Fail-closed: empty until the local player's discipline is known, so
    /// placeholders never show for the wrong class before detection.
    pub fn uptime_definitions(&self, target: DisplayTarget) -> Vec<&super::EffectDefinition> {
        let Some(discipline) = self.local_player_discipline.as_ref() else {
            return Vec::new();
        };
        let mut defs: Vec<_> = self
            .definitions
            .effects
            .values()
            .filter(|def| {
                def.enabled
                    && def.track_uptime
                    && def.displays_on(target)
                    && (def.disciplines.is_empty() || def.disciplines.contains(discipline))
                    && (!def.track_uptime_after_first
                        || self.uptime_last_icons.contains_key(&def.id))
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
        for effect in self.active_effects.values() {
            if effect.removed_at.is_some()
                || !self
                    .definitions
                    .effects
                    .get(&effect.definition_id)
                    .is_some_and(|d| d.track_uptime)
            {
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
    pub fn definition(&self, id: &str) -> Option<&super::EffectDefinition> {
        self.definitions.effects.get(id)
    }

    /// Icon for an uptime placeholder: last-activated icon if seen this
    /// session, else the definition's static fallback.
    pub fn uptime_placeholder_icon(&self, def: &super::EffectDefinition) -> u64 {
        self.uptime_last_icons
            .get(&def.id)
            .copied()
            .unwrap_or_else(|| def.uptime_icon_id())
    }

    /// Get effects destined for cooldown tracker
    pub fn cooldown_effects(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects
            .values()
            .filter(|e| e.display_targets.contains(&DisplayTarget::Cooldowns) && e.removed_at.is_none() && !e.timer_expired)
    }

    /// Get effects destined for cooldown tracker B
    pub fn cooldown_b_effects(&self) -> impl Iterator<Item = &ActiveEffect> {
        self.active_effects
            .values()
            .filter(|e| e.display_targets.contains(&DisplayTarget::CooldownsB) && e.removed_at.is_none() && !e.timer_expired)
    }

    /// Get effects destined for DOT tracker, grouped by target entity
    pub fn dot_tracker_effects(&self) -> std::collections::HashMap<i64, Vec<&ActiveEffect>> {
        let mut by_target: std::collections::HashMap<i64, Vec<&ActiveEffect>> =
            std::collections::HashMap::new();
        for effect in self.active_effects.values() {
            if effect.removed_at.is_none() && !effect.timer_expired && effect.display_targets.contains(&DisplayTarget::DotTracker) {
                by_target
                    .entry(effect.target_entity_id)
                    .or_default()
                    .push(effect);
            }
        }
        by_target
    }

    /// Get effects destined for the boss HP overlay, grouped by target entity id.
    /// Keyed by entity id (not name) so two NPCs sharing a name don't share icons.
    pub fn boss_health_effects_by_target_id(&self) -> std::collections::HashMap<i64, Vec<&ActiveEffect>> {
        let mut by_target: std::collections::HashMap<i64, Vec<&ActiveEffect>> =
            std::collections::HashMap::new();
        for effect in self.active_effects.values() {
            if effect.removed_at.is_none()
                && !effect.timer_expired
                && effect.display_targets.contains(&DisplayTarget::BossHealth)
            {
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
            .filter(|e| e.display_targets.contains(&DisplayTarget::EffectsOverlay) && e.removed_at.is_none() && !e.timer_expired)
    }

    /// Drain the queue of targets for raid frame registration attempts.
    /// Called by the service - the registry handles duplicate rejection.
    pub fn take_new_targets(&mut self) -> Vec<NewTargetInfo> {
        std::mem::take(&mut self.new_targets)
    }

    /// Whether an effect target may be queued for raid frame registration.
    ///
    /// The game only lands beneficial effects on teammates, so a cast from the
    /// local player — or, in PvP, from any player classified friendly — proves
    /// the target is a teammate. In PvP a cast where neither endpoint is known
    /// friendly must not register: an enemy healer's Kolto Shell would pull
    /// enemies onto the raid frames.
    fn can_register_target(
        local_player_id: Option<i64>,
        source_id: i64,
        target_id: i64,
        encounter: Option<&CombatEncounter>,
    ) -> bool {
        if local_player_id == Some(source_id) {
            return true;
        }
        let Some(enc) = encounter.filter(|e| e.is_pvp()) else {
            return true;
        };
        let local = local_player_id.unwrap_or(0);
        enc.pvp_factions.faction_of(source_id, local) == Some(PvpFaction::Friendly)
            || enc.pvp_factions.faction_of(target_id, local) == Some(PvpFaction::Friendly)
    }

    /// Tick the tracker - removes expired effects and updates state
    ///
    /// Uses interpolated game time for accurate remaining time calculations
    /// without comparing SWTOR's clock to the system clock.
    pub fn tick(&mut self) {
        let Some(current_time) = self.current_game_time else {
            return;
        };

        // Compute interpolated game time once for all effects this tick
        let interp_time = self.interpolated_game_time().unwrap_or(current_time);

        // Collect effects that just ended (duration expired or removed by signal).
        // Include audio info so alerts fire reliably before GC.
        let mut ended_effects: Vec<(String, Option<String>, bool, IStr, IStr)> = Vec::new();

        for effect in self.active_effects.values_mut() {
            // Handle duration-expired effects.
            // Timer expiry is a heuristic — the in-game effect may outlast our estimate.
            // Instead of immediately removing, mark as timer_expired so the effect stays
            // in active_effects and can be revived by refresh_abilities.
            // After the grace period, hard-remove for GC.
            if effect.removed_at.is_none() && effect.has_duration_expired(interp_time) {
                if !effect.timer_expired {
                    // First tick after timer expiry — mark as timer-expired
                    effect.timer_expired = true;
                    self.ticking_count = self.ticking_count.saturating_sub(1);
                }

                // After grace period, hard-remove (GC on next retain pass)
                if let Some(expires_at) = effect.expires_at {
                    let since_expiry_ms = interp_time
                        .signed_duration_since(expires_at)
                        .num_milliseconds();
                    if since_expiry_ms > TIMER_EXPIRY_GRACE_MS {
                        effect.mark_removed();
                    }
                }
            }

            // Collect alert info for effects that just ended (any reason)
            let remaining_total = effect.remaining_secs(interp_time).unwrap_or(0.0);
            if !effect.on_end_alert_fired
                && (effect.has_base_duration_ended(remaining_total) || effect.removed_at.is_some())
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
                    effect.source_name,
                    effect.target_name,
                ));
            }
        }

        // Fire OnExpire alerts (with audio for early removals)
        for (def_id, audio_file, audio_enabled, src_name, tgt_name) in ended_effects {
            if let Some(def) = self.definitions.effects.get(&def_id)
                && def.alert_on == AlertTrigger::OnExpire
                && let Some(alert_text) = &def.alert_text
            {
                let text = format_effect_alert(alert_text, src_name, tgt_name);
                self.fired_alerts.push(FiredAlert {
                    id: def_id,
                    name: def.name.clone(),
                    text,
                    color: def.color,
                    timestamp: current_time,
                    alert_text_enabled: true,
                    audio_enabled,
                    audio_file,
                    icon_ability_id: def.icon_ability_id,
                    remaining_secs: None,
                });
            }
        }

        // Emit live countdown alerts for active effects whose remaining time
        // has entered their configured trailing window. Each tick pushes a
        // fresh FiredAlert carrying remaining_secs — the alerts overlay
        // dedupes by id and auto-suppresses when remaining hits zero.
        //
        // For cooldown effects (`cooldown_ready_secs > 0`) the alert tracks
        // the BASE cooldown, not the total time-to-expiry. We want the
        // countdown to fire in the last N seconds before the cooldown becomes
        // ready, then disappear — not during the ready-state tail.
        for effect in self.active_effects.values() {
            if effect.removed_at.is_some() || effect.timer_expired {
                continue;
            }
            let Some(def) = self.definitions.effects.get(&effect.definition_id) else {
                continue;
            };
            if def.alert_on != AlertTrigger::Countdown {
                continue;
            }
            let Some(window) = def.alert_countdown_secs else {
                continue;
            };
            if window <= 0.0 {
                continue;
            }
            let Some(remaining_total) = effect.remaining_secs(interp_time) else {
                continue;
            };
            // Base = total - ready_state. For non-cooldown effects this
            // collapses to `remaining_total`.
            let remaining_base = effect.remaining_base_secs(remaining_total);
            if remaining_base <= 0.0 || remaining_base > window {
                continue;
            }

            let raw_name = def
                .alert_text
                .as_deref()
                .unwrap_or(&def.name);
            let formatted = format_effect_alert(raw_name, effect.source_name, effect.target_name);
            let text = format!("{} ({:.1})", formatted, remaining_base);
            self.fired_alerts.push(FiredAlert {
                id: def.id.clone(),
                name: def.name.clone(),
                text,
                color: def.color,
                timestamp: current_time,
                alert_text_enabled: true,
                audio_enabled: false,
                audio_file: None,
                icon_ability_id: def.icon_ability_id,
                remaining_secs: Some(remaining_base),
            });
        }

        // Remove effects that have been marked removed (immediate, no fade delay)
        self.active_effects
            .retain(|_, effect| effect.removed_at.is_none());
    }

    /// Handle effect application signal
    fn handle_effect_applied(
        &mut self,
        effect_id: i64,
        effect_name: IStr,
        action_id: i64,
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
        self.advance_game_time_anchor(timestamp);

        // Note: GC is handled by tick() - don't duplicate here to reduce work per signal

        let local_player_id = self.local_player_id;

        // Track alacrity buffs landing on the local player (affects future durations)
        if local_player_id == Some(target_id) {
            self.alacrity_buffs.on_applied(effect_id, charges, timestamp);
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
        let all_matches = self
            .definitions
            .find_matching(effect_id as u64, Some(effect_name_str));

        // Ids of single-instance defs sharing this game effect (e.g. kolto_shell
        // + kolto_shell_others) — the source-agnostic lookup group used below.
        // Collected before filtering: source filters keep only one of the pair.
        let single_instance_ids: Vec<&str> = all_matches
            .iter()
            .filter(|d| d.single_instance_per_target)
            .map(|d| d.id.as_str())
            .collect();

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
                pending_alerts.push(Self::build_instant_alert(def, timestamp, source_name, target_name));
                continue;
            }

            let key = EffectKey::for_scope(&def.id, def.refresh_scope, source_id, target_id);

            let duration = self.effective_duration(def, timestamp);

            // Single-instance buffs (Kolto Shell, Trauma Probe): the game keeps one
            // instance per target across all sources — a second caster's apply
            // refreshes the existing instance and the original caster keeps credit.
            // Source-agnostic lookup across the def group, in both directions
            // (local over other's AND other's over local).
            if def.single_instance_per_target
                && let Some(existing) = Self::find_single_instance_effect(
                    &mut self.active_effects,
                    &single_instance_ids,
                    target_id,
                )
                && existing.removed_at.is_none()
            {
                existing.refresh(timestamp, duration);
                if let Some(c) = charges {
                    existing.set_stacks(c);
                }
                // Register the target even when the cast came from another
                // player — in PvE anyone receiving this buff is a group member;
                // in PvP only once classified as a teammate.
                if def.display_targets.contains(&DisplayTarget::RaidFrames)
                    && target_entity_type == EntityType::Player
                    && Self::can_register_target(local_player_id, source_id, target_id, encounter)
                {
                    self.new_targets.push(NewTargetInfo {
                        entity_id: target_id,
                        name: target_name,
                    });
                }
                continue;
            }

            if let Some(existing) = self.active_effects.get_mut(&key) {
                if def.display_targets.contains(&DisplayTarget::DotTracker) {
                    if existing.removed_at.is_some() {
                        // EffectRemoved arrived before EffectApplied (reverse ordering).
                        // Revive the effect — EffectApplied is authoritative for new applications.
                        // Always restore ticking_count: it was decremented either when
                        // timer_expired was set OR by handle_effect_removed — exactly once.
                        existing.refresh(timestamp, duration);
                        self.ticking_count += 1;
                        Self::clear_pending_dot_refresh(&mut self.pending_dot_refresh, action_id, target_id);
                    } else {
                        // Don't refresh the duration here — wait for a DamageTaken
                        // event to confirm a real hit (handle_damage_for_dot_refresh).
                        // Touch last_refreshed_at so the 1-second stale-removal guard
                        // covers the incoming EffectRemoved for the old DOT instance.
                        existing.last_refreshed_at = timestamp;
                        if let Some(c) = charges {
                            existing.set_stacks(c);
                        }
                    }
                    continue;
                }

                // Skip duplicate log lines (same timestamp) to avoid corrupting timing
                if existing.last_refreshed_at == timestamp {
                    if let Some(c) = charges {
                        existing.set_stacks(c);
                    }
                    continue;
                }

                // Ignore refreshes: skip retrigger only while still in base duration.
                // Once the cooldown enters the ready state it has effectively expired,
                // so a new trigger is a fresh activation, not a refresh.
                if def.ignore_refreshes && existing.is_in_base_duration(timestamp) {
                    continue;
                }

                existing.refresh(timestamp, duration);
                if let Some(c) = charges {
                    existing.set_stacks(c);
                }
                should_register |= def.display_targets.contains(&DisplayTarget::RaidFrames);

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
                        icon_ability_id: def.icon_ability_id,
                    remaining_secs: None,
                    });
                }
            } else {
                // A logged application is authoritative: drop any deferred refresh for
                // this cast so a bystander's tick can't claim it afterwards.
                if def.display_targets.contains(&DisplayTarget::DotTracker) {
                    Self::clear_pending_dot_refresh(&mut self.pending_dot_refresh, action_id, target_id);
                }
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
                    def.display_targets.clone(),
                    icon_ability_id,
                    def.show_at_secs,
                    def.show_icon,
                    def.display_source,
                    def.show_target,
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
                should_register |= def.display_targets.contains(&DisplayTarget::RaidFrames);

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
                        icon_ability_id: def.icon_ability_id,
                    remaining_secs: None,
                    });
                }
            }
        }

        // Queue collected alerts
        self.fired_alerts.extend(pending_alerts);

        // Queue target for raid frame registration only when a raid-frame effect
        // was created or refreshed — a tracked cooldown/buff (e.g. a stim
        // re-applied at zone-in) must not claim a slot. Only players belong on
        // raid frames (not companions or NPCs)
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
        source_entity_type: EntityType,
        target_id: i64,
        target_name: IStr,
        target_entity_type: EntityType,
        timestamp: NaiveDateTime,
        encounter: Option<&crate::encounter::CombatEncounter>,
        trigger_type: RefreshTrigger,
        // For the Damage trigger: whether this damage event was immune or resisted.
        // Refresh abilities with `ignore_immune_resist` skip these events.
        is_immune_or_resist: bool,
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

        // Prepare source filter context
        let local_player_id = self.local_player_id;
        let current_target_id =
            local_player_id.and_then(|id| self.current_targets.get(&id).map(|(tid, _, _)| *tid));
        let boss_ids = get_boss_ids(encounter);
        let entities = get_entities(encounter);
        let pvp_factions = encounter.and_then(|e| e.pvp_faction_context());

        // In PvP, only classified teammates may claim raid-frame slots.
        let can_register =
            is_player && Self::can_register_target(local_player_id, source_id, target_id, encounter);

        // Collect matching definitions with all info needed for creation
        struct RefreshableEffect {
            id: String,
            name: String,
            display_text: String,
            duration: Option<Duration>,
            color: [u8; 4],
            display_targets: Vec<DisplayTarget>,
            icon_ability_id: u64,
            show_at_secs: f32,
            show_icon: bool,
            display_source: bool,
            show_target: bool,
            cooldown_ready_secs: f32,
            audio: crate::dsl::AudioConfig,
            alert_text: Option<String>,
            alert_on_expire: bool,
            default_charges: Option<u8>,
            /// Minimum stacks required for this refresh (None = any)
            min_stacks: Option<u8>,
            refresh_scope: RefreshScope,
            /// One instance per target across sources (Kolto Shell/Trauma Probe)
            single_instance_per_target: bool,
            /// Resolved per-ability flag: defer the refresh to the first damage
            /// event from this ability instead of firing on cast.
            defer_to_first_damage: bool,
        }

        let local_discipline = self.local_player_discipline;

        // Ids of single-instance defs refreshable by this ability (both the
        // local and `_others` variants) — the source-agnostic lookup group used
        // below. Collected before filtering: source filters keep only one.
        let single_instance_ids: Vec<String> = self
            .definitions
            .find_refreshable_by(action_id as u64, Some(action_name_str))
            .into_iter()
            .filter(|d| d.single_instance_per_target)
            .map(|d| d.id.clone())
            .collect();

        let refreshable_defs: Vec<_> = self
            .definitions
            .find_refreshable_by(action_id as u64, Some(action_name_str))
            .into_iter()
            .filter(|def| def.matches_discipline(local_discipline.as_ref()))
            .filter(|def| {
                // Check source/target filters — only refresh/create effects whose filters
                // match the actual source and target entities
                def.source_filter().matches(
                    entities,
                    source_id,
                    source_entity_type,
                    source_name,
                    0,
                    local_player_id,
                    current_target_id,
                    &boss_ids,
                    pvp_factions,
                ) && def.target_filter().matches(
                    entities,
                    target_id,
                    target_entity_type,
                    target_name,
                    0,
                    local_player_id,
                    current_target_id,
                    &boss_ids,
                    pvp_factions,
                )
            })
            .filter(|def| {
                self.matches_entity_disciplines(&def.source_disciplines, source_id, source_entity_type)
                    && self.matches_entity_disciplines(&def.target_disciplines, target_id, target_entity_type)
            })
            .filter_map(|def| {
                // Find the matching RefreshAbility entry to get conditions
                let refresh_ability = def.find_refresh_ability(action_id as u64, Some(action_name_str))?;

                // Check if trigger type matches
                if refresh_ability.trigger() != trigger_type {
                    return None;
                }

                // Damage trigger: optionally ignore immune/resist hits
                if is_immune_or_resist && refresh_ability.ignore_immune_resist() {
                    return None;
                }

                Some(RefreshableEffect {
                    id: def.id.clone(),
                    name: def.name.clone(),
                    display_text: def.display_text().to_string(),
                    duration: self.effective_duration(def, timestamp),
                    color: def.effective_color(),
                    display_targets: def.display_targets.clone(),
                    icon_ability_id: def.icon_ability_id.unwrap_or(action_id as u64),
                    show_at_secs: def.show_at_secs,
                    show_icon: def.show_icon,
                    display_source: def.display_source,
                    show_target: def.show_target,
                    cooldown_ready_secs: def.cooldown_ready_secs,
                    audio: def.audio.clone(),
                    alert_text: def.alert_text.clone(),
                    alert_on_expire: def.alert_on == AlertTrigger::OnExpire,
                    default_charges: def.default_charges,
                    min_stacks: refresh_ability.min_stacks(),
                    refresh_scope: def.refresh_scope,
                    single_instance_per_target: def.single_instance_per_target,
                    defer_to_first_damage: refresh_ability.refresh_on_first_damage(
                        def.display_targets.contains(&DisplayTarget::DotTracker),
                    ),
                })
            })
            .collect();

        for def in refreshable_defs {
            // First-damage refreshes (non-AoE) defer to the next DamageTaken event.
            // Instead of refreshing immediately on AbilityActivated, set pending state
            // that will be consumed when damage from this ability lands on a target.
            // Whether to defer is resolved per refresh ability (`on_first_damage`),
            // defaulting to true for DotTracker effects and false otherwise.
            // AoE refresh abilities are handled separately by the existing AoE damage
            // correlation path (setup_pending_aoe_refresh / handle_damage_for_aoe_refresh).
            if trigger_type == RefreshTrigger::Activation && def.defer_to_first_damage {
                self.pending_dot_refresh = Some(PendingDotRefresh {
                    ability_id: action_id,
                    source_id,
                    timestamp,
                    // The game logs a TargetSet on self alongside a fresh application,
                    // so a self snapshot carries no information — fall back to first damage.
                    target_id: current_target_id.filter(|&t| t != 0 && Some(t) != local_player_id),
                });
                continue;
            }

            // Single-instance buffs (Kolto Shell, Trauma Probe): the recast may be
            // silently refreshing any caster's instance, so look up source-agnostically
            // across the def group. When nothing is tracked, the cast may have refreshed
            // an instance whose apply predates the log (healer zoned in earlier) — don't
            // fabricate an instance with wrong credit; just make sure the target is on
            // the raid frames. The true instance is created by the next
            // ApplyEffect (fresh cast) or ModifyCharges (untracked instance).
            if def.single_instance_per_target {
                let group_ids: Vec<&str> =
                    single_instance_ids.iter().map(String::as_str).collect();
                let existing = Self::find_single_instance_effect(
                    &mut self.active_effects,
                    &group_ids,
                    target_id,
                );
                if let Some(effect) = existing.filter(|e| e.removed_at.is_none()) {
                    if let Some(min_stacks) = def.min_stacks
                        && effect.stacks < min_stacks
                    {
                        continue;
                    }
                    // Original caster keeps credit — only the duration refreshes.
                    effect.refresh(timestamp, def.duration);
                }
                if def.display_targets.contains(&DisplayTarget::RaidFrames) && can_register {
                    self.new_targets.push(NewTargetInfo {
                        entity_id: target_id,
                        name: target_name,
                    });
                }
                continue;
            }

            let key = EffectKey::for_scope(&def.id, def.refresh_scope, source_id, target_id);
            // Fallback: if the resolved target is an NPC, also try source_id as target.
            // Handles self-cast abilities (e.g. Dark Ward) where target resolution
            // resolves to the combat target but the effect is keyed to the caster.
            // Only for NPC targets — player targets are intentional (e.g. heals).
            let fallback_key = if target_id != source_id
                && target_entity_type != EntityType::Player
            {
                Some(EffectKey::for_scope(&def.id, def.refresh_scope, source_id, source_id))
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
                if def.display_targets.contains(&DisplayTarget::RaidFrames) && can_register {
                    self.new_targets.push(NewTargetInfo {
                        entity_id: target_id,
                        name: target_name,
                    });
                }
            } else if def.display_targets.contains(&DisplayTarget::RaidFrames) {
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
                    self.local_player_id == Some(source_id),
                    timestamp,
                    def.duration,
                    def.color,
                    def.display_targets.clone(),
                    def.icon_ability_id,
                    def.show_at_secs,
                    def.show_icon,
                    def.display_source,
                    def.show_target,
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
                if def.display_targets.contains(&DisplayTarget::RaidFrames) && can_register {
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
        source_id: i64,
        timestamp: NaiveDateTime,
        primary_target: i64,
    ) {
        if self.definitions.is_aoe_refresh(ability_id as u64) {
            self.pending_aoe_refresh = Some(PendingAoeRefresh {
                ability_id,
                source_id,
                timestamp,
                primary_target,
            });
            self.aoe_collecting = None;
        }
    }

    /// Handle damage event for AoE refresh correlation.
    ///
    /// Two paths based on `aoe_refresh_immediate`:
    /// - Strict (default): anchor+window collection to prevent dot ticks from
    ///   false-refreshing. Only damage within ±10ms of the primary target hit refreshes.
    /// - Immediate (`aoe_refresh_immediate = true`): any damage from the ability
    ///   after activation refreshes the target directly.
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

        // Check if we're in collecting state and this damage is within window (DOT path)
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
                // Outside window - finalize DOT refreshes on all collected targets
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

        let source_id = pending.source_id;
        let primary_target = pending.primary_target;

        // Immediate path: refresh effects with aoe_refresh_immediate on any damage hit
        self.refresh_aoe_immediate(ability_id, source_id, target_id, timestamp);

        // DOT path: anchor on primary target then collect within window
        if target_id == primary_target {
            self.aoe_collecting = Some(AoeRefreshCollecting {
                ability_id,
                source_id,
                anchor_timestamp: timestamp,
                targets: vec![target_id],
            });
            self.pending_aoe_refresh = None;
        }
    }

    /// Immediately refresh effects with `aoe_refresh_immediate` on the damaged target.
    /// These effects have no ongoing ticks that could cause false refreshes,
    /// so any damage from the ability after activation is a valid refresh trigger.
    fn refresh_aoe_immediate(
        &mut self,
        ability_id: i64,
        source_id: i64,
        target_id: i64,
        timestamp: NaiveDateTime,
    ) {
        let refreshable: Vec<_> = self
            .definitions
            .find_refreshable_by(ability_id as u64, None)
            .into_iter()
            .filter(|def| def.aoe_refresh_immediate)
            .map(|def| (def.id.clone(), def.refresh_scope, self.effective_duration(def, timestamp)))
            .collect();

        for (def_id, scope, duration) in &refreshable {
            let key = EffectKey::for_scope(def_id, *scope, source_id, target_id);
            if let Some(effect) = self.active_effects.get_mut(&key) {
                effect.refresh(timestamp, *duration);
            }
        }
    }

    /// Finalize AoE refresh - refresh strict-mode effects on all collected targets.
    /// Effects without `aoe_refresh_immediate` use the anchor+window collection
    /// to prevent ongoing dot ticks from causing false refreshes.
    fn finalize_aoe_refresh(&mut self) {
        let Some(collecting) = self.aoe_collecting.take() else {
            return;
        };

        let refreshable_def_ids: Vec<_> = self
            .definitions
            .find_refreshable_by(collecting.ability_id as u64, None)
            .into_iter()
            .filter(|def| !def.aoe_refresh_immediate)
            .map(|def| (def.id.clone(), def.refresh_scope, self.effective_duration(def, collecting.anchor_timestamp)))
            .collect();

        for target_id in collecting.targets {
            for (def_id, scope, duration) in &refreshable_def_ids {
                let key = EffectKey::for_scope(def_id, *scope, collecting.source_id, target_id);
                if let Some(effect) = self.active_effects.get_mut(&key) {
                    effect.refresh(collecting.anchor_timestamp, *duration);
                }
            }
        }
    }

    /// Handle damage event for single-target DotTracker refresh confirmation.
    ///
    /// When a refresh ability is cast for a DotTracker effect, the refresh is deferred
    /// until the next damage event from that ability lands. This confirms which target
    /// was actually hit and refreshes the DOT on that specific target only.
    fn handle_damage_for_dot_refresh(
        &mut self,
        ability_id: i64,
        target_id: i64,
        timestamp: NaiveDateTime,
    ) {
        const PENDING_TIMEOUT_MS: i64 = 2000;

        let Some(ref pending) = self.pending_dot_refresh else {
            return;
        };

        if pending.ability_id != ability_id {
            return;
        }

        // Skip self-targeted events (e.g. lifesteal heals on the caster that share the
        // same ability ID as the DOT). Don't consume pending — let the actual enemy hit claim it.
        if Some(target_id) == self.local_player_id {
            return;
        }

        // Check if pending has timed out
        let elapsed_ms = (timestamp - pending.timestamp).num_milliseconds();
        if elapsed_ms > PENDING_TIMEOUT_MS {
            self.pending_dot_refresh = None;
            return;
        }

        // With a target snapshot, only that target's hit confirms the refresh.
        // Other targets ticking the same DOT are bystanders — leave pending intact.
        if pending.target_id.is_some_and(|t| t != target_id) {
            return;
        }

        let source_id = pending.source_id;
        // Consume pending state — the confirming damage event triggers the refresh
        self.pending_dot_refresh = None;

        // Find all definitions whose refresh ability is configured to defer to the
        // first damage event from this ability, and refresh the effect on the
        // damaged target. Defaults to DotTracker effects when `on_first_damage`
        // is unspecified, but any display target can opt in.
        let refreshable_def_ids: Vec<_> = self
            .definitions
            .find_refreshable_by(ability_id as u64, None)
            .into_iter()
            .filter(|def| {
                let is_dot_tracker = def.display_targets.contains(&DisplayTarget::DotTracker);
                def.find_refresh_ability(ability_id as u64, None)
                    .is_some_and(|ra| ra.refresh_on_first_damage(is_dot_tracker))
            })
            .map(|def| (def.id.clone(), def.refresh_scope, self.effective_duration(def, timestamp)))
            .collect();

        for (def_id, scope, duration) in &refreshable_def_ids {
            let key = EffectKey::for_scope(def_id, *scope, source_id, target_id);
            if let Some(effect) = self.active_effects.get_mut(&key) {
                if effect.removed_at.is_none() {
                    effect.refresh(timestamp, *duration);
                }
            }
        }
    }

    /// Drop a deferred DOT refresh once its cast has been confirmed by a logged
    /// ApplyEffect on the same target (or on any target when no snapshot was taken).
    /// Takes the field directly so it can run while `self.definitions` is borrowed.
    fn clear_pending_dot_refresh(pending: &mut Option<PendingDotRefresh>, action_id: i64, target_id: i64) {
        if pending
            .as_ref()
            .is_some_and(|p| p.ability_id == action_id && p.target_id.is_none_or(|t| t == target_id))
        {
            *pending = None;
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
        let pvp_factions = encounter.and_then(|e| e.pvp_faction_context());
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
                    pvp_factions,
                )
            {
                continue;
            }

            if !self.matches_entity_disciplines(
                &def.source_disciplines,
                source_info.id,
                source_info.entity_type,
            ) {
                continue;
            }

            // Instant alerts: fire and skip — no ActiveEffect created
            if def.is_alert {
                self.fired_alerts.push(Self::build_instant_alert(def, timestamp, source_name, target_name));
                continue;
            }

            // Validate resolved target against the definition's target filter
            if !def.target_filter().matches(
                entities,
                target_id,
                target_entity_type,
                target_name,
                0,
                local_player_id,
                current_target_id,
                &boss_ids,
                pvp_factions,
            ) {
                continue;
            }

            if !self.matches_entity_disciplines(&def.target_disciplines, target_id, target_entity_type) {
                continue;
            }

            // AbilityCast trigger matched — track the effect on the caster (source)
            let key = EffectKey::for_scope(&def.id, def.refresh_scope, source_id, source_id);

            let duration = self.effective_duration(def, timestamp);

            if let Some(existing) = self.active_effects.get_mut(&key) {
                // Ignore refreshes: skip retrigger only while still in base duration.
                // Once the cooldown enters the ready state it has effectively expired,
                // so a new trigger is a fresh activation, not a refresh.
                if def.ignore_refreshes && existing.is_in_base_duration(timestamp) {
                    continue;
                }

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
                        icon_ability_id: def.icon_ability_id,
                    remaining_secs: None,
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
                    source_id,
                    source_name,
                    is_from_local,
                    timestamp,
                    duration,
                    def.effective_color(),
                    def.display_targets.clone(),
                    icon_ability_id,
                    def.show_at_secs,
                    def.show_icon,
                    def.display_source,
                    def.show_target,
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
                        icon_ability_id: def.icon_ability_id,
                    remaining_secs: None,
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
        self.advance_game_time_anchor(timestamp);
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
                self.fired_alerts.push(Self::build_instant_alert(def, timestamp, source_name, target_name));
                continue;
            }

            let key = EffectKey::for_scope(&def.id, def.refresh_scope, source_id, target_id);
            let duration = self.effective_duration(def, timestamp);

            if let Some(existing) = self.active_effects.get_mut(&key) {
                // Ignore refreshes: skip retrigger only while still in base duration.
                // Once the cooldown enters the ready state it has effectively expired,
                // so a new trigger is a fresh activation, not a refresh.
                if def.ignore_refreshes && existing.is_in_base_duration(timestamp) {
                    continue;
                }

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
                        icon_ability_id: def.icon_ability_id,
                    remaining_secs: None,
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
                    def.display_targets.clone(),
                    icon_ability_id,
                    def.show_at_secs,
                    def.show_icon,
                    def.display_source,
                    def.show_target,
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
                        icon_ability_id: def.icon_ability_id,
                    remaining_secs: None,
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
        self.advance_game_time_anchor(timestamp);
        let local_player_id = self.local_player_id;

        // Alacrity buff fell off the local player
        if local_player_id == Some(target_id) {
            self.alacrity_buffs.on_removed(effect_id);
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

        let matching_defs: Vec<_> = self
            .definitions
            .find_matching(effect_id as u64, Some(effect_name_str))
            .into_iter()
            .collect();

        // Source-agnostic lookup group for single-instance buffs (see
        // handle_effect_applied) — a merged instance keeps the original
        // caster's key, which may not match this signal's source.
        let single_instance_ids: Vec<&str> = matching_defs
            .iter()
            .filter(|d| d.single_instance_per_target)
            .map(|d| d.id.as_str())
            .collect();

        let is_from_local = local_player_id == Some(source_id);

        for def in &matching_defs {
            let key = EffectKey::for_scope(&def.id, def.refresh_scope, source_id, target_id);

            if def.is_effect_applied_trigger() {
                // Mark existing effect as removed (normal behavior)
                // Skip if ignore_effect_removed OR cooldowns (cooldowns always use timer-based expiry)
                let is_cooldown = def.display_targets.contains(&DisplayTarget::Cooldowns)
                    || def.display_targets.contains(&DisplayTarget::CooldownsB);
                if def.ignore_effect_removed || is_cooldown {
                    continue;
                }
                let matched = if self.active_effects.contains_key(&key) {
                    self.active_effects.get_mut(&key)
                } else if def.single_instance_per_target {
                    Self::find_single_instance_effect(
                        &mut self.active_effects,
                        &single_instance_ids,
                        target_id,
                    )
                } else {
                    None
                };
                if let Some(effect) = matched {
                    // Only honor removal if it occurred well AFTER the last refresh.
                    // DOT reapplication sends ApplyEffect then RemoveEffect - sometimes
                    // the RemoveEffect arrives up to ~1 second later (for the old DOT instance).
                    // Use a 1 second window to ignore stale RemoveEffect signals.
                    let since_refresh_ms = timestamp
                        .signed_duration_since(effect.last_refreshed_at)
                        .num_milliseconds();
                    if since_refresh_ms > 1000 {
                        // Only decrement ticking_count if the effect hasn't already been
                        // counted as expired by tick(). Timer-expired effects already had
                        // their count decremented when timer_expired was set.
                        if effect.mark_removed() && !effect.timer_expired {
                            self.ticking_count = self.ticking_count.saturating_sub(1);
                        }
                    }
                }
            } else if def.is_effect_removed_trigger()
                && self.matches_filters(def, source_info, target_info, encounter)
            {
                // Instant alerts: fire and skip — no ActiveEffect created
                if def.is_alert {
                    self.fired_alerts.push(Self::build_instant_alert(def, timestamp, source_name, target_name));
                    continue;
                }

                // Create new effect when the game effect is removed (cooldown tracking)
                let duration = self.effective_duration(def, timestamp);
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
                    def.display_targets.clone(),
                    icon_ability_id,
                    def.show_at_secs,
                    def.show_icon,
                    def.display_source,
                    def.show_target,
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
                        icon_ability_id: def.icon_ability_id,
                    remaining_secs: None,
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
        source_entity_type: EntityType,
        source_name: IStr,
        source_npc_id: i64,
        target_id: i64,
        target_entity_type: EntityType,
        target_name: IStr,
        target_npc_id: i64,
        timestamp: NaiveDateTime,
        charges: u8,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
        self.advance_game_time_anchor(timestamp);

        // Stack count changed on a local-player alacrity buff
        if self.local_player_id == Some(target_id) {
            self.alacrity_buffs.on_charges_changed(effect_id, charges, timestamp);
        }

        // Entity info for filter matching (single-instance creation below)
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

        // Find matching definitions (by ID or name)
        let effect_name_str = crate::context::resolve(effect_name);
        let matching_defs: Vec<_> = self
            .definitions
            .find_matching(effect_id as u64, Some(effect_name_str))
            .into_iter()
            .collect();

        // Source-agnostic lookup group for single-instance buffs (see
        // handle_effect_applied) — a merged instance keeps the original
        // caster's key, which may not match this signal's source.
        let single_instance_ids: Vec<&str> = matching_defs
            .iter()
            .filter(|d| d.single_instance_per_target)
            .map(|d| d.id.as_str())
            .collect();

        // Effects removed by `cancel` modifiers (ticking_count is adjusted after the
        // loop since `effect` holds a mutable borrow of active_effects).
        let mut cancelled = 0usize;

        for def in matching_defs {
            let key = EffectKey::for_scope(&def.id, def.refresh_scope, source_id, target_id);

            // Calculate duration before borrowing active_effects mutably.
            // Needed for the blanket is_refreshed_on_modify refresh AND for
            // refill_duration modifiers (which refill without is_refreshed_on_modify).
            let needs_duration =
                def.is_refreshed_on_modify || def.modifiers.iter().any(|m| m.refill_duration);
            let duration = if needs_duration { self.effective_duration(def, timestamp) } else { None };
            let icds: Vec<Option<f32>> = def.modifiers.iter().map(|m| self.effective_icd(m, timestamp)).collect();

            if let Some(effect) = if self.active_effects.contains_key(&key) {
                self.active_effects.get_mut(&key)
            } else if def.single_instance_per_target {
                Self::find_single_instance_effect(
                    &mut self.active_effects,
                    &single_instance_ids,
                    target_id,
                )
            } else {
                None
            } {
                let old_stacks = effect.stacks;
                effect.set_stacks(charges);

                // Refresh duration on ModifyCharges if is_refreshed_on_modify is set.
                if def.is_refreshed_on_modify && let Some(dur) = duration {
                    effect.refresh_duration(timestamp, dur);
                }

                // Evaluate SelfChargesChanged modifiers
                if !def.modifiers.is_empty() {
                    let modifier_count = def.modifiers.len();
                    effect.ensure_modifier_icd(modifier_count);

                    for (mod_idx, modifier) in def.modifiers.iter().enumerate() {
                        if !super::modifier_match::matches_self_charges(&modifier.trigger, old_stacks, charges) {
                            continue;
                        }
                        if !effect.modifier_icd_ready(mod_idx, icds[mod_idx], modifier.icd_from_application, timestamp) {
                            continue;
                        }
                        effect.modifier_last_proc[mod_idx] = Some(timestamp);

                        if modifier.cancel {
                            if effect.mark_removed() && !effect.timer_expired {
                                cancelled += 1;
                            }
                            break;
                        }

                        // Apply duration adjustment
                        if let Some(expires) = effect.expires_at {
                            let mut new_expires = if modifier.refill_duration {
                                if let Some(dur) = duration {
                                    timestamp + dur
                                } else {
                                    continue;
                                }
                            } else if modifier.adjust_duration_secs != 0.0 {
                                let delta = chrono::Duration::milliseconds(
                                    (modifier.adjust_duration_secs * 1000.0) as i64,
                                );
                                expires + delta
                            } else {
                                continue;
                            };
                            if let Some(max) = modifier.max_duration_secs {
                                let cap = effect.applied_at + chrono::Duration::milliseconds((max * 1000.0) as i64);
                                new_expires = new_expires.min(cap);
                                if effect.max_expires_at.is_none() {
                                    effect.max_expires_at = Some(cap);
                                }
                            }
                            new_expires = new_expires.max(timestamp);
                            effect.expires_at = Some(new_expires);
                            // NOTE: `duration` (the fill denominator) is intentionally
                            // left at the base/effective duration. fill_percent clamps
                            // to 1.0, so an extension shows a full bar until remaining
                            // drops back below the base duration. Inflating `duration`
                            // here folded elapsed time into the denominator and shrank
                            // the bar on every proc.
                            if modifier.refill_duration || modifier.adjust_duration_secs > 0.0 {
                                effect.audio_played = false;
                                effect.countdown_announced = [false; 10];
                                effect.on_end_alert_fired = false;
                            }
                        }
                    }
                }
            } else if def.single_instance_per_target
                && def.is_effect_applied_trigger()
                && !def.is_alert
                && self.matches_filters(def, source_info, target_info, encounter)
            {
                // Nothing tracked on this target: the original apply predates the
                // log (e.g. the local player zoned in after another healer applied).
                // ModifyCharges carries the true caster as source, so create the
                // instance here with correct credit. Real remaining duration is
                // unknown — start a full one and let refreshes/RemoveEffect
                // correct it. matches_filters picks the local vs `_others` def,
                // so the pair creates at most one instance.
                let duration = self.effective_duration(def, timestamp);
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
                    self.local_player_id == Some(source_id),
                    timestamp,
                    duration,
                    def.effective_color(),
                    def.display_targets.clone(),
                    icon_ability_id,
                    def.show_at_secs,
                    def.show_icon,
                    def.display_source,
                    def.show_target,
                    def.cooldown_ready_secs,
                    &def.audio,
                    def.alert_text.clone(),
                    def.alert_on == AlertTrigger::OnExpire,
                );
                effect.set_stacks(charges);
                self.active_effects.insert(key, effect);
                self.ticking_count += 1;
            }
        }
        self.ticking_count = self.ticking_count.saturating_sub(cancelled);
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
            if !persist && effect.mark_removed() && !effect.timer_expired {
                self.ticking_count = self.ticking_count.saturating_sub(1);
            }
        }
    }

    /// Handle combat end - optionally clear combat-only effects
    fn handle_combat_ended(&mut self) {
        // Clear pending refresh state
        self.pending_aoe_refresh = None;
        self.aoe_collecting = None;
        self.pending_dot_refresh = None;

        // Mark effects that don't track outside combat as removed
        let outside_combat_ids: HashSet<&str> = self
            .definitions
            .enabled()
            .filter(|def| def.track_outside_combat)
            .map(|def| def.id.as_str())
            .collect();

        for (key, effect) in self.active_effects.iter_mut() {
            if !outside_combat_ids.contains(key.definition_id.as_str()) {
                if effect.mark_removed() && !effect.timer_expired {
                    self.ticking_count = self.ticking_count.saturating_sub(1);
                }
            }
        }
    }

    /// Handle area change (zone transition) - clear all active effects
    fn handle_area_change(&mut self) {
        // Clear pending refresh state
        self.pending_aoe_refresh = None;
        self.aoe_collecting = None;
        self.pending_dot_refresh = None;
        self.alacrity_buffs.clear();

        for (_key, effect) in self.active_effects.iter_mut() {
            if effect.mark_removed() && !effect.timer_expired {
                self.ticking_count = self.ticking_count.saturating_sub(1);
            }
        }
    }

    /// Check a source/target discipline filter against an entity.
    /// Empty filter = no constraint. Non-empty: the entity must be a player
    /// with a known discipline matching one of the entries — NPCs, companions,
    /// and players whose discipline hasn't been seen yet never match.
    fn matches_entity_disciplines(
        &self,
        filters: &[DisciplineFilter],
        entity_id: i64,
        entity_type: EntityType,
    ) -> bool {
        if filters.is_empty() {
            return true;
        }
        entity_type == EntityType::Player
            && self
                .player_disciplines
                .get(&entity_id)
                .is_some_and(|d| filters.iter().any(|f| f.matches(*d)))
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

        // Check source/target discipline filters
        if !self.matches_entity_disciplines(&def.source_disciplines, source.id, source.entity_type)
            || !self.matches_entity_disciplines(&def.target_disciplines, target.id, target.entity_type)
        {
            return false;
        }

        // Get local player ID from self, boss entity IDs from encounter
        let local_player_id = self.local_player_id;
        let current_target_id =
            local_player_id.and_then(|id| self.current_targets.get(&id).map(|(tid, _, _)| *tid));
        let boss_ids = get_boss_ids(encounter);

        let entities = get_entities(encounter);
        let pvp_factions = encounter.and_then(|e| e.pvp_faction_context());

        def.source_filter().matches(
            entities,
            source.id,
            source.entity_type,
            source.name,
            source.npc_id,
            local_player_id,
            current_target_id,
            &boss_ids,
            pvp_factions,
        ) && def.target_filter().matches(
            entities,
            target.id,
            target.entity_type,
            target.name,
            target.npc_id,
            local_player_id,
            current_target_id,
            &boss_ids,
            pvp_factions,
        )
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Modifier Evaluation
    // ═══════════════════════════════════════════════════════════════════════════

    fn evaluate_modifiers(&mut self, signals: &[GameSignal]) {

        // Seed last_seen_charges from EffectApplied so first ChargesChanged has a baseline
        for s in signals {
            if let GameSignal::EffectApplied { effect_id, target_id, charges, .. } = s {
                if let Some(c) = charges {
                    if *c > 0 {
                        self.last_seen_charges.entry((*effect_id, *target_id)).or_insert(*c);
                    }
                }
            }
        }

        // Snapshot old charges for direction comparison, then update to new values
        let mut charge_snapshots: Vec<((i64, i64), u8, u8)> = Vec::new();
        for s in signals {
            if let GameSignal::EffectChargesChanged { effect_id, target_id, charges, .. } = s {
                let key = (*effect_id, *target_id);
                let old = self.last_seen_charges.get(&key).copied().unwrap_or(*charges);
                charge_snapshots.push((key, old, *charges));
                self.last_seen_charges.insert(key, *charges);
            }
        }

        // Collect modifications to apply (avoids borrow conflict on self)
        let mut adjustments: Vec<(EffectKey, ModifierAdjustment)> = Vec::new();

        for (key, effect) in &self.active_effects {
            // timer_expired is an interpolated estimate — a log event can still
            // prove the effect alive (checked per-hit below against log time).
            if effect.removed_at.is_some() {
                continue;
            }
            let Some(def) = self.definitions.effects.get(&key.definition_id) else {
                continue;
            };
            if def.modifiers.is_empty() {
                continue;
            }

            for (mod_idx, modifier) in def.modifiers.iter().enumerate() {
                // One hit per matching signal, stamped with that signal's log time
                let mut hits: Vec<super::modifier_match::Hit> = Vec::new();
                super::modifier_match::collect_hits(
                    &modifier.trigger,
                    modifier.requires_crit,
                    effect.target_entity_id,
                    signals,
                    &charge_snapshots,
                    &mut hits,
                );

                for hit in hits {
                    adjustments.push((key.clone(), ModifierAdjustment {
                        mod_idx,
                        at: hit.at,
                        adjust_duration_secs: modifier.adjust_duration_secs * hit.scale,
                        refill_duration: modifier.refill_duration,
                        icd_secs: self.effective_icd(modifier, hit.at),
                        icd_from_application: modifier.icd_from_application,
                        max_duration_secs: modifier.max_duration_secs,
                        cancel: modifier.cancel,
                    }));
                }
            }
        }

        // Apply collected adjustments
        for (key, adj) in adjustments {
            // Pre-compute effective duration for refill before mutable borrow
            let refill_dur = if adj.refill_duration {
                self.definitions.effects.get(&key.definition_id)
                    .and_then(|d| self.effective_duration(d, adj.at))
            } else {
                None
            };

            let Some(effect) = self.active_effects.get_mut(&key) else { continue };
            // Removed by an earlier hit in this batch (cancel), or genuinely
            // expired in log time before this hit — nothing to modify.
            if effect.removed_at.is_some()
                || (effect.timer_expired && effect.expires_at.is_some_and(|e| e < adj.at))
            {
                continue;
            }
            let modifier_count = self.definitions.effects.get(&key.definition_id)
                .map(|d| d.modifiers.len())
                .unwrap_or(0);
            effect.ensure_modifier_icd(modifier_count);

            if !effect.modifier_icd_ready(adj.mod_idx, adj.icd_secs, adj.icd_from_application, adj.at) {
                continue;
            }

            // Record proc time
            effect.modifier_last_proc[adj.mod_idx] = Some(adj.at);

            // Cancel: remove the effect outright
            if adj.cancel {
                if effect.mark_removed() && !effect.timer_expired {
                    self.ticking_count = self.ticking_count.saturating_sub(1);
                }
                continue;
            }

            // Apply duration adjustment
            if effect.expires_at.is_some() {
                let new_expires = if adj.refill_duration {
                    if let Some(dur) = refill_dur {
                        adj.at + dur
                    } else {
                        continue;
                    }
                } else if adj.adjust_duration_secs != 0.0 {
                    let delta = chrono::Duration::milliseconds((adj.adjust_duration_secs * 1000.0) as i64);
                    effect.expires_at.unwrap() + delta
                } else {
                    continue;
                };

                // Clamp to max/min relative to applied_at
                let clamped = if let Some(max) = adj.max_duration_secs {
                    let max_expires = effect.applied_at + chrono::Duration::milliseconds((max * 1000.0) as i64);
                    if effect.max_expires_at.is_none() {
                        effect.max_expires_at = Some(max_expires);
                    }
                    new_expires.min(max_expires)
                } else {
                    new_expires
                };
                // Don't let expires_at go into the past
                let final_expires = clamped.max(adj.at);
                effect.expires_at = Some(final_expires);
                // NOTE: `duration` (the fill denominator) is intentionally left at the
                // base/effective duration. fill_percent clamps to 1.0, so an extension
                // shows a full bar until remaining drops back below the base duration.
                // Inflating `duration` here folded elapsed time into the denominator and
                // shrank the bar on every proc.

                // Reset audio/alert state if duration was extended
                if adj.refill_duration || adj.adjust_duration_secs > 0.0 {
                    effect.audio_played = false;
                    effect.countdown_announced = [false; 10];
                    effect.on_end_alert_fired = false;
                }

                // Revive an interpolation-expired effect: the log proves it was
                // still active at `adj.at`. Restore ticking_count (decremented
                // when timer_expired was set in tick()).
                if effect.timer_expired && final_expires > adj.at {
                    effect.timer_expired = false;
                    self.ticking_count += 1;
                }
            }
        }
    }
}

struct ModifierAdjustment {
    mod_idx: usize,
    /// Log time of the triggering signal
    at: NaiveDateTime,
    adjust_duration_secs: f32,
    refill_duration: bool,
    icd_secs: Option<f32>,
    icd_from_application: bool,
    max_duration_secs: Option<f32>,
    cancel: bool,
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
        // Drop alacrity buffs that outlived their duration (missed remove events).
        // Log time, not the interpolated anchor — see `effective_duration`.
        if let Some(last) = signals.last() {
            self.alacrity_buffs.prune_expired(last.timestamp());
        }
        // Evaluate modifiers on active effects against this batch of signals
        self.evaluate_modifiers(signals);
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
                source_entity_type,
                source_name,
                source_npc_id,
                target_id,
                target_entity_type,
                target_name,
                target_npc_id,
                timestamp,
                charges,
            } => {
                self.handle_charges_changed(
                    *effect_id,
                    *effect_name,
                    *action_id,
                    *action_name,
                    *source_id,
                    *source_entity_type,
                    *source_name,
                    *source_npc_id,
                    *target_id,
                    *target_entity_type,
                    *target_name,
                    *target_npc_id,
                    *timestamp,
                    *charges,
                    encounter,
                );
            }
            GameSignal::EntityDeath { entity_id, .. } => {
                self.handle_entity_death(*entity_id);
            }
            GameSignal::CombatEnded { .. } => {
                self.handle_combat_ended();
            }
            GameSignal::AreaEntered { area_id, .. } => {
                // Instanced areas fire AreaEntered twice (plain + difficulty)
                // around the game's effect restatement — only wipe on an
                // actual area change.
                if self.last_area_id != Some(*area_id) {
                    self.last_area_id = Some(*area_id);
                    self.handle_area_change();
                }
            }
            GameSignal::DisciplineChanged {
                entity_id,
                discipline_id,
                ..
            } => {
                // Track per-player disciplines for source/target discipline filters
                let discipline = Discipline::from_guid(*discipline_id);
                if let Some(d) = discipline {
                    self.player_disciplines.insert(*entity_id, d);
                } else {
                    self.player_disciplines.remove(entity_id);
                }
                // Track local player's discipline for discipline-scoped effects
                if self.local_player_id == Some(*entity_id) {
                    self.local_player_discipline = discipline;
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
                self.advance_game_time_anchor(*timestamp);

                // Resolve actual target: the signal already has a resolved target from
                // encounter state, but our local current_targets cache may be more
                // up-to-date (works outside combat). Apply additional resolution if
                // the signal still reports self-targeting.
                let is_self_or_empty = *target_id == 0 || *target_id == *source_id;
                let (resolved_target, resolved_target_name, resolved_entity_type) = if is_self_or_empty {
                    if let Some((target, name, etype)) =
                        self.current_targets.get(source_id).copied()
                    {
                        (target, name, etype)
                    } else if let Some(target) =
                        encounter.and_then(|e| e.get_current_target(*source_id))
                    {
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
                        (*source_id, *source_name, *source_entity_type)
                    }
                } else {
                    (*target_id, *target_name, *target_entity_type)
                };

                // Handle AbilityCast-triggered effects (procs, cooldowns)
                self.handle_ability_cast(
                    *ability_id,
                    *ability_name,
                    *source_id,
                    *source_name,
                    *source_entity_type,
                    *source_npc_id,
                    resolved_target,
                    resolved_target_name,
                    resolved_entity_type,
                    *timestamp,
                    encounter,
                );

                self.refresh_effects_by_action(
                    *ability_id,
                    *ability_name,
                    *source_id,
                    *source_name,
                    *source_entity_type,
                    resolved_target,
                    resolved_target_name,
                    resolved_entity_type,
                    *timestamp,
                    encounter,
                    RefreshTrigger::Activation,
                    false,
                );

                // For AoE abilities, set up pending state for damage correlation
                self.setup_pending_aoe_refresh(*ability_id, *source_id, *timestamp, resolved_target);
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
                defense_type_id,
                ..
            } => {
                // AoE refresh damage correlation
                self.handle_damage_for_aoe_refresh(*ability_id, *target_id, *timestamp);
                // Single-target DotTracker refresh damage confirmation
                self.handle_damage_for_dot_refresh(*ability_id, *target_id, *timestamp);
                // Refresh abilities configured with the Damage trigger
                let is_immune_or_resist = *defense_type_id
                    == crate::game_data::defense_type::IMMUNE
                    || *defense_type_id == crate::game_data::defense_type::RESIST;
                self.refresh_effects_by_action(
                    *ability_id,
                    *ability_name,
                    *source_id,
                    *source_name,
                    *source_entity_type,
                    *target_id,
                    *target_name,
                    *target_entity_type,
                    *timestamp,
                    encounter,
                    RefreshTrigger::Damage,
                    is_immune_or_resist,
                );
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
                ..
            } => {
                // Refresh effects on heal completion
                self.refresh_effects_by_action(
                    *ability_id,
                    *ability_name,
                    *source_id,
                    *source_name,
                    *source_entity_type,
                    *target_id,
                    *target_name,
                    *target_entity_type,
                    *timestamp,
                    encounter,
                    RefreshTrigger::Heal,
                    false,
                );
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
