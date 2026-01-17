//! Error types for data storage operations

use std::path::PathBuf;
use thiserror::Error;

/// Errors during parquet file operations
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("failed to create file {path}")]
    CreateFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write parquet file {path}")]
    WriteParquet {
        path: PathBuf,
        #[source]
        source: parquet::errors::ParquetError,
    },

    #[error("arrow conversion error")]
    Arrow(#[from] arrow::error::ArrowError),

    #[error("parquet error")]
    Parquet(#[from] parquet::errors::ParquetError),

    #[error("failed to create data directory {path}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("IO error")]
    Io(#[from] std::io::Error),

    #[error("failed to build record batch: {reason}")]
    BuildRecordBatch { reason: String },
}
