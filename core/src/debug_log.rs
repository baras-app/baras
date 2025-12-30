//! Simple file-based debug logging for development
//!
//! Usage: `debug_log!("message {}", value);`
//! Writes to /tmp/baras-debug.log

use std::fs::OpenOptions;
use std::io::Write;
use std::sync::OnceLock;

static LOG_PATH: &str = "/tmp/baras-debug.log";
static ENABLED: OnceLock<bool> = OnceLock::new();

/// Check if debug logging is enabled (creates fresh log file on first call)
fn is_enabled() -> bool {
    *ENABLED.get_or_init(|| {
        // Truncate log file on startup
        if let Ok(mut f) = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(LOG_PATH)
        {
            let _ = writeln!(f, "=== BARAS Debug Log Started ===");
            true
        } else {
            false
        }
    })
}

/// Write a line to the debug log
pub fn log(msg: &str) {
    if !is_enabled() {
        return;
    }
    if let Ok(mut f) = OpenOptions::new().append(true).open(LOG_PATH) {
        let _ = writeln!(f, "{}", msg);
    }
}

/// Debug log macro - use like println!
#[macro_export]
macro_rules! debug_log {
    ($($arg:tt)*) => {
        $crate::debug_log::log(&format!($($arg)*))
    };
}
