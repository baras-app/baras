//! Enemy instances the group fought, shared by the Raid Debuffs and Aggro tabs.

use std::collections::HashMap;

use super::effects::REMOVE_EFFECT;
use super::*;
use crate::game_data::{effect_id, is_boss};

pub(super) const FROM_GROUP: &str = "source_entity_type IN ('Player', 'Companion') AND target_entity_type = 'Npc'";

/// One enemy instance, from the group's first action on it to its last
pub(super) struct EnemyInstance {
    pub id: i64,
    /// Its name, numbered by first appearance ("Name #2") when several
    /// instances share it
    pub label: String,
    pub is_boss: bool,
    pub first: f32,
    pub last: f32,
}

impl EncounterQuery<'_> {
    /// Every enemy instance the group damaged, by first appearance. An
    /// instance logged under several names or classes stays one instance.
    pub(super) async fn enemy_instances(&self) -> Result<Vec<EnemyInstance>, String> {
        let batches = self
            .sql(&format!(
                "SELECT target_id, target_name, target_class_id,
                        CAST(MIN(combat_time_secs) AS REAL), CAST(MAX(combat_time_secs) AS REAL),
                        CAST(COALESCE(SUM(dmg_amount), 0) AS DOUBLE)
                 FROM events
                 WHERE {FROM_GROUP} AND combat_time_secs >= 0
                   AND effect_id NOT IN ({}, {}) AND effect_type_id != {REMOVE_EFFECT}
                 GROUP BY target_id, target_name, target_class_id",
                effect_id::TARGETSET,
                effect_id::TARGETCLEARED,
            ))
            .await?;
        // id → (instance, damage taken)
        let mut by_id: HashMap<i64, (EnemyInstance, f64)> = HashMap::new();
        for batch in &batches {
            let (ids, names, classes) = (col_i64(batch, 0)?, col_strings(batch, 1)?, col_i64(batch, 2)?);
            let (first, last, dmg) = (col_f32(batch, 3)?, col_f32(batch, 4)?, col_f64(batch, 5)?);
            for (i, name) in names.into_iter().enumerate() {
                let (e, total) = by_id.entry(ids[i]).or_insert_with(|| {
                    let e = EnemyInstance {
                        id: ids[i],
                        label: name.clone(),
                        is_boss: false,
                        first: first[i],
                        last: last[i],
                    };
                    (e, 0.0)
                });
                // The name it first appeared under
                if first[i] < e.first {
                    e.label = name;
                }
                e.is_boss |= is_boss(classes[i]);
                e.first = e.first.min(first[i]);
                e.last = e.last.max(last[i]);
                *total += dmg[i];
            }
        }
        let mut out: Vec<EnemyInstance> = by_id.into_values().filter(|e| e.1 > 0.0).map(|e| e.0).collect();
        out.sort_by(|a, b| a.first.total_cmp(&b.first));

        let mut count: HashMap<String, u32> = HashMap::new();
        for e in &out {
            *count.entry(e.label.clone()).or_default() += 1;
        }
        let mut numbered: HashMap<String, u32> = HashMap::new();
        for e in &mut out {
            if count[&e.label] > 1 {
                let n = numbered.entry(e.label.clone()).or_default();
                *n += 1;
                e.label = format!("{} #{n}", e.label);
            }
        }
        Ok(out)
    }
}
