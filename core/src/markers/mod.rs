//! Map marker lifecycle engine.
//!
//! Drives declarative [`MapMarkerDefinition`]s from the combat-event stream in
//! **log time** (event timestamps, never wall-clock), so it behaves identically
//! live and on replay. Each group spawns a numbered marker per trigger match (an
//! NPC appearing), positions it from the entity, and removes it on a despawn
//! condition or a straggler timeout.
//!
//! Pure state machine — no I/O, no overlay, no service coupling — so it can be
//! unit-tested directly against real logs (see the tests below, which replay a
//! Revan "Unstable Aberration" pull and assert the timeline matches parsely).

use std::collections::HashSet;

use chrono::NaiveDateTime;

use crate::combat_log::{CombatEvent, Entity, EntityType};
use crate::context::resolve;
use crate::dsl::{EntityDefinition, MapMarkerDefinition, MarkerPosition, Trigger};

/// A marker currently shown for a group, in **game space** as returned by
/// [`MarkerTracker::active`]. [`crate::encounter`]'s
/// `CombatEncounter::active_markers` applies the group calibration and attaches
/// the group's visual elements, producing [`MarkerRender`] in overlay space.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveMarker {
    /// The marker group's id (e.g. "aberrations").
    pub group_id: String,
    /// Label number (1..N in appearance order).
    pub number: u32,
    /// Game-world x at spawn.
    pub x: f32,
    /// Game-world y at spawn.
    pub y: f32,
}

/// A marker ready for rendering: an overlay(map)-space point, its label number,
/// and the group's visual elements. Asset refs (icons / svg / image files) are
/// resolved downstream, app-side (core does no I/O).
#[derive(Debug, Clone)]
pub struct MarkerRender {
    pub x: f32,
    pub y: f32,
    /// Sequence number for `text` elements that have no fixed string.
    pub number: u32,
    pub elements: Vec<crate::dsl::MarkerElement>,
}

/// One tracked marker instance.
#[derive(Debug, Clone)]
struct Instance {
    /// Unique combat-instance id of the entity that spawned it.
    log_id: i64,
    number: u32,
    x: f32,
    y: f32,
    /// Log-time of the most recent event referencing this instance.
    last_seen: NaiveDateTime,
}

/// Per-group runtime state.
#[derive(Debug, Clone)]
struct GroupState {
    def: MapMarkerDefinition,
    instances: Vec<Instance>,
    /// Instance ids already handed a number this encounter. Prevents an entity
    /// that has already appeared (and possibly been removed) from being re-added
    /// with a fresh number — e.g. a multi-target detonation logs one line per
    /// target, and the NPC persists (it does not die), so later references to it
    /// must not resurrect the marker.
    consumed: HashSet<i64>,
    /// Next label to hand out; reset to 1 when a new wave starts.
    next_number: u32,
}

/// Tracks all marker groups for the active encounter.
#[derive(Debug, Default, Clone)]
pub struct MarkerTracker {
    groups: Vec<GroupState>,
}

impl MarkerTracker {
    /// Build a tracker for the given marker definitions.
    pub fn new(defs: &[MapMarkerDefinition]) -> Self {
        Self {
            groups: defs
                .iter()
                .cloned()
                .map(|def| GroupState {
                    def,
                    instances: Vec::new(),
                    consumed: HashSet::new(),
                    next_number: 1,
                })
                .collect(),
        }
    }

    /// Whether this tracker has any marker groups.
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// Clear all groups (combat start / new pull).
    pub fn reset(&mut self) {
        for g in &mut self.groups {
            g.instances.clear();
            g.consumed.clear();
            g.next_number = 1;
        }
    }

    /// Advance every group by one event. `phase` is the current phase id.
    pub fn on_event(&mut self, ev: &CombatEvent, entities: &[EntityDefinition], phase: Option<&str>) {
        for g in &mut self.groups {
            g.step(ev, entities, phase);
        }
    }

    /// Snapshot of all currently-shown markers, across active groups.
    pub fn active(&self) -> Vec<ActiveMarker> {
        let mut out = Vec::new();
        for g in &self.groups {
            for inst in &g.instances {
                out.push(ActiveMarker {
                    group_id: g.def.id.clone(),
                    number: inst.number,
                    x: inst.x,
                    y: inst.y,
                });
            }
        }
        out
    }
}

impl GroupState {
    fn phase_active(&self, phase: Option<&str>) -> bool {
        self.def.phases.is_empty()
            || phase.is_some_and(|p| self.def.phases.iter().any(|ph| ph == p))
    }

