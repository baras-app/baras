use hashbrown::HashMap;
use lasso::{Key, Spur, ThreadedRodeo};
use std::cell::RefCell;
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

// Per-thread caches in front of the shared interner. Its lookups take a
// shard lock and SipHash the key, which dominates both the parallel parse
// (lock contention on hot strings) and the serial resolve-heavy paths.
thread_local! {
    static INTERNED: RefCell<HashMap<&'static str, IStr>> = RefCell::new(HashMap::new());
    static RESOLVED: RefCell<Vec<Option<&'static str>>> = const { RefCell::new(Vec::new()) };
}

/// Intern a string, returning a key.
pub fn intern(s: &str) -> IStr {
    INTERNED.with_borrow_mut(|cache| {
        if let Some(&key) = cache.get(s) {
            return key;
        }
        let key = interner().get_or_intern(s);
        cache.insert(interner().resolve(&key), key);
        key
    })
}

/// Returns the IStr for an empty string. Use this instead of IStr::default()
/// since Spur::default() collides with the first interned string.
#[inline]
pub fn empty_istr() -> IStr {
    *EMPTY_ISTR.get_or_init(|| interner().get_or_intern(""))
}

/// Resolve an interned key back to a string.
pub fn resolve(key: IStr) -> &'static str {
    RESOLVED.with_borrow_mut(|cache| {
        let idx = key.into_usize();
        if let Some(Some(s)) = cache.get(idx) {
            return *s;
        }
        let s = interner().resolve(&key);
        if idx >= cache.len() {
            cache.resize(idx + 1, None);
        }
        cache[idx] = Some(s);
        s
    })
}
