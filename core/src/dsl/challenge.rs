//! Challenge definition types
//!
//! Challenges are metrics to calculate during boss encounters.
//! They support flexible filtering by phase, source/target entity,
//! ability, effect, counter state, and boss HP ranges.
//!
//! The matching system is designed to work for both live and historical data
//! by using pure functions that take a `ChallengeContext`.

use std::collections::HashMap;

use baras_types::ChallengeColumns;
use serde::{Deserialize, Serialize};

use super::ComparisonOp;
use crate::dsl::EntityDefinition;
use crate::dsl::entity_filter::{EntityFilter, EntityFilterMatching};

// ═══════════════════════════════════════════════════════════════════════════
// Challenge Definition
// ═══════════════════════════════════════════════════════════════════════════

/// A challenge metric to track during a boss encounter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeDefinition {
    /// Unique identifier (auto-generated from name if empty)
    pub id: String,

    /// Display name (used for ID generation, must be unique within encounter)
    pub name: String,

    /// Optional in-game display text (defaults to name if not set)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_text: Option<String>,

    /// Optional description for UI tooltips
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// What metric to accumulate
    pub metric: ChallengeMetric,

    /// All conditions must pass for an event to count (AND logic)
    #[serde(default)]
    pub conditions: Vec<ChallengeCondition>,

    /// Whether this challenge is enabled for display (default: true)
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Bar color for this challenge [r, g, b, a] (optional, uses default if not set)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[u8; 4]>,

    /// Which columns to display for this challenge
    #[serde(default)]
    pub columns: ChallengeColumns,
}

fn default_enabled() -> bool {
    true
}

// ═══════════════════════════════════════════════════════════════════════════
// Metrics
// ═══════════════════════════════════════════════════════════════════════════

/// What value to accumulate for a challenge
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChallengeMetric {
    /// Total damage dealt
    Damage,

    /// Total healing done
    Healing,

    /// Effective healing (healing - overhealing)
    EffectiveHealing,

    /// Total damage received
    DamageTaken,

    /// Total healing received
    HealingTaken,

    /// Count of ability activations
    AbilityCount,

    /// Count of effect applications
    EffectCount,

    /// Death count
    Deaths,

    /// Threat generated
    Threat,
}

// ═══════════════════════════════════════════════════════════════════════════
// Conditions
// ═══════════════════════════════════════════════════════════════════════════

/// A condition that must be met for an event to count toward a challenge
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ChallengeCondition {
    /// Only during specified phase(s)
    Phase {
        /// Phase IDs that match (any of these)
        phase_ids: Vec<String>,
    },

    /// Event source must match
    Source {
        #[serde(rename = "match")]
        matcher: EntityFilter,
    },

    /// Event target must match
    Target {
        #[serde(rename = "match")]
        matcher: EntityFilter,
    },

    /// Specific ability ID(s)
    Ability { ability_ids: Vec<u64> },

    /// Specific effect ID(s)
    Effect { effect_ids: Vec<u64> },

    /// Counter must meet threshold
    Counter {
        counter_id: String,
        operator: ComparisonOp,
        value: u32,
    },

    /// Boss HP must be within range
    BossHpRange {
        /// Minimum HP (inclusive), None = no minimum
        #[serde(default)]
        min_hp: Option<f32>,
        /// Maximum HP (inclusive), None = no maximum
        #[serde(default)]
        max_hp: Option<f32>,
        /// Specific NPC to check (None = any tracked boss)
        #[serde(default)]
        npc_id: Option<i64>,
    },
}

// ═══════════════════════════════════════════════════════════════════════════
// Impl Blocks
// ═══════════════════════════════════════════════════════════════════════════

impl ChallengeDefinition {
    /// Check if this challenge has a phase condition
    pub fn has_phase_condition(&self) -> bool {
        self.conditions
            .iter()
            .any(|c| matches!(c, ChallengeCondition::Phase { .. }))
    }

