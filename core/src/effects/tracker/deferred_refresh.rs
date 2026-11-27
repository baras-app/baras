//! Refreshes deferred to damage: AoE target correlation and first-damage DoT confirmation.

use super::*;

impl EffectTracker {
    /// Set up pending AoE refresh state when AbilityActivate has [=] target.
    /// Only sets up state for AoE refresh abilities (definitions with `is_aoe_refresh = true`)
    /// that use damage correlation instead of individual ApplyEffect signals.
    pub(super) fn setup_pending_aoe_refresh(
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
    pub(super) fn handle_damage_for_aoe_refresh(
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
    pub(super) fn refresh_aoe_immediate(
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
    pub(super) fn finalize_aoe_refresh(&mut self) {
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
    pub(super) fn handle_damage_for_dot_refresh(
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
    pub(super) fn clear_pending_dot_refresh(pending: &mut Option<PendingDotRefresh>, action_id: i64, target_id: i64) {
        if pending
            .as_ref()
            .is_some_and(|p| p.ability_id == action_id && p.target_id.is_none_or(|t| t == target_id))
        {
            *pending = None;
        }
    }
}
