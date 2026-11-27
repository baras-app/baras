//! Active effect instances (runtime state)
//!
//! An `ActiveEffect` represents a currently active effect on a specific entity.
//! It is created when a game signal matches an `EffectDefinition` and tracks
//! the live state until the effect is removed.
//!
//! # Removal Triggers
//!
//! Effects are removed when EITHER:
//! 1. `EffectRemoved` signal received from game (authoritative)
//! 2. Duration timer expires (fallback for missed/missing remove events)
//!
//! After removal, the effect is immediately deleted from the tracker.

use std::time::{Duration, Instant};

use chrono::NaiveDateTime;

use super::{DisplayTarget, EffectDefinition};
use crate::context::IStr;
use baras_types::RefreshScope;

/// An active effect instance on a specific entity
///
/// Created when an `EffectDefinition` matches a game signal.
/// The renderer receives these to display effect indicators.
#[derive(Debug, Clone)]
pub struct ActiveEffect {
    /// ID of the definition this effect came from
    pub definition_id: String,

    /// Game's numeric effect ID (from combat log)
    pub game_effect_id: u64,

    /// Display name (cached from definition)
    pub name: String,

    /// Display text for overlay (defaults to name if not set), placeholders resolved
    pub display_text: String,

    /// Raw display text, kept only when it contains `{stack}` (re-resolved on stack change)
    display_template: Option<String>,

    // ─── Entities ───────────────────────────────────────────────────────────
    /// Entity ID of who applied this effect
    pub source_entity_id: i64,

    /// Name of the source entity (from event)
    pub source_name: IStr,

    /// Entity ID of who has this effect
    pub target_entity_id: i64,

    /// Name of the target entity (from event)
    pub target_name: IStr,

    // ─── Timing (game time from combat log) ─────────────────────────────────
    /// When the effect was applied (game time)
    pub applied_at: NaiveDateTime,

    /// When the effect will expire based on duration (game time)
    /// None = indefinite duration (no auto-expire)
    pub expires_at: Option<NaiveDateTime>,

    /// When the effect was last refreshed (game time)
    pub last_refreshed_at: NaiveDateTime,

    /// Total duration (cached for fill calculations)
    pub duration: Option<Duration>,

    // ─── Removal state ─────────────────────────────────────────────────────
    /// When the effect was removed (system time).
    /// Set by either EffectRemoved signal OR duration expiry.
    /// Effect is immediately deleted from tracker on next tick.
    pub removed_at: Option<Instant>,

    // ─── State ──────────────────────────────────────────────────────────────
    /// Current stack/charge count
    pub stacks: u8,

    // ─── Display (cached from definition) ───────────────────────────────────
    /// RGBA color for display
    pub color: [u8; 4],

    /// Which overlays should display this effect (may be multiple)
    pub display_targets: Vec<DisplayTarget>,

    /// Ability ID for icon lookup (may differ from game_effect_id)
    pub icon_ability_id: u64,

    /// Only show when remaining time is at or below this (0 = always show)
    pub show_at_secs: f32,

    /// Whether to show the icon (true) or use colored square (false)
    pub show_icon: bool,

    /// Whether to display the source entity name on personal overlays
    pub display_source: bool,

    /// Whether to display the target entity name on personal overlays
    pub show_target: bool,

    /// Seconds to show "Ready" state after cooldown ends (0 = disabled)
    pub cooldown_ready_secs: f32,

    // ─── Audio ────────────────────────────────────────────────────────────────
    /// Whether on-apply audio has been played
    pub audio_played: bool,

    /// Tracks which countdown seconds have been announced (1-10)
    pub countdown_announced: [bool; 10],

    /// Countdown start second (0 = disabled)
    pub countdown_start: u8,

    /// Voice pack for countdown
    pub countdown_voice: String,

    /// Audio file for on-apply sound
    pub audio_file: Option<String>,

    /// Audio offset (seconds before expiration to play sound)
    pub audio_offset: u8,

    /// Whether audio is enabled for this effect
    pub audio_enabled: bool,

    /// Whether the "on end" alert has been fired (to prevent duplicates)
    pub on_end_alert_fired: bool,

