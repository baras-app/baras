//! Alphabetical ordering for the effects list. The order is worked
//! out when the set of entries (or the mode) changes, not on every edit, so
//! renaming an entry leaves it where it is until the list is next rebuilt.

use dioxus::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// Rank (position) per id for `items` as (id, name) pairs. With `az` off the
/// map is empty: callers sort by it with a missing rank last, which keeps
/// the given order.
pub fn use_stable_rank<'a>(items: impl Iterator<Item = (&'a str, &'a str)>, az: bool) -> Memo<HashMap<String, usize>> {
    let items: Vec<(String, String)> = items.map(|(i, n)| (i.to_string(), n.to_string())).collect();
    let mut ids: Vec<&str> = items.iter().map(|(i, _)| i.as_str()).collect();
    ids.sort_unstable();
    let signature = ids.join("\u{1f}");

    // Latest names, read (not tracked) when the order is rebuilt
    let latest = use_hook(|| Rc::new(RefCell::new(Vec::new())));
    *latest.borrow_mut() = items;

    use_memo(use_reactive!(|(signature, az)| {
        let _ = &signature;
        if !az {
            return HashMap::new();
        }
        let mut v = latest.borrow().clone();
        v.sort_by_cached_key(|(_, name)| name.to_lowercase());
        v.into_iter().enumerate().map(|(i, (id, _))| (id, i)).collect()
    }))
}

/// Sort `items` by a rank from [`use_stable_rank`]
pub fn apply_rank<T>(items: &mut [T], rank: &HashMap<String, usize>, id: impl Fn(&T) -> &str) {
    if !rank.is_empty() {
        items.sort_by_key(|t| rank.get(id(t)).copied().unwrap_or(usize::MAX));
    }
}
