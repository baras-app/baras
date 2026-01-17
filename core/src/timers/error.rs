//! Error types for timer operations

use std::path::PathBuf;
use thiserror::Error;

/// Errors during timer definition loading
#[derive(Debug, Error)]
pub enum TimerError {
    #[error("failed to read timer file {path}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse timer TOML in {path}")]
    ParseToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("failed to read timer directory {path}")]
    ReadDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid timer definition in {path}: {reason}")]
    InvalidDefinition { path: PathBuf, reason: String },
}
