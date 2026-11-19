//! World positions and trigger position constraints

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Position Types (world coordinates + trigger constraints)
// ─────────────────────────────────────────────────────────────────────────────

/// World position from a combat log entity segment: `(x,y,z,facing)`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub facing: f32,
}

/// Which entity of the triggering event a position constraint applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionEntity {
    Source,
    Target,
}

/// Coordinate axis to compare. The diagonal projections let a constraint
/// describe a 45° line in the XY plane (e.g. `x - y > -490`), which an
/// axis-aligned box can only approximate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PositionAxis {
    X,
    Y,
    Z,
    Facing,
    XMinusY,
    XPlusY,
}

/// Comparison applied to the axis value.
///
/// Exact float equality is intentionally not offered — use `Between` with a
/// tight range instead.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum PositionOp {
    Gt {
        value: f32,
    },
    Gte {
        value: f32,
    },
    Lt {
        value: f32,
    },
    Lte {
        value: f32,
    },
    /// Inclusive on both ends.
    Between {
        min: f32,
        max: f32,
    },
}

/// A single position constraint on a trigger.
///
/// ```toml
/// position = [
///   { entity = "source", axis = "x", op = "between", min = 100.0, max = 150.0 },
/// ]
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PositionConstraint {
    pub entity: PositionEntity,
    pub axis: PositionAxis,
    #[serde(flatten)]
    pub op: PositionOp,
}

impl PositionConstraint {
    /// Evaluate against the event's source/target positions.
    /// Missing position data fails the constraint.
    pub fn matches(&self, source: Option<Position>, target: Option<Position>) -> bool {
        let pos = match self.entity {
            PositionEntity::Source => source,
            PositionEntity::Target => target,
        };
        let Some(pos) = pos else {
            return false;
        };
        let v = match self.axis {
            PositionAxis::X => pos.x,
            PositionAxis::Y => pos.y,
            PositionAxis::Z => pos.z,
            PositionAxis::Facing => pos.facing,
            PositionAxis::XMinusY => pos.x - pos.y,
            PositionAxis::XPlusY => pos.x + pos.y,
        };
        match self.op {
            PositionOp::Gt { value } => v > value,
            PositionOp::Gte { value } => v >= value,
            PositionOp::Lt { value } => v < value,
            PositionOp::Lte { value } => v <= value,
            PositionOp::Between { min, max } => v >= min && v <= max,
        }
    }
}

/// Check all constraints (AND semantics). An empty list always passes.
pub fn matches_position_constraints(
    constraints: &[PositionConstraint],
    source: Option<Position>,
    target: Option<Position>,
) -> bool {
    constraints.iter().all(|c| c.matches(source, target))
}
