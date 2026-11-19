//! Metric entry creation helpers
//!
//! Functions for converting player metrics into overlay entries.

use std::collections::HashMap;
use std::sync::Arc;

use baras_core::game_data::Role as GameRole;
use baras_core::PlayerMetrics;
use baras_overlay::{Color, MetricEntry, Role as OverlayRole};

use super::types::MetricType;

/// Blue color for shielding portion of split bars
fn shield_blue() -> Color {
    Color::from_rgba8(70, 130, 180, 255) // Steel blue
}

/// Extracted metric values for overlay rendering
struct MetricValues {
    rate: i64,
    total: i64,
    split_rate: Option<i64>,
    split_total: Option<i64>,
    split_color: Option<Color>,
    display_value: Option<String>,
    display_total: Option<String>,
}

/// Extracts metric values from PlayerMetrics based on overlay type
fn extract_values(m: &PlayerMetrics, overlay_type: MetricType) -> MetricValues {
    match overlay_type {
        MetricType::Dps => MetricValues {
            rate: m.dps,
            total: m.total_damage,
            split_rate: None,
            split_total: None,
            split_color: None,
            display_value: None,
            display_total: None,
        },
        MetricType::EDps => MetricValues {
            rate: m.dps,
            total: m.total_damage,
            split_rate: Some(m.bossdps),
            split_total: Some(m.total_damage_boss),
            split_color: None,
            display_value: None,
            display_total: None,
        },
        MetricType::BossDps => MetricValues {
            rate: m.bossdps,
            total: m.total_damage_boss,
            split_rate: None,
            split_total: None,
            split_color: None,
            display_value: None,
            display_total: None,
        },
        MetricType::Hps => MetricValues {
            rate: m.hps,
            total: m.total_healing,
            split_rate: Some(m.ehps),
            split_total: Some(m.total_healing_effective),
            split_color: None,
            display_value: None,
            display_total: None,
        },
        MetricType::EHps => MetricValues {
            rate: m.ehps,
            total: m.total_healing_effective,
            split_rate: Some(m.ehps - m.abs),
            split_total: Some(m.total_healing_effective - m.total_shielding),
            split_color: Some(shield_blue()),
            display_value: None,
            display_total: None,
        },
        MetricType::Tps => MetricValues {
            rate: m.tps,
            total: m.total_threat,
            split_rate: None,
            split_total: None,
            split_color: None,
            display_value: None,
            display_total: None,
        },
        MetricType::Dtps => MetricValues {
            rate: m.edtps,
            total: m.total_damage_taken_effective,
            split_rate: None,
            split_total: None,
            split_color: None,
            display_value: None,
            display_total: None,
        },
        MetricType::Htps => MetricValues {
            rate: m.htps,
            total: m.total_healing_received,
            split_rate: Some(m.ehtps),
            split_total: Some(m.total_healing_received_effective),
            split_color: None,
            display_value: None,
            display_total: None,
        },
        MetricType::Apm => MetricValues {
            rate: m.apm as i64,
            total: 0,
            split_rate: None,
            split_total: None,
            split_color: None,
            display_value: Some(format!("{:.1}", m.apm)),
            display_total: Some("-".to_string()),
        },
        MetricType::Interrupts => MetricValues {
            rate: m.interrupt_casts as i64,
            total: 0,
            split_rate: None,
            split_total: None,
            split_color: None,
            display_value: Some(m.interrupt_casts.to_string()),
            display_total: Some("-".to_string()),
        },
        // Not derived from PlayerMetrics — entries come from CombatData::incoming_damage
        // via create_incoming_damage_entries
        MetricType::IncomingDamage => MetricValues {
            rate: 0,
            total: 0,
            split_rate: None,
            split_total: None,
            split_color: None,
            display_value: None,
            display_total: None,
        },
    }
}