    /// Get the phase IDs this challenge is restricted to (if any)
    pub fn phase_ids(&self) -> Option<&[String]> {
        self.conditions.iter().find_map(|c| match c {
            ChallengeCondition::Phase { phase_ids } => Some(phase_ids.as_slice()),
            _ => None,
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Matching Context
// ═══════════════════════════════════════════════════════════════════════════

/// Context for evaluating challenge conditions
///
/// This is a lightweight snapshot of state needed to determine if an event
/// counts toward a challenge. Works for both live and historical data.
#[derive(Debug, Clone, Default)]
pub struct ChallengeContext {
    /// Current phase (derived from HP thresholds or events)
    pub current_phase: Option<String>,

    /// Counter values
    pub counters: HashMap<String, u32>,

    /// Boss HP percentages by NPC class ID
    pub hp_by_npc_id: HashMap<i64, f32>,

    /// Boss NPC class IDs for this encounter (for AnyBoss/AnyAdd matching)
    pub boss_npc_ids: Vec<i64>,
}

/// Information about an entity for source/target matching
#[derive(Debug, Clone, Default)]
pub struct EntityInfo {
    /// Entity's runtime ID (for per-player tracking)
    pub entity_id: i64,

    /// Entity name
    pub name: String,

    /// Is this a player (vs NPC)?
    pub is_player: bool,

    /// Is this the local player?
    pub is_local_player: bool,

    /// NPC class ID (None for players)
    pub npc_id: Option<i64>,
}

impl EntityInfo {
    /// Create info for a player
    pub fn player(entity_id: i64, name: impl Into<String>, is_local: bool) -> Self {
        Self {
            entity_id,
            name: name.into(),
            is_player: true,
            is_local_player: is_local,
            npc_id: None,
        }
    }

    /// Create info for an NPC
    pub fn npc(entity_id: i64, name: impl Into<String>, npc_id: i64) -> Self {
        Self {
            entity_id,
            name: name.into(),
            is_player: false,
            is_local_player: false,
            npc_id: Some(npc_id),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Condition Matching
// ═══════════════════════════════════════════════════════════════════════════

impl ChallengeCondition {
    /// Check if this condition is met given the context and event data
    pub fn matches(
        &self,
        ctx: &ChallengeContext,
        entities: &[EntityDefinition],
        source: Option<&EntityInfo>,
        target: Option<&EntityInfo>,
        ability_id: Option<u64>,
        effect_id: Option<u64>,
    ) -> bool {
        match self {
            ChallengeCondition::Phase { phase_ids } => ctx
                .current_phase
                .as_ref()
                .is_some_and(|p| phase_ids.iter().any(|id| id == p)),

            ChallengeCondition::Source { matcher } => source.is_some_and(|s| {
                matcher.matches_challenge(
                    entities,
                    s.is_player,
                    s.is_local_player,
                    &s.name,
                    s.npc_id,
                    &ctx.boss_npc_ids,
                )
            }),

            ChallengeCondition::Target { matcher } => target.is_some_and(|t| {
                matcher.matches_challenge(
                    entities,
                    t.is_player,
                    t.is_local_player,
                    &t.name,
                    t.npc_id,
                    &ctx.boss_npc_ids,
                )
            }),

            ChallengeCondition::Ability { ability_ids } => {
                ability_id.is_some_and(|id| ability_ids.contains(&id))
            }

            ChallengeCondition::Effect { effect_ids } => {
                effect_id.is_some_and(|id| effect_ids.contains(&id))
            }

            ChallengeCondition::Counter {
                counter_id,
                operator,
                value,
            } => {
                let current = ctx.counters.get(counter_id).copied().unwrap_or(0);
                operator.evaluate(current, *value)
            }

            ChallengeCondition::BossHpRange {
                min_hp,
                max_hp,
                npc_id,
            } => {
                let hp = if let Some(id) = npc_id {
                    ctx.hp_by_npc_id.get(id).copied()
                } else {
                    // Use any tracked boss HP (take the first one)
                    ctx.hp_by_npc_id.values().next().copied()
                };

                hp.is_some_and(|h| {
                    min_hp.is_none_or(|min| h >= min) && max_hp.is_none_or(|max| h <= max)
                })
            }
        }
    }
}

impl ChallengeDefinition {
    /// Check if all conditions are met for this challenge
    pub fn matches(
        &self,
        ctx: &ChallengeContext,
        entities: &[EntityDefinition],
        source: Option<&EntityInfo>,
        target: Option<&EntityInfo>,
        ability_id: Option<u64>,
        effect_id: Option<u64>,
    ) -> bool {
        // Empty conditions = always matches
        if self.conditions.is_empty() {
            return true;
        }

        // All conditions must pass (AND logic)
        self.conditions
            .iter()
            .all(|c| c.matches(ctx, entities, source, target, ability_id, effect_id))
    }
}
// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::entity_filter::EntitySelector;

    fn test_context() -> ChallengeContext {
        let mut ctx = ChallengeContext::default();
        ctx.current_phase = Some("burn".to_string());
        ctx.boss_npc_ids = vec![1001, 1002];
        ctx.hp_by_npc_id.insert(1001, 25.0);
        ctx.counters.insert("stacks".to_string(), 5);
        ctx
    }

    /// Convert a parsed Entity to EntityInfo for challenge matching
    fn entity_to_info(entity: &crate::combat_log::Entity, local_player_id: i64) -> EntityInfo {
        use crate::combat_log::EntityType;
        use crate::context::resolve;

        match entity.entity_type {
            EntityType::Player => EntityInfo {
                entity_id: entity.log_id,
                name: resolve(entity.name).to_string(),
                is_player: true,
                is_local_player: entity.log_id == local_player_id,
                npc_id: None,
            },
            EntityType::Npc | EntityType::Companion => EntityInfo {
                entity_id: entity.log_id,
                name: resolve(entity.name).to_string(),
                is_player: false,
                is_local_player: false,
                npc_id: Some(entity.class_id),
            },
            _ => EntityInfo::default(),
        }
    }

    /// Helper to call matches_challenge on EntityFilter using EntityInfo
    fn filter_matches(filter: &EntityFilter, info: &EntityInfo, boss_ids: &[i64]) -> bool {
        filter.matches_challenge(
            &[],
            info.is_player,
            info.is_local_player,
            &info.name,
            info.npc_id,
            boss_ids,
        )
    }

    #[test]
    fn test_entity_filter_boss() {
        let boss_ids = vec![1001, 1002];
        let boss = EntityInfo::npc(1, "Boss", 1001);
        let add = EntityInfo::npc(2, "Add", 9999);
        let player = EntityInfo::player(3, "Player", false);

        assert!(filter_matches(&EntityFilter::Boss, &boss, &boss_ids));
        assert!(!filter_matches(&EntityFilter::Boss, &add, &boss_ids));
        assert!(!filter_matches(&EntityFilter::Boss, &player, &boss_ids));
    }

    #[test]
    fn test_entity_filter_npc_except_boss() {
        let boss_ids = vec![1001, 1002];
        let boss = EntityInfo::npc(1, "Boss", 1001);
        let add = EntityInfo::npc(2, "Add", 9999);
        let player = EntityInfo::player(3, "Player", false);

        assert!(!filter_matches(
            &EntityFilter::NpcExceptBoss,
            &boss,
            &boss_ids
        ));
        assert!(filter_matches(
            &EntityFilter::NpcExceptBoss,
            &add,
            &boss_ids
        ));
        assert!(!filter_matches(
            &EntityFilter::NpcExceptBoss,
            &player,
            &boss_ids
        ));
    }

    #[test]
    fn test_entity_filter_local_player() {
        let boss_ids = vec![];
        let local = EntityInfo::player(1, "Me", true);
        let other = EntityInfo::player(2, "Them", false);
        let npc = EntityInfo::npc(3, "NPC", 123);

        assert!(filter_matches(
            &EntityFilter::LocalPlayer,
            &local,
            &boss_ids
        ));
        assert!(!filter_matches(
            &EntityFilter::LocalPlayer,
            &other,
            &boss_ids
        ));
        assert!(!filter_matches(&EntityFilter::LocalPlayer, &npc, &boss_ids));
    }

    #[test]
    fn test_phase_condition() {
        let ctx = test_context();

        let cond = ChallengeCondition::Phase {
            phase_ids: vec!["burn".to_string()],
        };
        assert!(cond.matches(&ctx, &[], None, None, None, None));

        let cond_miss = ChallengeCondition::Phase {
            phase_ids: vec!["p1".to_string()],
        };
        assert!(!cond_miss.matches(&ctx, &[], None, None, None, None));
    }

    #[test]
    fn test_ability_condition() {
        let ctx = test_context();

        let cond = ChallengeCondition::Ability {
            ability_ids: vec![100, 200],
        };
        assert!(cond.matches(&ctx, &[], None, None, Some(100), None));
        assert!(cond.matches(&ctx, &[], None, None, Some(200), None));
        assert!(!cond.matches(&ctx, &[], None, None, Some(300), None));
        assert!(!cond.matches(&ctx, &[], None, None, None, None));
    }

    #[test]
    fn test_counter_condition() {
        let ctx = test_context();

        let cond_eq = ChallengeCondition::Counter {
            counter_id: "stacks".to_string(),
            operator: ComparisonOp::Eq,
            value: 5,
        };
        assert!(cond_eq.matches(&ctx, &[], None, None, None, None));

        let cond_gt = ChallengeCondition::Counter {
            counter_id: "stacks".to_string(),
            operator: ComparisonOp::Gt,
            value: 3,
        };
        assert!(cond_gt.matches(&ctx, &[], None, None, None, None));

        let cond_lt = ChallengeCondition::Counter {
            counter_id: "stacks".to_string(),
            operator: ComparisonOp::Lt,
            value: 3,
        };
        assert!(!cond_lt.matches(&ctx, &[], None, None, None, None));
    }

    #[test]
    fn test_boss_hp_range_condition() {
        let ctx = test_context();

        // HP is 25.0 for npc_id 1001
        let cond_in_range = ChallengeCondition::BossHpRange {
            min_hp: Some(0.0),
            max_hp: Some(30.0),
            npc_id: Some(1001),
        };
        assert!(cond_in_range.matches(&ctx, &[], None, None, None, None));

        let cond_above = ChallengeCondition::BossHpRange {
            min_hp: Some(30.0),
            max_hp: None,
            npc_id: Some(1001),
        };
        assert!(!cond_above.matches(&ctx, &[], None, None, None, None));
    }

    #[test]
    fn test_challenge_and_logic() {
        let ctx = test_context();
        let boss = EntityInfo::npc(1, "Boss", 1001);

        let challenge = ChallengeDefinition {
            id: "burn_dps".to_string(),
            name: "Burn Phase DPS".to_string(),
            display_text: None,
            description: None,
            metric: ChallengeMetric::Damage,
            conditions: vec![
                ChallengeCondition::Phase {
                    phase_ids: vec!["burn".to_string()],
                },
                ChallengeCondition::Target {
                    matcher: EntityFilter::Boss,
                },
            ],
            enabled: true,
            color: None,
            columns: ChallengeColumns::default(),
        };

        // Both conditions pass
        assert!(challenge.matches(&ctx, &[], None, Some(&boss), None, None));

        // Wrong phase
        let mut wrong_phase_ctx = ctx.clone();
        wrong_phase_ctx.current_phase = Some("p1".to_string());
        assert!(!challenge.matches(&wrong_phase_ctx, &[], None, Some(&boss), None, None));

        // Wrong target (add instead of boss)
        let add = EntityInfo::npc(2, "Add", 9999);
        assert!(!challenge.matches(&ctx, &[], None, Some(&add), None, None));
    }

    #[test]
    fn test_empty_conditions() {
        let ctx = test_context();

        let challenge = ChallengeDefinition {
            id: "all_damage".to_string(),
            name: "All Damage".to_string(),
            display_text: None,
            description: None,
            metric: ChallengeMetric::Damage,
            conditions: vec![],
            enabled: true,
            color: None,
            columns: ChallengeColumns::default(),
        };

        // Empty conditions = always matches
        assert!(challenge.matches(&ctx, &[], None, None, None, None));
    }

    #[test]
    fn test_real_log_challenge_matching() {
        use crate::combat_log::LogParser;
        use crate::game_data::effect_id;
        use std::fs;

        // Load the fixture file (path relative to core/ crate root)
        // Use lossy conversion for non-UTF8 characters in player names
        let fixture_path = "../integration-tests/fixtures/bestia_pull.txt";
        let bytes = match fs::read(fixture_path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!(
                    "Fixture file error: {} (cwd: {:?})",
                    e,
                    std::env::current_dir()
                );
                eprintln!("Skipping test - fixture not found at: {}", fixture_path);
                return;
            }
        };
        let content = String::from_utf8_lossy(&bytes);

        // Parse the session date from first line timestamp
        let session_date = chrono::NaiveDate::from_ymd_opt(2025, 12, 10)
            .unwrap()
            .and_hms_opt(18, 43, 0)
            .unwrap();
        let parser = LogParser::new(session_date);

        // Bestia NPC IDs
        const BESTIA_NPC_ID: i64 = 3273941900591104;
        const DREAD_LARVA_NPC_ID: i64 = 3292079547482112;
        const DREAD_MONSTER_NPC_ID: i64 = 3291675820556288;

        // Create challenge context
        let mut ctx = ChallengeContext::default();
        ctx.current_phase = Some("p1".to_string());
        ctx.boss_npc_ids = vec![BESTIA_NPC_ID];

        // Define challenges
        let boss_damage_challenge = ChallengeDefinition {
            id: "boss_damage".to_string(),
            name: "Boss Damage".to_string(),
            display_text: None,
            description: None,
            metric: ChallengeMetric::Damage,
            conditions: vec![ChallengeCondition::Target {
                matcher: EntityFilter::Boss,
            }],
            enabled: true,
            color: None,
            columns: ChallengeColumns::default(),
        };

        let add_damage_challenge = ChallengeDefinition {
            id: "add_damage".to_string(),
            name: "Add Damage".to_string(),
            display_text: None,
            description: None,
            metric: ChallengeMetric::Damage,
            conditions: vec![ChallengeCondition::Target {
                matcher: EntityFilter::Selector(vec![
                    EntitySelector::Id(DREAD_LARVA_NPC_ID),
                    EntitySelector::Id(DREAD_MONSTER_NPC_ID),
                ]),
            }],
            enabled: true,
            color: None,
            columns: ChallengeColumns::default(),
        };

        // Track accumulated values
        let mut boss_damage_total: i64 = 0;
        let mut boss_damage_events: u32 = 0;
        let mut boss_immune_events: u32 = 0;
        let mut add_damage_total: i64 = 0;
        let mut add_damage_events: u32 = 0;
        let mut total_lines = 0;
        let mut parsed_lines = 0;
        let mut damage_events = 0;

        for (line_num, line) in content.lines().enumerate() {
            total_lines += 1;
            let Some(event) = parser.parse_line(line_num as u64, line) else {
                continue;
            };
            parsed_lines += 1;

            // Only process damage events (effect_id == DAMAGE)
            if event.effect.effect_id != effect_id::DAMAGE {
                continue;
            }
            damage_events += 1;

            // Convert entities to EntityInfo
            // Use a fake local player ID (we don't know who the log owner is)
            let source_info = entity_to_info(&event.source_entity, 0);
            let target_info = entity_to_info(&event.target_entity, 0);

            let damage = event.details.dmg_effective as i64;

            // Check boss damage challenge (include 0-damage immune events for tracking)
            if boss_damage_challenge.matches(
                &ctx,
                &[],
                Some(&source_info),
                Some(&target_info),
                Some(event.action.action_id as u64),
                None,
            ) {
                if damage > 0 {
                    boss_damage_total += damage;
                    boss_damage_events += 1;
                } else {
                    boss_immune_events += 1;
                }
            }

            // Check add damage challenge (skip 0-damage)
            if damage > 0
                && add_damage_challenge.matches(
                    &ctx,
                    &[],
                    Some(&source_info),
                    Some(&target_info),
                    Some(event.action.action_id as u64),
                    None,
                )
            {
                add_damage_total += damage;
                add_damage_events += 1;
            }
        }

        eprintln!("=== Challenge Integration Test Results ===");
        eprintln!(
            "Total lines: {}, Parsed: {}, Damage events: {}",
            total_lines, parsed_lines, damage_events
        );
        eprintln!(
            "Boss damage: {} across {} events ({} immune hits)",
            boss_damage_total, boss_damage_events, boss_immune_events
        );
        eprintln!(
            "Add damage: {} across {} events",
            add_damage_total, add_damage_events
        );

        // Assertions - verify we actually processed real data
        assert!(total_lines > 800, "Should have >800 lines in fixture");
        assert!(parsed_lines > 700, "Should parse most lines successfully");
        assert!(damage_events > 100, "Should have many damage events");

        // Boss should have immune hits (Bestia is immune at pull start)
        assert!(
            boss_immune_events > 0,
            "Should have immune hits on boss (Bestia is immune at start)"
        );

        // Adds (Larva/Monster) should take real damage
        assert!(
            add_damage_events > 0,
            "Should have damage events to adds (Larva/Monster)"
        );
        assert!(add_damage_total > 0, "Add damage total should be positive");

        // Verify EntityFilter works - we matched specific NPC IDs
        assert!(
            add_damage_total > 1_000_000,
            "Should have accumulated >1M damage to adds"
        );

        eprintln!("=== Test passed: Real log data processed correctly ===");
    }

    #[test]
    fn test_phase_transition_and_counters() {
        use crate::combat_log::LogParser;
        use crate::game_data::effect_id;
        use std::fs;

        // Load the phase transition fixture
        let fixture_path = "../integration-tests/fixtures/bestia_phase_transition.txt";
        let bytes = match fs::read(fixture_path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("Fixture file error: {} - skipping test", e);
                return;
            }
        };
        let content = String::from_utf8_lossy(&bytes);

        let session_date = chrono::NaiveDate::from_ymd_opt(2025, 12, 10)
            .unwrap()
            .and_hms_opt(18, 48, 0)
            .unwrap();
        let parser = LogParser::new(session_date);

        // Constants
        const BESTIA_NPC_ID: i64 = 3273941900591104;
        const BURN_PHASE_THRESHOLD: f32 = 30.0;
        const DREAD_SCREAM_ABILITY_ID: u64 = 3302391763959808;

        // Create challenge context - starts in P1
        let mut ctx = ChallengeContext::default();
        ctx.current_phase = Some("p1".to_string());
        ctx.boss_npc_ids = vec![BESTIA_NPC_ID];

        // Define phase-conditional challenge
        let burn_phase_challenge = ChallengeDefinition {
            id: "burn_dps".to_string(),
            name: "Burn Phase DPS".to_string(),
            display_text: None,
            description: None,
            metric: ChallengeMetric::Damage,
            conditions: vec![
                ChallengeCondition::Phase {
                    phase_ids: vec!["burn".to_string()],
                },
                ChallengeCondition::Target {
                    matcher: EntityFilter::Boss,
                },
            ],
            enabled: true,
            color: None,
            columns: ChallengeColumns::default(),
        };

        // Track metrics
        let mut dread_scream_count: u32 = 0;
        let mut phase_transition_line: Option<usize> = None;
        let mut damage_before_burn: i64 = 0;
        let mut damage_during_burn: i64 = 0;
        let mut boss_hp_percent: f32 = 100.0;
        let mut total_lines = 0;

        for (line_num, line) in content.lines().enumerate() {
            total_lines += 1;
            let Some(event) = parser.parse_line(line_num as u64, line) else {
                continue;
            };

            // Track boss HP from any event involving Bestia
            let target_npc_id =
                if event.target_entity.entity_type == crate::combat_log::EntityType::Npc {
                    Some(event.target_entity.class_id)
                } else {
                    None
                };

            if target_npc_id == Some(BESTIA_NPC_ID) && event.target_entity.health.1 > 0 {
                let current_hp = event.target_entity.health.0 as i64;
                let max_hp = event.target_entity.health.1 as i64;
                boss_hp_percent = (current_hp as f32 / max_hp as f32) * 100.0;
                ctx.hp_by_npc_id.insert(BESTIA_NPC_ID, boss_hp_percent);

                // Check for phase transition
                if ctx.current_phase.as_deref() == Some("p1")
                    && boss_hp_percent < BURN_PHASE_THRESHOLD
                {
                    ctx.current_phase = Some("burn".to_string());
                    phase_transition_line = Some(line_num);
                    eprintln!(
                        "Phase transition to BURN at line {} (HP: {:.1}%)",
                        line_num, boss_hp_percent
                    );
                }
            }

            // Count Dread Scream casts (counter)
            if event.effect.effect_id == effect_id::ABILITYACTIVATE
                && event.action.action_id as u64 == DREAD_SCREAM_ABILITY_ID
            {
                dread_scream_count += 1;
            }

            // Track damage (phase-conditional)
            if event.effect.effect_id == effect_id::DAMAGE {
                let damage = event.details.dmg_effective as i64;
                if damage > 0 {
                    let source_info = entity_to_info(&event.source_entity, 0);
                    let target_info = entity_to_info(&event.target_entity, 0);

                    // Only count boss damage
                    if target_info.npc_id == Some(BESTIA_NPC_ID) {
                        if ctx.current_phase.as_deref() == Some("burn") {
                            damage_during_burn += damage;
                        } else {
                            damage_before_burn += damage;
                        }
                    }

                    // Also test the challenge matcher
                    if burn_phase_challenge.matches(
                        &ctx,
                        &[],
                        Some(&source_info),
                        Some(&target_info),
                        Some(event.action.action_id as u64),
                        None,
                    ) {
                        // This should only match during burn phase
                        assert_eq!(
                            ctx.current_phase.as_deref(),
                            Some("burn"),
                            "Challenge should only match during burn phase"
                        );
                    }
                }
            }
        }

        eprintln!("=== Phase & Counter Test Results ===");
        eprintln!("Total lines: {}", total_lines);
        eprintln!("Dread Scream count: {}", dread_scream_count);
        eprintln!("Phase transition at line: {:?}", phase_transition_line);
        eprintln!("Final boss HP: {:.1}%", boss_hp_percent);
        eprintln!(
            "Damage before burn: {}, during burn: {}",
            damage_before_burn, damage_during_burn
        );

        // Assertions
        assert!(total_lines > 700, "Should have >700 lines");
        assert!(
            phase_transition_line.is_some(),
            "Should detect burn phase transition"
        );
        assert!(
            dread_scream_count >= 2,
            "Should count at least 2 Dread Screams"
        );
        assert!(
            damage_before_burn > 0,
            "Should have damage before burn phase"
        );
        assert!(
            damage_during_burn > 0,
            "Should have damage during burn phase"
        );
        assert!(boss_hp_percent < 30.0, "Final HP should be below 30%");

        eprintln!("=== Test passed: Phase transitions and counters work correctly ===");
    }
}
