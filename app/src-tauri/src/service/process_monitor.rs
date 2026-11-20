//! Game process monitor
//!
//! Detects whether the SWTOR game process is currently running. Matches
//! `swtor.exe` and the DX12 test client builds `swtor_RC*.exe` (e.g. `swtor_RC2.exe`).
//! Used by the auto-hide feature to hide overlays when the game closes.
//!
//! Returns `Some(true)` if the process is found, `Some(false)` if the command
//! succeeded and the process is not running, or `None` if the check failed.
//! On failure, the safe default is to assume the game is running (overlays stay visible).

use tracing::warn;

/// Check if the SWTOR game process is currently running.
///
/// - `Some(true)` — process is running
/// - `Some(false)` — command succeeded, process is NOT running
/// - `None` — check failed (command not found, permissions, etc.)
///
/// On failure, callers should assume the game is running (safe default = overlays stay visible).
pub async fn is_game_running() -> Option<bool> {
    #[cfg(target_os = "windows")]
    {
        is_game_running_windows().await
    }
    #[cfg(not(target_os = "windows"))]
    {
        is_game_running_unix().await
    }
}

/// Windows: use `tasklist` to check for `swtor.exe` / `swtor_RC*.exe`
#[cfg(target_os = "windows")]
async fn is_game_running_windows() -> Option<bool> {
    // tasklist only allows a trailing wildcard, so filter broadly on `swtor*` and
    // whitelist exact names below. CSV rows start with `"<image name>"`.
    let output = tokio::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq swtor*", "/FO", "CSV", "/NH"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .creation_flags(0x08000000) // CREATE_NO_WINDOW — prevent console flash
        .output()
        .await;

    match output {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Some(stdout.lines().any(|l| {
                let l = l.to_ascii_lowercase();
                l.starts_with("\"swtor.exe\"") || l.starts_with("\"swtor_rc")
            }))
        }
        Ok(output) => {
            warn!(
                exit_code = ?output.status.code(),
                "tasklist command failed"
            );
            None
        }
        Err(e) => {
            warn!(error = %e, "Failed to spawn tasklist for process monitoring");
            None
        }
    }
}

/// Linux/macOS: use `pgrep` to check for `swtor.exe` / `swtor_RC*.exe`
#[cfg(not(target_os = "windows"))]
async fn is_game_running_unix() -> Option<bool> {
    // `-f` matches the full Wine/Proton command line (e.g. `Z:\...\swtor\retailclient\swtor_RC2.exe`),
    // so anchor to the start of the executable basename rather than the line start.
    let output = tokio::process::Command::new("pgrep")
        .args(["-if", r"(^|[/\\])swtor(_rc[^/\\]*)?\.exe"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .await;

    match output {
        Ok(output) => match output.status.code() {
            Some(0) => Some(true),  // Process found
            Some(1) => Some(false), // No matching process (normal pgrep "not found" exit)
            other => {
                warn!(exit_code = ?other, "pgrep returned unexpected exit code");
                None
            }
        },
        Err(e) => {
            warn!(error = %e, "Failed to spawn pgrep for process monitoring");
            None
        }
    }
}
