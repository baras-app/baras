//! Log file area indexing
//!
//! Lightweight indexing of AreaEntered events in combat log files.
//! Enables filtering log files by operation/area name in the file browser.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Result as IoResult};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use encoding_rs::WINDOWS_1252;
use serde::{Deserialize, Serialize};

use crate::game_data::Difficulty;

/// A single area visit entry extracted from a log file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAreaEntry {
    pub area_id: i64,
    pub area_name: String,
    pub difficulty_id: i64,
    pub difficulty_name: String,
}

impl FileAreaEntry {
    /// Format for display: "AreaName Difficulty" with shorthand for 8/16 player content
    /// Examples:
    /// - "8 Player Master" -> "Dread Palace NiM 8"
    /// - "16 Player Veteran" -> "Dxun HM 16"
    /// - "4 Player Veteran" -> "Hammer Station 4 Player Veteran" (kept as-is)
    pub fn display_name(&self) -> String {
        // Language-independent path: derive shorthand from the difficulty ID
        if let Some(diff) = Difficulty::from_difficulty_id(self.difficulty_id) {
            let size = diff.group_size();
            if size == 8 || size == 16 {
                let short = match diff.config_key() {
                    "master" => "NiM",
                    "veteran" => "HM",
                    _ => "SM",
                };
                return format!("{} {} {}", self.area_name, short, size);
            }
        }

        if self.difficulty_name.is_empty() {
            return self.area_name.clone();
        }

        // Fallback for unknown IDs: parse the (English) difficulty string
        // "8 Player Master", "16 Player Veteran", etc.
        let parts: Vec<&str> = self.difficulty_name.split_whitespace().collect();

        // Expected format: ["8", "Player", "Master"] or ["16", "Player", "Veteran"]
        if parts.len() >= 3 && parts[1].eq_ignore_ascii_case("Player") {
            if let Ok(player_count) = parts[0].parse::<u8>() {
                let difficulty_word = parts[2..].join(" "); // Handle "Story Mode" etc.

                // Only apply shorthand for 8 and 16 player content
                if player_count == 8 || player_count == 16 {
                    let short = match difficulty_word.to_lowercase().as_str() {
                        "master" => "NiM",
                        "veteran" => "HM",
                        "story" => "SM",
                        _ => return format!("{} {}", self.area_name, self.difficulty_name),
                    };
                    return format!("{} {} {}", self.area_name, short, player_count);
                }
            }
        }

        // Fallback: keep original format for 4-player or unrecognized
        format!("{} {}", self.area_name, self.difficulty_name)
    }

    /// Badge tier for the file browser, derived from the difficulty ID
    /// (language-independent): "fp" for 4-player, "nim"/"hm"/"sm" for 8/16.
    pub fn difficulty_tier(&self) -> Option<&'static str> {
        let diff = Difficulty::from_difficulty_id(self.difficulty_id)?;
        Some(if diff.group_size() == 4 {
            "fp"
        } else {
            match diff.config_key() {
                "master" => "nim",
                "veteran" => "hm",
                _ => "sm",
            }
        })
    }
}

/// Indexed area information for a single log file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAreaIndex {
    /// File modification time (seconds since epoch) for cache invalidation
    pub modified_secs: u64,
    /// Areas visited in this file (deduplicated by area_id + difficulty_id)
    pub areas: Vec<FileAreaEntry>,
}

/// Cache of area indexes for all log files
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogAreaCache {
    /// Cache format version (increment when format changes)
    pub version: u32,
    /// Map of file path -> area index
    pub entries: HashMap<PathBuf, FileAreaIndex>,
}

const CACHE_VERSION: u32 = 1;

impl LogAreaCache {
    pub fn new() -> Self {
        Self {
            version: CACHE_VERSION,
            entries: HashMap::new(),
        }
    }

