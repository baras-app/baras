//! Time series queries (DPS, HPS, DTPS over time).

use super::*;

/// Configuration for time series queries.
struct TimeSeriesConfig<'a> {
    /// Column to sum ("dmg_amount" or "heal_amount")
    value_column: &'static str,
    /// Which side of the event the entity is on ("source" or "target")
    entity: &'static str,
    /// Optional entity selector: a name, or `id:<instance>` for one NPC spawn
    entity_filter: Option<&'a str>,
    /// Exclude self-damage (source_id == target_id) from aggregation.
    /// Only set for outgoing damage, matching the breakdown/overview pattern.
    exclude_self_damage: bool,
}

impl EncounterQuery<'_> {
    /// Generic time series query - buckets values over time with optional entity filter.
    async fn query_time_series(
        &self,
        bucket_ms: i64,
        config: TimeSeriesConfig<'_>,
        time_range: Option<&TimeRange>,
    ) -> Result<Vec<TimeSeriesPoint>, String> {
        let bucket_secs = (bucket_ms as f64 / 1000.0).max(1.0);
        let value_col = config.value_column;

        // Base conditions for time range (used for bounds calculation)
        let mut tr_conditions = vec!["combat_time_secs IS NOT NULL".to_string()];
        if let Some(tr) = time_range {
            tr_conditions.push(tr.sql_filter());
        }
        let tr_filter = format!("WHERE {}", tr_conditions.join(" AND "));

        // Entity-specific conditions (used for value aggregation)
        let mut entity_conditions = tr_conditions.clone();
        if config.exclude_self_damage {
            entity_conditions.push("source_id != target_id".to_string());
        }
        if let Some(sel) = config.entity_filter {
            entity_conditions.push(entity_match_sql(config.entity, sel));
        }
        let entity_filter = format!("WHERE {}", entity_conditions.join(" AND "));

        let batches = self
            .sql(&format!(
                r#"
WITH bounds AS (
    SELECT
        CAST(MIN(FLOOR(combat_time_secs / {bucket_secs})) as BIGINT) as min_bucket,
        CAST(MAX(FLOOR(combat_time_secs / {bucket_secs})) as BIGINT) as max_bucket
    FROM events
    {tr_filter}
),
time_series AS (
    SELECT
      unnest(generate_series(bounds.min_bucket, bounds.max_bucket, 1)) * {bucket_secs} * 1000 AS bucket_start_ms
    FROM bounds
),
entity_ts AS (
    SELECT CAST(FLOOR(combat_time_secs / {bucket_secs}) * {bucket_secs} * 1000 AS BIGINT) as bucket_start_ms,
           SUM({value_col}) as total_value
    FROM events
    {entity_filter}
    GROUP BY bucket_start_ms
)
SELECT
    time_series.bucket_start_ms,
    COALESCE(entity_ts.total_value, 0) as total_value
FROM time_series
LEFT JOIN entity_ts ON time_series.bucket_start_ms = entity_ts.bucket_start_ms
ORDER BY time_series.bucket_start_ms
            "#
            ))
            .await?;

        let mut results = Vec::new();
        for batch in &batches {
            let buckets = col_i64(batch, 0)?;
            let values = col_f64(batch, 1)?;
            for i in 0..batch.num_rows() {
                results.push(TimeSeriesPoint {
                    bucket_start_ms: buckets[i],
                    total_value: values[i],
                });
            }
        }
        Ok(results)
    }

    /// Query DPS (damage per second) over time, bucketed by time interval.
    pub async fn dps_over_time(
        &self,
        bucket_ms: i64,
        source_name: Option<&str>,
        time_range: Option<&TimeRange>,
    ) -> Result<Vec<TimeSeriesPoint>, String> {
        self.query_time_series(
            bucket_ms,
            TimeSeriesConfig {
                value_column: "dmg_amount",
                entity: "source",
                entity_filter: source_name,
                exclude_self_damage: true,
            },
            time_range,
        )
        .await
    }

    /// Query HPS (healing per second) over time, bucketed by time interval.
    pub async fn hps_over_time(
        &self,
        bucket_ms: i64,
        source_name: Option<&str>,
        time_range: Option<&TimeRange>,
    ) -> Result<Vec<TimeSeriesPoint>, String> {
        self.query_time_series(
            bucket_ms,
            TimeSeriesConfig {
                value_column: "heal_amount",
                entity: "source",
                entity_filter: source_name,
                exclude_self_damage: false,
            },
            time_range,
        )
        .await
    }

    /// Query EHPS (effective healing per second) over time, bucketed by time interval.
    pub async fn ehps_over_time(
        &self,
        bucket_ms: i64,
        source_name: Option<&str>,
        time_range: Option<&TimeRange>,
    ) -> Result<Vec<TimeSeriesPoint>, String> {
        self.query_time_series(
            bucket_ms,
            TimeSeriesConfig {
                value_column: "heal_effective",
                entity: "source",
                entity_filter: source_name,
                exclude_self_damage: false,
            },
            time_range,
        )
        .await
    }

    /// Query EHT (effective healing taken per second) over time for a target entity.
    pub async fn eht_over_time(
        &self,
        bucket_ms: i64,
        target_name: Option<&str>,
        time_range: Option<&TimeRange>,
    ) -> Result<Vec<TimeSeriesPoint>, String> {
        self.query_time_series(
            bucket_ms,
            TimeSeriesConfig {
                value_column: "heal_effective",
                entity: "target",
                entity_filter: target_name,
                exclude_self_damage: false,
            },
            time_range,
        )
        .await
    }

    /// Query DTPS (damage taken per second) over time for a target entity.
    pub async fn dtps_over_time(
        &self,
        bucket_ms: i64,
        target_name: Option<&str>,
        time_range: Option<&TimeRange>,
    ) -> Result<Vec<TimeSeriesPoint>, String> {
        self.query_time_series(
            bucket_ms,
            TimeSeriesConfig {
                value_column: "dmg_amount",
                entity: "target",
                entity_filter: target_name,
                exclude_self_damage: false,
            },
            time_range,
        )
        .await
    }

    /// Query HP% over time for an entity, taking the last HP% per bucket.
    ///
    /// Picks the last event per time bucket (by line_number) and returns
    /// hp_pct, current_hp, max_hp. Missing buckets are forward-filled with
    /// the last known value to avoid fake-zero spikes.
    pub async fn hp_over_time(
        &self,
        bucket_ms: i64,
        entity_name: Option<&str>,
        time_range: Option<&TimeRange>,
    ) -> Result<Vec<HpPoint>, String> {
        use datafusion::arrow::array::{Float64Array, Int32Array, Int64Array};

        let bucket_secs = (bucket_ms as f64 / 1000.0).max(1.0);

        let mut tr_conditions = vec!["combat_time_secs IS NOT NULL".to_string()];
        if let Some(tr) = time_range {
            tr_conditions.push(tr.sql_filter());
        }
        let tr_filter = format!("WHERE {}", tr_conditions.join(" AND "));

        // Without a selector every row qualifies, matching the old COALESCE(NULL, col) = col trick
        let (src_match, tgt_match) = match entity_name {
            Some(sel) => (entity_match_sql("source", sel), entity_match_sql("target", sel)),
            None => ("TRUE".to_string(), "TRUE".to_string()),
        };
        let mut entity_conditions = tr_conditions.clone();
        if entity_name.is_some() {
            entity_conditions.push(format!("({src_match} OR {tgt_match})"));
        }
        let entity_filter = format!("WHERE {}", entity_conditions.join(" AND "));

        let batches = self
            .sql(&format!(
                r#"
WITH bounds AS (
    SELECT
        CAST(MIN(FLOOR(combat_time_secs / {bucket_secs})) as BIGINT) as min_bucket,
        CAST(MAX(FLOOR(combat_time_secs / {bucket_secs})) as BIGINT) as max_bucket
    FROM events
    {tr_filter}
),
time_series AS (
    SELECT
      unnest(generate_series(bounds.min_bucket, bounds.max_bucket, 1)) * {bucket_secs} * 1000 AS bucket_start_ms
    FROM bounds
),
hp_events AS (
    SELECT
        CAST(FLOOR(combat_time_secs / {bucket_secs}) * {bucket_secs} * 1000 AS BIGINT) as bucket_start_ms,
        line_number,
        CASE
            WHEN {src_match} AND source_max_hp > 0
                THEN CAST(source_hp AS DOUBLE) * 100.0 / CAST(source_max_hp AS DOUBLE)
            WHEN {tgt_match} AND target_max_hp > 0
                THEN CAST(target_hp AS DOUBLE) * 100.0 / CAST(target_max_hp AS DOUBLE)
            ELSE NULL
        END as hp_pct,
        CASE
            WHEN {src_match} AND source_max_hp > 0
                THEN source_hp
            WHEN {tgt_match} AND target_max_hp > 0
                THEN target_hp
            ELSE NULL
        END as current_hp,
        CASE
            WHEN {src_match} AND source_max_hp > 0
                THEN source_max_hp
            WHEN {tgt_match} AND target_max_hp > 0
                THEN target_max_hp
            ELSE NULL
        END as max_hp
    FROM events
    {entity_filter}
),
ranked AS (
    SELECT
        bucket_start_ms,
        hp_pct,
        current_hp,
        max_hp,
        ROW_NUMBER() OVER (PARTITION BY bucket_start_ms ORDER BY line_number DESC) as rn
    FROM hp_events
    WHERE hp_pct IS NOT NULL
)
SELECT
    time_series.bucket_start_ms,
    ranked.hp_pct,
    ranked.current_hp,
    ranked.max_hp
FROM time_series
LEFT JOIN ranked ON time_series.bucket_start_ms = ranked.bucket_start_ms AND ranked.rn = 1
ORDER BY time_series.bucket_start_ms
            "#
            ))
            .await?;

        // Forward-fill: carry last known HP values through empty buckets
        let mut results = Vec::new();
        let mut last_pct = 100.0f64;
        let mut last_hp = 0i64;
        let mut last_max = 0i64;

        for batch in &batches {
            let buckets = col_i64(batch, 0)?;
            let pct_col = batch.column(1);
            let hp_col = batch.column(2);
            let max_col = batch.column(3);

            for i in 0..batch.num_rows() {
                if !pct_col.is_null(i) {
                    // Extract values — columns may be f64/i32/i64 depending on cast
                    if let Some(a) = pct_col.as_any().downcast_ref::<Float64Array>() {
                        last_pct = a.value(i);
                    }
                    if let Some(a) = hp_col.as_any().downcast_ref::<Int32Array>() {
                        last_hp = a.value(i) as i64;
                    } else if let Some(a) = hp_col.as_any().downcast_ref::<Int64Array>() {
                        last_hp = a.value(i);
                    }
                    if let Some(a) = max_col.as_any().downcast_ref::<Int32Array>() {
                        last_max = a.value(i) as i64;
                    } else if let Some(a) = max_col.as_any().downcast_ref::<Int64Array>() {
                        last_max = a.value(i);
                    }
                }
                results.push(HpPoint {
                    bucket_start_ms: buckets[i],
                    hp_pct: last_pct,
                    current_hp: last_hp,
                    max_hp: last_max,
                });
            }
        }
        Ok(results)
    }
}

