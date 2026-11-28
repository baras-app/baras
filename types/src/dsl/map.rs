//! Replay maps of a boss encounter (`[[boss.maps]]`)

use serde::{Deserialize, Serialize};

/// A rectangle in world coordinates
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct MapRect {
    pub min_x: f32,
    pub max_x: f32,
    pub min_y: f32,
    pub max_y: f32,
}

/// One map image and the world area it covers. A boss lists several when the
/// fight spans floors or teleports. Entries may overlap: the replay shows the
/// one containing most of the raid, the earliest on a tie.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MapDefinition {
    /// File name under a `maps/` definitions directory
    pub image: String,
    /// Name on the map tabs and cards; the image's file name when absent
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// World area the whole image covers
    #[serde(flatten)]
    pub bounds: MapRect,
    /// Degrees the image is turned relative to world axes
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f32>,
    /// Part of the map to open on, for bosses on very large maps
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<MapRect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_z: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_z: Option<f32>,
    /// Phase ids this entry is limited to; empty = any phase
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub phases: Vec<String>,
}

impl MapRect {
    pub fn has_area(&self) -> bool {
        self.max_x > self.min_x && self.max_y > self.min_y
    }
}

impl MapDefinition {
    /// Why the replay could not draw this entry, if it could not
    pub fn problem(&self) -> Option<&'static str> {
        if self.image.trim().is_empty() {
            Some("has no image")
        } else if !self.bounds.has_area() {
            Some("needs Max X above Min X and Max Y above Min Y")
        } else if self.view.is_some_and(|v| !v.has_area()) {
            Some("has an empty zoomed-in area")
        } else {
            None
        }
    }
}