/// Create meter entries for a specific overlay type from player metrics
///
/// Note: Entry colors are NOT set here - entries use the default (dps_bar_fill) color
/// so that the overlay renderer will use the configured bar_color from appearance settings.
/// This allows users to customize bar colors via the config panel.
pub fn create_entries_for_type(
    overlay_type: MetricType,
    metrics: &[Arc<PlayerMetrics>],
    local_player_id: i64,
) -> Vec<MetricEntry> {
    let mut values: Vec<_> = metrics
        .iter()
        .map(|m| {
            let v = extract_values(m, overlay_type);
            let class_icon = m.class_icon.clone();
            let discipline_icon = m.discipline.map(|d| d.icon_name().to_string());
            let class_name = m.class_name.clone();
            let role = m.discipline.map(|d| match d.role() {
                GameRole::Tank => OverlayRole::Tank,
                GameRole::Healer => OverlayRole::Healer,
                GameRole::Dps => OverlayRole::Damage,
            });
            let is_local = m.entity_id == local_player_id;
            let is_enemy = m.pvp_faction == Some(baras_core::encounter::PvpFaction::Enemy);
            (m.name.clone(), v, class_icon, discipline_icon, class_name, role, is_local, is_enemy, m)
        })
        .collect();

    // Friendly group first, then enemies (PvP); rate descending within each group
    values.sort_by(|a, b| a.7.cmp(&b.7).then(b.1.rate.cmp(&a.1.rate)));

    let max_value = values.iter().map(|v| v.1.rate).max().unwrap_or(1);

    values
        .into_iter()
        .map(|(name, v, class_icon, discipline_icon, class_name, role, is_local, is_enemy, player)| {
            let mut entry = MetricEntry::new(&name, v.rate, max_value).with_total(v.total);
            entry.player = Some(Arc::clone(player));
            if let (Some(dv), Some(dt)) = (v.display_value, v.display_total) {
                entry = entry.with_display(dv, dt);
            }
            entry.is_local = is_local;
            entry.is_enemy = is_enemy;
            if let (Some(sr), Some(st)) = (v.split_rate, v.split_total) {
                entry = entry.with_split(sr, st);
                if let Some(color) = v.split_color {
                    entry = entry.with_split_color(color);
                }
            }
            if let Some(icon) = class_icon {
                if let Some(role) = role {
                    entry = entry.with_class_icon(icon, role);
                } else {
                    entry = entry.with_icon(icon);
                }
            }
            if let Some(icon) = discipline_icon {
                entry = entry.with_discipline_icon(icon);
            }
            if let Some(name) = class_name {
                entry = entry.with_class_name(name);
            }
            entry
        })
        .collect()
}

/// Create entries for the Incoming Damage overlay (local player's DTPS by source).
/// `metrics` provides class/discipline icons for attackers that are players.
pub fn create_incoming_damage_entries(
    rows: &[crate::service::IncomingDamageRow],
    metrics: &[PlayerMetrics],
) -> Vec<MetricEntry> {
    let max_value = rows.iter().map(|r| r.rate).max().unwrap_or(1);
    rows.iter()
        .map(|row| {
            let mut entry = MetricEntry::new(&row.name, row.rate, max_value).with_total(row.total);
            if let Some(m) = metrics.iter().find(|m| m.entity_id == row.entity_id) {
                let role = m.discipline.map(|d| match d.role() {
                    GameRole::Tank => OverlayRole::Tank,
                    GameRole::Healer => OverlayRole::Healer,
                    GameRole::Dps => OverlayRole::Damage,
                });
                if let Some(icon) = m.class_icon.clone() {
                    if let Some(role) = role {
                        entry = entry.with_class_icon(icon, role);
                    } else {
                        entry = entry.with_icon(icon);
                    }
                }
                if let Some(d) = m.discipline {
                    entry = entry.with_discipline_icon(d.icon_name().to_string());
                }
                if let Some(name) = m.class_name.clone() {
                    entry = entry.with_class_name(name);
                }
            }
            entry
        })
        .collect()
}

/// Players' stats, shared by every overlay's entries (template placeholders)
pub fn shared_metrics(metrics: &[PlayerMetrics]) -> Vec<Arc<PlayerMetrics>> {
    metrics.iter().cloned().map(Arc::new).collect()
}

/// Create entries for all overlay types from metrics
pub fn create_all_entries(
    metrics: &[PlayerMetrics],
    local_player_id: i64,
) -> HashMap<MetricType, Vec<MetricEntry>> {
    let metrics = &shared_metrics(metrics);
    let mut result = HashMap::new();
    for overlay_type in MetricType::all() {
        // Incoming Damage is built from CombatData::incoming_damage by the router
        if *overlay_type == MetricType::IncomingDamage {
            continue;
        }
        result.insert(
            *overlay_type,
            create_entries_for_type(*overlay_type, metrics, local_player_id),
        );
    }
    result
}
