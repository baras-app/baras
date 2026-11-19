//! Embedded class icons with role-based tinting
//!
//! Icons are embedded at compile time and decoded on first access.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

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
            Role::Tank => (100, 149, 237), // Cornflower blue
            Role::Healer => (50, 205, 50), // Lime green
            Role::Damage => (220, 80, 80), // Soft red
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

// Embed discipline icons at compile time
static DISCIPLINE_ICON_DATA: &[(&str, &[u8])] = &[
    (
        "advanced-prototype",
        include_bytes!("../assets/discipline/advanced-prototype.png"),
    ),
    ("annihilation", include_bytes!("../assets/discipline/annihilation.png")),
    ("arsenal", include_bytes!("../assets/discipline/arsenal.png")),
    (
        "assault-specialist",
        include_bytes!("../assets/discipline/assault-specialist.png"),
    ),
    ("balance", include_bytes!("../assets/discipline/balance.png")),
    ("bodyguard", include_bytes!("../assets/discipline/bodyguard.png")),
    ("carnage", include_bytes!("../assets/discipline/carnage.png")),
    ("combat", include_bytes!("../assets/discipline/combat.png")),
    ("combat-medic", include_bytes!("../assets/discipline/combat-medic.png")),
    ("concealment", include_bytes!("../assets/discipline/concealment.png")),
    (
        "concentration",
        include_bytes!("../assets/discipline/concentration.png"),
    ),
    ("corruption", include_bytes!("../assets/discipline/corruption.png")),
    ("darkness", include_bytes!("../assets/discipline/darkness.png")),
    ("deception", include_bytes!("../assets/discipline/deception.png")),
    ("defense", include_bytes!("../assets/discipline/defense.png")),
    (
        "dirty-fighting",
        include_bytes!("../assets/discipline/dirty-fighting.png"),
    ),
    ("engineering", include_bytes!("../assets/discipline/engineering.png")),
    ("focus", include_bytes!("../assets/discipline/focus.png")),
    ("fury", include_bytes!("../assets/discipline/fury.png")),
    ("gunnery", include_bytes!("../assets/discipline/gunnery.png")),
    ("hatred", include_bytes!("../assets/discipline/hatred.png")),
    ("immortal", include_bytes!("../assets/discipline/immortal.png")),
    ("infiltration", include_bytes!("../assets/discipline/infiltration.png")),
    (
        "innovative-ordnance",
        include_bytes!("../assets/discipline/innovative-ordnance.png"),
    ),
    (
        "kinetic-combat",
        include_bytes!("../assets/discipline/kinetic-combat.png"),
    ),
    ("lethality", include_bytes!("../assets/discipline/lethality.png")),
    ("lightning", include_bytes!("../assets/discipline/lightning.png")),
    ("madness", include_bytes!("../assets/discipline/madness.png")),
    ("marksmanship", include_bytes!("../assets/discipline/marksmanship.png")),
    ("medicine", include_bytes!("../assets/discipline/medicine.png")),
    ("plasmatech", include_bytes!("../assets/discipline/plasmatech.png")),
    ("pyrotech", include_bytes!("../assets/discipline/pyrotech.png")),
    ("rage", include_bytes!("../assets/discipline/rage.png")),
    ("ruffian", include_bytes!("../assets/discipline/ruffian.png")),
    ("saboteur", include_bytes!("../assets/discipline/saboteur.png")),
    ("sawbones", include_bytes!("../assets/discipline/sawbones.png")),
    ("scrapper", include_bytes!("../assets/discipline/scrapper.png")),
    ("seer", include_bytes!("../assets/discipline/seer.png")),
    ("serenity", include_bytes!("../assets/discipline/serenity.png")),
    ("sharpshooter", include_bytes!("../assets/discipline/sharpshooter.png")),
    (
        "shield-specialist",
        include_bytes!("../assets/discipline/shield-specialist.png"),
    ),
    ("shield-tech", include_bytes!("../assets/discipline/shield-tech.png")),
    ("tactics", include_bytes!("../assets/discipline/tactics.png")),
    ("telekinetics", include_bytes!("../assets/discipline/telekinetics.png")),
    ("vengeance", include_bytes!("../assets/discipline/vengeance.png")),
    ("vigilance", include_bytes!("../assets/discipline/vigilance.png")),
    ("virulence", include_bytes!("../assets/discipline/virulence.png")),
    ("watchman", include_bytes!("../assets/discipline/watchman.png")),
];

static DECODED_DISCIPLINE_ICONS: OnceLock<HashMap<String, ClassIcon>> = OnceLock::new();

// Embed role icons at compile time
static ROLE_ICON_DATA: &[(&str, &[u8])] = &[
    ("icon_tank", include_bytes!("../assets/role/icon_tank.png")),
    ("icon_heal", include_bytes!("../assets/role/icon_heal.png")),
    ("icon_dps", include_bytes!("../assets/role/icon_dps.png")),
];

static DECODED_ROLE_ICONS: OnceLock<HashMap<String, ClassIcon>> = OnceLock::new();

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

