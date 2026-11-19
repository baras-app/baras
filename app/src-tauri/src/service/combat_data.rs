//! Unified combat data for metric overlays

use crate::state::SharedState;
use baras_core::context::{ChallengeColumns, resolve};
use baras_core::encounter::summary::classify_encounter;
use baras_core::encounter::{EncounterState, PhaseType};
use baras_core::game_data::{Difficulty, Discipline};
use baras_core::{EntityType, IncomingDamageRow, PlayerMetrics};
use baras_overlay::{ChallengeData, ChallengeEntry, Color, PersonalStats, PlayerContribution};
use std::sync::Arc;

/// Calculate unified combat data for all overlays
/// An enemy absent from the log this long is dimmed with a counter.
const ENEMY_STALE_SECS: i64 = 20;

pub(super) async fn calculate_combat_data(shared: &Arc<SharedState>) -> Option<CombatData> {
    let session_guard = shared.session.read().await;
    let session = session_guard.as_ref()?;
    let session = session.read().await;
    let cache = session.session_cache.as_ref()?;

    // Get player info for class/discipline and entity ID
    let player_info = &cache.player;
    // Spec shows the discipline alone; class is the fallback before it's known
    let class_discipline = [&player_info.discipline_name, &player_info.class_name]
        .into_iter()
        .find(|s| !s.is_empty())
        .cloned();
    let player_entity_id = player_info.id;

    // Try live encounter first, fall back to historical summary for initial hydration
    if let Some(encounter) = cache.last_combat_encounter() {
        // Live encounter path - full data including challenges and phase info
        let encounter_count = cache
            .encounters()
            .filter(|e| e.state != EncounterState::NotStarted)
            .map(|e| e.id + 1)
            .max()
            .unwrap_or(0) as usize;
        let encounter_time_secs = encounter
            .duration_seconds(session.interpolated_game_time())
            .unwrap_or(0) as u64;

        // Classify the encounter to get phase type and boss info
        // Use encounter's stored area/difficulty info (falls back to cache if not set)
        let area_id_for_classification = encounter.area_id.unwrap_or(cache.current_area.area_id);
        let difficulty_id_for_classification = encounter.difficulty_id.unwrap_or(cache.current_area.difficulty_id);
        let (encounter_type, boss_info) =
            classify_encounter(encounter, area_id_for_classification, difficulty_id_for_classification);

        // Generate encounter name with pull count
        // Priority: definition name > hardcoded boss name > phase type
        //
        // Important: PostCombat state alone doesn't mean the encounter is finalized.
        // During the grace period, the encounter is PostCombat but hasn't been added
        // to history yet (that happens when push_new_encounter() is called after
        // the grace period expires). We detect this by checking last_combat_exit_time:
        // - Some(_) = grace period active, encounter not yet in history
        // - None = grace period expired, encounter has been finalized to history
        let is_finalized =
            matches!(encounter.state, EncounterState::PostCombat { .. }) && cache.last_combat_exit_time.is_none();

        let encounter_name = if is_finalized {
            // Encounter finalized and in history - use the display_name from history
            cache
                .encounter_history
                .summaries()
                .last()
                .map(|s| s.display_name.clone())
        } else if let Some(def) = encounter.active_boss_definition() {
            // Definition is active - use definition name with pull count
            let pull_count = cache.encounter_history.peek_pull_count(&def.name);
            Some(format!("{} - {}", def.name, pull_count))
        } else if let Some(boss) = boss_info {
            // Hardcoded boss detected (no definition) - use boss name with pull count
            let pull_count = cache.encounter_history.peek_pull_count(boss.boss);
            Some(format!("{} - {}", boss.boss, pull_count))
        } else {
            // Trash encounter - use phase type with trash count
            let trash_count = cache.encounter_history.peek_trash_count();
            match encounter_type {
                PhaseType::Raid => Some(format!("Raid Trash {trash_count}")),
                PhaseType::Flashpoint => Some(format!("Flashpoint Trash {trash_count}")),
                PhaseType::DummyParse => Some(format!("Dummy Parse {trash_count}")),
                PhaseType::PvP => Some(format!(
                    "{} - {}",
                    baras_core::game_data::pvp_match_label(area_id_for_classification),
                    trash_count
                )),
                PhaseType::OpenWorld => Some(format!("Open World {trash_count}")),
            }
        };

        // Short "Master 8" label from the difficulty id; the localized area
        // name is the fallback for ids we don't know (blank in open world)
        let difficulty = short_difficulty(difficulty_id_for_classification, &cache.current_area.difficulty_name);

        // Calculate metrics for all players (use session-level discipline registry)
        let entity_metrics =
            encounter.calculate_entity_metrics(&cache.player_disciplines, session.interpolated_game_time())?;
        let is_pvp = encounter.is_pvp();
        let metrics: Vec<PlayerMetrics> = entity_metrics
            .into_iter()
            .filter(|m| m.entity_type != EntityType::Npc)
            .map(|m| {
                let mut pm = m.to_player_metrics();
                // Faction relative to the local player (drives the friendly/enemy
                // split in metric overlays)
                if is_pvp {
                    pm.pvp_faction = encounter.pvp_factions.faction_of(pm.entity_id, player_entity_id);
                }
                pm
            })
            .collect();

        // Enemy player frames for the PvP enemy HP overlay: stable first-seen
        // slot order; discipline from the session registry (source of truth).
        // Only while still inside the PvP area — after zoning out the match is
        // over, and a re-shown overlay must not resurrect the ended match's
        // frames from the lingering encounter.
        let in_pvp_area = baras_core::game_data::is_pvp_area(cache.current_area.area_id);
        let enemy_frames: Vec<baras_overlay::EnemyFrame> = if is_pvp && in_pvp_area {
            let mut enemies: Vec<_> = encounter
                .players
                .values()
                .filter(|p| {
                    encounter.pvp_factions.faction_of(p.id, player_entity_id)
                        == Some(baras_core::encounter::PvpFaction::Enemy)
                })
                .collect();
            // The grid holds one team; anyone beyond that was replaced mid-match
            // and the enemies we have not seen for longest are the ones who left.
            let team_size = match encounter.pvp_kind() {
                Some(baras_core::game_data::PvpAreaKind::Arena) => 4,
                _ => 8,
            };
            if enemies.len() > team_size {
                enemies.sort_by_key(|p| std::cmp::Reverse((p.last_seen_at, p.id)));
                enemies.truncate(team_size);
            }
            enemies.sort_by_key(|p| (p.first_seen_at, p.id));
            let now = session.interpolated_game_time().or(session.last_event_time);
            let stale_secs = |seen: Option<chrono::NaiveDateTime>| {
                let gone = (now? - seen?).num_seconds();
                (gone >= ENEMY_STALE_SECS).then_some(gone as u32)
            };
            enemies
                .into_iter()
                .map(|p| {
                    let disc = cache
                        .player_disciplines
                        .get(&p.id)
                        .and_then(|info| Discipline::from_guid(info.discipline_id));
                    let target_id = if p.current_target_id != 0 {
                        p.current_target_id
                    } else {
                        p.last_offensive_target_id
                    };
                    let target_name = (target_id != 0)
                        .then(|| encounter.get_entity_name(target_id))
                        .flatten()
                        .map(|n| resolve(n).to_string());
                    baras_overlay::EnemyFrame {
                        entity_id: p.id,
                        name: resolve(p.name).to_string(),
                        class_name: disc.map(|d| format!("{:?}", d.class())),
                        discipline_icon: disc.map(|d| d.icon_name().to_string()),
                        current_hp: p.current_hp as i64,
                        max_hp: p.max_hp as i64,
                        target_name,
                        targeting_you: target_id == player_entity_id,
                        // Last observed HP only: no revive tracking in PvP, a
                        // respawn simply shows up alive in the next event.
                        is_dead: p.current_hp == 0 && p.max_hp > 0,
                        guarded: false,
                        stale_secs: stale_secs(p.last_seen_at),
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        // Per-source incoming damage for the local player (game-clock anchored
        // so the sliding window stays in sync with the display timer)
        let incoming_damage: Vec<IncomingDamageRow> = session
            .interpolated_game_time()
            .or(session.last_event_time)
            .map(|now| {
                encounter
                    .incoming_damage
                    .snapshot(now)
                    .into_iter()
                    .filter_map(|(id, rate, total)| {
                        let name = encounter.get_entity_name(id)?;
                        Some(IncomingDamageRow {
                            entity_id: id,
                            name: resolve(name).to_string(),
                            rate,
                            total,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Build challenge data from encounter's tracker (persists with encounter, not boss state)
        let challenges = if encounter.challenge_tracker.is_active() {
            let boss_name = encounter
                .active_boss_idx()
                .and_then(|idx| encounter.boss_definitions().get(idx).map(|def| def.name.clone()));
            let overall_duration = encounter.combat_time_secs.max(1.0);
            // Use exit time when in PostCombat (grace window) so duration doesn't keep ticking.
            // Fall back to last event time (game time) instead of system clock to avoid
            // clock skew between SWTOR's timestamps and the system clock.
            let current_time = encounter
                .exit_combat_time
                .or(session.last_event_time)
                .unwrap_or_else(|| chrono::Local::now().naive_local());

            // Lookup map for class/discipline icons from already-computed metrics —
            // mirrors what create_entries_for_type does for metric overlays so the
            // challenge bars can render the same icons + bold local player.
            use baras_core::game_data::Role as GameRole;
            use baras_overlay::Role as OverlayRole;
            let player_lookup: std::collections::HashMap<i64, (Option<String>, Option<String>, Option<OverlayRole>)> =
                metrics
                    .iter()
                    .map(|m| {
                        let class_icon = m.class_icon.clone();
                        let discipline_icon = m.discipline.map(|d| d.icon_name().to_string());
                        let role = m.discipline.map(|d| match d.role() {
                            GameRole::Tank => OverlayRole::Tank,
                            GameRole::Healer => OverlayRole::Healer,
                            GameRole::Dps => OverlayRole::Damage,
                        });
                        (m.entity_id, (class_icon, discipline_icon, role))
                    })
                    .collect();

            let entries: Vec<ChallengeEntry> = encounter
                .challenge_tracker
                .snapshot_live(current_time)
                .into_iter()
                .map(|val| {
                    // Use the challenge's own duration (phase-scoped or total)
                    let challenge_duration = val.duration_secs.max(1.0);

                    // Build per-player breakdown, sorted by value descending
                    let mut by_player: Vec<PlayerContribution> = val
                        .by_player
                        .iter()
                        .map(|(&entity_id, &value)| {
                            // Resolve player name from encounter
                            let name = encounter
                                .players
                                .get(&entity_id)
                                .map(|p| resolve(p.name).to_string())
                                .unwrap_or_else(|| format!("Player {}", entity_id));

                            let percent = if val.value > 0 {
                                (value as f32 / val.value as f32) * 100.0
                            } else {
                                0.0
                            };

                            let (class_icon, discipline_icon, role) =
                                player_lookup.get(&entity_id).cloned().unwrap_or((None, None, None));

                            PlayerContribution {
                                entity_id,
                                name,
                                value,
                                percent,
                                per_second: if value > 0 {
                                    Some(value as f32 / challenge_duration)
                                } else {
                                    None
                                },
                                is_local: entity_id == player_entity_id,
                                class_icon,
                                discipline_icon,
                                role,
                            }
                        })
                        .collect();

                    // Sort by value descending (top contributors first)
                    by_player.sort_by_key(|x| std::cmp::Reverse(x.value));

                    ChallengeEntry {
                        name: val.name,
                        value: val.value,
                        event_count: val.event_count,
                        per_second: if val.value > 0 {
                            Some(val.value as f32 / challenge_duration)
                        } else {
                            None
                        },
                        by_player,
                        duration_secs: challenge_duration,
                        // Display settings from challenge definition
                        enabled: val.enabled,
                        color: val.color.map(|c| Color::from_rgba8(c[0], c[1], c[2], c[3])),
                        columns: val.columns,
                    }
                })
                .collect();

            Some(ChallengeData {
                entries,
                boss_name,
                duration_secs: overall_duration,
                phase_durations: encounter.challenge_tracker.phase_durations().clone(),
            })
        } else {
            None
        };

        // Get phase info from encounter's boss state
        // Look up the phase display name from the boss definition
        let current_phase = encounter.current_phase.as_ref().and_then(|phase_id| {
            encounter
                .active_boss_definition()
                .and_then(|def| def.phases.iter().find(|p| &p.id == phase_id).map(|p| p.name.clone()))
        });
        let phase_time_secs = encounter
            .phase_started_at
            .zip(session.interpolated_game_time().or(session.last_event_time))
            .map(|(start, now)| (now - start).num_milliseconds() as f32 / 1000.0)
            .unwrap_or(0.0);

        Some(CombatData {
            metrics,
            player_entity_id,
            encounter_time_secs,
            encounter_count,
            class_discipline,
            encounter_name,
            difficulty,
            challenges,
            current_phase,
            phase_time_secs,
            incoming_damage,
            is_warzone: encounter.is_warzone(),
            enemy_frames,
        })
    } else if let Some(summary) = cache.encounter_history.summaries().last() {
        // Fallback to historical summary for initial hydration when no live encounter exists
        let encounter_count = cache.encounter_history.summaries().len();
        let encounter_time_secs = summary.duration_seconds.max(0) as u64;
        let encounter_name = Some(summary.display_name.clone());
        let difficulty = short_difficulty(
            cache.current_area.difficulty_id,
            summary.difficulty.as_deref().unwrap_or_default(),
        );
        let metrics = summary.player_metrics.clone();

        // Lookup map for class/discipline icons from the persisted player
        // metrics — same as the live path — so historical loads render icons
        // and bold the local player too.
        use baras_core::game_data::Role as GameRole;
        use baras_overlay::Role as OverlayRole;
        let player_lookup: std::collections::HashMap<i64, (Option<String>, Option<String>, Option<OverlayRole>)> =
            metrics
                .iter()
                .map(|m| {
                    let class_icon = m.class_icon.clone();
                    let discipline_icon = m.discipline.map(|d| d.icon_name().to_string());
                    let role = m.discipline.map(|d| match d.role() {
                        GameRole::Tank => OverlayRole::Tank,
                        GameRole::Healer => OverlayRole::Healer,
                        GameRole::Dps => OverlayRole::Damage,
                    });
                    (m.entity_id, (class_icon, discipline_icon, role))
                })
                .collect();

        // Build ChallengeData from historical summary if challenges exist
        let challenges = if !summary.challenges.is_empty() {
            let entries: Vec<ChallengeEntry> = summary
                .challenges
                .iter()
                .map(|cs| {
                    let by_player: Vec<PlayerContribution> = cs
                        .by_player
                        .iter()
                        .map(|p| {
                            let (class_icon, discipline_icon, role) =
                                player_lookup.get(&p.entity_id).cloned().unwrap_or((None, None, None));
                            PlayerContribution {
                                entity_id: p.entity_id,
                                name: p.name.clone(),
                                value: p.value,
                                percent: p.percent,
                                per_second: p.per_second,
                                is_local: p.entity_id == player_entity_id,
                                class_icon,
                                discipline_icon,
                                role,
                            }
                        })
                        .collect();
                    ChallengeEntry {
                        name: cs.name.clone(),
                        value: cs.total_value,
                        event_count: cs.event_count,
                        per_second: cs.per_second,
                        by_player,
                        duration_secs: cs.duration_secs,
                        enabled: true,
                        color: cs.color.map(|c| Color::from_rgba8(c[0], c[1], c[2], c[3])),
                        columns: match cs.columns.as_str() {
                            "TotalPercent" => ChallengeColumns::TotalPercent,
                            "TotalPerSecond" => ChallengeColumns::TotalPerSecond,
                            "PerSecondPercent" => ChallengeColumns::PerSecondPercent,
                            "TotalOnly" => ChallengeColumns::TotalOnly,
                            "PerSecondOnly" => ChallengeColumns::PerSecondOnly,
                            "PercentOnly" => ChallengeColumns::PercentOnly,
                            _ => ChallengeColumns::default(),
                        },
                    }
                })
                .collect();
            Some(ChallengeData {
                entries,
                boss_name: summary.boss_name.clone(),
                duration_secs: summary.duration_seconds.max(1) as f32,
                phase_durations: Default::default(),
            })
        } else {
            None
        };

        Some(CombatData {
            metrics,
            player_entity_id,
            encounter_time_secs,
            encounter_count,
            class_discipline,
            encounter_name,
            difficulty,
            challenges,
            current_phase: None,
            phase_time_secs: 0.0,
            incoming_damage: summary.incoming_damage.clone(),
            is_warzone: summary.is_warzone,
            // Historical hydration: the match is over, enemy frames are stale
            enemy_frames: Vec::new(),
        })
    } else {
        None
    }
}

/// Difficulty label for overlays: "Master 8" when the id is known, else the
/// raw game string, `None` when that's empty (open world).
fn short_difficulty(difficulty_id: i64, fallback: &str) -> Option<String> {
    Difficulty::from_difficulty_id(difficulty_id)
        .map(|d| d.short_label().to_string())
        .or_else(|| (!fallback.is_empty()).then(|| fallback.to_string()))
}

/// Unified combat data for metric overlays
#[derive(Debug, Clone)]
pub struct CombatData {
    /// Metrics for all players
    pub metrics: Vec<PlayerMetrics>,
    /// Entity ID of the primary player (for personal overlay)
    pub player_entity_id: i64,
    /// Duration of current encounter in seconds
    pub encounter_time_secs: u64,
    /// Number of encounters in the session
    pub encounter_count: usize,
    /// Player's discipline (e.g., "Corruption"), or class if discipline is unknown
    pub class_discipline: Option<String>,
    /// Current encounter display name (e.g., "Raid Trash 3" or "Dread Master Bestia Pull 1")
    pub encounter_name: Option<String>,
    /// Current area difficulty (e.g., "NiM 8") or phase type for non-instanced content
    pub difficulty: Option<String>,
    /// Challenge metrics for boss encounters (polled with other metrics)
    pub challenges: Option<ChallengeData>,
    /// Current boss phase (if in a defined encounter)
    pub current_phase: Option<String>,
    /// Time spent in the current phase (seconds)
    pub phase_time_secs: f32,
    /// Damage taken by the local player per source (Incoming Damage overlay)
    pub incoming_damage: Vec<IncomingDamageRow>,
    /// Warzone match in progress — metric overlays suppress the totals column
    pub is_warzone: bool,
    /// Enemy player frames (PvP enemy HP overlay); empty outside PvP
    pub enemy_frames: Vec<baras_overlay::EnemyFrame>,
}

impl CombatData {
    /// Convert to PersonalStats by finding the player's entry in metrics
    pub fn to_personal_stats(&self) -> Option<PersonalStats> {
        let player = self.metrics.iter().find(|m| m.entity_id == self.player_entity_id)?;
        Some(PersonalStats {
            encounter_name: self.encounter_name.clone(),
            difficulty: self.difficulty.clone(),
            encounter_time_secs: self.encounter_time_secs,
            encounter_count: self.encounter_count,
            class_discipline: self.class_discipline.clone(),
            apm: player.apm,
            dps: player.dps as i32,
            edps: player.edps as i32,
            bossdps: player.bossdps as i32,
            total_damage: player.total_damage,
            total_damage_boss: player.total_damage_boss,
            hps: player.hps as i32,
            ehps: player.ehps as i32,
            total_healing: player.total_healing,
            total_healing_effective: player.total_healing_effective,
            dtps: player.dtps as i32,
            edtps: player.edtps as i32,
            total_damage_taken: player.total_damage_taken,
            total_damage_taken_effective: player.total_damage_taken_effective,
            ehtps: player.ehtps as i32,
            total_healing_received_effective: player.total_healing_received_effective,
            tps: player.tps as i32,
            total_threat: player.total_threat,
            damage_crit_pct: player.damage_crit_pct,
            heal_crit_pct: player.heal_crit_pct,
            effective_heal_pct: player.effective_heal_pct,
            defense_pct: player.defense_pct,
            shield_pct: player.shield_pct,
            total_shield_absorbed: player.total_shield_absorbed,
            current_phase: self.current_phase.clone(),
            phase_time_secs: self.phase_time_secs,
        })
    }
}
