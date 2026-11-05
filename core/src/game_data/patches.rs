//! Game patch and server lookups for the History tab.
//!
//! The combat log does not carry the patch version (its `<v7.0.0b>` tag is
//! the log format, unchanged since 7.0), so the version is derived from the
//! log date. Keep `PATCHES` sorted ascending; the first day of each patch.

/// `(YYYY-MM-DD, version)`, ascending. Dates from the official patch notes.
const PATCHES: &[(&str, &str)] = &[
    ("2022-02-15", "7.0"),
    ("2022-08-02", "7.1"),
    ("2022-12-13", "7.2"),
    ("2023-06-13", "7.3"),
    ("2023-12-12", "7.4"),
    ("2024-05-29", "7.5"),
    ("2024-12-10", "7.6"),
    ("2025-05-28", "7.7"),
    ("2025-12-09", "7.8"),
    ("2026-06-02", "7.9"),
    // 8.0: holiday 2026 window announced; add its date when it ships
];

/// Patch live on `day` (`YYYY-MM-DD`); empty before 7.0
pub fn game_version_at(day: &str) -> &'static str {
    PATCHES
        .iter()
        .rev()
        .find(|(start, _)| day >= *start)
        .map_or("", |(_, v)| v)
}

/// Server codes from the AreaEntered trailer. The five merge-era servers are
/// confirmed; Shae Vizla (APAC, 2023) and the PTS are not documented yet, so
/// an unknown code is shown as-is until someone logs on it and reports it.
const SERVERS: &[(&str, &str)] = &[
    ("he3000", "Star Forge"),
    ("he3001", "Satele Shan"),
    ("he4000", "Darth Malgus"),
    ("he4001", "Tulak Hord"),
    ("he4002", "The Leviathan"),
];

/// Display name for a server code, e.g. `he3000` → "Star Forge". Unknown
/// codes come back unchanged (still identifies the server, just not by name).
pub fn server_name(code: &str) -> &str {
    SERVERS
        .iter()
        .find(|(c, _)| *c == code)
        .map_or(code, |(_, name)| name)
}
