//! Embedded class icons with role-based tinting
//!
//! Icons are embedded at compile time and decoded on first access.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Decoded RGBA icon with dimensions
pub struct ClassIcon {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Role for determining icon tint color
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Tank,
    Healer,
    Damage,
}

impl Role {
    /// Get the tint color for this role as (r, g, b)
    pub fn tint_color(&self) -> (u8, u8, u8) {
        match self {
            Role::Tank => (100, 149, 237),  // Cornflower blue
            Role::Healer => (50, 205, 50),  // Lime green
            Role::Damage => (220, 80, 80),  // Soft red
        }
    }
}

// Embed all class icons at compile time
static ICON_DATA: &[(&str, &[u8])] = &[
    ("assassin", include_bytes!("../assets/class/assassin.png")),
    ("bountyhunter", include_bytes!("../assets/class/bountyhunter.png")),
    ("commando", include_bytes!("../assets/class/commando.png")),
    ("guardian", include_bytes!("../assets/class/guardian.png")),
    ("gunslinger", include_bytes!("../assets/class/gunslinger.png")),
    ("jediconsular", include_bytes!("../assets/class/jediconsular.png")),
    ("jediknight", include_bytes!("../assets/class/jediknight.png")),
    ("juggernaut", include_bytes!("../assets/class/juggernaut.png")),
    ("marauder", include_bytes!("../assets/class/marauder.png")),
    ("mercenary", include_bytes!("../assets/class/mercenary.png")),
    ("operative", include_bytes!("../assets/class/operative.png")),
    ("powertech", include_bytes!("../assets/class/powertech.png")),
    ("sage", include_bytes!("../assets/class/sage.png")),
    ("scoundrel", include_bytes!("../assets/class/scoundrel.png")),
    ("sentinel", include_bytes!("../assets/class/sentinel.png")),
    ("shadow", include_bytes!("../assets/class/shadow.png")),
    ("sithsorcerer", include_bytes!("../assets/class/sithsorcerer.png")),
    ("sithwarrior", include_bytes!("../assets/class/sithwarrior.png")),
    ("smuggler", include_bytes!("../assets/class/smuggler.png")),
    ("sniper", include_bytes!("../assets/class/sniper.png")),
    ("sorcerer", include_bytes!("../assets/class/sorcerer.png")),
    ("spy", include_bytes!("../assets/class/spy.png")),
    ("trooper", include_bytes!("../assets/class/trooper.png")),
    ("vanguard", include_bytes!("../assets/class/vanguard.png")),
];

static DECODED_ICONS: OnceLock<HashMap<String, ClassIcon>> = OnceLock::new();

/// Get decoded class icons (lazily initialized)
fn get_icons() -> &'static HashMap<String, ClassIcon> {
    DECODED_ICONS.get_or_init(|| {
        let mut map = HashMap::new();
        for (name, data) in ICON_DATA {
            if let Some(icon) = decode_png(data) {
                map.insert((*name).to_string(), icon);
            }
        }
        map
    })
}

/// Get a class icon by name (e.g., "assassin", "guardian", or "assassin.png")
pub fn get_class_icon(name: &str) -> Option<&'static ClassIcon> {
    // Strip .png extension if present
    let key = name.strip_suffix(".png").unwrap_or(name);
    get_icons().get(key)
}

/// Get a class icon with role-based tinting applied
pub fn get_tinted_class_icon(name: &str, role: Role) -> Option<ClassIcon> {
    let base = get_class_icon(name)?;
    let (tr, tg, tb) = role.tint_color();

    // Apply tint by multiplying each pixel's color with the tint
    let mut tinted = base.rgba.clone();
    for chunk in tinted.chunks_exact_mut(4) {
        // Multiply blend: result = (original * tint) / 255
        chunk[0] = ((chunk[0] as u16 * tr as u16) / 255) as u8;
        chunk[1] = ((chunk[1] as u16 * tg as u16) / 255) as u8;
        chunk[2] = ((chunk[2] as u16 * tb as u16) / 255) as u8;
        // Keep alpha unchanged
    }

    Some(ClassIcon {
        rgba: tinted,
        width: base.width,
        height: base.height,
    })
}

/// Get a class icon as a white silhouette (preserves alpha, all visible pixels white)
pub fn get_white_class_icon(name: &str) -> Option<ClassIcon> {
    let base = get_class_icon(name)?;

    let mut result = base.rgba.clone();
    for chunk in result.chunks_exact_mut(4) {
        // Make all visible pixels white, preserve alpha
        if chunk[3] > 0 {
            chunk[0] = 255;
            chunk[1] = 255;
            chunk[2] = 255;
        }
    }

    Some(ClassIcon {
        rgba: result,
        width: base.width,
        height: base.height,
    })
}

/// Get a class icon with role-based tinting and white outline
pub fn get_outlined_tinted_icon(name: &str, role: Role) -> Option<ClassIcon> {
    let tinted = get_tinted_class_icon(name, role)?;
    let width = tinted.width as usize;
    let height = tinted.height as usize;
    let mut result = tinted.rgba.clone();

    // Find pixels within N pixels of transparent edge and make them white
    let outline_thickness = 2;

    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            let alpha = tinted.rgba[idx + 3];

            // Skip transparent pixels
            if alpha < 128 {
                continue;
            }

            // Check if any pixel within outline_thickness is transparent
            let mut is_near_edge = false;
            'outer: for dy in -(outline_thickness as i32)..=(outline_thickness as i32) {
                for dx in -(outline_thickness as i32)..=(outline_thickness as i32) {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;

                    // Out of bounds counts as edge
                    if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                        is_near_edge = true;
                        break 'outer;
                    }

                    let nidx = (ny as usize * width + nx as usize) * 4;
                    if tinted.rgba[nidx + 3] < 128 {
                        is_near_edge = true;
                        break 'outer;
                    }
                }
            }

            if is_near_edge {
                // Make edge pixel white
                result[idx] = 255;
                result[idx + 1] = 255;
                result[idx + 2] = 255;
            }
        }
    }

    Some(ClassIcon {
        rgba: result,
        width: tinted.width,
        height: tinted.height,
    })
}

/// Decode PNG data to RGBA
fn decode_png(data: &[u8]) -> Option<ClassIcon> {
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

    Some(ClassIcon { rgba, width, height })
}
