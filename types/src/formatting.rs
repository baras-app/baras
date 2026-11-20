//! Centralized number formatting utilities.
//!
//! All numeric display formatting goes through this module to ensure
//! consistency across overlays and app components, and to support
//! European-style number formatting (swapping `.` and `,`).

/// Apply European number format by swapping `.` and `,` in a formatted string.
///
/// This performs a three-step swap using a placeholder to avoid conflicts:
/// `.` -> `,` and `,` -> `.`
fn europeanize(s: &str) -> String {
    // We only want to swap within the numeric portion, but since our formatted
    // strings are purely numeric (with optional K/M/% suffix), a global swap is safe.
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '.' => result.push(','),
            ',' => result.push('.'),
            _ => result.push(c),
        }
    }
    result
}

/// Apply European formatting conditionally.
#[inline]
fn maybe_eu(s: String, european: bool) -> String {
    if european { europeanize(&s) } else { s }
}

/// Format a large number with K/M suffix for compact display.
///
/// - Values >= 1,000,000 are formatted as `X.XXM`
/// - Values >= 1,000 are formatted as `X.XXK`
/// - Values below 1,000 are formatted as-is
///
/// # Examples
/// ```
/// use baras_types::formatting::format_compact;
/// assert_eq!(format_compact(500, false), "500");
/// assert_eq!(format_compact(1_500, false), "1.50K");
/// assert_eq!(format_compact(15_000, false), "15.00K");
/// assert_eq!(format_compact(1_500_000, false), "1.50M");
/// assert_eq!(format_compact(1_500, true), "1,50K");
/// ```
pub fn format_compact(n: i64, european: bool) -> String {
    let s = if n >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.2}K", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    };
    maybe_eu(s, european)
}

/// Format a large f64 number with K/M suffix for compact display.
///
/// Same thresholds and precision as [`format_compact`] but accepts f64 input.
///
/// # Examples
/// ```
/// use baras_types::formatting::format_compact_f64;
/// assert_eq!(format_compact_f64(1_500.0, false), "1.50K");
/// assert_eq!(format_compact_f64(1_500.0, true), "1,50K");
/// ```
pub fn format_compact_f64(n: f64, european: bool) -> String {
    let n_abs = n.abs();
    let s = if n_abs >= 1_000_000.0 {
        format!("{:.2}M", n / 1_000_000.0)
    } else if n_abs >= 1_000.0 {
        format!("{:.2}K", n / 1_000.0)
    } else {
        format!("{:.0}", n)
    };
    maybe_eu(s, european)
}

/// Format a number with thousands separators (for combat log raw values).
///
/// Returns empty string for zero (matching combat log behavior).
///
/// - Standard: `1,234,567`
/// - European: `1.234.567`
///
/// # Examples
/// ```
/// use baras_types::formatting::format_thousands;
/// assert_eq!(format_thousands(0), "");
/// assert_eq!(format_thousands(500), "500");
/// assert_eq!(format_thousands(1_500), "1,500");
/// assert_eq!(format_thousands(1_500_000), "1,500,000");
/// ```
pub fn format_thousands(n: i32) -> String {
    if n == 0 {
        return String::new();
    }
    let s = n.abs().to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.insert(0, ',');
        }
        result.insert(0, c);
    }
    if n < 0 {
        result.insert(0, '-');
    }
    result
}

/// Apply European formatting to a thousands-separated string.
///
/// This swaps `,` separators to `.` separators.
///
/// # Examples
/// ```
/// use baras_types::formatting::{format_thousands, format_thousands_eu};
/// let s = format_thousands(1_500_000);
/// assert_eq!(format_thousands_eu(&s, false), "1,500,000");
/// assert_eq!(format_thousands_eu(&s, true), "1.500.000");
/// ```
pub fn format_thousands_eu(s: &str, european: bool) -> String {
    if european {
        // Thousands-separated strings only have `,` as separators, no `.`
        // so a simple replace is safe.
        s.replace(',', ".")
    } else {
        s.to_string()
    }
}

/// Format a percentage value with 1 decimal place.
///
/// # Examples
/// ```
/// use baras_types::formatting::format_pct;
/// assert_eq!(format_pct(42.7, false), "42.7%");
/// assert_eq!(format_pct(42.7, true), "42,7%");
/// ```
pub fn format_pct(n: f64, european: bool) -> String {
    maybe_eu(format!("{:.1}%", n), european)
}