impl EncounterQuery<'_> {
    /// Every player's (and companion's) metric per bucket in one pass, on a
    /// shared spine from the first to the last event in range, for the
    /// Overview's group chart.
    pub async fn group_over_time(
        &self,
        bucket_ms: i64,
        metric: GroupMetric,
        time_range: Option<&TimeRange>,
    ) -> Result<GroupTimeSeries, String> {
        let bucket_secs = (bucket_ms as f64 / 1000.0).max(1.0);
        let (side, value_col, extra) = match metric {
            GroupMetric::Damage => ("source", "dmg_amount", "AND source_id != target_id"),
            GroupMetric::Healing => ("source", "heal_effective", ""),
            GroupMetric::DamageTaken => ("target", "dmg_amount", ""),
            GroupMetric::Threat => ("source", "threat", ""), // see floored_threat
        };
        let tr_filter = time_range.map(|tr| format!("AND {}", tr.sql_filter())).unwrap_or_default();

        let bounds = self
            .sql(&format!(
                "SELECT CAST(MIN(FLOOR(combat_time_secs / {bucket_secs})) AS BIGINT),
                        CAST(MAX(FLOOR(combat_time_secs / {bucket_secs})) AS BIGINT)
                 FROM events WHERE combat_time_secs IS NOT NULL {tr_filter}"
            ))
            .await?;
        let Some(b) = bounds.first().filter(|b| b.num_rows() > 0 && !b.column(0).is_null(0)) else {
            return Ok(GroupTimeSeries::default());
        };
        let (min_bucket, max_bucket) = (col_i64(b, 0)?[0], col_i64(b, 1)?[0]);
        let len = (max_bucket - min_bucket + 1).max(0) as usize;

        // (player, bucket, value) rows; threat is floored per player first
        let mut rows: Vec<(String, i64, f64)> = Vec::new();
        if metric == GroupMetric::Threat {
            rows = self.floored_threat(bucket_secs, &tr_filter).await?;
        } else {
            let batches = self
                .sql(&format!(
                    "SELECT {side}_name,
                            CAST(FLOOR(combat_time_secs / {bucket_secs}) AS BIGINT) AS bucket,
                            CAST(SUM({value_col}) AS DOUBLE)
                     FROM events
                     WHERE combat_time_secs IS NOT NULL {tr_filter}
                       AND {side}_entity_type IN ('Player', 'Companion') {extra}
                     GROUP BY {side}_name, bucket"
                ))
                .await?;
            for batch in &batches {
                let (names, buckets, values) = (col_strings(batch, 0)?, col_i64(batch, 1)?, col_f64(batch, 2)?);
                rows.extend(names.into_iter().zip(buckets).zip(values).map(|((n, b), v)| (n, b, v)));
            }
        }

        let mut series: Vec<(String, Vec<f64>)> = Vec::new();
        for (name, bucket, value) in rows {
            let Some(slot) = usize::try_from(bucket - min_bucket).ok().filter(|i| *i < len) else {
                continue;
            };
            let idx = match series.iter().position(|(n, _)| *n == name) {
                Some(i) => i,
                None => {
                    series.push((name, vec![0.0; len]));
                    series.len() - 1
                }
            };
            series[idx].1[slot] += value;
        }
        Ok(GroupTimeSeries { start_secs: min_bucket as f64 * bucket_secs, bucket_secs, series })
    }

    /// Threat with each player's running total floored at 0, so a drop only
    /// removes what was there. Floored per player, not per target: the log
    /// books heal threat (incl. tank self-heals) on the friendly, so per-NPC
    /// totals undercount and clip the full drop of a cloak-out or threat wipe
    /// to a small dip. Returns
    /// (player, bucket, threat actually gained or lost) per event, in log order.
    pub(super) async fn floored_threat(&self, bucket_secs: f64, tr_filter: &str) -> Result<Vec<(String, i64, f64)>, String> {
        let batches = self
            .sql(&format!(
                "SELECT source_name,
                        CAST(FLOOR(combat_time_secs / {bucket_secs}) AS BIGINT),
                        CAST(threat AS DOUBLE)
                 FROM events
                 WHERE combat_time_secs IS NOT NULL AND threat != 0 {tr_filter}
                   AND source_entity_type IN ('Player', 'Companion')
                 ORDER BY line_number"
            ))
            .await?;
        let mut held: hashbrown::HashMap<String, f64> = hashbrown::HashMap::new();
        let mut out = Vec::new();
        for batch in &batches {
            let (names, buckets, values) = (col_strings(batch, 0)?, col_i64(batch, 1)?, col_f64(batch, 2)?);
            for ((name, bucket), value) in names.into_iter().zip(buckets).zip(values) {
                let run = held.entry_ref(name.as_str()).or_insert(0.0);
                let next = (*run + value).max(0.0);
                out.push((name, bucket, next - *run));
                *run = next;
            }
        }
        Ok(out)
    }
}
