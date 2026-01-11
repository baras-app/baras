//! Icon registry for ability/effect icon lookups.
//!
//! Loads ability ID to icon name mappings from icons.csv.
//! Icons are lazy-loaded from ZIP archives with LRU caching.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;

/// Registry mapping ability IDs to icon names.
///
/// Loaded from icons.csv at startup. The icon names reference
/// PNG files in the icons.zip archives.
#[derive(Debug, Default)]
pub struct IconRegistry {
    /// Maps ability_id -> icon_name (without path or extension)
    ability_to_icon: HashMap<u64, String>,
    /// Maps ability_id -> english name (for fallback display)
    ability_to_name: HashMap<u64, String>,
}

impl IconRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load icon mappings from CSV file.
    ///
    /// CSV format: `ability_id,en,icon`
    /// - ability_id: SWTOR ability/effect GUID
    /// - en: English display name
    /// - icon: Icon filename (without path or .png extension)
    pub fn load_from_csv(path: &Path) -> std::io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let reader = BufReader::new(file);
        let mut registry = Self::new();

        for (line_num, line_result) in reader.lines().enumerate() {
            let line = line_result?;

            // Skip header and empty lines
            if line_num == 0 || line.trim().is_empty() {
                continue;
            }

            // Parse CSV: ability_id,en,icon
            let parts: Vec<&str> = line.splitn(3, ',').collect();
            if parts.len() < 3 {
                continue;
            }

            // Parse ability_id, skip BOM if present on first data line
            let id_str = parts[0].trim().trim_start_matches('\u{feff}');
            let Ok(ability_id) = id_str.parse::<u64>() else {
                continue;
            };

            let name = parts[1].trim().to_string();
            let icon = parts[2].trim().to_string();

            if !icon.is_empty() {
                registry.ability_to_icon.insert(ability_id, icon);
            }
            if !name.is_empty() {
                registry.ability_to_name.insert(ability_id, name);
            }
        }

        Ok(registry)
    }

    /// Get the icon name for an ability ID.
    ///
    /// Returns the icon filename without path or extension.
    /// Use this to look up the actual icon file in ZIP archives.
    #[inline]
    pub fn get_icon(&self, ability_id: u64) -> Option<&str> {
        self.ability_to_icon.get(&ability_id).map(|s| s.as_str())
    }

    /// Get the English name for an ability ID.
    ///
    /// Useful as fallback text when icon is unavailable.
    #[inline]
    pub fn get_name(&self, ability_id: u64) -> Option<&str> {
        self.ability_to_name.get(&ability_id).map(|s| s.as_str())
    }

    /// Check if an ability ID has an icon mapping.
    #[inline]
    pub fn has_icon(&self, ability_id: u64) -> bool {
        self.ability_to_icon.contains_key(&ability_id)
    }

    /// Get the number of loaded icon mappings.
    #[inline]
    pub fn len(&self) -> usize {
        self.ability_to_icon.len()
    }

    /// Check if the registry is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ability_to_icon.is_empty()
    }
}

/// Server tick bias for duration calculations (30ms).
pub const TICK_BIAS_SECS: f32 = 0.030;

/// Calculate actual effect duration accounting for alacrity, latency, and server tick bias.
///
/// Formula: `(base_duration / (1 + alacrity)) + latency + tick_bias`
///
/// # Arguments
/// * `base_duration_secs` - Base duration from effect definition
/// * `alacrity_percent` - Player's alacrity percentage (e.g., 15.4 for 15.4%)
/// * `latency_ms` - Average network latency in milliseconds
///
/// # Returns
/// Adjusted duration in seconds
#[inline]
pub fn calculate_effect_duration(
    base_duration_secs: f32,
    alacrity_percent: f32,
    latency_ms: u16,
) -> f32 {
    let alacrity_decimal = alacrity_percent / 100.0;
    let latency_secs = latency_ms as f32 / 1000.0;
    (base_duration_secs / (1.0 + alacrity_decimal)) + latency_secs + TICK_BIAS_SECS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_duration_calculation() {
        // Base 18s DOT with 15% alacrity, 50ms latency
        let duration = calculate_effect_duration(18.0, 15.0, 50);
        // Expected: 18 / 1.15 + 0.05 + 0.03 ≈ 15.65 + 0.08 ≈ 15.73
        assert!((duration - 15.73).abs() < 0.1);
    }

    #[test]
    fn test_duration_no_alacrity() {
        // No alacrity, no latency
        let duration = calculate_effect_duration(18.0, 0.0, 0);
        // Expected: 18 / 1.0 + 0 + 0.03 = 18.03
        assert!((duration - 18.03).abs() < 0.01);
    }
}