/// Format a percentage from count/total.
///
/// Returns `"0%"` if total is zero.
///
/// # Examples
/// ```
/// use baras_types::formatting::format_pct_ratio;
/// assert_eq!(format_pct_ratio(3, 10, false), "30.0%");
/// assert_eq!(format_pct_ratio(3, 10, true), "30,0%");
/// assert_eq!(format_pct_ratio(0, 0, false), "0%");
/// ```
pub fn format_pct_ratio(count: i64, total: i64, european: bool) -> String {
    if total == 0 {
        return "0%".to_string();
    }
    format_pct(count as f64 / total as f64 * 100.0, european)
}

/// Format a percentage value with 1 decimal place (f32 input).
///
/// # Examples
/// ```
/// use baras_types::formatting::format_pct_f32;
/// assert_eq!(format_pct_f32(42.7, false), "42.7%");
/// assert_eq!(format_pct_f32(42.7, true), "42,7%");
/// ```
pub fn format_pct_f32(n: f32, european: bool) -> String {
    maybe_eu(format!("{:.1}%", n), european)
}

/// Format a decimal number with the specified precision.
///
/// # Examples
/// ```
/// use baras_types::formatting::format_decimal;
/// assert_eq!(format_decimal(3.5, 1, false), "3.5");
/// assert_eq!(format_decimal(3.5, 1, true), "3,5");
/// assert_eq!(format_decimal(1.234, 3, false), "1.234");
/// assert_eq!(format_decimal(1.234, 3, true), "1,234");
/// ```
pub fn format_decimal(n: f32, precision: usize, european: bool) -> String {
    maybe_eu(format!("{:.prec$}", n, prec = precision), european)
}

/// Format a decimal f64 number with the specified precision.
///
/// # Examples
/// ```
/// use baras_types::formatting::format_decimal_f64;
/// assert_eq!(format_decimal_f64(1.234, 3, false), "1.234");
/// assert_eq!(format_decimal_f64(1.234, 3, true), "1,234");
/// ```
pub fn format_decimal_f64(n: f64, precision: usize, european: bool) -> String {
    maybe_eu(format!("{:.prec$}", n, prec = precision), european)
}

/// Format a f32 with 1 decimal place.
///
/// Convenience wrapper for common `{:.1}` pattern (e.g., APM).
///
/// # Examples
/// ```
/// use baras_types::formatting::format_f32_1;
/// assert_eq!(format_f32_1(3.5, false), "3.5");
/// assert_eq!(format_f32_1(3.5, true), "3,5");
/// ```
pub fn format_f32_1(n: f32, european: bool) -> String {
    format_decimal(n, 1, european)
}

/// Format a countdown/timer value for overlay display.
///
/// - Values >= 60s: `M:SS` (no decimal, no swap needed)
/// - Values >= 10s: whole seconds (no decimal, no swap needed)
/// - Values < 10s: one decimal place (needs european swap)
/// - Values <= 0: returns the provided `zero_label`
///
/// The `suffix` parameter appends a string (e.g., `"s"`) to the formatted value.
///
/// # Examples
/// ```
/// use baras_types::formatting::format_countdown;
/// assert_eq!(format_countdown(75.3, "", "0:00", false), "1:15");
/// assert_eq!(format_countdown(15.7, "", "0:00", false), "16");
/// assert_eq!(format_countdown(3.5, "", "0:00", false), "3.5");
/// assert_eq!(format_countdown(3.5, "", "0:00", true), "3,5");
/// assert_eq!(format_countdown(3.5, "s", "Ready", false), "3.5s");
/// assert_eq!(format_countdown(0.0, "", "0:00", false), "0:00");
/// assert_eq!(format_countdown(0.0, "s", "Ready", false), "Ready");
/// ```
pub fn format_countdown(secs: f32, suffix: &str, zero_label: &str, european: bool) -> String {
    if secs <= 0.0 {
        return zero_label.to_string();
    }
    if secs >= 60.0 {
        let mins = (secs / 60.0).floor() as u32;
        let remaining_secs = (secs % 60.0).floor() as u32;
        format!("{}:{:02}", mins, remaining_secs)
    } else if secs >= 10.0 {
        format!("{:.0}{}", secs, suffix)
    } else {
        maybe_eu(format!("{:.1}{}", secs, suffix), european)
    }
}

