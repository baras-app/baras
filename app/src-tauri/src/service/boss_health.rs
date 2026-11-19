//! Boss health overlay data

use super::effect_data::{base_total_secs, calculate_base_remaining_secs, calculate_remaining_secs};
use crate::state::SharedState;
use baras_overlay::{BossEffectIcon, BossHealthData};
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// Build boss health data from the current encounter
pub(super) async fn build_boss_health_data(
    shared: &Arc<SharedState>,
    icon_cache: Option<&Arc<baras_overlay::icons::IconCache>>,
) -> Option<BossHealthData> {
    use std::collections::HashMap;
    use std::sync::Arc as StdArc;

    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;
    let cache = session.session_cache.as_ref()?;

    // If not in combat, send empty data to clear overlay (if auto_hide enabled)
    let in_combat = shared.in_combat.load(Ordering::SeqCst);
    if !in_combat {
        return Some(BossHealthData::default());
    }

    let entries = cache.get_boss_health();

    // Collect BossHealth-targeted effects grouped by target entity id (so two
    // NPCs sharing a display name don't share icons).
    let boss_icons: HashMap<i64, Vec<BossEffectIcon>> = if let Some(effect_tracker) = session.effect_tracker() {
        let tracker = effect_tracker.lock().unwrap_or_else(|p| p.into_inner());
        let interp_time = tracker.interpolated_game_time();
        tracker
            .boss_health_effects_by_target_id()
            .into_iter()
            .filter_map(|(entity_id, effects)| {
                let icons: Vec<BossEffectIcon> = effects
                    .into_iter()
                    .filter_map(|effect| {
                        let remaining_total = calculate_remaining_secs(effect, interp_time)?;
                        if !effect.is_visible(remaining_total) {
                            return None;
                        }
                        let remaining_secs = calculate_base_remaining_secs(effect, interp_time)?;
                        let total_secs = base_total_secs(effect)?;
                        let icon = icon_cache.and_then(|cache| {
                            cache
                                .get_icon(effect.icon_ability_id)
                                .map(|data| StdArc::new((data.width, data.height, data.rgba)))
                        });
                        Some(BossEffectIcon {
                            effect_id: effect.game_effect_id,
                            icon_ability_id: effect.icon_ability_id,
                            name: effect.name.clone(),
                            remaining_secs,
                            total_secs,
                            color: effect.color,
                            show_icon: effect.show_icon,
                            icon,
                        })
                    })
                    .collect();
                if icons.is_empty() {
                    None
                } else {
                    Some((entity_id, icons))
                }
            })
            .collect()
    } else {
        HashMap::new()
    };

    // Resolve game icons for active shields (deduped by ability id).
    let mut shield_icons: HashMap<u64, StdArc<(u32, u32, Vec<u8>)>> = HashMap::new();
    if let Some(cache) = icon_cache {
        for id in entries
            .iter()
            .flat_map(|e| e.active_shields.iter())
            .filter_map(|s| s.icon_ability_id)
        {
            if !shield_icons.contains_key(&id)
                && let Some(data) = cache.get_icon(id)
            {
                shield_icons.insert(id, StdArc::new((data.width, data.height, data.rgba)));
            }
        }
    }

    Some(BossHealthData {
        entries,
        boss_icons,
        shield_icons,
        force_clear: false,
    })
}
