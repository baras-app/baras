//! Time series queries (DPS, HPS, DTPS over time).

use super::*;

/// Configuration for time series queries.
struct TimeSeriesConfig<'a> {
    /// Column to sum ("dmg_amount" or "heal_amount")
    value_column: &'static str,
    /// Column to filter by entity ("source_name" or "target_name")
    entity_column: &'static str,
    /// Optional entity name filter
    entity_filter: Option<&'a str>,
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
        let entity_col = config.entity_column;

        // Base conditions for time range (used for bounds calculation)
        let mut tr_conditions = vec!["combat_time_secs IS NOT NULL".to_string()];
        if let Some(tr) = time_range {
            tr_conditions.push(tr.sql_filter());
        }
        let tr_filter = format!("WHERE {}", tr_conditions.join(" AND "));

        // Entity-specific conditions (used for value aggregation)
        let mut entity_conditions = tr_conditions.clone();
        if let Some(name) = config.entity_filter {
            entity_conditions.push(format!("{} = '{}'", entity_col, sql_escape(name)));
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
                entity_column: "source_name",
                entity_filter: source_name,
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
                entity_column: "source_name",
                entity_filter: source_name,
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
                entity_column: "target_name",
                entity_filter: target_name,
            },
            time_range,
        )
        .await
    }
}