/// Format a countdown for compact display (minutes as `Xm`).
///
/// Used by effects_ab and dot_tracker overlays.
///
/// - Values >= 60s: `Xm`
/// - Values >= 10s: whole seconds
/// - Values < 10s: one decimal place
/// - Values <= 0: returns the provided `zero_label`
///
/// # Examples
/// ```
/// use baras_types::formatting::format_countdown_compact;
/// assert_eq!(format_countdown_compact(75.3, "0", false), "1m");
/// assert_eq!(format_countdown_compact(15.7, "0", false), "16");
/// assert_eq!(format_countdown_compact(3.5, "0", false), "3.5");
/// assert_eq!(format_countdown_compact(3.5, "0", true), "3,5");
/// ```
pub fn format_countdown_compact(secs: f32, zero_label: &str, european: bool) -> String {
    if secs <= 0.0 {
        return zero_label.to_string();
    }
    if secs >= 60.0 {
        let mins = (secs / 60.0).floor() as u32;
        format!("{}m", mins)
    } else if secs >= 10.0 {
        format!("{:.0}", secs)
    } else {
        maybe_eu(format!("{:.1}", secs), european)
    }
}

/// Format a duration as `M:SS`.
///
/// # Examples
/// ```
/// use baras_types::formatting::format_duration;
/// assert_eq!(format_duration(125), "2:05");
/// assert_eq!(format_duration(59), "0:59");
/// assert_eq!(format_duration(0), "0:00");
/// ```
pub fn format_duration(secs: i64) -> String {
    let mins = secs / 60;
    let secs = secs % 60;
    format!("{}:{:02}", mins, secs)
}

/// Format a duration from f32 seconds as `M:SS` (rounded).
///
/// # Examples
/// ```
/// use baras_types::formatting::format_duration_f32;
/// assert_eq!(format_duration_f32(125.7), "2:06");
/// assert_eq!(format_duration_f32(59.4), "0:59");
/// ```
pub fn format_duration_f32(secs: f32) -> String {
    let total_secs = secs.round() as i64;
    format_duration(total_secs)
}