    fn step(&mut self, ev: &CombatEvent, entities: &[EntityDefinition], phase: Option<&str>) {
        // 1. Phase gate: outside the group's phases, nothing is shown.
        if !self.phase_active(phase) {
            self.instances.clear();
            self.next_number = 1;
            return;
        }

        // 2. Straggler timeout (log-time): drop instances not seen for too long.
        if let Some(max_secs) = self.def.remove_after_secs {
            let ts = ev.timestamp;
            self.instances
                .retain(|inst| secs_since(inst.last_seen, ts) <= max_secs);
        }

        // 3. Appear (before remove, so a same-event appear+detonate nets to gone).
        self.try_appear(&ev.source_entity, ev.timestamp, entities);
        self.try_appear(&ev.target_entity, ev.timestamp, entities);

        // 4. Refresh last_seen for any tracked instance this event references.
        for ent in [&ev.source_entity, &ev.target_entity] {
            if ent.log_id != 0
                && let Some(inst) = self.instances.iter_mut().find(|i| i.log_id == ent.log_id)
            {
                inst.last_seen = ev.timestamp;
            }
        }

        // 5. Remove: the instance that fired the despawn condition (e.g. detonation).
        if let Some(remove_on) = &self.def.remove_on
            && remove_matches(remove_on, ev)
        {
            let src = ev.source_entity.log_id;
            self.instances.retain(|i| i.log_id != src);
        }
    }

    fn try_appear(&mut self, ent: &Entity, ts: NaiveDateTime, entities: &[EntityDefinition]) {
        if ent.entity_type != EntityType::Npc || ent.class_id == 0 || ent.log_id == 0 {
            return;
        }
        // Appear at most once per instance for the whole encounter — never
        // resurrect one that already got a number (see `consumed`).
        if self.consumed.contains(&ent.log_id) {
            return;
        }
        // TODO(markers, review #5): only `npc_appears` can spawn a marker. Other
        // trigger kinds (time_elapsed, ability_cast, phase_entered, …) parse via
        // the schema but never fire here. Add a state/time-driven appear path
        // (needed for time-based markers) and validate unsupported triggers at load.
        let name = resolve(ent.name);
        if !self
            .def
            .trigger
            .matches_npc_appears(entities, ent.class_id, name)
        {
            return;
        }
        let (x, y) = match self.def.position {
            MarkerPosition::Entity => match ent.position {
                Some(p) => (p.x, p.y),
                None => return, // no coordinates to place at
            },
        };
        // New wave: restart numbering when the group is empty.
        if self.instances.is_empty() && self.def.reset_numbering {
            self.next_number = 1;
        }
        let number = self.next_number;
        self.next_number += 1;
        self.consumed.insert(ent.log_id);
        self.instances.push(Instance {
            log_id: ent.log_id,
            number,
            x,
            y,
            last_seen: ts,
        });
    }
}

/// Whole seconds (fractional) elapsed between two log timestamps.
fn secs_since(from: NaiveDateTime, to: NaiveDateTime) -> f32 {
    (to - from).num_milliseconds() as f32 / 1000.0
}

