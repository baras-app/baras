//! Serde default helpers shared across modules

// ─────────────────────────────────────────────────────────────────────────────
// Serde Default Helpers
// ─────────────────────────────────────────────────────────────────────────────

pub(crate) fn default_lockout_tiers() -> [bool; 3] {
    [true; 3]
}

pub(crate) fn default_true() -> bool {
    true
}
pub(crate) fn default_scaling_factor() -> f32 {
    1.0
}

pub(crate) fn is_zero_f32_ref(v: &f32) -> bool {
    *v == 0.0
}

pub(crate) fn is_false_ref(v: &bool) -> bool {
    !v
}
