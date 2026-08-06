//! Map marker definitions.
//!
//! Declarative, log-driven markers drawn on the map overlay: a `[[boss.map_marker]]`
//! group spawns a numbered marker per trigger match (e.g. each NPC that appears),
//! positions it from the entity's coordinates, and removes it on a despawn
//! condition. The per-group calibration turns game coordinates into overlay
//! space. Each marker renders an ordered list of [`MarkerElement`]s (text, icon,
//! svg, image), each with its own offset/anchor — so an "icon + number" marker is
//! just two elements. Reuses the shared [`Trigger`] vocabulary; nothing here is
//! fight-specific — a new fight is another `[[boss.map_marker]]`.

use super::triggers::Trigger;
use serde::{Deserialize, Serialize};

/// Game -> overlay space transform: `out = scale * game + offset`, per axis.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MapCalibration {
    pub x: AxisCalibration,
    pub y: AxisCalibration,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AxisCalibration {
    pub scale: f32,
    pub offset: f32,
}

/// Offset of a drawn element from the marker point, in overlay(map) space.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Offset {
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
}

/// How an element's bounding box aligns to the (offset) marker point.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
    #[default]
    Center,
    TopLeft,
    Top,
    TopRight,
    Left,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

/// A `[[boss.map_marker]]` group.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapMarkerDefinition {
    /// Unique identifier within the encounter.
    pub id: String,

    /// Display name (optional).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,

    /// Phases the group is active in (empty = whole encounter).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phases: Vec<String>,

    /// Spawns one marker per match.
    pub trigger: Trigger,

    /// Game -> overlay space transform for placing this group's markers.
    pub calibration: MapCalibration,

    /// Where the marker sits.
    #[serde(default)]
    pub position: MarkerPosition,

    /// Restart the `sequence` label at 1 when the group empties (per-wave numbering).
    #[serde(default = "crate::serde_defaults::default_true")]
    pub reset_numbering: bool,

    /// The visual elements drawn for each marker, in order (bottom to top).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<MarkerElement>,

    /// Removes a specific marker instance (evaluated per instance, keyed to the
    /// entity that spawned it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remove_on: Option<Trigger>,

    /// Straggler backstop: remove a marker after this many log-seconds elapse
    /// (log-time) since it was last seen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remove_after_secs: Option<f32>,
}

/// One drawable layer of a marker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MarkerElement {
    /// Text — the sequence number by default, or a fixed `text` string.
    Text {
        /// Fixed text; when absent, the marker's sequence number is drawn.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default = "default_text_color")]
        color: [u8; 4],
        #[serde(default = "default_text_size")]
        size: f32,
        #[serde(default)]
        offset: Offset,
        #[serde(default)]
        anchor: Anchor,
    },
    /// An ability icon from the icon cache.
    Icon {
        ability_id: i64,
        #[serde(default = "default_icon_size")]
        size: f32,
        #[serde(default)]
        offset: Offset,
        #[serde(default)]
        anchor: Anchor,
    },
    /// An SVG file resolved next to the map (`map-overlays/…`), rasterized to `size`.
    Svg {
        file: String,
        #[serde(default = "default_icon_size")]
        size: f32,
        #[serde(default)]
        offset: Offset,
        #[serde(default)]
        anchor: Anchor,
    },
    /// A raster image file (PNG) resolved next to the map, drawn at `size`.
    Image {
        file: String,
        #[serde(default = "default_icon_size")]
        size: f32,
        #[serde(default)]
        offset: Offset,
        #[serde(default)]
        anchor: Anchor,
    },
}

fn default_text_color() -> [u8; 4] {
    [255, 255, 255, 255]
}
fn default_text_size() -> f32 {
    18.0
}
fn default_icon_size() -> f32 {
    28.0
}

/// Where a marker is positioned.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerPosition {
    /// From the triggering entity's coordinates.
    #[default]
    Entity,
}

/// A `[[boss.map]]` block: the map(s) for a set of phases, plus an optional
/// timeline of timed overlay images composited on top of the base.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapDefinition {
    /// Phases this map applies to (empty = whole encounter). A phase should
    /// appear in at most one `[[boss.map]]`; on overlap the first wins.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phases: Vec<String>,

    /// Base map SVG, always shown, resolved as `<encounter>/<svg>.svg`. Empty =
    /// fall back to the phase-name file (today's behavior).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub svg: String,

    /// Clock the frame offsets are measured against.
    #[serde(default)]
    pub clock: MapClock,

    /// Timed overlays drawn on top of the base while their window is active.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub frames: Vec<MapFrame>,
}

/// One timed overlay: appears at `at_secs`, stays `duration_secs`, drawn on top
/// of the base map. Resolved as `<encounter>/<svg>.svg`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapFrame {
    pub at_secs: f32,
    pub duration_secs: f32,
    pub svg: String,
}

/// Which log-time clock a map timeline is measured against.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapClock {
    /// Seconds since the current phase started (default).
    #[default]
    Phase,
    /// Seconds since combat started.
    Combat,
}

impl MapDefinition {
    /// Whether this block governs `phase` (empty `phases` = whole encounter).
    pub fn applies_to(&self, phase: Option<&str>) -> bool {
        if self.phases.is_empty() {
            return true;
        }
        matches!(phase, Some(p) if self.phases.iter().any(|x| x == p))
    }

    /// The base svg name, or `None` when unset (router falls back to phase name).
    pub fn base(&self) -> Option<&str> {
        (!self.svg.is_empty()).then_some(self.svg.as_str())
    }

