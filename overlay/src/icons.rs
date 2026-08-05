use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::Mutex;
use zip::ZipArchive;

/// Decoded RGBA icon data
#[derive(Clone)]
pub struct IconData {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct IconCache {
    /// ability_id → lowercase icon name
    ability_to_icon: HashMap<u64, String>,
    /// lowercase icon_name → IconData
    cache: Mutex<HashMap<String, IconData>>,
    zip_paths: Vec<String>,
    max_cache_size: usize,
}

impl IconCache {
    pub fn new(csv_path: &Path, zip_path: &Path, max_cache_size: usize) -> Result<Self, String> {
        let mut ability_to_icon = load_icon_csv(csv_path)?;
        // Ensure all stored icon names are lowercase
        for icon in ability_to_icon.values_mut() {
            *icon = icon.to_lowercase();
        }
        tracing::debug!(
            count = ability_to_icon.len(),
            "Loaded ability→icon mappings from CSV"
        );

        let mut zip_paths = vec![zip_path.to_string_lossy().to_string()];
        if let Some(parent) = zip_path.parent() {
            let zip2_path = parent.join("icons2.zip");
            if zip2_path.exists() {
                tracing::debug!(path = ?zip2_path, "Found secondary icon ZIP");
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

    pub fn get_icon_name(&self, ability_id: u64) -> Option<&str> {
        self.ability_to_icon.get(&ability_id).map(|s| s.as_str())
    }

    pub fn get_icon(&self, ability_id: u64) -> Option<IconData> {
        let icon_name = self.ability_to_icon.get(&ability_id)?;
        self.get_icon_by_name(icon_name)
    }

    pub fn get_icon_by_name(&self, icon_name: &str) -> Option<IconData> {
        let icon_name_lower = icon_name.to_lowercase();

        // Fast path: check cache
        {
            let cache = self.cache.lock().ok()?;
            if let Some(data) = cache.get(&icon_name_lower) {
                return Some(data.clone());
            }
        }

        let data = self.load_from_zip(&icon_name_lower)?;

        {
            let mut cache = self.cache.lock().ok()?;
            if cache.len() >= self.max_cache_size {
                if let Some(key) = cache.keys().next().cloned() {
                    cache.remove(&key);
                }
            }
            cache.insert(icon_name_lower, data.clone());
        }

        Some(data)
    }

    fn load_from_zip(&self, icon_name_lower: &str) -> Option<IconData> {
        let filename = format!("{}.png", icon_name_lower);

        for zip_path in &self.zip_paths {
            if let Ok(file) = File::open(zip_path) {
                let reader = BufReader::new(file);
                if let Ok(mut archive) = ZipArchive::new(reader) {
                    // Try exact match first (most common case)
                    if let Ok(mut zip_file) = archive.by_name(&filename) {
                        return read_and_decode(&mut zip_file);
                    }

                    // Fall back to case-insensitive search (slow path)
for i in 0..archive.len() {
    let Ok(mut file) = archive.by_index(i) else { continue };

    if file.name().to_lowercase() == filename {
        // We already have the mutable ZipFile — just read it
        let mut png_data = Vec::new();
        file.read_to_end(&mut png_data).ok()?;
        return decode_png(&png_data);
    }
}
                }
            }
        }
        None
    }

    pub fn has_icon(&self, ability_id: u64) -> bool {
        self.ability_to_icon.contains_key(&ability_id)
    }

    pub fn clear_cache(&self) {
        if let Ok(mut cache) = self.cache.lock() {
            cache.clear();
        }
    }
}

fn read_and_decode<R: Read>(zip_file: &mut zip::read::ZipFile<'_, R>) -> Option<IconData> {
    let mut png_data = Vec::new();
    zip_file.read_to_end(&mut png_data).ok()?;
    decode_png(&png_data)
}

fn load_icon_csv(path: &Path) -> Result<HashMap<u64, String>, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read icons.csv: {}", e))?;

    let mut map = HashMap::new();
    for line in content.lines().skip(1) {
        let line = line.trim_start_matches('\u{feff}').trim();
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

/// Decode a PNG into `(width, height, rgba)`. Public so the app layer can resolve
/// `image` marker elements to bytes the overlay can draw.
pub fn decode_png_rgba(data: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    decode_png(data).map(|d| (d.width, d.height, d.rgba))
}

fn decode_png(data: &[u8]) -> Option<IconData> {
    // (unchanged from original)
    let decoder = png::Decoder::new(data);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let width = info.width;
    let height = info.height;

    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => {
            let rgb = &buf[..info.buffer_size()];
            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            for chunk in rgb.chunks(3) {
                rgba.extend_from_slice(chunk);
                rgba.push(255);
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
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
            let g = &buf[..info.buffer_size()];
            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            for &gray in g {
                rgba.extend_from_slice(&[gray, gray, gray, 255]);
            }
            rgba
        }
        png::ColorType::Indexed => return None,
    };

    Some(IconData { rgba, width, height })
}
