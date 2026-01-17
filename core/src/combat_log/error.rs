//! Error types for combat log parsing

use std::path::PathBuf;
use thiserror::Error;

/// Errors during combat log line parsing
#[derive(Debug, Error)]
pub enum ParseError {
    #[error("invalid line format at line {line_number}: expected bracket-delimited segments")]
    InvalidLineFormat { line_number: u64 },

    #[error("invalid timestamp at line {line_number}: {segment}")]
    InvalidTimestamp { line_number: u64, segment: String },

    #[error("invalid entity format at line {line_number}")]
    InvalidEntity { line_number: u64 },

    #[error("invalid effect format at line {line_number}")]
    InvalidEffect { line_number: u64 },

    #[error("invalid value format at line {line_number}: {detail}")]
    InvalidValue { line_number: u64, detail: String },
}

/// Errors during log file reading operations
#[derive(Debug, Error)]
pub enum ReaderError {
    #[error("failed to open log file {path}")]
    OpenFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to memory map file {path}")]
    MemoryMap {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("encoding error in file {path}: not valid Windows-1252")]
    Encoding { path: PathBuf },

    #[error("failed to read file {path}")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("failed to seek in file {path}")]
    Seek {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
