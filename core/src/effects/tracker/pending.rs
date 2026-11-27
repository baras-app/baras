//! State held between a refresh ability's cast and the damage that confirms it.

use chrono::NaiveDateTime;

use crate::context::IStr;

/// Pending AoE refresh waiting for damage correlation
#[derive(Debug, Clone)]
pub(super) struct PendingAoeRefresh {
    /// The ability that was activated
    pub ability_id: i64,
    /// Who cast the ability
    pub source_id: i64,
    /// When the ability was activated
    pub timestamp: NaiveDateTime,
    /// The primary target (resolved at cast time)
    pub primary_target: i64,
}

/// Pending single-target DotTracker refresh waiting for damage confirmation.
/// Set when AbilityActivated fires for a refresh ability on a non-AoE DotTracker effect.
/// Consumed by the next DamageTaken event from the same ability.
#[derive(Debug, Clone)]
pub(super) struct PendingDotRefresh {
    /// The ability that was activated
    pub ability_id: i64,
    /// Who cast the ability
    pub source_id: i64,
    /// When the ability was activated
    pub timestamp: NaiveDateTime,
    /// Caster's selected target at activation (None when unknown or self).
    /// Only a damage event on this target may confirm the refresh — a bystander's
    /// periodic tick from the same DOT must not claim it.
    pub target_id: Option<i64>,
}

/// State for collecting AoE damage targets after finding anchor
#[derive(Debug, Clone)]
pub(super) struct AoeRefreshCollecting {
    /// The ability being tracked
    pub ability_id: i64,
    /// Its name, for refresh abilities selected by name
    pub ability_name: IStr,
    /// Who cast the ability
    pub source_id: i64,
    /// Anchor timestamp (when primary target was hit)
    pub anchor_timestamp: NaiveDateTime,
    /// Targets collected so far (within ±10ms window)
    pub targets: Vec<i64>,
}
