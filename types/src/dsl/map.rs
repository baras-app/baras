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
/// fight spans floors or teleports; an entity is on the first entry whose
/// bounds, z range and phases contain it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct MapDefinition {
    /// File name under a `maps/` definitions directory
    pub image: String,
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