    /// Whether this effect was expired by the app's duration timer (heuristic)
    /// rather than an authoritative EffectRemoved signal.
    ///
    /// Timer-expired effects remain in `active_effects` so that `refresh_abilities`
    /// can still revive them (the game duration may exceed our estimate). They are
    /// hidden from display and alerts fire normally on timer expiry.
    /// After a grace period, the effect is hard-removed for GC.
    pub timer_expired: bool,

    /// Set by `discard()`: the end-alert and audio flags it overrode, restored
    /// if the game confirms the removal before the effect is dropped
    discarded: Option<(bool, bool)>,

    // ─── Modifier state ───────────────────────────────────────────────────────
    /// Absolute expiry cap set by refill modifiers with max_duration_secs.
    /// Used by the overlay to show "elapsed / max" budget text.
    pub max_expires_at: Option<NaiveDateTime>,

    /// Last activation time for each modifier (parallel to definition's `modifiers` vec).
    /// Used to enforce internal cooldown (`icd_secs`). Initialized lazily on first use.
    pub modifier_last_proc: Vec<Option<NaiveDateTime>>,
}

impl ActiveEffect {
    /// Create an instance of `def` from the triggering event. `game_effect_id`
    /// is the logged effect (or ability) ID, the icon fallback when the
    /// definition sets none. `duration` is the effective (alacrity-scaled) one.
    pub fn from_definition(
        def: &EffectDefinition,
        game_effect_id: u64,
        (source_entity_id, source_name): (i64, IStr),
        (target_entity_id, target_name): (i64, IStr),
        event_timestamp: NaiveDateTime,
        duration: Option<Duration>,
    ) -> Self {
        let expires_at = duration.map(|d| event_timestamp + chrono::Duration::milliseconds(d.as_millis() as i64));
        let mut effect = Self {
            definition_id: def.id.clone(),
            game_effect_id,
            name: String::new(),
            display_text: String::new(),
            display_template: None,
            source_entity_id,
            source_name,
            target_entity_id,
            target_name,
            applied_at: event_timestamp,
            expires_at,
            last_refreshed_at: event_timestamp,
            duration,
            removed_at: None,
            stacks: 1,
            color: [0; 4],
            display_targets: Vec::new(),
            icon_ability_id: 0,
            show_at_secs: 0.0,
            show_icon: false,
            display_source: false,
            show_target: false,
            cooldown_ready_secs: 0.0,
            audio_played: false,
            countdown_announced: [false; 10],
            countdown_start: 0,
            countdown_voice: String::new(),
            audio_file: None,
            audio_offset: 0,
            audio_enabled: false,
            on_end_alert_fired: false,
            timer_expired: false,
            discarded: None,
            max_expires_at: None,
            modifier_last_proc: Vec::new(),
        };
        effect.apply_definition(def);
        effect
    }

    /// Copy the display and audio settings cached from the definition.
    /// The single place they're set: on creation and on definition reload.
    pub fn apply_definition(&mut self, def: &EffectDefinition) {
        self.name = def.name.clone();
        self.set_display_template(def.display_text());
        self.color = def.effective_color();
        self.display_targets = def.display_targets.clone();
        self.icon_ability_id = def.icon_ability_id.unwrap_or(self.game_effect_id);
        self.show_at_secs = def.show_at_secs;
        self.show_icon = def.show_icon;
        self.display_source = def.display_source;
        self.show_target = def.show_target;
        self.cooldown_ready_secs = def.cooldown_ready_secs;
        self.countdown_start = def.audio.countdown_start;
        self.countdown_voice = def.audio.countdown_voice.clone().unwrap_or_else(|| "Amy".to_string());
        self.audio_file = def.audio.file.clone();
        self.audio_offset = def.audio.offset;
        self.audio_enabled = def.audio.enabled;
    }

    /// Ensure modifier ICD vec is sized to match definition's modifier count.
    /// Called lazily on first modifier evaluation to avoid allocation for effects
    /// that have no modifiers.
    pub fn ensure_modifier_icd(&mut self, modifier_count: usize) {
        if self.modifier_last_proc.len() != modifier_count {
            self.modifier_last_proc.resize(modifier_count, None);
        }
    }

