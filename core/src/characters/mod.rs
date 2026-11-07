//! Character roster for the Characters tab.
//!
//! A character is a (name, server) pair. Everything the logs can tell
//! (classes played, kills, last session, lockouts) is derived from the
//! `PullIndex` on every query; the user-owned half (crew skills, guild, note,
//! overrides) is `CharacterStore`, one JSON file a re-parse never touches.

mod lockouts;
mod roster;
mod store;

pub use lockouts::{lockouts, next_reset, now_utc_ms};
pub use roster::roster;
pub use store::CharacterStore;
