//! Common utility functions for overlay rendering
//!
//! These are shared across different overlay types.

use tiny_skia::Color;

/// Convert [u8; 4] RGBA array to tiny_skia Color
#[inline]
pub fn color_from_rgba(rgba: [u8; 4]) -> Color {
    Color::from_rgba8(rgba[0], rgba[1], rgba[2], rgba[3])
}

/// Truncate a string to max_chars, adding "..." if truncated
pub fn truncate_name(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        name.to_string()
    } else {
        let truncated: String = name.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

/// Format a duration in seconds as MM:SS
pub fn format_time(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// Format a duration in seconds as compact M:SS string
pub fn format_duration_short(secs: f32) -> String {
    let total_secs = secs.round() as u64;
    format!("{}:{:02}", total_secs / 60, total_secs % 60)
}

/// Format a large number with K/M suffix for compact display
pub fn format_number(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 10_000 {
        format!("{:.2}K", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_name() {
        assert_eq!(truncate_name("short", 10), "short");
        assert_eq!(truncate_name("this is a very long name", 10), "this is...");
        assert_eq!(truncate_name("exactly10!", 10), "exactly10!");
    }

    #[test]
    fn test_format_time() {
        assert_eq!(format_time(0), "0:00");
        assert_eq!(format_time(59), "0:59");
        assert_eq!(format_time(60), "1:00");
        assert_eq!(format_time(125), "2:05");
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(500), "500");
        assert_eq!(format_number(9999), "9999");
        assert_eq!(format_number(10000), "10.0K");
        assert_eq!(format_number(1500000), "1.5M");
    }
}
