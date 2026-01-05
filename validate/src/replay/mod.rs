//! Replay engine for timer validation
//!
//! Provides timing simulation for replaying combat logs at various speeds
//! with accurate lag compensation.

pub mod clock;
pub mod lag;

pub use clock::VirtualClock;
pub use lag::LagSimulator;
