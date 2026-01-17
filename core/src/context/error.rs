//! Error types for context operations

use std::path::PathBuf;
use thiserror::Error;

/// Errors during directory watching and indexing
#[derive(Debug, Error)]
pub enum WatcherError {
    #[error("failed to read directory {path}")]
    ReadDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to read file metadata for {path}")]
    Metadata {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to initialize file watcher")]
    InitWatcher(#[source] notify::Error),

    #[error("failed to watch path {path}")]
    WatchPath {
        path: PathBuf,
        #[source]
        source: notify::Error,
    },

    #[error("no log files found in {path}")]
    NoLogFiles { path: PathBuf },
}

/// Errors during configuration operations
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to load configuration")]
    Load(#[from] confy::ConfyError),

    #[error("failed to save configuration")]
    Save(#[source] confy::ConfyError),

    #[error("profile '{name}' not found")]
    ProfileNotFound { name: String },

    #[error("maximum profiles reached ({max})")]
    MaxProfilesReached { max: usize },

    #[error("profile name '{name}' already exists")]
    ProfileNameTaken { name: String },

    #[error("failed to create config directory")]
    CreateDir(#[source] std::io::Error),
}