    /// Check whether modifier `mod_idx` is off cooldown at `now`.
    ///
    /// With `from_application`, the initial application counts as a proc so the
    /// modifier is gated until `icd` seconds after `applied_at`.
    pub fn modifier_icd_ready(
        &self,
        mod_idx: usize,
        icd: Option<f32>,
        from_application: bool,
        now: NaiveDateTime,
    ) -> bool {
        let Some(icd) = icd else { return true };
        let last = self.modifier_last_proc[mod_idx].or(if from_application { Some(self.applied_at) } else { None });
        match last {
            Some(last) => (now - last).num_milliseconds() as f32 / 1000.0 >= icd,
            None => true,
        }
    }

    /// Refresh the effect (reapplication extends duration)
    pub fn refresh(&mut self, event_timestamp: NaiveDateTime, duration: Option<Duration>) {
        self.last_refreshed_at = event_timestamp;

        if let Some(d) = duration {
            self.expires_at = Some(event_timestamp + chrono::Duration::milliseconds(d.as_millis() as i64));
            self.duration = Some(d);
        }

        // Clear removed state if we were fading out (effect came back)
        self.removed_at = None;
        // Clear timer-expired state (refresh revives the effect)
        self.timer_expired = false;
        self.discarded = None;
        // Reset on-end alert so it can fire again
        self.on_end_alert_fired = false;
        // Reset audio so it fires again on the next expiration/offset
        self.audio_played = false;
        self.countdown_announced = [false; 10];
    }

    /// Refresh only the duration (without updating last_refreshed_at)
    ///
    /// Used by ModifyCharges with is_refreshed_on_modify. Unlike `refresh()`,
    /// this does NOT update `last_refreshed_at`, so the stale-removal window
    /// (which guards against DOT reapplication) isn't affected.
    pub fn refresh_duration(&mut self, event_timestamp: NaiveDateTime, duration: Duration) {
        self.expires_at = Some(event_timestamp + chrono::Duration::milliseconds(duration.as_millis() as i64));
        self.duration = Some(duration);

        self.removed_at = None;
        self.timer_expired = false;
        self.discarded = None;
        self.on_end_alert_fired = false;
        // Reset audio so it fires again on the next expiration/offset
        self.audio_played = false;
        self.countdown_announced = [false; 10];
    }

    /// Update stack count
    pub fn set_stacks(&mut self, stacks: u8) {
        if self.stacks != stacks
            && let Some(template) = &self.display_template
        {
            self.display_text = Self::resolve_display(template, self.source_name, self.target_name, stacks).0;
        }
        self.stacks = stacks;
    }

    /// True when the display text carries the stack count via `{stack}`
    pub fn stack_in_text(&self) -> bool {
        self.display_template.is_some()
    }

    /// Replace the display text template (definition reload) and re-resolve placeholders
    pub fn set_display_template(&mut self, template: &str) {
        (self.display_text, self.display_template) =
            Self::resolve_display(template, self.source_name, self.target_name, self.stacks);
    }

    /// Resolve placeholders; returns the template back only if it needs re-resolving on stack change
    fn resolve_display(template: &str, source: IStr, target: IStr, stacks: u8) -> (String, Option<String>) {
        let text = crate::dsl::format_placeholders(
            template,
            crate::context::resolve(source),
            crate::context::resolve(target),
            Some(stacks),
        );
        (text, template.contains("{stack}").then(|| template.to_string()))
    }

    /// Mark the effect as removed; the next tick drops it
    pub fn mark_removed(&mut self) {
        if self.removed_at.is_none() {
            self.removed_at = Some(Instant::now());
        }
    }

    /// Remove for tracker housekeeping (combat end, zone change, death):
    /// the effect didn't end in the game's terms, so no end alert or sound
    /// fires. No-op on an effect that is already removed.
    pub fn discard(&mut self) {
        if self.removed_at.is_some() {
            return;
        }
        self.discarded = Some((self.on_end_alert_fired, self.audio_played));
        self.on_end_alert_fired = true;
        self.audio_played = true;
        self.mark_removed();
    }

    /// Authoritative removal (game `RemoveEffect`). Re-arms the end alert
    /// and sound of an effect discarded earlier in the same batch, e.g. a
    /// death handled just before the game's removal line.
    pub fn confirm_removed(&mut self) {
        if let Some((alert_fired, audio_played)) = self.discarded.take() {
            self.on_end_alert_fired = alert_fired;
            self.audio_played = audio_played;
        }
        self.mark_removed();
    }

