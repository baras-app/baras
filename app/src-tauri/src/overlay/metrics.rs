//! Metric entry creation helpers
//!
//! Functions for converting player metrics into overlay entries.

use std::collections::HashMap;

use baras_core::PlayerMetrics;
use baras_overlay::{Color, MetricEntry};

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
        },
        MetricType::EDps => MetricValues {
            rate: m.edps,
            total: m.total_damage_effective,
            split_rate: Some(m.bossdps),
            split_total: Some(m.total_damage_boss),
            split_color: None, // Uses default lighter color for adds
        },
        MetricType::BossDps => MetricValues {
            rate: m.bossdps,
            total: m.total_damage_boss,
            split_rate: None,
            split_total: None,
            split_color: None,
        },
        MetricType::Hps => MetricValues {
            rate: m.hps,
            total: m.total_healing,
            split_rate: Some(m.ehps),
            split_total: Some(m.total_healing_effective),
            split_color: None, // Uses default lighter color for overheal
        },
        MetricType::EHps => MetricValues {
            // ehps/total now include shielding, split shows healing vs shields
            rate: m.ehps,
            total: m.total_healing_effective,
            split_rate: Some(m.ehps - m.abs), // Healing only (exclude shields)
            split_total: Some(m.total_healing_effective - m.total_shielding),
            split_color: Some(shield_blue()), // Blue for shield portion
        },
        MetricType::Tps => MetricValues {
            rate: m.tps,
            total: m.total_threat,
            split_rate: None,
            split_total: None,
            split_color: None,
        },
        MetricType::Dtps => MetricValues {
            rate: m.edtps,
            total: m.total_damage_taken_effective,
            split_rate: None,
            split_total: None,
            split_color: None,
        },
        MetricType::Abs => MetricValues {
            rate: m.abs,
            total: m.total_shielding,
            split_rate: None,
            split_total: None,
            split_color: None,
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
    metrics: &[PlayerMetrics],
) -> Vec<MetricEntry> {
    let mut values: Vec<_> = metrics
        .iter()
        .map(|m| {
            let v = extract_values(m, overlay_type);
            (m.name.clone(), v)
        })
        .collect();

    // Sort by rate value descending (highest first)
    values.sort_by(|a, b| b.1.rate.cmp(&a.1.rate));

    let max_value = values.iter().map(|(_, v)| v.rate).max().unwrap_or(1);

    values
        .into_iter()
        .map(|(name, v)| {
            let mut entry = MetricEntry::new(&name, v.rate, max_value).with_total(v.total);
            if let (Some(sr), Some(st)) = (v.split_rate, v.split_total) {
                entry = entry.with_split(sr, st);
                if let Some(color) = v.split_color {
                    entry = entry.with_split_color(color);
                }
            }
            entry
        })
        .collect()
}

/// Create entries for all overlay types from metrics
pub fn create_all_entries(metrics: &[PlayerMetrics]) -> HashMap<MetricType, Vec<MetricEntry>> {
    let mut result = HashMap::new();
    for overlay_type in MetricType::all() {
        result.insert(
            *overlay_type,
            create_entries_for_type(*overlay_type, metrics),
        );
    }
    result
}
