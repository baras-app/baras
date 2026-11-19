//! Position constraints for event-driven triggers.
//!
//! Constraints gate a trigger on the world coordinates of the event's source
//! or target entity (e.g., only fire when the caster is inside a coordinate
//! range). A trigger's constraint list uses AND semantics; an entity without
//! position data never matches.
//!
//! Types live in `baras-types` (shared with the WASM frontend); this module
//! re-exports them for DSL consumers.

pub use baras_types::{PositionAxis, PositionConstraint, PositionEntity, PositionOp, matches_position_constraints};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat_log::Position;

    fn pos(x: f32, y: f32, z: f32) -> Option<Position> {
        Some(Position { x, y, z, facing: 0.0 })
    }

    #[test]
    fn between_is_inclusive() {
        let c = PositionConstraint {
            entity: PositionEntity::Source,
            axis: PositionAxis::X,
            op: PositionOp::Between { min: 100.0, max: 150.0 },
        };
        assert!(c.matches(pos(100.0, 0.0, 0.0), None));
        assert!(c.matches(pos(150.0, 0.0, 0.0), None));
        assert!(!c.matches(pos(99.99, 0.0, 0.0), None));
        assert!(!c.matches(pos(150.01, 0.0, 0.0), None));
    }

    #[test]
    fn missing_position_fails() {
        let c = PositionConstraint {
            entity: PositionEntity::Target,
            axis: PositionAxis::Y,
            op: PositionOp::Gt { value: 0.0 },
        };
        assert!(!c.matches(pos(1.0, 1.0, 1.0), None));
    }

    #[test]
    fn empty_constraints_pass() {
        assert!(matches_position_constraints(&[], None, None));
    }

    #[test]
    fn serde_round_trip() {
        let c = PositionConstraint {
            entity: PositionEntity::Source,
            axis: PositionAxis::Z,
            op: PositionOp::Between { min: -10.0, max: 10.0 },
        };
        let toml = toml::to_string(&c).unwrap();
        let parsed: PositionConstraint = toml::from_str(&toml).unwrap();
        assert_eq!(c, parsed);

        let gt: PositionConstraint = toml::from_str(
            r#"entity = "target"
axis = "x"
op = "gt"
value = 5.5"#,
        )
        .unwrap();
        assert_eq!(gt.op, PositionOp::Gt { value: 5.5 });
    }

    #[test]
    fn trigger_toml_round_trip_with_position() {
        use crate::dsl::Trigger;
        let toml_src = r#"
type = "ability_cast"
abilities = [12345]
position = [
  { entity = "source", axis = "x", op = "between", min = 100.0, max = 150.0 },
  { entity = "source", axis = "y", op = "gte", value = -220.0 },
]
"#;
        let trigger: Trigger = toml::from_str(toml_src).unwrap();
        assert_eq!(trigger.position_constraints().len(), 2);
        let out = toml::to_string(&trigger).unwrap();
        let reparsed: Trigger = toml::from_str(&out).unwrap();
        assert_eq!(trigger, reparsed);
    }
}
