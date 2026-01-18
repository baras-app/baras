//! Logging configuration with file-based output and size-based rotation.
//!
//! Writes logs to `~/.config/baras/baras.log` (or platform equivalent) with
//! 10 MB size-based rotation. Set `DEBUG_LOGGING=1` to enable debug output
//! for baras crates.

use rolling_file::{BasicRollingFileAppender, RollingConditionBasic};
use tracing_subscriber::{
    fmt::{self, format::FmtSpan},
    layer::SubscriberExt,
    util::SubscriberInitExt,
    EnvFilter,
};

/// Initialize logging with dual-output (file + stdout).
///
/// Returns a `WorkerGuard` that MUST be held for the application lifetime
/// to ensure all buffered logs are flushed on shutdown.
///
/// # Behavior
/// - **File output:** Always INFO+ level, written to `~/.config/baras/baras.log`
/// - **Stdout output:** INFO+ by default, DEBUG+ for baras crates when `DEBUG_LOGGING=1`
/// - **Rotation:** Size-based at 10 MB, keeps only latest rotated file
///
/// # Fallback
/// If log directory creation fails, returns `None` and falls back to stdout-only logging.
pub fn init() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let debug_logging = std::env::var("DEBUG_LOGGING").is_ok();

    // Get config directory: ~/.config/baras on Linux, %APPDATA%/baras on Windows
    let log_dir = match dirs::config_dir() {
        Some(config) => config.join("baras"),
        None => {
            // Fallback: stdout-only logging
            init_stdout_only(debug_logging);
            return None;
        }
    };

    // Create log directory if needed
    if let Err(e) = std::fs::create_dir_all(&log_dir) {
        // Can't use tracing yet since subscriber not initialized
        eprintln!(
            "Failed to create log directory {:?}: {}, using stdout only",
            log_dir, e
        );
        init_stdout_only(debug_logging);
        return None;
    }

    // Create size-based rolling file appender (10 MB, keep 1 rotated file)
    let log_path = log_dir.join("baras.log");
    let file_appender = match BasicRollingFileAppender::new(
        &log_path,
        RollingConditionBasic::new().max_size(10 * 1024 * 1024), // 10 MB
        1, // Keep only the latest rotated file (baras.log and baras.log.1)
    ) {
        Ok(appender) => appender,
        Err(e) => {
            eprintln!("Failed to create log file at {:?}: {}", log_path, e);
            init_stdout_only(debug_logging);
            return None;
        }
    };

    // Wrap in non-blocking writer for async-safe logging
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // File layer: INFO+ level, no ANSI colors
    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .with_span_events(FmtSpan::NONE);

    // Stdout layer
    let stdout_layer = fmt::layer()
        .with_writer(std::io::stdout)
        .with_target(true)
        .with_span_events(FmtSpan::NONE);

    // Build filter directives based on DEBUG_LOGGING
    let filter_directive = if debug_logging {
        // DEBUG_LOGGING=1: debug for baras crates, info for dependencies
        "info,app_lib=debug,baras_core=debug,baras_overlay=debug"
    } else {
        // Default: INFO+ level for everything
        "info"
    };

    // Single filter for both layers (file always gets same filter to avoid complexity)
    let filter = EnvFilter::new(filter_directive);

    tracing_subscriber::registry()
        .with(file_layer)
        .with(stdout_layer)
        .with(filter)
        .init();

    tracing::info!(
        log_file = ?log_path,
        debug_logging,
        "BARAS logging initialized"
    );

    Some(guard)
}

/// Fallback: Initialize stdout-only logging when file logging fails.
fn init_stdout_only(debug_logging: bool) {
    let stdout_layer = fmt::layer()
        .with_writer(std::io::stdout)
        .with_target(true)
        .with_span_events(FmtSpan::NONE);

    let filter_directive = if debug_logging {
        "info,app_lib=debug,baras_core=debug,baras_overlay=debug"
    } else {
        "info"
    };

    let filter = EnvFilter::new(filter_directive);

    tracing_subscriber::registry()
        .with(stdout_layer)
        .with(filter)
        .init();

    tracing::info!(debug_logging, "BARAS logging initialized (stdout only)");
}