    /// Load cache from disk, returning empty cache if file doesn't exist or is invalid
    pub fn load_from_disk(cache_path: &Path) -> Self {
        match fs::read_to_string(cache_path) {
            Ok(content) => match serde_json::from_str::<LogAreaCache>(&content) {
                Ok(cache) if cache.version == CACHE_VERSION => cache,
                Ok(_) => {
                    tracing::info!("Area cache version mismatch, rebuilding");
                    Self::new()
                }
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to parse area cache, rebuilding");
                    Self::new()
                }
            },
            Err(_) => Self::new(),
        }
    }

    /// Save cache to disk
    pub fn save_to_disk(&self, cache_path: &Path) -> IoResult<()> {
        // Ensure parent directory exists
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        fs::write(cache_path, content)
    }

    /// Get cached areas for a file path
    pub fn get(&self, path: &Path) -> Option<&FileAreaIndex> {
        self.entries.get(path)
    }

    /// Insert or update an entry
    pub fn insert(&mut self, path: PathBuf, index: FileAreaIndex) {
        self.entries.insert(path, index);
    }

    /// Check if a file needs to be (re)indexed
    /// Returns true if file is not in cache or has been modified since caching
    pub fn needs_update(&self, path: &Path, current_modified: SystemTime) -> bool {
        let current_secs = current_modified
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        match self.entries.get(path) {
            Some(entry) => entry.modified_secs != current_secs,
            None => true,
        }
    }

    /// Forget one file (deleted through the library)
    pub fn remove(&mut self, path: &Path) {
        self.entries.remove(path);
    }

    /// Remove entries for files that no longer exist
    pub fn prune_missing(&mut self) {
        self.entries.retain(|path, _| path.exists());
    }

    /// Get the number of cached entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Area Extraction
// ─────────────────────────────────────────────────────────────────────────────

/// The AreaEntered effect type ID as bytes - for fast pattern matching on raw bytes
const MARKER_BYTES: &[u8] = b"{836045448953664}";

/// The AreaEntered effect type ID as string - for parsing decoded lines
const AREA_ENTERED_MARKER: &str = "{836045448953664}";

/// Extract AreaEntered events from a log file efficiently.
///
/// Reads file as raw bytes line-by-line, pattern matches on ASCII marker,
/// then decodes only matching lines with Windows-1252 encoding.
/// Only returns areas that exist in `known_area_ids` (from definition files).
///
/// # Arguments
/// * `path` - Path to the combat log file
/// * `known_area_ids` - Set of area IDs that have definitions (only these are indexed)
///
/// # Returns
/// Deduplicated list of areas visited in the file
pub fn extract_areas_from_file(
    path: &Path,
    known_area_ids: &HashSet<i64>,
) -> IoResult<Vec<FileAreaEntry>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    let mut areas = Vec::new();
    let mut seen: HashSet<(i64, i64)> = HashSet::new(); // (area_id, difficulty_id)
    let mut line_buf = Vec::new();

    // Read line by line as raw bytes (handles Windows-1252 encoding properly)
    loop {
        line_buf.clear();
        let bytes_read = reader.read_until(b'\n', &mut line_buf)?;
        if bytes_read == 0 {
            break; // EOF
        }

        // Fast path: check if marker exists in raw bytes (ASCII matching)
        // This avoids decoding lines that don't contain AreaEntered events
        if !line_buf
            .windows(MARKER_BYTES.len())
            .any(|w| w == MARKER_BYTES)
        {
            continue;
        }

        // Only decode lines that contain the marker (Windows-1252 -> UTF-8)
        let (line, _, _) = WINDOWS_1252.decode(&line_buf);

        // Parse just the effect segment to extract area info
        if let Some(entry) = parse_area_entered_line(&line) {
            // Only include if area is in our definitions
            if known_area_ids.contains(&entry.area_id) {
                let key = (entry.area_id, entry.difficulty_id);
                if seen.insert(key) {
                    areas.push(entry);
                }
            }
        }
    }

    Ok(areas)
}