    /// The most-recent timed overlay whose `[at_secs, at_secs+duration)` window
    /// contains `elapsed`, or `None`.
    pub fn active_overlay(&self, elapsed: f32) -> Option<&str> {
        self.frames
            .iter()
            .filter(|f| elapsed >= f.at_secs && elapsed < f.at_secs + f.duration_secs)
            .max_by(|a, b| a.at_secs.total_cmp(&b.at_secs))
            .map(|f| f.svg.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::BossConfig;

    #[test]
    fn parses_marker() {
        let toml = r#"
[[boss]]
id = "revan"

[[boss.map_marker]]
id        = "aberrations"
name      = "Unstable Aberration"
phases    = ["revan_p4", "revan_core"]
trigger   = { type = "npc_appears", selector = [3449584588161024] }
calibration.x = { scale = 2.3509, offset = -922.39 }
calibration.y = { scale = 2.9394, offset = -2982.79 }
position  = "entity"
remove_on = { type = "damage_dealt", abilities = [3449069192085504] }
remove_after_secs = 30.0
elements = [
  { type = "text", color = [46, 204, 113, 255], size = 18.0 },
]
"#;
        let cfg: BossConfig = toml::from_str(toml).expect("config parses");

        let boss = &cfg.bosses[0];
        assert_eq!(boss.map_markers.len(), 1);
        let m = &boss.map_markers[0];
        assert_eq!(m.id, "aberrations");
        assert_eq!(m.phases, ["revan_p4", "revan_core"]);
        assert_eq!(m.calibration.x.scale, 2.3509);
        assert_eq!(m.calibration.y.offset, -2982.79);
        assert!(m.reset_numbering);
        assert_eq!(m.remove_after_secs, Some(30.0));
        assert!(matches!(m.position, MarkerPosition::Entity));
        assert!(m.remove_on.is_some());
        assert_eq!(m.elements.len(), 1);
        match &m.elements[0] {
            MarkerElement::Text { color, size, anchor, .. } => {
                assert_eq!(*color, [46, 204, 113, 255]);
                assert_eq!(*size, 18.0);
                assert_eq!(*anchor, Anchor::Center);
            }
            other => panic!("expected text element, got {other:?}"),
        }
    }

    #[test]
    fn parses_multi_element_marker() {
        // Icon with the sequence number layered on top.
        let toml = r#"
[[boss]]
id = "x"

[[boss.map_marker]]
id = "m"
trigger = { type = "npc_appears", selector = [1] }
calibration.x = { scale = 1.0, offset = 0.0 }
calibration.y = { scale = 1.0, offset = 0.0 }
elements = [
  { type = "icon", ability_id = 12345, size = 28.0 },
  { type = "text", color = [255,255,255,255], offset = { x = 0.0, y = 10.0 }, anchor = "bottom" },
]
"#;
        let cfg: BossConfig = toml::from_str(toml).unwrap();
        let m = &cfg.bosses[0].map_markers[0];
        assert_eq!(m.elements.len(), 2);
        assert!(matches!(m.elements[0], MarkerElement::Icon { ability_id: 12345, .. }));
        match &m.elements[1] {
            MarkerElement::Text { offset, anchor, .. } => {
                assert_eq!(offset.y, 10.0);
                assert_eq!(*anchor, Anchor::Bottom);
            }
            other => panic!("expected text, got {other:?}"),
        }
        // Defaults on the minimal marker.
        assert!(m.reset_numbering);
    }

    #[test]
    fn parses_map_timeline() {
        let toml = r#"
[[boss]]
id = "revan"

[[boss.map]]
phases = ["revan_resonance", "revan_p4", "revan_core"]
svg    = "revan_resonance"
clock  = "phase"
frames = [
  { at_secs = 25.0, duration_secs = 4.0, svg = "revan_detonation" },
]
"#;
        let cfg: BossConfig = toml::from_str(toml).unwrap();
        let m = &cfg.bosses[0].maps[0];
        assert_eq!(m.base(), Some("revan_resonance"));
        assert!(m.applies_to(Some("revan_p4")));
        assert!(!m.applies_to(Some("some_other_phase")));
        assert_eq!(m.clock, MapClock::Phase);
        assert_eq!(m.active_overlay(24.9), None); // before the window
        assert_eq!(m.active_overlay(26.0), Some("revan_detonation"));
        assert_eq!(m.active_overlay(29.0), None); // past the 4s window
    }

    #[test]
    fn map_without_base_falls_back() {
        let toml = r#"
[[boss]]
id = "x"

[[boss.map]]
phases = ["p1"]
frames = [ { at_secs = 0.0, duration_secs = 5.0, svg = "flash" } ]
"#;
        let cfg: BossConfig = toml::from_str(toml).unwrap();
        let m = &cfg.bosses[0].maps[0];
        assert_eq!(m.base(), None); // no svg -> phase-name fallback in the app
        assert_eq!(m.active_overlay(1.0), Some("flash"));
    }

    #[test]
    fn map_applies_to_all_phases_when_empty() {
        let m = MapDefinition {
            phases: Vec::new(),
            svg: "arena".into(),
            clock: MapClock::Phase,
            frames: Vec::new(),
        };
        assert!(m.applies_to(Some("anything")));
        assert!(m.applies_to(None));
    }

    #[test]
    fn calibration_maps_game_to_overlay() {
        // The revan_resonance calibration: a wave-1 spawn at game (497,1076)
        // must land on the map's clock-6 pixel (246,180).
        let cal = MapCalibration {
            x: AxisCalibration { scale: 2.3509, offset: -922.39 },
            y: AxisCalibration { scale: 2.9394, offset: -2982.79 },
        };
        let ox = cal.x.scale * 497.0 + cal.x.offset;
        let oy = cal.y.scale * 1076.0 + cal.y.offset;
        assert!((ox - 246.0).abs() < 0.2, "x mapped to {ox}");
        assert!((oy - 180.0).abs() < 0.2, "y mapped to {oy}");
    }
}
