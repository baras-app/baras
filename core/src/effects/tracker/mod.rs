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
use super::log_names::LogNames;
use super::modifier_adjust::ModifierAdjustment;
use super::{
    ActiveEffect, AlertTrigger, DefId, DefinitionSet, DisplayTarget, EffectDefinition, EffectKey, RefreshTrigger,
};

mod apply;
mod cast;
mod deferred_refresh;
mod filters;
mod modifiers;
mod pending;
mod queries;
mod refresh;
mod signal;
mod tick;

use pending::{AoeRefreshCollecting, PendingAoeRefresh, PendingDotRefresh};

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
    fn effective_duration(&self, def: &EffectDefinition, at: NaiveDateTime) -> Option<Duration> {
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
    /// Drain the queue of targets for raid frame registration attempts.
    /// Called by the service - the registry handles duplicate rejection.
    pub fn take_new_targets(&mut self) -> Vec<NewTargetInfo> {
        std::mem::take(&mut self.new_targets)
    }
}