/// Format a duration as `M:SS` from u64 seconds.
///
/// # Examples
/// ```
/// use baras_types::formatting::format_duration_u64;
/// assert_eq!(format_duration_u64(125), "2:05");
/// assert_eq!(format_duration_u64(0), "0:00");
/// ```
pub fn format_duration_u64(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// Format a duration from f32 seconds as `M:SS.mmm` with millisecond precision.
///
/// # Examples
/// ```
/// use baras_types::formatting::format_duration_ms;
/// assert_eq!(format_duration_ms(0.486), "0:00.486");
/// assert_eq!(format_duration_ms(65.25), "1:05.250");
/// assert_eq!(format_duration_ms(342.861), "5:42.861");
/// assert_eq!(format_duration_ms(0.0), "0:00.000");
/// ```
pub fn format_duration_ms(secs: f32) -> String {
    let total_ms = (secs * 1000.0).round() as u64;
    let mins = total_ms / 60_000;
    let remaining_ms = total_ms % 60_000;
    let whole_secs = remaining_ms / 1000;
    let ms = remaining_ms % 1000;
    format!("{}:{:02}.{:03}", mins, whole_secs, ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_compact() {
        assert_eq!(format_compact(0, false), "0");
        assert_eq!(format_compact(500, false), "500");
        assert_eq!(format_compact(999, false), "999");
        assert_eq!(format_compact(1_000, false), "1.00K");
        assert_eq!(format_compact(1_500, false), "1.50K");
        assert_eq!(format_compact(9_990, false), "9.99K");
        assert_eq!(format_compact(10_000, false), "10.00K");
        assert_eq!(format_compact(15_000, false), "15.00K");
        assert_eq!(format_compact(999_990, false), "999.99K");
        assert_eq!(format_compact(1_000_000, false), "1.00M");
        assert_eq!(format_compact(1_500_000, false), "1.50M");
    }

    #[test]
    fn test_format_compact_european() {
        assert_eq!(format_compact(500, true), "500");
        assert_eq!(format_compact(1_500, true), "1,50K");
        assert_eq!(format_compact(15_000, true), "15,00K");
        assert_eq!(format_compact(1_500_000, true), "1,50M");
    }

    #[test]
    fn test_format_compact_f64() {
        assert_eq!(format_compact_f64(500.0, false), "500");
        assert_eq!(format_compact_f64(1_500.0, false), "1.50K");
        assert_eq!(format_compact_f64(1_500_000.0, false), "1.50M");
        assert_eq!(format_compact_f64(1_500.0, true), "1,50K");
    }

    #[test]
    fn test_format_thousands() {
        assert_eq!(format_thousands(0), "");
        assert_eq!(format_thousands(500), "500");
        assert_eq!(format_thousands(1_500), "1,500");
        assert_eq!(format_thousands(1_500_000), "1,500,000");
        assert_eq!(format_thousands(-1_500), "-1,500");
    }

    #[test]
    fn test_format_thousands_eu() {
        assert_eq!(format_thousands_eu("1,500,000", false), "1,500,000");
        assert_eq!(format_thousands_eu("1,500,000", true), "1.500.000");
        assert_eq!(format_thousands_eu("500", true), "500");
    }

    #[test]
    fn test_format_pct() {
        assert_eq!(format_pct(42.7, false), "42.7%");
        assert_eq!(format_pct(42.7, true), "42,7%");
        assert_eq!(format_pct(0.0, false), "0.0%");
        assert_eq!(format_pct(100.0, false), "100.0%");
    }

    #[test]
    fn test_format_pct_ratio() {
        assert_eq!(format_pct_ratio(3, 10, false), "30.0%");
        assert_eq!(format_pct_ratio(3, 10, true), "30,0%");
        assert_eq!(format_pct_ratio(0, 0, false), "0%");
    }

    #[test]
    fn test_format_decimal() {
        assert_eq!(format_decimal(3.5, 1, false), "3.5");
        assert_eq!(format_decimal(3.5, 1, true), "3,5");
        assert_eq!(format_decimal(1.234, 3, false), "1.234");
        assert_eq!(format_decimal(1.234, 3, true), "1,234");
    }

    #[test]
    fn test_format_countdown() {
        assert_eq!(format_countdown(75.3, "", "0:00", false), "1:15");
        assert_eq!(format_countdown(15.7, "", "0:00", false), "16");
        assert_eq!(format_countdown(3.5, "", "0:00", false), "3.5");
        assert_eq!(format_countdown(3.5, "", "0:00", true), "3,5");
        assert_eq!(format_countdown(0.0, "", "0:00", false), "0:00");
        assert_eq!(format_countdown(3.5, "s", "Ready", false), "3.5s");
        assert_eq!(format_countdown(3.5, "s", "Ready", true), "3,5s");
        assert_eq!(format_countdown(0.0, "s", "Ready", false), "Ready");
    }

    #[test]
    fn test_format_countdown_compact() {
        assert_eq!(format_countdown_compact(75.3, "0", false), "1m");
        assert_eq!(format_countdown_compact(15.7, "0", false), "16");
        assert_eq!(format_countdown_compact(3.5, "0", false), "3.5");
        assert_eq!(format_countdown_compact(3.5, "0", true), "3,5");
        assert_eq!(format_countdown_compact(0.0, "0", false), "0");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "0:00");
        assert_eq!(format_duration(59), "0:59");
        assert_eq!(format_duration(60), "1:00");
        assert_eq!(format_duration(125), "2:05");
    }

    #[test]
    fn test_format_duration_f32() {
        assert_eq!(format_duration_f32(125.7), "2:06");
        assert_eq!(format_duration_f32(59.4), "0:59");
    }

    #[test]
    fn test_format_duration_ms() {
        assert_eq!(format_duration_ms(0.0), "0:00.000");
        assert_eq!(format_duration_ms(0.486), "0:00.486");
        assert_eq!(format_duration_ms(65.25), "1:05.250");
        assert_eq!(format_duration_ms(342.861), "5:42.861");
        assert_eq!(format_duration_ms(59.999), "0:59.999");
        assert_eq!(format_duration_ms(60.0), "1:00.000");
    }

    #[test]
    fn test_europeanize() {
        assert_eq!(europeanize("1.50K"), "1,50K");
        assert_eq!(europeanize("42.7%"), "42,7%");
        assert_eq!(europeanize("1,500,000"), "1.500.000");
        assert_eq!(europeanize("500"), "500");
    }
}
