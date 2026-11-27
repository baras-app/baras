//! Effect tracking handler
//!
//! Tracks active effects on entities by matching game signals against
//! configured effect definitions. Produces `ActiveEffect` instances
//! that can be fed to overlay renderers.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use chrono::NaiveDateTime;

use crate::combat_log::EntityType;
use crate::context::IStr;
use crate::encounter::{CombatEncounter, PvpFaction};
use crate::game_data::{Discipline, DisciplineFilter};
use crate::signal_processor::{GameSignal, SignalHandler};

use crate::timers::FiredAlert;

use super::alacrity::AlacrityBuffTracker;
use super::alerts;
use super::filter_ctx::{EntityInfo, FilterCtx};
use super::modifier_adjust::ModifierAdjustment;
use super::{
    ActiveEffect, AlertTrigger, DefId, DefinitionSet, DisplayTarget, EffectDefinition, EffectKey, RefreshTrigger,
};

/// Grace period (ms) after the app's duration timer expires before hard-removing
/// an effect from the tracker. During this window, `refresh_abilities` can still
/// revive the effect — the timer is a heuristic and the in-game buff may outlast it.
/// An authoritative `EffectRemoved` signal always removes immediately, bypassing this.
const TIMER_EXPIRY_GRACE_MS: i64 = 8000;

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
    /// Its name, for refresh abilities selected by name
    ability_name: IStr,
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
        let alacrity = if modifier.icd_affected_by_alacrity {
            self.total_alacrity(at)
        } else {
            0.0
        };
        Some(if alacrity > 0.0 {
            icd / (1.0 + alacrity / 100.0)
        } else {
            icd
        })
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
        group: &[DefId],
        target_id: i64,
    ) -> Option<&'a mut ActiveEffect> {
        active_effects
            .iter_mut()
            .find(|(k, e)| e.target_entity_id == target_id && group.contains(&k.def))
            .map(|(_, e)| e)
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
        // Re-key onto the new set's ids; drop effects whose definitions are
        // now disabled or deleted, refresh the rest's cached settings
        let old = std::mem::take(&mut self.active_effects);
        self.active_effects = old
            .into_iter()
            .filter_map(|(key, mut effect)| {
                let def_id = definitions.id_of(&effect.definition_id)?;
                let def = definitions.def(def_id);
                if !def.enabled {
                    return None;
                }
                // An effect switched to OnExpire mid-flight must not fire an
                // unexpected end alert for an instance that started without one
                let was_on_expire = self.definitions.def(key.def).alert_on == AlertTrigger::OnExpire;
                if def.alert_on == AlertTrigger::OnExpire && !was_on_expire {
                    effect.on_end_alert_fired = true;
                }
                effect.apply_definition(def);
                Some((EffectKey { def: def_id, ..key }, effect))
            })
            .collect();

        self.definitions = definitions;
    }

    /// Check if there are any active effects (cheap check before full iteration)
    pub fn has_active_effects(&self) -> bool {
        !self.active_effects.is_empty()
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
                if event_timestamp > interp {
                    event_timestamp
                } else {
                    interp
                }
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
    pub fn uptime_definitions(&self, target: DisplayTarget) -> Vec<&super::EffectDefinition> {
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
    pub fn definition(&self, id: &str) -> Option<&super::EffectDefinition> {
        self.definitions.get(id)
    }

    /// Icon for an uptime placeholder: last-activated icon if seen this
    /// session, else the definition's static fallback.
    pub fn uptime_placeholder_icon(&self, def: &super::EffectDefinition) -> u64 {
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
        if let Some(now) = self.interpolated_game_time() {
            self.tick_at(now);
        }
    }

    /// Tick at an explicit game time. Live code goes through `tick()`; replay
    /// tools pass log time so results don't depend on wall-clock speed.
    pub fn tick_at(&mut self, interp_time: NaiveDateTime) {
        let Some(current_time) = self.current_game_time else {
            return;
        };

        for (key, effect) in self.active_effects.iter_mut() {
            // Handle duration-expired effects.
            // Timer expiry is a heuristic — the in-game effect may outlast our estimate.
            // Instead of immediately removing, mark as timer_expired so the effect stays
            // in active_effects and can be revived by refresh_abilities.
            // After the grace period, hard-remove for GC.
            if !effect.is_removed() && effect.has_duration_expired(interp_time) {
                effect.mark_timer_expired();

                // After grace period, hard-remove (GC on next retain pass)
                if let Some(expires_at) = effect.expires_at {
                    let since_expiry_ms = interp_time.signed_duration_since(expires_at).num_milliseconds();
                    if since_expiry_ms > TIMER_EXPIRY_GRACE_MS {
                        effect.mark_removed();
                    }
                }
            }

            // End-of-effect alert and expiration sound (any reason except a
            // silent discard). The single owner of expiration audio: the
            // effect may be dropped below before any audio poll could see it.
            let remaining_total = effect.remaining_secs(interp_time).unwrap_or(0.0);
            if effect.on_end_alert_fired || !(effect.has_base_duration_ended(remaining_total) || effect.is_removed()) {
                continue;
            }
            effect.on_end_alert_fired = true;
            let play_audio =
                effect.audio_enabled && !effect.audio_played && effect.audio_offset == 0 && effect.audio_file.is_some();
            effect.audio_played |= play_audio;
            let def = self.definitions.def(key.def);
            let alert = match &def.alert_text {
                Some(text) if def.alert_on == AlertTrigger::OnExpire => {
                    let text = alerts::format_text(text, effect.source_name, effect.target_name, effect.stacks);
                    alerts::text_alert(def, text, current_time)
                }
                // No on-expire alert to carry the sound: send it audio-only
                _ if play_audio => FiredAlert {
                    alert_text_enabled: false,
                    ..alerts::text_alert(def, effect.display_text.clone(), current_time)
                },
                _ => continue,
            };
            self.fired_alerts.push(FiredAlert {
                audio_enabled: play_audio,
                audio_file: effect.audio_file.clone(),
                ..alert
            });
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
        for (key, effect) in &self.active_effects {
            if !effect.is_displayed() {
                continue;
            }
            let def = self.definitions.def(key.def);
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

            let raw_name = def.alert_text.as_deref().unwrap_or(&def.name);
            let formatted = alerts::format_text(raw_name, effect.source_name, effect.target_name, effect.stacks);
            let text = format!("{} ({:.1})", formatted, remaining_base);
            self.fired_alerts.push(FiredAlert {
                remaining_secs: Some(remaining_base),
                ..alerts::text_alert(def, text, current_time)
            });
        }

        // Remove effects that have been marked removed (immediate, no fade delay)
        self.active_effects.retain(|_, effect| !effect.is_removed());
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
        let ctx = self.filter_ctx(encounter);

        // Resolve effect name for matching
        let effect_name_str = crate::context::resolve(effect_name);

        // Find matching definitions (only those that trigger on EffectApplied)
        let all_matches = self.definitions.find_matching(effect_id as u64, Some(effect_name_str));

        // Ids of single-instance defs sharing this game effect (e.g. kolto_shell
        // + kolto_shell_others) — the source-agnostic lookup group used below.
        // Collected before filtering: source filters keep only one of the pair.
        let single_instance_ids: Vec<DefId> = all_matches
            .iter()
            .filter(|(_, d)| d.single_instance_per_target)
            .map(|(id, _)| *id)
            .collect();

        let matching_defs: Vec<_> = all_matches
            .into_iter()
            .filter(|(_, def)| def.is_effect_applied_trigger())
            .filter(|(_, def)| self.matches_filters(&ctx, def, source_info, target_info))
            .collect();

        let is_from_local = local_player_id == Some(source_id);
        let mut should_register = false;
        let mut pending_alerts: Vec<FiredAlert> = Vec::new();

        for (def_id, def) in matching_defs {
            // Instant alerts: fire and skip — no ActiveEffect created
            if def.is_alert {
                pending_alerts.push(alerts::instant(
                    def,
                    timestamp,
                    source_name,
                    target_name,
                    charges.unwrap_or(1),
                ));
                continue;
            }

            let key = EffectKey::for_scope(def_id, def.refresh_scope, source_id, target_id);

            let duration = self.effective_duration(def, timestamp);

            // Single-instance buffs (Kolto Shell, Trauma Probe): the game keeps one
            // instance per target across all sources — a second caster's apply
            // refreshes the existing instance and the original caster keeps credit.
            // Source-agnostic lookup across the def group, in both directions
            // (local over other's AND other's over local).
            if def.single_instance_per_target
                && let Some(existing) =
                    Self::find_single_instance_effect(&mut self.active_effects, &single_instance_ids, target_id)
                && !existing.is_removed()
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
                    if existing.is_removed() {
                        // EffectRemoved arrived before EffectApplied (reverse ordering).
                        // Revive the effect — EffectApplied is authoritative for new applications.
                        existing.refresh(timestamp, duration);
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
                pending_alerts.extend(alerts::on_apply(
                    def,
                    timestamp,
                    source_name,
                    target_name,
                    charges.unwrap_or(1),
                ));
            } else {
                // A logged application is authoritative: drop any deferred refresh for
                // this cast so a bystander's tick can't claim it afterwards.
                if def.display_targets.contains(&DisplayTarget::DotTracker) {
                    Self::clear_pending_dot_refresh(&mut self.pending_dot_refresh, action_id, target_id);
                }
                // Create new effect
                let mut effect = ActiveEffect::from_definition(
                    def,
                    effect_id as u64,
                    (source_id, source_name),
                    (target_id, target_name),
                    timestamp,
                    duration,
                );

                if let Some(c) = charges {
                    effect.set_stacks(c);
                }

                self.active_effects.insert(key, effect);
                should_register |= def.display_targets.contains(&DisplayTarget::RaidFrames);

                // Collect alert for effect start if configured
                pending_alerts.extend(alerts::on_apply(
                    def,
                    timestamp,
                    source_name,
                    target_name,
                    charges.unwrap_or(1),
                ));
            }
        }

        // Queue collected alerts
        self.fired_alerts.extend(pending_alerts);

        // Queue target for raid frame registration only when a raid-frame effect
        // was created or refreshed — a tracked cooldown/buff (e.g. a stim
        // re-applied at zone-in) must not claim a slot. Only players belong on
        // raid frames (not companions or NPCs)
        if should_register && is_from_local && target_entity_type == EntityType::Player {
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
        source: EntityInfo,
        target: EntityInfo,
        timestamp: NaiveDateTime,
        encounter: Option<&crate::encounter::CombatEncounter>,
        trigger_type: RefreshTrigger,
        // For the Damage trigger: whether this damage event was immune or resisted.
        // Refresh abilities with `ignore_immune_resist` skip these events.
        is_immune_or_resist: bool,
    ) {
        let EntityInfo {
            id: source_id,
            name: source_name,
            ..
        } = source;
        let EntityInfo {
            id: target_id,
            entity_type: target_entity_type,
            name: target_name,
            ..
        } = target;
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
        let candidates = self
            .definitions
            .find_refreshable_by(action_id as u64, Some(action_name_str));
        if candidates.is_empty() {
            return;
        }

        let local_player_id = self.local_player_id;
        let current_target_id = self.local_target_id();
        let ctx = self.filter_ctx(encounter);

        // In PvP, only classified teammates may claim raid-frame slots.
        let can_register = is_player && Self::can_register_target(local_player_id, source_id, target_id, encounter);

        // Collect matching definitions with all info needed for creation
        struct RefreshableEffect<'a> {
            def_id: DefId,
            def: &'a EffectDefinition,
            duration: Option<Duration>,
            /// Minimum stacks required for this refresh (None = any)
            min_stacks: Option<u8>,
            /// Resolved per-ability flag: defer the refresh to the first damage
            /// event from this ability instead of firing on cast.
            defer_to_first_damage: bool,
            /// Target filter only matched the caster: refresh the caster's
            /// instance, never touch or create one on the cast target.
            self_only: bool,
        }

        let local_discipline = self.local_player_discipline;

        // Ids of single-instance defs refreshable by this ability (both the
        // local and `_others` variants) — the source-agnostic lookup group used
        // below. Collected before filtering: source filters keep only one.
        let single_instance_ids: Vec<DefId> = candidates
            .iter()
            .filter(|(_, d)| d.single_instance_per_target)
            .map(|(id, _)| *id)
            .collect();

        let refreshable_defs: Vec<_> = candidates
            .into_iter()
            .filter(|(_, def)| def.matches_discipline(local_discipline.as_ref()))
            .filter_map(|(def_id, def)| {
                // Only refresh/create effects whose filters match the actual entities
                if !self.source_ok(&ctx, def, source) {
                    return None;
                }
                // Self-buffs refreshed by an offensive cast: the effect lives on the
                // caster, so retry the target filter against the caster (same
                // condition as the self fallback key below).
                let self_only = if self.target_ok(&ctx, def, target) {
                    false
                } else if target_id != source_id
                    && target_entity_type != EntityType::Player
                    && self.target_ok(&ctx, def, source)
                {
                    true
                } else {
                    return None;
                };
                Some((def_id, def, self_only))
            })
            .filter_map(|(def_id, def, self_only)| {
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
                    def_id,
                    def,
                    duration: self.effective_duration(def, timestamp),
                    min_stacks: refresh_ability.min_stacks(),
                    defer_to_first_damage: refresh_ability
                        .refresh_on_first_damage(def.display_targets.contains(&DisplayTarget::DotTracker)),
                    self_only,
                })
            })
            .collect();

        for r in refreshable_defs {
            // A self-only match keys on the caster; the fallback key below then
            // collapses to the primary key and late creation is skipped.
            let target_id = if r.self_only { source_id } else { target_id };

            // First-damage refreshes (non-AoE) defer to the next DamageTaken event.
            // Instead of refreshing immediately on AbilityActivated, set pending state
            // that will be consumed when damage from this ability lands on a target.
            // Whether to defer is resolved per refresh ability (`on_first_damage`),
            // defaulting to true for DotTracker effects and false otherwise.
            // AoE refresh abilities are handled separately by the existing AoE damage
            // correlation path (setup_pending_aoe_refresh / handle_damage_for_aoe_refresh).
            if trigger_type == RefreshTrigger::Activation && r.defer_to_first_damage && !r.self_only {
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
            if r.def.single_instance_per_target {
                let existing =
                    Self::find_single_instance_effect(&mut self.active_effects, &single_instance_ids, target_id);
                if let Some(effect) = existing.filter(|e| !e.is_removed()) {
                    if let Some(min_stacks) = r.min_stacks
                        && effect.stacks < min_stacks
                    {
                        continue;
                    }
                    // Original caster keeps credit — only the duration refreshes.
                    effect.refresh(timestamp, r.duration);
                }
                if r.def.display_targets.contains(&DisplayTarget::RaidFrames) && can_register {
                    self.new_targets.push(NewTargetInfo {
                        entity_id: target_id,
                        name: target_name,
                    });
                }
                continue;
            }

            let key = EffectKey::for_scope(r.def_id, r.def.refresh_scope, source_id, target_id);
            // Fallback: if the resolved target is an NPC, also try source_id as target.
            // Handles self-cast abilities (e.g. Dark Ward) where target resolution
            // resolves to the combat target but the effect is keyed to the caster.
            // Only for NPC targets — player targets are intentional (e.g. heals).
            let fallback_key = if target_id != source_id && target_entity_type != EntityType::Player {
                Some(EffectKey::for_scope(
                    r.def_id,
                    r.def.refresh_scope,
                    source_id,
                    source_id,
                ))
            } else {
                None
            };

            let matched_key = if self.active_effects.contains_key(&key) {
                Some(key)
            } else {
                fallback_key.filter(|k| self.active_effects.contains_key(k))
            };

            if let Some(effect) = matched_key.and_then(|k| self.active_effects.get_mut(&k)) {
                // Don't resurrect effects that have been authoritatively removed.
                // EffectRemoved is the source of truth — if the game says the effect
                // is gone, refresh abilities cannot bring it back.
                // (timer_expired effects CAN still be refreshed — that's the grace period
                // for when our duration estimate expires before the in-game effect.)
                if effect.is_removed() {
                    continue;
                }

                // Check min_stacks condition if specified
                if let Some(min_stacks) = r.min_stacks
                    && effect.stacks < min_stacks
                {
                    continue; // Skip refresh - not enough stacks
                }

                // Existing effect - refresh duration
                effect.refresh(timestamp, r.duration);

                // Re-register for raid frames (in case user cleared the slot)
                if r.def.display_targets.contains(&DisplayTarget::RaidFrames) && can_register {
                    self.new_targets.push(NewTargetInfo {
                        entity_id: target_id,
                        name: target_name,
                    });
                }
            } else if r.def.display_targets.contains(&DisplayTarget::RaidFrames) && !r.self_only {
                // Don't late-register if min_stacks is required — no existing effect
                // means 0 stacks, which can't satisfy the minimum. Only unconditional
                // refresh abilities (Simple variant, no min_stacks) should late-register.
                if r.min_stacks.is_some() {
                    continue;
                }
                // Raid frame effect doesn't exist - create it (late registration)
                let mut effect = ActiveEffect::from_definition(
                    r.def,
                    action_id as u64,
                    (source_id, source_name),
                    (target_id, target_name),
                    timestamp,
                    r.duration,
                );

                if let Some(charges) = r.def.default_charges {
                    effect.set_stacks(charges);
                }

                self.active_effects.insert(key, effect);

                // Queue target for raid frame registration (only players)
                if r.def.display_targets.contains(&DisplayTarget::RaidFrames) && can_register {
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
        ability_name: IStr,
        source_id: i64,
        timestamp: NaiveDateTime,
        primary_target: i64,
    ) {
        if self
            .definitions
            .is_aoe_refresh(ability_id as u64, crate::context::resolve(ability_name))
        {
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
        ability_name: IStr,
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
            let diff_ms = (timestamp - collecting.anchor_timestamp).num_milliseconds().abs();
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
        self.refresh_aoe_immediate(ability_id, ability_name, source_id, target_id, timestamp);

        // DOT path: anchor on primary target then collect within window
        if target_id == primary_target {
            self.aoe_collecting = Some(AoeRefreshCollecting {
                ability_id,
                ability_name,
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
        ability_name: IStr,
        source_id: i64,
        target_id: i64,
        timestamp: NaiveDateTime,
    ) {
        let refreshable: Vec<_> = self
            .definitions
            .find_refreshable_by(ability_id as u64, Some(crate::context::resolve(ability_name)))
            .into_iter()
            .filter(|(_, def)| def.aoe_refresh_immediate)
            .map(|(id, def)| (id, def.refresh_scope, self.effective_duration(def, timestamp)))
            .collect();

        for (def_id, scope, duration) in &refreshable {
            let key = EffectKey::for_scope(*def_id, *scope, source_id, target_id);
            // The game's RemoveEffect is authoritative: a later hit can't revive it
            if let Some(effect) = self.active_effects.get_mut(&key)
                && !effect.is_removed()
            {
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
            .find_refreshable_by(
                collecting.ability_id as u64,
                Some(crate::context::resolve(collecting.ability_name)),
            )
            .into_iter()
            .filter(|(_, def)| !def.aoe_refresh_immediate)
            .map(|(id, def)| {
                (
                    id,
                    def.refresh_scope,
                    self.effective_duration(def, collecting.anchor_timestamp),
                )
            })
            .collect();

        for target_id in collecting.targets {
            for (def_id, scope, duration) in &refreshable_def_ids {
                let key = EffectKey::for_scope(*def_id, *scope, collecting.source_id, target_id);
                if let Some(effect) = self.active_effects.get_mut(&key)
                    && !effect.is_removed()
                {
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
        ability_name: IStr,
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
        let name = Some(crate::context::resolve(ability_name));
        let refreshable_def_ids: Vec<_> = self
            .definitions
            .find_refreshable_by(ability_id as u64, name)
            .into_iter()
            .filter(|(_, def)| {
                let is_dot_tracker = def.display_targets.contains(&DisplayTarget::DotTracker);
                def.find_refresh_ability(ability_id as u64, name)
                    .is_some_and(|ra| ra.refresh_on_first_damage(is_dot_tracker))
            })
            .map(|(id, def)| (id, def.refresh_scope, self.effective_duration(def, timestamp)))
            .collect();

        for (def_id, scope, duration) in &refreshable_def_ids {
            let key = EffectKey::for_scope(*def_id, *scope, source_id, target_id);
            if let Some(effect) = self.active_effects.get_mut(&key)
                && !effect.is_removed()
            {
                effect.refresh(timestamp, *duration);
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
        target_npc_id: i64,
        timestamp: NaiveDateTime,
        encounter: Option<&crate::encounter::CombatEncounter>,
    ) {
        let ability_name_str = crate::context::resolve(ability_name);

        // Find definitions with AbilityCast triggers that match this ability
        let matching_defs: Vec<_> = self
            .definitions
            .find_ability_cast_matching(ability_id as u64, Some(ability_name_str));

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

        let ctx = self.filter_ctx(encounter);
        let target_info = EntityInfo {
            id: target_id,
            npc_id: target_npc_id,
            entity_type: target_entity_type,
            name: target_name,
        };
        for (def_id, def) in matching_defs {
            // Only process AbilityCast triggers here (index also contains DamageTaken/HealingTaken)
            if !def.is_ability_cast_trigger() {
                continue;
            }

            // Check discipline filter
            if !def.matches_discipline(self.local_player_discipline.as_ref()) {
                continue;
            }

            if !self.source_ok(&ctx, def, source_info) {
                continue;
            }

            // Instant alerts: fire and skip — no ActiveEffect created
            if def.is_alert {
                self.fired_alerts
                    .push(alerts::instant(def, timestamp, source_name, target_name, 1));
                continue;
            }

            // Validate resolved target against the definition's target filter
            if !self.target_ok(&ctx, def, target_info) {
                continue;
            }

            // AbilityCast trigger matched — track the effect on the caster (source)
            let key = EffectKey::for_scope(def_id, def.refresh_scope, source_id, source_id);

            let duration = self.effective_duration(def, timestamp);

            if let Some(existing) = self.active_effects.get_mut(&key) {
                // Ignore refreshes: skip retrigger only while still in base duration.
                // Once the cooldown enters the ready state it has effectively expired,
                // so a new trigger is a fresh activation, not a refresh.
                if def.ignore_refreshes && existing.is_in_base_duration(timestamp) {
                    continue;
                }

                existing.refresh(timestamp, duration);

                // Recast on a different target: update the displayed target
                if existing.target_name != target_name {
                    existing.target_name = target_name;
                    existing.set_display_template(def.display_text());
                }

                // Fire OnApply alert on refresh
                self.fired_alerts
                    .extend(alerts::on_apply(def, timestamp, source_name, target_name, 1));
            } else {
                // Tracked on the caster (keying, death cleanup); only the
                // displayed name is the target resolved at cast time
                let effect = ActiveEffect::from_definition(
                    def,
                    ability_id as u64,
                    (source_id, source_name),
                    (source_id, target_name),
                    timestamp,
                    duration,
                );
                self.active_effects.insert(key, effect);

                // Fire OnApply alert for new effect
                self.fired_alerts
                    .extend(alerts::on_apply(def, timestamp, source_name, target_name, 1));
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
        let ability_name_str = crate::context::resolve(ability_name);

        let matching_defs: Vec<_> = self
            .definitions
            .find_ability_cast_matching(ability_id as u64, Some(ability_name_str));

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
        let ctx = self.filter_ctx(encounter);

        for (def_id, def) in matching_defs {
            if !trigger_check(def) {
                continue;
            }
            if !self.matches_filters(&ctx, def, source_info, target_info) {
                continue;
            }

            // Instant alerts: fire and skip — no ActiveEffect created
            if def.is_alert {
                self.fired_alerts
                    .push(alerts::instant(def, timestamp, source_name, target_name, 1));
                continue;
            }

            let key = EffectKey::for_scope(def_id, def.refresh_scope, source_id, target_id);
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
                self.fired_alerts
                    .extend(alerts::on_apply(def, timestamp, source_name, target_name, 1));
            } else {
                let effect = ActiveEffect::from_definition(
                    def,
                    ability_id as u64,
                    (source_id, source_name),
                    (target_id, target_name),
                    timestamp,
                    duration,
                );
                self.active_effects.insert(key, effect);

                // Fire OnApply alert for new effect
                self.fired_alerts
                    .extend(alerts::on_apply(def, timestamp, source_name, target_name, 1));
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
        let ctx = self.filter_ctx(encounter);

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
        let single_instance_ids: Vec<DefId> = matching_defs
            .iter()
            .filter(|(_, d)| d.single_instance_per_target)
            .map(|(id, _)| *id)
            .collect();

        for &(def_id, def) in &matching_defs {
            let key = EffectKey::for_scope(def_id, def.refresh_scope, source_id, target_id);

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
                    Self::find_single_instance_effect(&mut self.active_effects, &single_instance_ids, target_id)
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
                        effect.confirm_removed();
                    }
                }
            } else if def.is_effect_removed_trigger() && self.matches_filters(&ctx, def, source_info, target_info) {
                // Instant alerts: fire and skip — no ActiveEffect created
                if def.is_alert {
                    self.fired_alerts
                        .push(alerts::instant(def, timestamp, source_name, target_name, 1));
                    continue;
                }

                // Create new effect when the game effect is removed (cooldown tracking)
                let duration = self.effective_duration(def, timestamp);
                let effect = ActiveEffect::from_definition(
                    def,
                    effect_id as u64,
                    (source_id, source_name),
                    (target_id, target_name),
                    timestamp,
                    duration,
                );
                self.active_effects.insert(key, effect);

                // Fire OnApply alert for new EffectRemoved-triggered effect
                self.fired_alerts
                    .extend(alerts::on_apply(def, timestamp, source_name, target_name, 1));
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
        let ctx = self.filter_ctx(encounter);

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
        let single_instance_ids: Vec<DefId> = matching_defs
            .iter()
            .filter(|(_, d)| d.single_instance_per_target)
            .map(|(id, _)| *id)
            .collect();

        for (def_id, def) in matching_defs {
            let key = EffectKey::for_scope(def_id, def.refresh_scope, source_id, target_id);

            // Calculate duration before borrowing active_effects mutably.
            // Needed for the blanket is_refreshed_on_modify refresh AND for
            // refill_duration modifiers (which refill without is_refreshed_on_modify).
            let needs_duration = def.is_refreshed_on_modify || def.modifiers.iter().any(|m| m.refill_duration);
            let duration = if needs_duration {
                self.effective_duration(def, timestamp)
            } else {
                None
            };
            let icds: Vec<Option<f32>> = def.modifiers.iter().map(|m| self.effective_icd(m, timestamp)).collect();

            if let Some(effect) = if self.active_effects.contains_key(&key) {
                self.active_effects.get_mut(&key)
            } else if def.single_instance_per_target {
                Self::find_single_instance_effect(&mut self.active_effects, &single_instance_ids, target_id)
            } else {
                None
            } {
                let old_stacks = effect.stacks;
                effect.set_stacks(charges);

                // Refresh duration on ModifyCharges if is_refreshed_on_modify is set.
                if def.is_refreshed_on_modify
                    && let Some(dur) = duration
                {
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
                        let adj = ModifierAdjustment::new(modifier, mod_idx, timestamp, 1.0, icds[mod_idx]);
                        if !effect.modifier_icd_ready(mod_idx, adj.icd_secs, adj.icd_from_application, timestamp) {
                            continue;
                        }
                        effect.modifier_last_proc[mod_idx] = Some(timestamp);
                        effect.apply_modifier(&adj, duration, true);
                        if adj.cancel {
                            break;
                        }
                    }
                }
            } else if def.single_instance_per_target
                && def.is_effect_applied_trigger()
                && !def.is_alert
                && self.matches_filters(&ctx, def, source_info, target_info)
            {
                // Nothing tracked on this target: the original apply predates the
                // log (e.g. the local player zoned in after another healer applied).
                // ModifyCharges carries the true caster as source, so create the
                // instance here with correct credit. Real remaining duration is
                // unknown — start a full one and let refreshes/RemoveEffect
                // correct it. matches_filters picks the local vs `_others` def,
                // so the pair creates at most one instance.
                let duration = self.effective_duration(def, timestamp);
                let mut effect = ActiveEffect::from_definition(
                    def,
                    effect_id as u64,
                    (source_id, source_name),
                    (target_id, target_name),
                    timestamp,
                    duration,
                );
                effect.set_stacks(charges);
                self.active_effects.insert(key, effect);
            }
        }
    }

    /// Handle entity death - clear effects unless persist_past_death
    fn handle_entity_death(&mut self, entity_id: i64) {
        for (key, effect) in self.active_effects.iter_mut() {
            if effect.target_entity_id != entity_id {
                continue;
            }
            if !self.definitions.def(key.def).persist_past_death {
                effect.discard();
            }
        }
    }

    /// Handle combat end - optionally clear combat-only effects
    fn handle_combat_ended(&mut self) {
        // Clear pending refresh state
        self.pending_aoe_refresh = None;
        self.aoe_collecting = None;
        self.pending_dot_refresh = None;

        // Discard effects that don't track outside combat
        for (key, effect) in self.active_effects.iter_mut() {
            if !self.definitions.def(key.def).track_outside_combat {
                effect.discard();
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

        for effect in self.active_effects.values_mut() {
            effect.discard();
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

    /// Local player's current target (TargetChanged cache)
    fn local_target_id(&self) -> Option<i64> {
        self.local_player_id
            .and_then(|id| self.current_targets.get(&id).map(|(tid, _, _)| *tid))
    }

    fn filter_ctx<'a>(&self, encounter: Option<&'a CombatEncounter>) -> FilterCtx<'a> {
        FilterCtx::new(encounter, self.local_player_id, self.local_target_id())
    }

    /// Source filter and source discipline filter
    fn source_ok(&self, ctx: &FilterCtx, def: &EffectDefinition, e: EntityInfo) -> bool {
        ctx.matches(def.source_filter(), e)
            && self.matches_entity_disciplines(&def.source_disciplines, e.id, e.entity_type)
    }

    /// Target filter and target discipline filter
    fn target_ok(&self, ctx: &FilterCtx, def: &EffectDefinition, e: EntityInfo) -> bool {
        ctx.matches(def.target_filter(), e)
            && self.matches_entity_disciplines(&def.target_disciplines, e.id, e.entity_type)
    }

    /// Local discipline scope plus source and target filters
    fn matches_filters(&self, ctx: &FilterCtx, def: &EffectDefinition, source: EntityInfo, target: EntityInfo) -> bool {
        def.matches_discipline(self.local_player_discipline.as_ref())
            && self.source_ok(ctx, def, source)
            && self.target_ok(ctx, def, target)
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // Modifier Evaluation
    // ═══════════════════════════════════════════════════════════════════════════

    fn evaluate_modifiers(&mut self, signals: &[GameSignal]) {
        if !self.definitions.has_modifiers() {
            return;
        }
        // Seed last_seen_charges from EffectApplied so first ChargesChanged has a baseline
        for s in signals {
            if let GameSignal::EffectApplied {
                effect_id,
                target_id,
                charges,
                ..
            } = s
                && let Some(c) = charges
                && *c > 0
            {
                self.last_seen_charges.entry((*effect_id, *target_id)).or_insert(*c);
            }
        }

        // Snapshot old charges for direction comparison, then update to new values
        let mut charge_snapshots: Vec<((i64, i64), u8, u8)> = Vec::new();
        for s in signals {
            if let GameSignal::EffectChargesChanged {
                effect_id,
                target_id,
                charges,
                ..
            } = s
            {
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
            if effect.is_removed() {
                continue;
            }
            let def = self.definitions.def(key.def);
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
                    let icd = self.effective_icd(modifier, hit.at);
                    adjustments.push((*key, ModifierAdjustment::new(modifier, mod_idx, hit.at, hit.scale, icd)));
                }
            }
        }

        // Apply collected adjustments
        for (key, adj) in adjustments {
            // Pre-compute effective duration for refill before mutable borrow
            let refill_dur = if adj.refill_duration {
                self.effective_duration(self.definitions.def(key.def), adj.at)
            } else {
                None
            };

            let Some(effect) = self.active_effects.get_mut(&key) else {
                continue;
            };
            // Removed by an earlier hit in this batch (cancel), or genuinely
            // expired in log time before this hit — nothing to modify.
            if effect.is_removed() || (effect.is_timer_expired() && effect.expires_at.is_some_and(|e| e < adj.at)) {
                continue;
            }
            let modifier_count = self.definitions.def(key.def).modifiers.len();
            effect.ensure_modifier_icd(modifier_count);

            if !effect.modifier_icd_ready(adj.mod_idx, adj.icd_secs, adj.icd_from_application, adj.at) {
                continue;
            }

            effect.modifier_last_proc[adj.mod_idx] = Some(adj.at);
            effect.apply_modifier(&adj, refill_dur, true);
        }
    }
}

impl SignalHandler for EffectTracker {
    fn handle_signals(&mut self, signals: &[GameSignal], encounter: Option<&crate::encounter::CombatEncounter>) {
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
        if let Some(ref collecting) = self.aoe_collecting
            && let Some(current_time) = self.current_game_time
        {
            let elapsed_ms = (current_time - collecting.anchor_timestamp).num_milliseconds();
            if elapsed_ms > 10 {
                self.finalize_aoe_refresh();
            }
        }
    }

    fn handle_signal(&mut self, signal: &GameSignal, encounter: Option<&crate::encounter::CombatEncounter>) {
        self.advance_game_time_anchor(signal.timestamp());
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
            GameSignal::AreaEntered { area_id, .. }
                // Instanced areas fire AreaEntered twice (plain + difficulty)
                // around the game's effect restatement — only wipe on an
                // actual area change.
                if self.last_area_id != Some(*area_id) => {
                    self.last_area_id = Some(*area_id);
                    self.handle_area_change();
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
                target_npc_id,
                timestamp,
            } => {
                let source = EntityInfo {
                    id: *source_id,
                    npc_id: *source_npc_id,
                    entity_type: *source_entity_type,
                    name: *source_name,
                };
                // Resolve actual target: the signal already has a resolved target from
                // encounter state, but our local current_targets cache may be more
                // up-to-date (works outside combat). Apply additional resolution if
                // the signal still reports self-targeting. A target resolved from
                // either cache has no known NPC class id.
                let is_self_or_empty = *target_id == 0 || *target_id == *source_id;
                let target = if !is_self_or_empty {
                    EntityInfo {
                        id: *target_id,
                        npc_id: *target_npc_id,
                        entity_type: *target_entity_type,
                        name: *target_name,
                    }
                } else if let Some((id, name, entity_type)) = self.current_targets.get(source_id).copied() {
                    EntityInfo {
                        id,
                        npc_id: 0,
                        entity_type,
                        name,
                    }
                } else if let Some(id) = encounter.and_then(|e| e.get_current_target(*source_id)) {
                    let player = encounter.and_then(|e| e.players.get(&id));
                    EntityInfo {
                        id,
                        npc_id: 0,
                        entity_type: if player.is_some() {
                            EntityType::Player
                        } else {
                            EntityType::Npc
                        },
                        name: player.map(|p| p.name).unwrap_or(*source_name),
                    }
                } else {
                    source
                };

                // Handle AbilityCast-triggered effects (procs, cooldowns)
                self.handle_ability_cast(
                    *ability_id,
                    *ability_name,
                    *source_id,
                    *source_name,
                    *source_entity_type,
                    *source_npc_id,
                    target.id,
                    target.name,
                    target.entity_type,
                    target.npc_id,
                    *timestamp,
                    encounter,
                );

                self.refresh_effects_by_action(
                    *ability_id,
                    *ability_name,
                    source,
                    target,
                    *timestamp,
                    encounter,
                    RefreshTrigger::Activation,
                    false,
                );

                // For AoE abilities, set up pending state for damage correlation
                self.setup_pending_aoe_refresh(*ability_id, *ability_name, *source_id, *timestamp, target.id);
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
                self.handle_damage_for_aoe_refresh(*ability_id, *ability_name, *target_id, *timestamp);
                // Single-target DotTracker refresh damage confirmation
                self.handle_damage_for_dot_refresh(*ability_id, *ability_name, *target_id, *timestamp);
                // Refresh abilities configured with the Damage trigger
                let is_immune_or_resist = *defense_type_id
                    == crate::game_data::defense_type::IMMUNE
                    || *defense_type_id == crate::game_data::defense_type::RESIST;
                self.refresh_effects_by_action(
                    *ability_id,
                    *ability_name,
                    EntityInfo {
                        id: *source_id,
                        npc_id: *source_npc_id,
                        entity_type: *source_entity_type,
                        name: *source_name,
                    },
                    EntityInfo {
                        id: *target_id,
                        npc_id: *target_npc_id,
                        entity_type: *target_entity_type,
                        name: *target_name,
                    },
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
                    EntityInfo {
                        id: *source_id,
                        npc_id: *source_npc_id,
                        entity_type: *source_entity_type,
                        name: *source_name,
                    },
                    EntityInfo {
                        id: *target_id,
                        npc_id: *target_npc_id,
                        entity_type: *target_entity_type,
                        name: *target_name,
                    },
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
