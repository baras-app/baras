//! Common serde default value functions
//!
//! Used across timer, effect, and boss definitions to avoid duplication.
//! Also provides predicates for `skip_serializing_if` to keep TOML files clean.

/// Default for enabled fields
pub fn default_true() -> bool {
    true
}

// ─────────────────────────────────────────────────────────────────────────────
// skip_serializing_if predicates
// ─────────────────────────────────────────────────────────────────────────────

/// Skip serializing if bool is false (the default)
pub fn is_false(b: &bool) -> bool {
    !*b
}

/// Skip serializing if u8 is zero
pub fn is_zero_u8(n: &u8) -> bool {
    *n == 0
}

/// Skip serializing if f32 is zero
pub fn is_zero_f32(n: &f32) -> bool {
    *n == 0.0
}

/// Skip serializing if Vec is empty
pub fn is_empty_vec<T>(v: &Vec<T>) -> bool {
    v.is_empty()
}

/// Default timer/effect color (light gray with full opacity)
pub fn default_timer_color() -> [u8; 4] {
    [200, 200, 200, 255]
}

/// Default entity filter for boss timer source/target (matches any entity)
/// Boss timers need permissive defaults since abilities come from NPCs, not players.
pub fn default_entity_filter_any() -> crate::effects::EntityFilter {
    crate::effects::EntityFilter::Any
}
