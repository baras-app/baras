//! Operation lockouts from recorded kills.
//!
//! The game locks a boss once you kill it, and every boss before it in the
//! operation, until the next reset: weekly (Tuesday) for Veteran and Master,
//! daily for Story, both at the configured UTC hour. Log timestamps are local
//! time, so kills are shifted by the machine's UTC offset before comparing.

use chrono::{DateTime, Datelike, Duration, Utc, Weekday};
use hashbrown::HashMap;

use baras_types::characters::{Lockout, ManualLock, operation_key};
use baras_types::history::PullRow;

use crate::history::{AreaCatalog, difficulty_parts};

pub fn now_utc_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// Local-time millis (as the index stores them) → true UTC millis
pub fn to_utc_ms(local_ms: i64) -> i64 {
    local_ms - i64::from(chrono::Local::now().offset().local_minus_utc()) * 1000
}

/// Story mode resets daily; everything else weekly
fn is_weekly(tier: &str) -> bool {
    tier != "Story"
}

/// First reset strictly after `after_ms` (true UTC): Tuesday at `hour` UTC
/// for weekly lockouts, `hour` UTC every day otherwise.
pub fn next_reset(after_ms: i64, weekly: bool, hour: u8) -> i64 {
    let after = DateTime::<Utc>::from_timestamp_millis(after_ms).unwrap_or_default();
    let mut at = after
        .date_naive()
        .and_hms_opt(u32::from(hour.min(23)), 0, 0)
        .unwrap_or_default()
        .and_utc();
    let period = if weekly {
        let ahead = (i64::from(Weekday::Tue.num_days_from_monday())
            - i64::from(at.weekday().num_days_from_monday()))
        .rem_euclid(7);
        at += Duration::days(ahead);
        7
    } else {
        1
    };
    if at <= after {
        at += Duration::days(period);
    }
    at.timestamp_millis()
}

struct Group {
    operation: String,
    area_id: i64,
    tier: &'static str,
    /// Group size of the newest kill, only to pick the boss list
    size: u8,
    /// Bosses killed this reset window
    kills: Vec<String>,
    expires_ms: i64,
    manual: bool,
}

/// Keyed by operation (area id, else name) and mode
type Groups = HashMap<(String, &'static str), Group>;

/// The group for one kill or mark, if its reset window is still open
fn touch<'g>(groups: &'g mut Groups, operation: &str, area_id: i64, tier: &'static str, size: u8, at_ms: i64, hour: u8, now_ms: i64) -> Option<&'g mut Group> {
    let expires = next_reset(at_ms, is_weekly(tier), hour);
    if expires <= now_ms {
        return None;
    }
    let g = groups
        .entry((operation_key(area_id, operation), tier))
        .or_insert(Group { operation: operation.to_string(), area_id, tier, size, kills: Vec::new(), expires_ms: expires, manual: false });
    g.expires_ms = g.expires_ms.max(expires);
    if g.area_id == 0 {
        g.area_id = area_id;
    }
    Some(g)
}

fn tier_of(name: &str) -> &'static str {
    ["Story", "Veteran", "Master"].into_iter().find(|t| t.eq_ignore_ascii_case(name)).unwrap_or("")
}

/// Current lockouts from one character's pulls and manual marks. Only
/// operation kills count; an expired window contributes nothing.
pub fn lockouts(rows: &[&PullRow], manual: &[ManualLock], catalog: &AreaCatalog, hour: u8, now_ms: i64) -> Vec<Lockout> {
    let mut groups = Groups::new();
    let mut sorted: Vec<&&PullRow> = rows.iter().filter(|r| r.is_operation && r.success).collect();
    sorted.sort_by_key(|r| r.start_ms);
    for r in sorted {
        let killed_at = to_utc_ms(r.start_ms + r.duration_seconds * 1000);
        let (tier, size) = difficulty_parts(&r.difficulty);
        if let Some(g) = touch(&mut groups, &r.operation, r.area_id, tier, size, killed_at, hour, now_ms) {
            g.size = size;
            if !g.kills.contains(&r.boss) {
                g.kills.push(r.boss.clone());
            }
        }
    }
    for m in manual {
        if let Some(g) = touch(&mut groups, &m.operation, m.area_id, tier_of(&m.tier), 8, m.marked_ms, hour, now_ms) {
            g.manual = true;
        }
    }

    let mut out: Vec<Lockout> = groups
        .into_values()
        .map(|g| {
            let order: Vec<String> = catalog
                .area(g.area_id)
                .map(|a| a.bosses_at(&g.tier.to_lowercase(), g.size).map(|b| b.name.clone()).collect())
                .unwrap_or_default();
            let locked = if g.manual {
                if order.is_empty() { vec!["All bosses".to_string()] } else { order.clone() }
            } else if order.is_empty() {
                g.kills
            } else {
                // Furthest kill in operation order locks everything before it
                let furthest = g.kills.iter().filter_map(|k| order.iter().position(|b| b == k)).max();
                furthest.map_or(g.kills, |i| order[..=i].to_vec())
            };
            Lockout {
                operation: g.operation,
                area_id: g.area_id,
                tier: g.tier.to_string(),
                locked,
                total: order.len() as u8,
                expires_ms: g.expires_ms,
                manual: g.manual,
            }
        })
        .filter(|l| !l.locked.is_empty())
        .collect();
    out.sort_by(|a, b| a.expires_ms.cmp(&b.expires_ms).then_with(|| a.operation.cmp(&b.operation)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(iso: &str) -> i64 {
        baras_types::history::iso_to_ms(iso).unwrap()
    }

    #[test]
    fn weekly_reset_lands_on_next_tuesday_at_hour() {
        // 2026-11-05 is a Thursday
        assert_eq!(next_reset(ms("2026-11-05T20:00:00"), true, 12), ms("2026-11-10T12:00:00"));
        // A kill on Tuesday before the hour resets the same day
        assert_eq!(next_reset(ms("2026-11-10T03:00:00"), true, 12), ms("2026-11-10T12:00:00"));
        // Exactly at reset belongs to the next week
        assert_eq!(next_reset(ms("2026-11-10T12:00:00"), true, 12), ms("2026-11-17T12:00:00"));
    }

    #[test]
    fn daily_reset_is_next_hour_mark() {
        assert_eq!(next_reset(ms("2026-11-05T11:59:00"), false, 12), ms("2026-11-05T12:00:00"));
        assert_eq!(next_reset(ms("2026-11-05T12:00:00"), false, 12), ms("2026-11-06T12:00:00"));
    }
}
