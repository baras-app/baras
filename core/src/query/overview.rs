//! Raid overview and related aggregate queries.

use std::collections::HashMap;

use super::*;
use crate::game_data::effect_id;

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
        _time_range: Option<&TimeRange>,
    ) -> Result<HashMap<String, f64>, String> {
        // Query with UNNEST, only fetch columns we need for FIFO attribution
        // Only keep position=1 rows (first shield) to avoid double-counting
        let batches = self
            .sql(
                r#"
            SELECT
                CAST(dmg_absorbed AS BIGINT) as dmg_absorbed,
                shield['source_id'] as source_id
            FROM (
                SELECT dmg_absorbed, UNNEST(active_shields) as shield
                FROM events
                WHERE dmg_absorbed > 0 AND cardinality(active_shields) > 0
            )
            WHERE CAST(shield['position'] AS BIGINT) = 1
        "#,
            )
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

    /// Query raid overview - aggregated stats per player across all metrics.
    /// Returns damage dealt, threat, damage taken, absorbed, and healing for each player.
    pub async fn query_raid_overview(
        &self,
        time_range: Option<&TimeRange>,
        duration_secs: Option<f32>,
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

        // CTE-based query to aggregate multiple metrics per player
        // participants: all unique source names (players who did anything)
        // damage_dealt: sum of dmg_amount WHERE source = player
        // threat: sum of threat WHERE source = player
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
            threat AS (
                SELECT source_name as name,
                    SUM(threat) as threat_total
                FROM events
                WHERE threat > 0 {time_filter}
                GROUP BY source_name
            )
            SELECT
                p.name,
                p.entity_type,
                COALESCE(d.damage_total, 0) as damage_total,
                COALESCE(th.threat_total, 0) as threat_total,
                COALESCE(t.damage_taken_total, 0) as damage_taken_total,
                COALESCE(t.absorbed_total, 0) as absorbed_total,
                COALESCE(h.healing_total, 0) as healing_total,
                COALESCE(h.healing_effective, 0) as healing_effective
            FROM participants p
            LEFT JOIN damage_dealt d ON p.name = d.name
            LEFT JOIN damage_taken t ON p.name = t.name
            LEFT JOIN healing_done h ON p.name = h.name
            LEFT JOIN threat as th ON p.name = th.name
            ORDER BY damage_total DESC
        "#
            ))
            .await?;

        let mut results = Vec::new();
        for batch in &batches {
            let names = col_strings(batch, 0)?;
            let entity_types = col_strings(batch, 1)?;
            let damage_totals = col_f64(batch, 2)?;
            let threat_totals = col_f64(batch, 3)?;
            let damage_taken_totals = col_f64(batch, 4)?;
            let absorbed_totals = col_f64(batch, 5)?;
            let healing_totals = col_f64(batch, 6)?;
            let healing_effectives = col_f64(batch, 7)?;

            for i in 0..batch.num_rows() {
                let name = names[i].clone();
                let shield_total = shielding_given.get(&name).copied().unwrap_or(0.0);
                // Include shielding in healing totals (shields are pre-emptive healing)
                let healing_total = healing_totals[i] + shield_total;
                let healing_effective = healing_effectives[i] + shield_total;
                let healing_pct = if healing_total > 0.0 {
                    healing_effective * 100.0 / healing_total
                } else {
                    0.0
                };
                results.push(RaidOverviewRow {
                    name,
                    entity_type: entity_types[i].clone(),
                    class_name: None,
                    discipline_name: None,
                    class_icon: None,
                    role_icon: None,
                    damage_total: damage_totals[i],
                    dps: damage_totals[i] * 1000.0 / duration_ms as f64,
                    threat_total: threat_totals[i],
                    tps: threat_totals[i] * 1000.0 / duration_ms as f64,
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
                });
            }
        }
        Ok(results)
    }

    /// Query player deaths in the encounter.
    /// Returns a list of player deaths ordered by time.
    pub async fn query_player_deaths(&self) -> Result<Vec<PlayerDeath>, String> {
        // Death events are identified by effect_id::DEATH
        // and target_entity_type = 'Player' or 'Companion'
        let sql = format!(
            r#"
            SELECT
                target_name,
                combat_time_secs
            FROM events
            WHERE effect_id = {}
              AND (target_entity_type = 'Player' OR target_entity_type = 'Companion')
              AND combat_time_secs IS NOT NULL
            ORDER BY combat_time_secs ASC
            "#,
            effect_id::DEATH
        );

        let batches = self.sql(&sql).await?;

        let mut results = Vec::new();
        for batch in &batches {
            let names = col_strings(batch, 0)?;
            let times = col_f32(batch, 1)?;

            for (name, time) in names.into_iter().zip(times) {
                results.push(PlayerDeath {
                    name,
                    death_time_secs: time,
                });
            }
        }
        Ok(results)
    }
}
