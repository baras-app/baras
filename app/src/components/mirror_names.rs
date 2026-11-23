//! Republic ⇄ Imperial ability naming for the Data Explorer.
//!
//! Ability names in query results are whatever the combat log recorded. This
//! module holds the user's naming preference and the mirror map (fetched once
//! from the backend) and resolves the id/name to display for a given row.

use std::borrow::Cow;
use std::collections::HashMap;

use dioxus::prelude::*;

use crate::api;
use crate::types::{AbilityNaming, MirrorAbility};
use baras_types::base_ability_id;

pub static ABILITY_NAMING: GlobalSignal<AbilityNaming> = Signal::global(AbilityNaming::default);
static MIRROR_MAP: GlobalSignal<HashMap<i64, MirrorAbility>> = Signal::global(HashMap::new);
static LOAD_STARTED: GlobalSignal<bool> = Signal::global(|| false);

/// Fetch the mirror map once per app lifetime. Safe to call from any component body.
pub fn ensure_loaded() {
    if *LOAD_STARTED.peek() {
        return;
    }
    *LOAD_STARTED.write() = true;
    spawn(async move {
        let map = api::get_mirror_abilities()
            .await
            .into_iter()
            .map(|m| (m.id, m))
            .collect();
        *MIRROR_MAP.write() = map;
    });
}

/// Mirror entry for `id` (an ability or effect ID) if the current naming
/// preference requires a swap. An unlisted ID is taken as a variant of the
/// ability it belongs to.
fn swap_for(id: i64) -> Option<MirrorAbility> {
    let want_imperial = match *ABILITY_NAMING.read() {
        AbilityNaming::AsLogged => return None,
        AbilityNaming::Republic => false,
        AbilityNaming::Imperial => true,
    };
    let map = MIRROR_MAP.read();
    map.get(&id)
        .or_else(|| map.get(&base_ability_id(id)))
        .filter(|m| m.is_imperial != want_imperial)
        .cloned()
}

/// Ability or effect id to use for icons under the current naming preference.
pub fn display_id(id: i64) -> i64 {
    swap_for(id).map_or(id, |m| m.mirror_id)
}

/// Ability or effect name to show under the current naming preference.
pub fn display_name(id: i64, name: &str) -> Cow<'_, str> {
    swap_for(id).map_or(Cow::Borrowed(name), |m| Cow::Owned(m.mirror_name))
}
