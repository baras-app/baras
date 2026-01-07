use lasso::{Spur, ThreadedRodeo};
use std::sync::OnceLock;

/// Interned string key - 4 bytes instead of 24 for String.
pub type IStr = Spur;

/// Global string interner for combat log data.
static INTERNER: OnceLock<ThreadedRodeo> = OnceLock::new();

/// Cached empty string Spur to avoid repeated lookups.
static EMPTY_ISTR: OnceLock<Spur> = OnceLock::new();

/// Get the global interner (initializes on first call).
pub fn interner() -> &'static ThreadedRodeo {
    INTERNER.get_or_init(ThreadedRodeo::default)
}

/// Intern a string, returning a key.
pub fn intern(s: &str) -> IStr {
    interner().get_or_intern(s)
}

/// Returns the IStr for an empty string. Use this instead of IStr::default()
/// since Spur::default() collides with the first interned string.
#[inline]
pub fn empty_istr() -> IStr {
    *EMPTY_ISTR.get_or_init(|| interner().get_or_intern(""))
}

/// Resolve an interned key back to a string.
pub fn resolve(key: IStr) -> &'static str {
    interner().resolve(&key)
}
