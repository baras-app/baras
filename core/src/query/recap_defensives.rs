//! Death recap: the defensive cooldowns the player pressed before dying.

use std::collections::HashMap;

use super::*;
use crate::game_data::{effect_id, is_defensive_ability};

impl EncounterQuery<'_> {
    /// Defensive cooldowns `player_name` pressed in the fight up to
    /// `death_time`, latest press first.
    pub(super) async fn recap_defensives(
        &self,
        player_name: &str,
        death_time: f32,
    ) -> Result<Vec<DeathRecapDefensive>, String> {
        let sql = format!(
            "SELECT ability_id, ability_name, CAST(COALESCE(combat_time_secs, 0.0) AS REAL)
             FROM events
             WHERE source_name = '{}' AND effect_id = {}
               AND COALESCE(combat_time_secs, 0.0) <= {death_time}
             ORDER BY line_number",
            sql_escape(player_name),
            effect_id::ABILITYACTIVATE,
        );
        let mut used: HashMap<i64, DeathRecapDefensive> = HashMap::new();
        for batch in &self.sql(&sql).await? {
            let (ids, names, times) = (col_i64(batch, 0)?, col_strings(batch, 1)?, col_f32(batch, 2)?);
            for ((id, name), time) in ids.into_iter().zip(names).zip(times) {
                if !is_defensive_ability(id) {
                    continue;
                }
                let entry = used.entry(id).or_insert(DeathRecapDefensive {
                    ability_id: id,
                    ability_name: name,
                    last_used_secs: time,
                    uses: 0,
                });
                entry.last_used_secs = time;
                entry.uses += 1;
            }
        }
        let mut out: Vec<DeathRecapDefensive> = used.into_values().collect();
        out.sort_by(|a, b| b.last_used_secs.total_cmp(&a.last_used_secs));
        Ok(out)
    }
}
