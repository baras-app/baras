//! Dynamic boss NPC registry
//!
//! Tracks boss NPC IDs from loaded definition files. This allows new content
//! to be added via TOML configs without requiring code changes.
//!
//! The registry is checked FIRST by `is_boss()`, with hardcoded data as fallback.

use hashbrown::HashSet;
use std::sync::{LazyLock, RwLock};

/// Global registry of boss NPC IDs from loaded definitions
static BOSS_NPC_REGISTRY: LazyLock<RwLock<HashSet<i64>>> =
    LazyLock::new(|| RwLock::new(HashSet::new()));

/// Register boss NPC IDs from a loaded definition.
/// Called when boss definitions are loaded (e.g., on AreaEntered).
pub fn register_boss_npcs(npc_ids: &[i64]) {
    if let Ok(mut registry) = BOSS_NPC_REGISTRY.write() {
        for id in npc_ids {
            registry.insert(*id);
        }
    }
}

/// Clear all registered boss NPCs.
/// Called when leaving an area or resetting definitions.
pub fn clear_boss_registry() {
    if let Ok(mut registry) = BOSS_NPC_REGISTRY.write() {
        registry.clear();
    }
}

/// Check if an NPC ID is registered as a boss (from loaded definitions).
/// Returns None if registry is empty or lock failed (caller should use fallback).
pub fn is_registered_boss(npc_id: i64) -> Option<bool> {
    BOSS_NPC_REGISTRY
        .read()
        .ok()
        .filter(|r| !r.is_empty())
        .map(|r| r.contains(&npc_id))
}

/// Get count of registered boss NPCs (for diagnostics).
pub fn registered_boss_count() -> usize {
    BOSS_NPC_REGISTRY
        .read()
        .map(|r| r.len())
        .unwrap_or(0)
}
