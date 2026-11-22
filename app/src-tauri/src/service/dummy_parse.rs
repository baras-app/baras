//! Training dummy parse summary for the Data Explorer.

use baras_types::DummyParse;

use super::ServiceHandle;

impl ServiceHandle {
    /// One player's numbers on a finished dummy parse, its GCD delays and,
    /// for a kill, its Parsely percentiles.
    pub async fn dummy_parse(&self, encounter_idx: u32, source_name: String) -> Result<DummyParse, String> {
        // Copy what the lookup needs so the session lock isn't held over a query or fetch
        let (mut parse, discipline, rating_path) = {
            let session_guard = self.shared.session.read().await;
            let session = session_guard.as_ref().ok_or("No active session")?.read().await;
            let summaries = session
                .session_cache
                .as_ref()
                .ok_or("No session cache")?
                .encounter_history
                .summaries();
            let summary = summaries
                .iter()
                .find(|s| s.encounter_id == encounter_idx as u64)
                .ok_or("Encounter not found")?;
            let player = summary
                .player_metrics
                .iter()
                .find(|p| p.name == source_name)
                .ok_or("Player not in encounter")?;
            let killed = summary.success && !summary.partial;
            (
                DummyParse {
                    dps: player.dps,
                    apm: player.apm,
                    duration_secs: summary.duration_seconds,
                    killed,
                    ..Default::default()
                },
                player.discipline,
                summary.parsely_rating.clone().filter(|_| killed),
            )
        };

        self.register_encounter(Some(encounter_idx)).await?;
        {
            let ctx = self.shared.query_context.query().await;
            let query = ctx.query();
            (parse.crit_pct, parse.crit_damage_pct) = query.query_crit_shares(&source_name).await?;
            parse.gcd = query.query_gcd_analysis(&source_name).await?;
        }

        if let Some((path, discipline)) = rating_path.zip(discipline) {
            parse.percentiles = self.dummy_percentiles(&path, discipline, &parse).await;
        }
        Ok(parse)
    }
}