    /// Neither removed nor past its (heuristic) timer — the overlays show it
    pub fn is_displayed(&self) -> bool {
        self.removed_at.is_none() && !self.timer_expired
    }

    /// Check if the effect has expired based on duration
    /// Returns true if we have a duration and current game time is past expiry
    pub fn has_duration_expired(&self, current_game_time: NaiveDateTime) -> bool {
        self.expires_at
            .map(|expires| current_game_time >= expires)
            .unwrap_or(false)
    }

    /// Check if the effect is still active (not removed and not expired)
    pub fn is_active(&self, current_game_time: NaiveDateTime) -> bool {
        self.removed_at.is_none() && !self.has_duration_expired(current_game_time)
    }

    /// Check if the effect is active AND still in its base duration (not in ready state).
    ///
    /// Used by `ignore_refreshes`: once a cooldown enters the ready state, the
    /// cooldown has effectively expired and a new trigger is a fresh activation
    /// rather than a refresh. For effects without `cooldown_ready_secs` this is
    /// equivalent to `is_active`.
    pub fn is_in_base_duration(&self, current_game_time: NaiveDateTime) -> bool {
        if !self.is_active(current_game_time) {
            return false;
        }
        let remaining_total = self.remaining_secs(current_game_time).unwrap_or(0.0);
        !self.has_base_duration_ended(remaining_total)
    }

    /// Get fill percentage for countdown display (1.0 = full, 0.0 = expired)
    ///
    /// Pass the current game time (latest log timestamp) for accurate calculation.
    pub fn fill_percent(&self, current_game_time: NaiveDateTime) -> f32 {
        match (self.expires_at, self.duration) {
            (Some(expires), Some(duration)) => {
                let remaining = expires.signed_duration_since(current_game_time);
                let remaining_ms = remaining.num_milliseconds().max(0) as f32;
                let duration_ms = duration.as_millis() as f32;

                if duration_ms > 0.0 {
                    (remaining_ms / duration_ms).clamp(0.0, 1.0)
                } else {
                    1.0
                }
            }
            // No duration = indefinite, show full
            _ => 1.0,
        }
    }

    /// Get remaining time in seconds (None if indefinite)
    pub fn remaining_secs(&self, current_game_time: NaiveDateTime) -> Option<f32> {
        self.expires_at.map(|expires| {
            let remaining = expires.signed_duration_since(current_game_time);
            (remaining.num_milliseconds().max(0) as f32) / 1000.0
        })
    }

    // ─── Audio/Display Methods ────────────────────────────────────────────────
    //
    // All methods that need remaining time now accept it as a parameter.
    // The EffectTracker computes interpolated game time once per tick and
    // passes the resulting remaining seconds to each effect. This avoids
    // comparing SWTOR's clock with the system clock (which can drift).

    /// Get remaining BASE time (excludes ready state time)
    ///
    /// `remaining_total` is the total remaining seconds (from `remaining_secs(game_time)`)
    /// computed by the tracker using interpolated game time.
    pub fn remaining_base_secs(&self, remaining_total: f32) -> f32 {
        (remaining_total - self.cooldown_ready_secs).max(0.0)
    }

    /// Check if base duration has ended (entered ready state or fully expired)
    /// Returns true when remaining <= cooldown_ready_secs (or <= 0 if no ready state)
    ///
    /// `remaining_total` is the total remaining seconds from interpolated game time.
    pub fn has_base_duration_ended(&self, remaining_total: f32) -> bool {
        self.remaining_base_secs(remaining_total) <= 0.0
    }

    /// Check if effect should be visible based on show_at_secs threshold
    ///
    /// Returns true if:
    /// - show_at_secs is 0 (always show), OR
    /// - remaining BASE time is at or below show_at_secs threshold
    ///
    /// For cooldowns with ready state, we add cooldown_ready_secs to the threshold
    /// so that show_at_secs represents seconds of BASE duration remaining.
    /// E.g., show_at_secs=30 with ready_secs=20 shows when total remaining <= 50,
    /// which is when base remaining = 30.
    ///
    /// `remaining_total` is the total remaining seconds from interpolated game time.
    pub fn is_visible(&self, remaining_total: f32) -> bool {
        if self.show_at_secs <= 0.0 {
            return true; // 0 means always show
        }
        // Add ready_secs to threshold so comparison is against total remaining
        // but threshold represents base duration remaining
        let effective_threshold = self.show_at_secs + self.cooldown_ready_secs;
        remaining_total <= effective_threshold
    }

