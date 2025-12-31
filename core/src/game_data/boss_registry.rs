//! Dynamic HP overlay entity registry
//!
//! Tracks NPC IDs and display names for entities that should appear on the
//! Boss HP overlay. This allows custom entities from TOML configs to show
//! on the HP bar without requiring code changes.
//!
//! The registry is populated from `show_on_hp_overlay` entity definitions.

use hashbrown::HashMap;
use std::sync::{LazyLock, RwLock};

/// Global registry of HP overlay entities (npc_id -> display_name)
static HP_OVERLAY_REGISTRY: LazyLock<RwLock<HashMap<i64, String>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Register an entity for HP overlay display.
/// Called when boss definitions are loaded (e.g., on AreaEntered).
pub fn register_hp_overlay_entity(npc_id: i64, name: &str) {
    if let Ok(mut registry) = HP_OVERLAY_REGISTRY.write() {
        registry.insert(npc_id, name.to_string());
    }
}

/// Clear all registered HP overlay entities.
/// Called when leaving an area or resetting definitions.
pub fn clear_boss_registry() {
    if let Ok(mut registry) = HP_OVERLAY_REGISTRY.write() {
        registry.clear();
    }
}

/// Look up the display name for a registered HP overlay entity.
/// Returns None if not registered or lock failed.
pub fn lookup_registered_name(npc_id: i64) -> Option<String> {
    HP_OVERLAY_REGISTRY
        .read()
        .ok()
        .and_then(|r| r.get(&npc_id).cloned())
}

/// Check if an NPC ID is registered for HP overlay (from loaded definitions).
/// Returns None if registry is empty or lock failed (caller should use fallback).
pub fn is_registered_boss(npc_id: i64) -> Option<bool> {
    HP_OVERLAY_REGISTRY
        .read()
        .ok()
        .filter(|r| !r.is_empty())
        .map(|r| r.contains_key(&npc_id))
}