/// Map discipline icon names to their parent class icon names.
/// This allows the overlay to resolve discipline-based icon names (e.g., "lightning")
/// to the corresponding class silhouette PNG (e.g., "sorcerer").
fn discipline_to_class(name: &str) -> Option<&'static str> {
    match name {
        // Sorcerer
        "lightning" | "madness" | "corruption" => Some("sorcerer"),
        // Assassin
        "hatred" | "darkness" | "deception" => Some("assassin"),
        // Juggernaut
        "vengeance" | "immortal" | "rage" => Some("juggernaut"),
        // Marauder
        "annihilation" | "carnage" | "fury" => Some("marauder"),
        // Mercenary
        "arsenal" | "innovative-ordnance" | "bodyguard" => Some("mercenary"),
        // Powertech
        "shield-tech" | "pyrotech" | "advanced-prototype" => Some("powertech"),
        // Operative
        "concealment" | "lethality" | "medicine" => Some("operative"),
        // Sniper
        "marksmanship" | "engineering" | "virulence" => Some("sniper"),
        // Sage
        "telekinetics" | "seer" | "balance" => Some("sage"),
        // Shadow
        "infiltration" | "kinetic-combat" | "serenity" => Some("shadow"),
        // Guardian
        "focus" | "vigilance" | "defense" => Some("guardian"),
        // Sentinel
        "combat" | "watchman" | "concentration" => Some("sentinel"),
        // Commando
        "gunnery" | "assault-specialist" | "combat-medic" => Some("commando"),
        // Vanguard
        "plasmatech" | "shield-specialist" | "tactics" => Some("vanguard"),
        // Scoundrel
        "scrapper" | "ruffian" | "sawbones" => Some("scoundrel"),
        // Gunslinger
        "sharpshooter" | "saboteur" | "dirty-fighting" => Some("gunslinger"),
        _ => None,
    }
}

/// Get a class icon by name. Accepts class names (e.g., "assassin", "assassin.png")
/// or discipline names (e.g., "lightning", "lightning.png") which are resolved to
/// their parent class icon.
pub fn get_class_icon(name: &str) -> Option<&'static ClassIcon> {
    // Strip .png extension if present
    let key = name.strip_suffix(".png").unwrap_or(name);
    let icons = get_icons();
    // Try direct class name lookup first, then fall back to discipline-to-class mapping
    icons
        .get(key)
        .or_else(|| discipline_to_class(key).and_then(|class_key| icons.get(class_key)))
}

/// Get decoded role icons (lazily initialized)
fn get_role_icons() -> &'static HashMap<String, ClassIcon> {
    DECODED_ROLE_ICONS.get_or_init(|| {
        let mut map = HashMap::new();
        for (name, data) in ROLE_ICON_DATA {
            if let Some(icon) = decode_png(data) {
                map.insert((*name).to_string(), icon);
            }
        }
        map
    })
}

/// Get a role icon by name (e.g., "icon_tank", "icon_heal", or "icon_tank.png")
pub fn get_role_icon(name: &str) -> Option<&'static ClassIcon> {
    let key = name.strip_suffix(".png").unwrap_or(name);
    get_role_icons().get(key)
}

/// Get decoded discipline icons (lazily initialized)
fn get_discipline_icons() -> &'static HashMap<String, ClassIcon> {
    DECODED_DISCIPLINE_ICONS.get_or_init(|| {
        let mut map = HashMap::new();
        for (name, data) in DISCIPLINE_ICON_DATA {
            if let Some(icon) = decode_png(data) {
                map.insert((*name).to_string(), icon);
            }
        }
        map
    })
}

/// Get a discipline icon by name as raw RGBA (e.g., "lightning", "lightning.png").
/// Returns `None` if the discipline is unknown.
pub fn get_discipline_icon(name: &str) -> Option<&'static ClassIcon> {
    let key = name.strip_suffix(".png").unwrap_or(name);
    get_discipline_icons().get(key)
}

/// Recolored copies of the decoded icons, built once each and kept for the
/// process: keyed by the source icon's address plus the variant.
static DERIVED_ICONS: OnceLock<Mutex<HashMap<(usize, [u8; 4]), &'static ClassIcon>>> = OnceLock::new();

fn derived(base: &'static ClassIcon, variant: [u8; 4], recolor: impl Fn(&mut [u8])) -> &'static ClassIcon {
    let mut icons = DERIVED_ICONS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    icons
        .entry((std::ptr::from_ref(base) as usize, variant))
        .or_insert_with(|| {
            let mut rgba = base.rgba.clone();
            rgba.chunks_exact_mut(4).for_each(&recolor);
            Box::leak(Box::new(ClassIcon {
                rgba,
                width: base.width,
                height: base.height,
            }))
        })
}

/// Visible pixels set to one color, alpha preserved
fn silhouette(base: &'static ClassIcon, (r, g, b): (u8, u8, u8)) -> &'static ClassIcon {
    derived(base, [1, r, g, b], |px| {
        if px[3] > 0 {
            px[..3].copy_from_slice(&[r, g, b]);
        }
    })
}

/// Black copy at reduced alpha, drawn around an icon as its outline
pub fn shadow_of(icon: &'static ClassIcon) -> &'static ClassIcon {
    derived(icon, [0; 4], |px| {
        px[3] = (px[3] as u16 * 180 / 255) as u8;
        px[..3].fill(0);
    })
}

/// Get a class icon as a role-colored silhouette (preserves alpha, all visible pixels
/// set to the role's light tint color). Useful for overlays where icons sit on colored bars.
pub fn get_role_colored_class_icon(name: &str, role: Role) -> Option<&'static ClassIcon> {
    Some(silhouette(get_class_icon(name)?, role.tint_color()))
}

/// Get a class icon as a white silhouette (preserves alpha, all visible pixels white)
pub fn get_white_class_icon(name: &str) -> Option<&'static ClassIcon> {
    Some(silhouette(get_class_icon(name)?, (255, 255, 255)))
}

/// Decode PNG data to RGBA
fn decode_png(data: &[u8]) -> Option<ClassIcon> {
    let decoder = png::Decoder::new(std::io::Cursor::new(data));
    let mut reader = decoder.read_info().ok()?;

    let mut buf = vec![0; reader.output_buffer_size()?];
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