    /// Check if countdown should be announced (matches timer logic exactly)
    ///
    /// Announces N when remaining is in [N, N+0.3) to sync with visual display:
    /// - remaining 3.8s → no announcement (too early)
    /// - remaining 3.2s → announces 3 (in window [3.0, 3.3))
    /// - remaining 2.2s → announces 2
    ///
    /// `remaining_base` is the BASE remaining seconds (excludes ready state time),
    /// computed from interpolated game time.
    pub fn check_countdown(&mut self, remaining_base: f32) -> Option<u8> {
        // Don't announce if effect was manually removed
        if self.removed_at.is_some() {
            return None;
        }

        if !self.audio_enabled || self.countdown_start == 0 {
            return None;
        }

        // Check each second from countdown_start down to 1
        for seconds in (1..=self.countdown_start.min(10)).rev() {
            let lower = seconds as f32;
            let upper = lower + 0.3;

            // Announce when remaining is in [N, N+0.3)
            if remaining_base >= lower && remaining_base < upper {
                let index = (seconds - 1) as usize;
                if !self.countdown_announced[index] {
                    self.countdown_announced[index] = true;
                    return Some(seconds);
                }
            }
        }
        None
    }

    /// Check if audio should fire at the configured offset (matches timer logic)
    ///
    /// Returns true (and marks as fired) when:
    /// - audio_file is Some
    /// - audio_offset > 0 (offset of 0 means fire on expiration, handled separately)
    /// - remaining time crossed below the offset threshold
    /// - hasn't already fired
    ///
    /// `remaining_base` is the BASE remaining seconds (excludes ready state time),
    /// computed from interpolated game time.
    pub fn check_audio_offset(&mut self, remaining_base: f32) -> bool {
        // Don't play if effect was manually removed
        if self.removed_at.is_some() {
            return false;
        }

        // No audio file configured
        if self.audio_file.is_none() {
            return false;
        }

        // offset=0 means fire on expiration, not here
        if self.audio_offset == 0 {
            return false;
        }

        // Already fired
        if self.audio_played {
            return false;
        }

        // Fire when we cross into the offset window
        if remaining_base <= self.audio_offset as f32 && remaining_base > 0.0 {
            self.audio_played = true;
            return true;
        }

        false
    }

    // Note: On-end expiration alerts are fired exclusively by EffectTracker::tick(),
    // which checks on_end_alert_fired + has_base_duration_ended()/removed_at after
    // signals have been dispatched. This avoids false "effect ended" alerts when an
    // effect is refreshed just before its timer expires.
}

/// Key for identifying unique effect instances
///
/// An effect is unique per (definition, source, target) triple by default.
/// Source is included so that two healers of the same class tracking
/// the same game effect (e.g., Kolto Shell from local vs other player)
/// don't collide — each source entity gets its own slot.
/// If the same source reapplies the same effect to the same target, it refreshes.
///
/// `RefreshScope::Source` and `Target` collapse one axis by setting the
/// corresponding field to `None`. `Option<i64>` is used (not a sentinel like 0)
/// because `target_id == 0` is a real value in the combat log meaning AoE / no target.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EffectKey {
    pub definition_id: String,
    pub source_entity_id: Option<i64>,
    pub target_entity_id: Option<i64>,
}

impl EffectKey {
    /// Construct a key for a definition, dropping one axis when scope is
    /// `Source` or `Target`. This is the canonical constructor for both
    /// storage and lookup so they always agree.
    pub fn for_scope(definition_id: &str, scope: RefreshScope, source_entity_id: i64, target_entity_id: i64) -> Self {
        let (s, t) = match scope {
            RefreshScope::Both => (Some(source_entity_id), Some(target_entity_id)),
            RefreshScope::Source => (Some(source_entity_id), None),
            RefreshScope::Target => (None, Some(target_entity_id)),
        };
        Self {
            definition_id: definition_id.to_string(),
            source_entity_id: s,
            target_entity_id: t,
        }
    }
}
