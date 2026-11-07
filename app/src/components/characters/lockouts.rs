//! Lockout chips and the time helpers the Characters tab shares.

use dioxus::prelude::*;

use baras_types::characters::Lockout;
use baras_types::history::iso_to_ms;

const MINUTE_MS: f64 = 60_000.0;
const HOUR_MS: f64 = 3_600_000.0;
const DAY_MS: f64 = 86_400_000.0;

/// "SM" | "VM" | "NiM"
pub fn tier_short(tier: &str) -> &'static str {
    match tier {
        "Story" => "SM",
        "Veteran" => "VM",
        "Master" => "NiM",
        _ => "",
    }
}

/// "The Dread Fortress" → "DF"; words like "the"/"of" are skipped
pub fn acronym(name: &str) -> String {
    let short: String = name
        .split(|c: char| c.is_whitespace() || c == '-')
        .filter(|w| !matches!(w.to_ascii_lowercase().as_str(), "the" | "of" | "and" | "from" | "&"))
        .filter_map(|w| w.chars().next())
        .filter(|c| c.is_alphanumeric())
        .collect();
    if short.len() < 2 { name.to_string() } else { short.to_uppercase() }
}

/// "2d 4h" / "5h 12m" / "3m" until a true-UTC epoch instant
pub fn until(ms: i64) -> String {
    let left = ms as f64 - js_sys::Date::now();
    if left <= 0.0 {
        return "now".into();
    }
    let days = (left / DAY_MS).floor();
    let hours = ((left % DAY_MS) / HOUR_MS).floor();
    let minutes = ((left % HOUR_MS) / MINUTE_MS).floor();
    if days >= 1.0 {
        format!("{days}d {hours}h")
    } else if hours >= 1.0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

/// Local-time ISO stamp → "Today" / "Yesterday" / "N days ago" / "N months ago" / "N years ago"
pub fn ago(iso: &str) -> String {
    let Some(seen) = iso_to_ms(iso) else {
        return "never".into();
    };
    // Log stamps are local time read as UTC; shift "now" the same way
    let now_local = js_sys::Date::now() - js_sys::Date::new_0().get_timezone_offset() * MINUTE_MS;
    let days = ((now_local - seen as f64) / DAY_MS).floor();
    let plural = |n: i64, unit: &str| if n == 1 { format!("1 {unit} ago") } else { format!("{n} {unit}s ago") };
    match days as i64 {
        i64::MIN..=0 => "Today".into(),
        1 => "Yesterday".into(),
        d @ 2..=59 => plural(d, "day"),
        d @ 60..=729 => plural(d / 30, "month"),
        d => plural(d / 365, "year"),
    }
}

#[component]
pub fn LockoutChips(lockouts: Vec<Lockout>) -> Element {
    rsx! {
        if lockouts.is_empty() {
            span { class: "lockout-free", title: "No operation kills this reset", "Free" }
        } else {
            div { class: "lockout-chips",
                for l in lockouts.iter() {
                    {
                        let tier = tier_short(&l.tier);
                        let progress = if l.total > 0 { format!("{}/{}", l.locked.len(), l.total) } else { l.locked.len().to_string() };
                        let class = format!(
                            "lockout-chip tier-{}{}{}",
                            l.tier.to_lowercase(),
                            if l.is_full() { " full" } else { "" },
                            if l.manual { " manual" } else { "" },
                        );
                        let title = format!(
                            "{} · {} mode{}\nLocked: {}\nResets in {}",
                            l.operation,
                            l.tier,
                            if l.manual { " (marked by hand)" } else { "" },
                            l.locked.join(", "),
                            until(l.expires_ms),
                        );
                        rsx! {
                            span { class: "{class}", title: "{title}",
                                span { class: "lockout-op", {acronym(&l.operation)} }
                                span { class: "lockout-tier", "{tier}" }
                                span { class: "lockout-progress", "{progress}" }
                            }
                        }
                    }
                }
            }
        }
    }
}
