//! Icon loading and caching for ability icons
//!
//! Loads icons from a ZIP archive on demand with LRU caching.
//! Icons are decoded to RGBA format for direct rendering.

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::Mutex;

use zip::ZipArchive;

/// Decoded RGBA icon data
#[derive(Clone)]
pub struct IconData {
    /// RGBA pixel data (width * height * 4 bytes)
    pub rgba: Vec<u8>,
    /// Icon width in pixels
    pub width: u32,
    /// Icon height in pixels
    pub height: u32,
}

/// Icon cache with LRU eviction
pub struct IconCache {
    /// Ability ID -> icon name mapping
    ability_to_icon: HashMap<u64, String>,
    /// Cached decoded icons (icon_name -> IconData)
    cache: Mutex<HashMap<String, IconData>>,
    /// Paths to icons ZIP files (checked in order)
    zip_paths: Vec<String>,
    /// Maximum cache size
    max_cache_size: usize,
}

impl IconCache {
    /// Create a new icon cache
    ///
    /// # Arguments
    /// * `csv_path` - Path to icons.csv (ability_id,en,icon)
    /// * `zip_path` - Path to icons.zip (will also check icons2.zip in same dir)
    /// * `max_cache_size` - Maximum number of icons to cache
    pub fn new(csv_path: &Path, zip_path: &Path, max_cache_size: usize) -> Result<Self, String> {
        let ability_to_icon = load_icon_csv(csv_path)?;
        eprintln!("[ICONS] Loaded {} ability->icon mappings from CSV", ability_to_icon.len());

        // Build list of ZIP paths to check
        let mut zip_paths = vec![zip_path.to_string_lossy().to_string()];

        // Also check icons2.zip in the same directory
        if let Some(parent) = zip_path.parent() {
            let zip2_path = parent.join("icons2.zip");
            if zip2_path.exists() {
                eprintln!("[ICONS] Found secondary ZIP: {:?}", zip2_path);
                zip_paths.push(zip2_path.to_string_lossy().to_string());
            }
        }

        Ok(Self {
            ability_to_icon,
            cache: Mutex::new(HashMap::new()),
            zip_paths,
            max_cache_size,
        })
    }

    /// Get icon name for an ability ID
    pub fn get_icon_name(&self, ability_id: u64) -> Option<&str> {
        self.ability_to_icon.get(&ability_id).map(|s| s.as_str())
    }

    /// Get icon data for an ability ID (loads from ZIP if not cached)
    pub fn get_icon(&self, ability_id: u64) -> Option<IconData> {
        let icon_name = match self.ability_to_icon.get(&ability_id) {
            Some(name) => name,
            None => {
                eprintln!("[ICONS] No CSV mapping for ability_id={}", ability_id);
                return None;
            }
        };
        match self.get_icon_by_name(icon_name) {
            Some(data) => Some(data),
            None => {
                eprintln!("[ICONS] Failed to load '{}' for ability_id={}", icon_name, ability_id);
                None
            }
        }
    }

    /// Get icon data by name (loads from ZIP if not cached)
    pub fn get_icon_by_name(&self, icon_name: &str) -> Option<IconData> {
        // Check cache first
        {
            let cache = self.cache.lock().ok()?;
            if let Some(data) = cache.get(icon_name) {
                return Some(data.clone());
            }
        }

        // Load from ZIP
        let data = self.load_from_zip(icon_name)?;

        // Cache it (with simple eviction if full)
        {
            let mut cache = self.cache.lock().ok()?;
            if cache.len() >= self.max_cache_size {
                // Simple eviction: remove first entry
                if let Some(key) = cache.keys().next().cloned() {
                    cache.remove(&key);
                }
            }
            cache.insert(icon_name.to_string(), data.clone());
        }

        Some(data)
    }

    /// Load icon from ZIP files (tries each in order)
    fn load_from_zip(&self, icon_name: &str) -> Option<IconData> {
        let filename = format!("{}.png", icon_name);

        for zip_path in &self.zip_paths {
            if let Ok(file) = File::open(zip_path) {
                let reader = BufReader::new(file);
                if let Ok(mut archive) = ZipArchive::new(reader) {
                    if let Ok(mut zip_file) = archive.by_name(&filename) {
                        let mut png_data = Vec::new();
                        if zip_file.read_to_end(&mut png_data).is_ok() {
                            if let Some(data) = decode_png(&png_data) {
                                return Some(data);
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Check if an icon exists for the given ability ID
    pub fn has_icon(&self, ability_id: u64) -> bool {
        self.ability_to_icon.contains_key(&ability_id)
    }

    /// Clear the cache
    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }
}

/// Load icon CSV mapping
fn load_icon_csv(path: &Path) -> Result<HashMap<u64, String>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read icons.csv: {}", e))?;

    let mut map = HashMap::new();

    for line in content.lines().skip(1) {
        // Skip BOM and header
        let line = line.trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with("ability_id") {
            continue;
        }

        let parts: Vec<&str> = line.splitn(3, ',').collect();
        if parts.len() >= 3 {
            if let Ok(ability_id) = parts[0].parse::<u64>() {
                let icon_name = parts[2].trim().to_lowercase();
                if !icon_name.is_empty() {
                    map.insert(ability_id, icon_name);
                }
            }
        }
    }

    Ok(map)
}

/// Decode PNG data to RGBA
fn decode_png(data: &[u8]) -> Option<IconData> {
    let decoder = png::Decoder::new(data);
    let mut reader = decoder.read_info().ok()?;

    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;

    let width = info.width;
    let height = info.height;

    // Convert to RGBA if needed
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => {
            // Convert RGB to RGBA
            let rgb = &buf[..info.buffer_size()];
            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            for chunk in rgb.chunks(3) {
                rgba.extend_from_slice(chunk);
                rgba.push(255); // Alpha
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
            // Convert GA to RGBA
            let ga = &buf[..info.buffer_size()];
            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            for chunk in ga.chunks(2) {
                let gray = chunk[0];
                let alpha = chunk[1];
                rgba.extend_from_slice(&[gray, gray, gray, alpha]);
            }
            rgba
        }
        png::ColorType::Grayscale => {
            // Convert G to RGBA
            let g = &buf[..info.buffer_size()];
            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            for &gray in g {
                rgba.extend_from_slice(&[gray, gray, gray, 255]);
            }
            rgba
        }
        png::ColorType::Indexed => {
            // For indexed, we'd need the palette - skip for now
            return None;
        }
    };

    Some(IconData { rgba, width, height })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_csv() {
        let path = Path::new("../icons/icons.csv");
        if path.exists() {
            let map = load_icon_csv(path).unwrap();
            assert!(!map.is_empty());
            // Check a known entry
            assert!(map.contains_key(&3244358165856256)); // Power Surge
        }
    }
}