/// Parse an AreaEntered line to extract area information.
///
/// Line format example:
/// `[18:28:08.183] [@Player|...] [] [] [AreaEntered {836045448953664}: The Dread Palace {137438993410} 8 Player Master {836045448953655}] (he3000) <v7.0.0b>`
///
/// We need to extract:
/// - Area name and ID from the first name/ID pair after the colon
/// - Difficulty name and ID from the optional second name/ID pair
fn parse_area_entered_line(line: &str) -> Option<FileAreaEntry> {
    // Find the effect segment: [AreaEntered {type_id}: AreaName {area_id} ...]
    // Look for the marker and work backwards/forwards from there

    let marker_pos = line.find(AREA_ENTERED_MARKER)?;

    // Find the start of this bracket segment (the '[' before AreaEntered)
    let segment_start = line[..marker_pos].rfind('[')?;

    // Find the end of this bracket segment
    let segment_end = line[marker_pos..].find(']').map(|p| marker_pos + p)?;

    let segment = &line[segment_start + 1..segment_end];

    // segment is now like: "AreaEntered {836045448953664}: The Dread Palace {137438993410} 8 Player Master {836045448953655}"
    // or for German: "GebietBetreten {836045448953664}: Imperiale Flotte {137438989504}"

    // Find the colon that separates type from content
    let colon_pos = segment.find(':')?;
    let content = segment[colon_pos + 1..].trim();

    // Parse all {id} segments and the text between them
    let mut brace_positions: Vec<(usize, usize)> = Vec::new();
    let mut pos = 0;
    while let Some(start) = content[pos..].find('{') {
        let abs_start = pos + start;
        if let Some(end) = content[abs_start..].find('}') {
            let abs_end = abs_start + end;
            brace_positions.push((abs_start, abs_end));
            pos = abs_end + 1;
        } else {
            break;
        }
    }

    if brace_positions.is_empty() {
        return None;
    }

    // First brace pair is the area ID
    let (area_id_start, area_id_end) = brace_positions[0];
    let area_id: i64 = content[area_id_start + 1..area_id_end].parse().ok()?;
    let area_name = content[..area_id_start].trim().to_string();

    // Second brace pair (if exists) is the difficulty ID
    let (difficulty_name, difficulty_id) = if brace_positions.len() >= 2 {
        let (diff_id_start, diff_id_end) = brace_positions[1];
        let diff_id: i64 = content[diff_id_start + 1..diff_id_end].parse().ok()?;
        let diff_name = content[area_id_end + 1..diff_id_start].trim().to_string();
        (diff_name, diff_id)
    } else {
        (String::new(), 0)
    };

    Some(FileAreaEntry {
        area_id,
        area_name,
        difficulty_id,
        difficulty_name,
    })
}

