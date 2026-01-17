//! Error types for data queries

use thiserror::Error;

/// Errors during data queries
#[derive(Debug, Error)]
pub enum QueryError {
    #[error("datafusion error")]
    DataFusion(#[from] datafusion::error::DataFusionError),

    #[error("arrow error")]
    Arrow(#[from] arrow::error::ArrowError),

    #[error("column {name} not found in result")]
    ColumnNotFound { name: String },

    #[error("unexpected column type for {name}: expected {expected}, got {actual}")]
    UnexpectedColumnType {
        name: String,
        expected: &'static str,
        actual: String,
    },

    #[error("no data available for query")]
    NoData,

    #[error("failed to register parquet file: {path}")]
    RegisterParquet {
        path: String,
        #[source]
        source: datafusion::error::DataFusionError,
    },

    #[error("SQL execution failed: {query}")]
    SqlExecution {
        query: String,
        #[source]
        source: datafusion::error::DataFusionError,
    },
}
