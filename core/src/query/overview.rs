//! Raid overview and related aggregate queries.

use std::collections::HashMap;

use super::*;
use crate::encounter::{PvpFaction, PvpFactionTracker};
use crate::game_data::{INTERRUPT_ABILITIES, effect_id, effect_type_id};

impl EncounterQuery<'_> {
    /// Query shield attribution - maps shield source IDs to total shielding given.
    ///
    /// Uses the pre-computed `active_shields` column which contains shield context
    /// (source_id, position, estimated_max) captured at parse time.
    ///
    /// Attribution logic:
    /// - Single shield: credit it fully with dmg_absorbed
    /// - Multiple shields + full absorb: credit FIRST applied (FIFO)
    /// - Multiple shields + damage through: credit earlier shields with estimated_max,
    ///   LAST shield with actual dmg_absorbed
    async fn query_shield_attribution(
        &self,
        time_range: Option<&TimeRange>,
    ) -> Result<HashMap<String, f64>, String> {
        let time_filter = time_range
            .map(|tr| format!("AND {}", tr.sql_filter()))
            .unwrap_or_default();
        // Query with UNNEST, only fetch columns we need for FIFO attribution
        // Only keep position=1 rows (first shield) to avoid double-counting
        let batches = self
            .sql(&format!(
                r#"
            SELECT
                CAST(dmg_absorbed AS BIGINT) as dmg_absorbed,
                shield['source_id'] as source_id
            FROM (
                SELECT dmg_absorbed, UNNEST(active_shields) as shield
                FROM events
                WHERE dmg_absorbed > 0 AND cardinality(active_shields) > 0 {time_filter}
            )
            WHERE CAST(shield['position'] AS BIGINT) = 1
        "#
            ))
            .await;

        let batches = match batches {
            Ok(b) => b,
            Err(_) => return Ok(HashMap::new()),
        };

        // Simple FIFO attribution: credit all absorbed damage to the first shield.
        // The log's dmg_absorbed is the TOTAL absorbed by all shields combined.
        let mut shielding_given: HashMap<i64, f64> = HashMap::new();

        for batch in &batches {
            let dmg_absorbeds = col_i64(batch, 0)?;
            let source_ids = col_i64(batch, 1)?;

            for i in 0..batch.num_rows() {
                let dmg_absorbed = dmg_absorbeds[i] as f64;
                let source_id = source_ids[i];
                *shielding_given.entry(source_id).or_default() += dmg_absorbed;
            }
        }

        // Convert source_id to source_name
        let entity_names = self.get_entity_names().await?;
        Ok(shielding_given
            .into_iter()
            .filter_map(|(id, total)| entity_names.get(&id).map(|name| (name.clone(), total)))
            .collect())
    }

    /// Get entity ID to name mapping
    async fn get_entity_names(&self) -> Result<HashMap<i64, String>, String> {
        let batches = self
            .sql("SELECT DISTINCT source_id, source_name FROM events")
            .await?;

        let mut names: HashMap<i64, String> = HashMap::new();
        for batch in &batches {
            let ids = col_i64(batch, 0)?;
            let source_names = col_strings(batch, 1)?;
            for i in 0..batch.num_rows() {
                names.insert(ids[i], source_names[i].clone());
            }
        }
        Ok(names)
    }

    /// Infer friend/enemy factions for a PvP encounter by replaying stored
    /// player-to-player damage/heal events through the faction tracker, in log
    /// order — same semantics as live inference. Runs over the whole encounter
    /// (not the selected time range) so classification converges regardless of
    /// the viewed window. Returns a name → faction map.
    /// The local player is the source of EnterCombat (only logged for them).
    async fn query_local_player_id(&self) -> Result<Option<i64>, String> {
        let batches = self
            .sql(&format!(
                "SELECT source_id FROM events WHERE effect_id = {} ORDER BY line_number LIMIT 1",
                effect_id::ENTERCOMBAT
            ))
            .await?;
        Ok(batches
            .iter()
            .find_map(|b| col_i64(b, 0).ok().and_then(|v| v.first().copied())))
    }

    async fn query_pvp_factions(&self) -> Result<HashMap<String, PvpFaction>, String> {
        let Some(local_id) = self.query_local_player_id().await? else {
            return Ok(HashMap::new());
        };

        let batches = self
            .sql(&format!(
                r#"
            SELECT source_id, source_name, target_id, target_name,
                   CAST(dmg_amount AS BIGINT) as dmg
            FROM events
            WHERE (dmg_amount > 0 OR heal_amount > 0)
              AND source_entity_type = 'Player'
              AND target_entity_type = 'Player'
              AND source_id != target_id
            ORDER BY line_number
            "#
            ))
            .await?;

        let mut tracker = PvpFactionTracker::default();
        let mut names: HashMap<i64, String> = HashMap::new();
        for batch in &batches {
            let src_ids = col_i64(batch, 0)?;
            let src_names = col_strings(batch, 1)?;
            let tgt_ids = col_i64(batch, 2)?;
            let tgt_names = col_strings(batch, 3)?;
            let dmgs = col_i64(batch, 4)?;

            for i in 0..batch.num_rows() {
                names.entry(src_ids[i]).or_insert_with(|| src_names[i].clone());
                names.entry(tgt_ids[i]).or_insert_with(|| tgt_names[i].clone());
                tracker.observe_pair(src_ids[i], tgt_ids[i], dmgs[i] > 0, local_id);
            }
        }

        Ok(names
            .into_iter()
            .filter_map(|(id, name)| tracker.faction_of(id, local_id).map(|f| (name, f)))
            .collect())
    }

    /// Query raid overview - aggregated stats per player across all metrics.
    /// Returns damage dealt, threat, damage taken, absorbed, and healing for each player.
    /// For PvP encounters (`pvp = true`), rows carry a friend/enemy faction and
    /// are sorted friendlies-first, each side by damage descending.
    pub async fn query_raid_overview(
        &self,
        time_range: Option<&TimeRange>,
        duration_secs: Option<f32>,
        pvp: bool,
    ) -> Result<Vec<RaidOverviewRow>, String> {
        let time_filter = time_range
            .map(|tr| format!("AND {}", tr.sql_filter()))
            .unwrap_or_default();
        // Use milliseconds as base to match MetricAccumulator precision
        let duration_ms = (duration_secs.unwrap_or(1.0).max(0.001) * 1000.0).round() as i64;

        // Query shield attribution
        let shielding_given = self
            .query_shield_attribution(time_range)
            .await
            .unwrap_or_default();

        // Threat floored at 0 per target, as in game (matches the group chart)
        let mut threat: HashMap<String, f64> = HashMap::new();
        for (name, _, gained) in self.floored_threat(1.0, &time_filter).await? {
            *threat.entry(name).or_default() += gained;
        }

        // Friend/enemy classification (PvP only)
        let factions = if pvp {
            self.query_pvp_factions().await.unwrap_or_default()
        } else {
            HashMap::new()
        };

        // CTE-based query to aggregate multiple metrics per player
        // participants: all unique source names (players who did anything)
        // damage_dealt: sum of dmg_amount WHERE source = player
        // threat: floored per target in Rust (floored_threat), not summed here
        // damage_taken: sum of dmg_amount WHERE target = player
        // absorbed: sum of dmg_absorbed WHERE target = player
        // healing: sum of heal_amount WHERE source = player
        let batches = self
            .sql(&format!(
                r#"
            WITH participants AS (
                SELECT DISTINCT source_name as name, source_entity_type as entity_type
                FROM events
                WHERE 1=1 {time_filter}
            ),
            damage_dealt AS (
                SELECT source_name as name,
                       SUM(dmg_amount) as damage_total,
                FROM events
                WHERE dmg_amount > 0 AND source_id != target_id {time_filter}
                GROUP BY source_name
            ),
            damage_taken AS (
                SELECT target_name as name,
                       SUM(dmg_amount) as damage_taken_total,
                       SUM(dmg_absorbed) as absorbed_total
                FROM events
                WHERE dmg_amount > 0 {time_filter}
                GROUP BY target_name
            ),
            healing_done AS (
                SELECT source_name as name,
                       SUM(heal_amount) as healing_total,
                       SUM(heal_effective) as healing_effective
                FROM events
                WHERE heal_amount > 0 {time_filter}
                GROUP BY source_name
            ),
            actions AS (
                SELECT source_name as name,
                       COUNT(*) as action_count
                FROM events
                WHERE effect_id = {ability_activate} {time_filter}
                GROUP BY source_name
            ),
            interrupts AS (
                SELECT source_name as name,
                       COUNT(*) as interrupt_count
                FROM events
                WHERE effect_id = {ability_activate}
                  AND ability_id IN ({interrupt_ids}) {time_filter}
                GROUP BY source_name
            )
            SELECT
                p.name,
                p.entity_type,
                COALESCE(d.damage_total, 0) as damage_total,
                COALESCE(t.damage_taken_total, 0) as damage_taken_total,
                COALESCE(t.absorbed_total, 0) as absorbed_total,
                COALESCE(h.healing_total, 0) as healing_total,
                COALESCE(h.healing_effective, 0) as healing_effective,
                COALESCE(a.action_count, 0) as action_count,
                COALESCE(i.interrupt_count, 0) as interrupt_count
            FROM participants p
            LEFT JOIN damage_dealt d ON p.name = d.name
            LEFT JOIN damage_taken t ON p.name = t.name
            LEFT JOIN healing_done h ON p.name = h.name
            LEFT JOIN actions a ON p.name = a.name
            LEFT JOIN interrupts i ON p.name = i.name
            ORDER BY damage_total DESC
        "#,
                ability_activate = effect_id::ABILITYACTIVATE,
                interrupt_ids = INTERRUPT_ABILITIES
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            ))
            .await?;

        let mut results = Vec::new();
        for batch in &batches {
            let names = col_strings(batch, 0)?;
            let entity_types = col_strings(batch, 1)?;
            let damage_totals = col_f64(batch, 2)?;
            let damage_taken_totals = col_f64(batch, 3)?;
            let absorbed_totals = col_f64(batch, 4)?;
            let healing_totals = col_f64(batch, 5)?;
            let healing_effectives = col_f64(batch, 6)?;
            let action_counts = col_f64(batch, 7)?;
            let interrupt_counts = col_f64(batch, 8)?;

            for i in 0..batch.num_rows() {
                let name = names[i].clone();
                let shield_total = shielding_given.get(&name).copied().unwrap_or(0.0);
                let threat_total = threat.get(&name).copied().unwrap_or(0.0);
                // Include shielding in healing totals (shields are pre-emptive healing)
                let healing_total = healing_totals[i] + shield_total;
                let healing_effective = healing_effectives[i] + shield_total;
                let healing_pct = if healing_total > 0.0 {
                    healing_effective * 100.0 / healing_total
                } else {
                    0.0
                };
                results.push(RaidOverviewRow {
                    faction: factions.get(&name).copied(),
                    name,
                    entity_type: entity_types[i].clone(),
                    class_name: None,
                    discipline_name: None,
                    class_icon: None,
                    role_icon: None,
                    damage_total: damage_totals[i],
                    dps: damage_totals[i] * 1000.0 / duration_ms as f64,
                    threat_total,
                    tps: threat_total * 1000.0 / duration_ms as f64,
                    damage_taken_total: damage_taken_totals[i],
                    dtps: damage_taken_totals[i] * 1000.0 / duration_ms as f64,
                    aps: absorbed_totals[i] * 1000.0 / duration_ms as f64,
                    shielding_given_total: shield_total,
                    sps: shield_total * 1000.0 / duration_ms as f64,
                    healing_total,
                    hps: healing_total * 1000.0 / duration_ms as f64,
                    healing_effective,
                    ehps: healing_effective * 1000.0 / duration_ms as f64,
                    healing_pct,
                    apm: action_counts[i] * 60000.0 / duration_ms as f64,
                    interrupts: interrupt_counts[i],
                });
            }
        }

        // PvP: friendlies first, then enemies, then unclassified — each by damage desc
        if pvp {
            results.sort_by(|a, b| {
                let rank = |r: &RaidOverviewRow| match r.faction {
                    Some(PvpFaction::Friendly) => 0u8,
                    Some(PvpFaction::Enemy) => 1,
                    None => 2,
                };
                rank(a).cmp(&rank(b)).then(
                    b.damage_total
                        .partial_cmp(&a.damage_total)
                        .unwrap_or(std::cmp::Ordering::Equal),
                )
            });
        }

        Ok(results)
    }

    /// Query player deaths in the encounter.
    /// Returns a list of player deaths ordered by time.
    /// For PvP encounters (`pvp = true`), deaths carry the victim's faction and
    /// flags for local-player deaths and killing blows.
    pub async fn query_player_deaths(&self, pvp: bool) -> Result<Vec<PlayerDeath>, String> {
        // Death events are identified by effect_id::DEATH
        // and target_entity_type = 'Player' or 'Companion'
        let sql = format!(
            r#"
            SELECT
                target_name,
                combat_time_secs,
                target_id,
                source_id
            FROM events
            WHERE effect_id = {}
              AND (target_entity_type = 'Player' OR target_entity_type = 'Companion')
              AND combat_time_secs IS NOT NULL
            ORDER BY combat_time_secs ASC
            "#,
            effect_id::DEATH
        );

        let batches = self.sql(&sql).await?;

        let (local_id, factions) = if pvp {
            (
                self.query_local_player_id().await.unwrap_or(None),
                self.query_pvp_factions().await.unwrap_or_default(),
            )
        } else {
            (None, HashMap::new())
        };

        let mut results = Vec::new();
        for batch in &batches {
            let names = col_strings(batch, 0)?;
            let times = col_f32(batch, 1)?;
            let target_ids = col_i64(batch, 2)?;
            let source_ids = col_i64(batch, 3)?;

            for i in 0..batch.num_rows() {
                let name = names[i].clone();
                let is_local = local_id.is_some_and(|id| target_ids[i] == id);
                results.push(PlayerDeath {
                    faction: factions.get(&name).copied(),
                    is_local,
                    // Killing blow by the local player (own deaths don't count)
                    local_kill: !is_local && local_id.is_some_and(|id| source_ids[i] == id),
                    name,
                    death_time_secs: times[i],
                });
            }
        }
        Ok(results)
    }

    /// Query final health state of all NPC instances in the encounter.
    /// Partitions by target_id (unique instance), sorted by max_hp DESC, limited to 36.
    /// Only rows with a combat time are considered (matches the combat log view).
    /// TargetSet/TargetCleared/RemoveEffect rows are ignored: they can't change HP,
    /// and a boss reset emits them at full HP right as the encounter ends.
    ///
    /// Uses FIRST appearance for max_hp (avoids SWTOR combat log bug where AoE events
    /// can report incorrect max_hp from other entities).
    pub async fn query_npc_health(
        &self,
        time_range: Option<&TimeRange>,
    ) -> Result<Vec<NpcHealthRow>, String> {
        let time_filter = time_range
            .map(|tr| format!("AND {}", tr.sql_filter()))
            .unwrap_or_default();

        // Death/damage events can trail slightly after combat exit; give a 2s buffer
        let death_time_filter = time_range
            .map(|tr| {
                format!(
                    "AND {}",
                    TimeRange::new(tr.start, tr.end + 2.0).sql_filter()
                )
            })
            .unwrap_or_default();

        let batches = self
            .sql(&format!(
                r#"
            WITH first_hp AS (
                SELECT target_id, target_name, target_max_hp,
                       ROW_NUMBER() OVER (PARTITION BY target_id ORDER BY line_number ASC) as rn
                FROM events
                WHERE target_entity_type = 'Npc' AND target_max_hp > 0
                  AND combat_time_secs IS NOT NULL
                  AND effect_id NOT IN ({targetset}, {targetcleared}) AND effect_type_id != {removeeffect} {time_filter}
            ),
            last_hp AS (
                SELECT target_id, target_hp,
                       ROW_NUMBER() OVER (PARTITION BY target_id ORDER BY line_number DESC) as rn
                FROM events
                WHERE target_entity_type = 'Npc' AND target_max_hp > 0
                  AND combat_time_secs IS NOT NULL
                  AND effect_id NOT IN ({targetset}, {targetcleared}) AND effect_type_id != {removeeffect} {death_time_filter}
            ),
            first_seen AS (
                SELECT target_id, MIN(combat_time_secs) as first_seen_secs
                FROM events
                WHERE target_entity_type = 'Npc' AND combat_time_secs IS NOT NULL
                  AND effect_id NOT IN ({targetset}, {targetcleared}) AND effect_type_id != {removeeffect} {time_filter}
                GROUP BY target_id
            ),
            deaths AS (
                SELECT target_id, MIN(combat_time_secs) as death_time_secs
                FROM events
                WHERE target_entity_type = 'Npc' AND effect_id = {death_id}
                  AND combat_time_secs IS NOT NULL {death_time_filter}
                GROUP BY target_id
            )
            SELECT fh.target_name,
                   CAST(COALESCE(fs.first_seen_secs, 0) AS FLOAT) as first_seen_secs,
                   CAST(d.death_time_secs AS FLOAT) as death_time_secs,
                   CAST(lh.target_hp AS BIGINT) as final_hp,
                   CAST(fh.target_max_hp AS BIGINT) as max_hp
            FROM first_hp fh
            JOIN last_hp lh ON fh.target_id = lh.target_id AND lh.rn = 1
            LEFT JOIN first_seen fs ON fh.target_id = fs.target_id
            LEFT JOIN deaths d ON fh.target_id = d.target_id
            WHERE fh.rn = 1
            ORDER BY fh.target_max_hp DESC, fh.target_name ASC
            "#,
                targetset = effect_id::TARGETSET,
                targetcleared = effect_id::TARGETCLEARED,
                removeeffect = effect_type_id::REMOVEEFFECT,
                death_id = effect_id::DEATH,
            ))
            .await?;

        let mut results = Vec::new();
        for batch in &batches {
            let names = col_strings(batch, 0)?;
            let first_seens = col_f32(batch, 1)?;
            let death_times = col_opt_f32(batch, 2)?;
            let hps = col_i64(batch, 3)?;
            let max_hps = col_i64(batch, 4)?;

            for i in 0..batch.num_rows() {
                let max_hp = max_hps[i];
                // If the NPC has a death record, final HP is 0 regardless of last event
                let final_hp = if death_times[i].is_some() { 0 } else { hps[i].max(0) };
                let pct = if max_hp > 0 {
                    (final_hp as f32 / max_hp as f32) * 100.0
                } else {
                    0.0
                };
                results.push(NpcHealthRow {
                    name: names[i].clone(),
                    first_seen_secs: first_seens[i],
                    death_time_secs: death_times[i],
                    max_hp,
                    final_hp,
                    final_hp_pct: pct,
                });
            }
        }
        Ok(results)
    }
}
