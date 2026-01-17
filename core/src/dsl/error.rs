//! Error types for DSL definition loading

use std::path::PathBuf;
use thiserror::Error;

/// Errors during DSL definition loading and saving
#[derive(Debug, Error)]
pub enum DslError {
    #[error("failed to read {path}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to parse TOML in {path}")]
    ParseToml {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },

    #[error("failed to read directory {path}")]
    ReadDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to serialize configuration")]
    Serialize(#[from] toml::ser::Error),

    #[error("failed to create directory {path}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to write {path}")]
    WriteFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid definition in {path}: {reason}")]
    InvalidDefinition { path: PathBuf, reason: String },
}