/// Whether an event satisfies a marker's `remove_on` condition. For
/// `damage_dealt` this requires *real, typed* damage (`dmg_type_id != 0`) so the
/// no-damage knockback (`(272792 )`, same ability id as the detonation) does not
/// count — only the actual detonation removes the marker.
///
/// TODO(markers, review #3): only the `abilities` filter of `damage_dealt` is
/// honored — its `source`/`target`/`mitigation` filters are silently ignored,
/// diverging from how `DamageDealt` is evaluated elsewhere. Reuse the shared
/// trigger evaluation when generalizing.
/// TODO(markers, review #4): the `dmg_type_id != 0` knockback guard is hardcoded
/// for every fight's `damage_dealt` removal, with no opt-out. Move it to config
/// (e.g. a `require_typed_damage` / mitigation option) when a fight needs it.
fn remove_matches(remove_on: &Trigger, ev: &CombatEvent) -> bool {
    match remove_on {
        Trigger::DamageDealt { abilities, .. } => {
            ev.details.dmg_type_id != 0
                && (abilities.is_empty()
                    || abilities
                        .iter()
                        .any(|s| s.matches(ev.action.action_id as u64, Some(resolve(ev.action.name)))))
        }
        Trigger::AnyOf { conditions } => conditions.iter().any(|c| remove_matches(c, ev)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat_log::LogParser;
    use crate::dsl::BossConfig;
    use chrono::NaiveDateTime;

    // The `aberrations` marker exactly as configured in temple_of_sacrifice.toml.
    const MARKER_TOML: &str = r#"
[[boss]]
id = "revan"

[[boss.map_marker]]
id        = "aberrations"
phases    = ["revan_p4", "revan_core"]
trigger   = { type = "npc_appears", selector = [3449584588161024] }
calibration.x = { scale = 1.0, offset = 0.0 }
calibration.y = { scale = 1.0, offset = 0.0 }
position  = "entity"
remove_on = { type = "damage_dealt", abilities = [3449069192085504] }
remove_after_secs = 30.0
elements  = [ { type = "text" } ]
"#;

    // 15 real log lines from the longest Revan pull (waves 1 & 2), in order.
    const FIXTURE: &str = include_str!("fixtures/revan_aberrations.log");

    fn defs() -> Vec<MapMarkerDefinition> {
        toml::from_str::<BossConfig>(MARKER_TOML)
            .unwrap()
            .bosses
            .remove(0)
            .map_markers
    }

    /// Active markers as `(number, x, y)`, sorted by number, for stable asserts.
    fn snapshot(t: &MarkerTracker) -> Vec<(u32, f32, f32)> {
        let mut v: Vec<_> = t.active().into_iter().map(|m| (m.number, m.x, m.y)).collect();
        v.sort_by_key(|m| m.0);
        v
    }

    #[test]
    fn aberration_wave_lifecycle_matches_parsely() {
        let defs = defs();
        let mut tracker = MarkerTracker::new(&defs);
        let parser = LogParser::new(
            NaiveDateTime::parse_from_str("2026-07-24 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
        );

        // Replay the fixture, capturing the active set after each line.
        let mut snaps: Vec<Vec<(u32, f32, f32)>> = Vec::new();
        for (n, line) in FIXTURE.lines().filter(|l| !l.is_empty()).enumerate() {
            let ev = parser
                .parse_line(n as u64, line)
                .unwrap_or_else(|| panic!("line {n} failed to parse: {line}"));
            tracker.on_event(&ev, &[], Some("revan_core"));
            snaps.push(snapshot(&tracker));
        }

        // Line indices (0-based) of each fixture event.
        // 0-3 wave-1 appears; 4 =420278 detonate; 5 =421436 knockback;
        // 6 =420665 detonate; 7 =421436 detonate; 8 =439946 appear; ...
        // After the 4 wave-1 spawns: numbered 1..4 in appearance order.
        assert_eq!(
            snaps[3],
            vec![(1, 497.0, 1076.0), (2, 485.0, 1094.0), (3, 469.0, 1043.0), (4, 440.0, 1076.0)],
            "wave 1 should number 1..4 by appearance"
        );

        // 420278 (#3) detonates at 22:38:26 -> gone.
        assert_eq!(snaps[4], vec![(1, 497.0, 1076.0), (2, 485.0, 1094.0), (4, 440.0, 1076.0)]);

        // 421436 knockback at 22:38:27 must NOT despawn it (still #1 present).
        assert_eq!(
            snaps[5],
            vec![(1, 497.0, 1076.0), (2, 485.0, 1094.0), (4, 440.0, 1076.0)],
            "the (272792) knockback must not remove the marker"
        );

        // 420665 (#4) detonates at 22:38:29.
        assert_eq!(snaps[6], vec![(1, 497.0, 1076.0), (2, 485.0, 1094.0)]);

        // 421436 (#1) detonates at 22:38:35 -> only #2 (…421124) remains, still
        // within its 30s straggler window.
        assert_eq!(snaps[7], vec![(2, 485.0, 1094.0)]);

        // 439946 appears at 22:38:49: …421124 has now exceeded 30s and times out,
        // emptying the group, so numbering restarts at 1 for wave 2.
        assert_eq!(snaps[8], vec![(1, 469.0, 1043.0)], "wave 2 restarts numbering at 1");

        // 439718 appears as #2; the two flash adds (…438859, …439490) appear and
        // detonate on their own single event, so they never persist — #1 and #2 hold.
        assert_eq!(snaps[9], vec![(1, 469.0, 1043.0), (2, 485.0, 1094.0)], "439718 is #2");
        assert_eq!(snaps[10], vec![(1, 469.0, 1043.0), (2, 485.0, 1094.0)], "flash 438859 doesn't disrupt");
        assert_eq!(snaps[11], vec![(1, 469.0, 1043.0), (2, 485.0, 1094.0)], "flash 439490 doesn't disrupt");

        // End state: every add has detonated -> nothing left.
        assert!(snaps.last().unwrap().is_empty(), "all markers cleared by end");
    }

    #[test]
    fn reset_clears_everything() {
        let defs = defs();
        let mut tracker = MarkerTracker::new(&defs);
        let parser = LogParser::new(
            NaiveDateTime::parse_from_str("2026-07-24 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
        );
        let first = FIXTURE.lines().find(|l| !l.is_empty()).unwrap();
        let ev = parser.parse_line(0, first).unwrap();
        tracker.on_event(&ev, &[], Some("revan_core"));
        assert!(!tracker.active().is_empty());
        tracker.reset();
        assert!(tracker.active().is_empty());
    }

    fn test_parser() -> LogParser {
        LogParser::new(
            NaiveDateTime::parse_from_str("2026-07-24 00:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
        )
    }

    fn parse(parser: &LogParser, n: u64, line: &str) -> crate::combat_log::CombatEvent {
        parser
            .parse_line(n, line)
            .unwrap_or_else(|| panic!("parse failed: {line}"))
    }

    // A synthetic aberration `TargetSet` (spawn) line at `inst`/`pos`.
    fn appear_line(ts: &str, inst: i64, x: f32, y: f32) -> String {
        format!(
            "[{ts}] [Unstable Aberration {{3449584588161024}}:{inst}|({x:.2},{y:.2},320.00,0.00)|(100/100)] \
             [@P#1|(0.00,0.00,0.00,0.00)|(100/100)] [] [Event {{836045448945472}}: TargetSet {{836045448953668}}]"
        )
    }

    // A synthetic aberration detonation (real kinetic damage) line.
    fn detonate_line(ts: &str, inst: i64) -> String {
        format!(
            "[{ts}] [Unstable Aberration {{3449584588161024}}:{inst}|(497.00,1076.00,320.00,0.00)|(100/100)] \
             [@P#1|(0.00,0.00,0.00,0.00)|(100/100)] [ {{3449069192085504}}] \
             [ApplyEffect {{836045448945477}}: Damage {{836045448945501}}] (17309 kinetic {{836045448940873}}) <17309.0>"
        )
    }

    /// A detonation logs one line per target hit; the NPC does not die, so
    /// without the consumed-id guard those extra lines re-appear it with fresh
    /// numbers and inflate the sequence. Here A(#1)+B(#2) spawn, A detonates to
    /// three targets, then C must still be #3.
    #[test]
    fn multi_target_detonation_does_not_inflate_numbers() {
        let defs = defs();
        let mut t = MarkerTracker::new(&defs);
        let p = test_parser();
        let lines = [
            appear_line("22:38:10.000", 1001, 497.0, 1076.0), // A -> #1
            appear_line("22:38:11.000", 1002, 485.0, 1094.0), // B -> #2
            detonate_line("22:38:20.000", 1001),              // A detonation, hit 1
            detonate_line("22:38:20.000", 1001),              // hit 2 (must not re-add)
            detonate_line("22:38:20.000", 1001),              // hit 3 (must not re-add)
            appear_line("22:38:21.000", 1003, 469.0, 1043.0), // C -> #3, not inflated
        ];
        for (i, l) in lines.iter().enumerate() {
            t.on_event(&parse(&p, i as u64, l), &[], Some("revan_core"));
        }
        assert_eq!(snapshot(&t), vec![(2, 485.0, 1094.0), (3, 469.0, 1043.0)]);
    }

    /// Markers are only shown while the current phase is in the group's `phases`;
    /// the p4<->core transition keeps them (both listed); leaving clears them.
    #[test]
    fn phase_gate_controls_visibility() {
        let defs = defs();
        let p = test_parser();
        let appear = appear_line("22:38:10.000", 2001, 497.0, 1076.0);
        // A player-only event (no aberration) used to drive the tracker forward.
        let benign = "[22:38:12.000] [@P#1|(0.00,0.00,0.00,0.00)|(100/100)] [@P#1|(0.00,0.00,0.00,0.00)|(100/100)] [Foo {123}] [ApplyEffect {836045448945477}: Damage {836045448945501}] (100 kinetic {836045448940873}) <100.0>";

        // Appearing outside the marker's phases shows nothing.
        let mut t = MarkerTracker::new(&defs);
        t.on_event(&parse(&p, 0, &appear), &[], Some("some_other_phase"));
        assert!(t.active().is_empty(), "gated out in a non-listed phase");

        // Appear in revan_p4, survive the p4->core transition, clear on leaving.
        let mut t = MarkerTracker::new(&defs);
        t.on_event(&parse(&p, 0, &appear), &[], Some("revan_p4"));
        assert_eq!(t.active().len(), 1, "shown in revan_p4");
        t.on_event(&parse(&p, 1, benign), &[], Some("revan_core"));
        assert_eq!(t.active().len(), 1, "kept across p4->core");
        t.on_event(&parse(&p, 2, benign), &[], Some("revan_burn"));
        assert!(t.active().is_empty(), "cleared on leaving the phase set");
    }
}