/// Get the default cache file path
pub fn default_cache_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("baras").join("area_cache.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_area_entered_with_difficulty() {
        let line = r#"[18:28:08.183] [@Jerran Zeva#689501114780828|(-8.56,3.11,-0.98,358.89)|(426912/442951)] [] [] [AreaEntered {836045448953664}: The Dread Palace {137438993410} 8 Player Master {836045448953655}] (he3000) <v7.0.0b>"#;

        let entry = parse_area_entered_line(line).expect("Should parse");
        assert_eq!(entry.area_name, "The Dread Palace");
        assert_eq!(entry.area_id, 137438993410);
        assert_eq!(entry.difficulty_name, "8 Player Master");
        assert_eq!(entry.difficulty_id, 836045448953655);
        // 8 Player Master -> NiM 8
        assert_eq!(entry.display_name(), "The Dread Palace NiM 8");
    }

    #[test]
    fn test_parse_area_entered_without_difficulty() {
        let line = r#"[18:13:06.139] [@Jerran Zeva#689501114780828|(4618.41,4698.29,706.02,-78.94)|(1/426912)] [] [] [AreaEntered {836045448953664}: Imperial Fleet {137438989504}] (he3000) <v7.0.0b>"#;

        let entry = parse_area_entered_line(line).expect("Should parse");
        assert_eq!(entry.area_name, "Imperial Fleet");
        assert_eq!(entry.area_id, 137438989504);
        assert_eq!(entry.difficulty_name, "");
        assert_eq!(entry.difficulty_id, 0);
        assert_eq!(entry.display_name(), "Imperial Fleet");
    }

    #[test]
    fn test_parse_german_localization() {
        let line = r#"[19:57:55.960] [@Calstone#690129162696566|(4659.45,4702.27,708.82,-41.55)|(1/416194)] [] [] [GebietBetreten {836045448953664}: Imperiale Flotte {137438989504}] (he4001) <v7.0.0b>"#;

        let entry = parse_area_entered_line(line).expect("Should parse");
        assert_eq!(entry.area_name, "Imperiale Flotte");
        assert_eq!(entry.area_id, 137438989504);
    }

    #[test]
    fn test_german_difficulty_uses_id() {
        // German client: "8 Spieler, Meister" — string matching would fail,
        // but the difficulty ID (MASTER_8) is language-independent
        let line = r#"[19:58:12.345] [@Kemrydas#690129162696566|(1.0,2.0,3.0,0.0)|(400000/400000)] [] [] [GebietBetreten {836045448953664}: Tal der Maschinengötter {137438993410} 8 Spieler, Meister {836045448953655}] (he4001) <v7.0.0b>"#;

        let entry = parse_area_entered_line(line).expect("Should parse");
        assert_eq!(entry.difficulty_id, 836045448953655);
        assert_eq!(entry.difficulty_tier(), Some("nim"));
        assert_eq!(entry.display_name(), "Tal der Maschinengötter NiM 8");
    }

    #[test]
    fn test_difficulty_tier() {
        let mut entry = FileAreaEntry {
            area_id: 1,
            area_name: "Dxun".to_string(),
            difficulty_id: crate::game_data::difficulty_id::VETERAN_16,
            difficulty_name: String::new(),
        };
        assert_eq!(entry.difficulty_tier(), Some("hm"));

        entry.difficulty_id = crate::game_data::difficulty_id::STORY_8;
        assert_eq!(entry.difficulty_tier(), Some("sm"));

        entry.difficulty_id = crate::game_data::difficulty_id::MASTER_4;
        assert_eq!(entry.difficulty_tier(), Some("fp"));

        // Unknown ID (open world / no difficulty)
        entry.difficulty_id = 0;
        assert_eq!(entry.difficulty_tier(), None);
    }

    #[test]
    fn test_display_name() {
        // 16 Player Master -> NiM 16
        let nim_16 = FileAreaEntry {
            area_id: 1,
            area_name: "Dxun".to_string(),
            difficulty_id: 2,
            difficulty_name: "16 Player Master".to_string(),
        };
        assert_eq!(nim_16.display_name(), "Dxun NiM 16");

        // 8 Player Veteran -> HM 8
        let hm_8 = FileAreaEntry {
            area_id: 1,
            area_name: "The Ravagers".to_string(),
            difficulty_id: 2,
            difficulty_name: "8 Player Veteran".to_string(),
        };
        assert_eq!(hm_8.display_name(), "The Ravagers HM 8");

        // 16 Player Story -> SM 16
        let sm_16 = FileAreaEntry {
            area_id: 1,
            area_name: "Eternity Vault".to_string(),
            difficulty_id: 2,
            difficulty_name: "16 Player Story".to_string(),
        };
        assert_eq!(sm_16.display_name(), "Eternity Vault SM 16");

        // 4 Player content - keep as-is
        let fp_4 = FileAreaEntry {
            area_id: 1,
            area_name: "Hammer Station".to_string(),
            difficulty_id: 2,
            difficulty_name: "4 Player Veteran".to_string(),
        };
        assert_eq!(fp_4.display_name(), "Hammer Station 4 Player Veteran");

        // No difficulty
        let without_diff = FileAreaEntry {
            area_id: 1,
            area_name: "Imperial Fleet".to_string(),
            difficulty_id: 0,
            difficulty_name: String::new(),
        };
        assert_eq!(without_diff.display_name(), "Imperial Fleet");
    }
}
