//! Checkpoint verification for timer validation
//!
//! Defines expected timer sequences and verifies them against actual behavior.

pub mod checkpoint;

pub use checkpoint::{CheckpointVerifier, Expectations};
