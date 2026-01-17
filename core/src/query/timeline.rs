//! Encounter timeline and phase detection queries.

use super::*;

impl EncounterQuery<'_> {
    /// Get encounter timeline with phase segments (handles repeated phases).
    pub async fn encounter_timeline(&self) -> Result<EncounterTimeline, String> {
        // Calculate duration from combat_time_secs (only includes actual combat events)
        let duration_secs = scalar_f32(
            &self
                .sql(
                    "SELECT COALESCE(MAX(combat_time_secs), 0) FROM events WHERE combat_time_secs IS NOT NULL",
                )
                .await?,
        );

        // Window functions to detect phase transitions and number instances
        // Filter: phase_id must be non-null AND non-empty string
        let batches = self
            .sql(
                r#"
            WITH filtered AS (
                SELECT combat_time_secs, phase_id, phase_name
                FROM events
                WHERE phase_id IS NOT NULL
                  AND phase_id != ''
                  AND combat_time_secs IS NOT NULL
            ),
            transitions AS (
                SELECT combat_time_secs, phase_id, phase_name,
                       CASE WHEN phase_id != LAG(phase_id) OVER (ORDER BY combat_time_secs)
                                 OR LAG(phase_id) OVER (ORDER BY combat_time_secs) IS NULL
                            THEN 1 ELSE 0 END as is_new
                FROM filtered
            ),
            segments AS (
                SELECT *, SUM(is_new) OVER (ORDER BY combat_time_secs) as seg_id FROM transitions
            ),
            bounds AS (
                SELECT phase_id, phase_name, seg_id,
                       MIN(combat_time_secs) as start_secs, MAX(combat_time_secs) as end_secs
                FROM segments GROUP BY phase_id, phase_name, seg_id
            ),
            valid_bounds AS (
                SELECT * FROM bounds WHERE start_secs < end_secs
            )
            SELECT phase_id, phase_name,
                   ROW_NUMBER() OVER (PARTITION BY phase_id ORDER BY seg_id) as instance,
                   start_secs, end_secs
            FROM valid_bounds
            ORDER BY start_secs
        "#,
            )
            .await?;

        let mut phases = Vec::new();
        for batch in &batches {
            let ids = col_strings(batch, 0)?;
            let names = col_strings(batch, 1)?;
            let instances = col_i64(batch, 2)?;
            let starts = col_f32(batch, 3)?;
            let ends = col_f32(batch, 4)?;

            for i in 0..batch.num_rows() {
                phases.push(PhaseSegment {
                    phase_id: ids[i].clone(),
                    phase_name: names[i].clone(),
                    instance: instances[i],
                    start_secs: starts[i],
                    end_secs: ends[i],
                });
            }
        }

        Ok(EncounterTimeline {
            duration_secs,
            phases,
        })
    }
}
