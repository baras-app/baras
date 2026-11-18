//! URL opening commands
//!
//! Provides cross-platform URL opening with Linux-specific portal support for
//! better compatibility with immutable distros (Bazzite, Fedora Silverblue, etc.)
//! and sandboxed environments.

use tauri::command;

/// Open a URL in the default browser
///
/// On Linux, this uses the XDG Desktop Portal (OpenURI) for better compatibility
/// with immutable distros and sandboxed environments. Falls back to tauri-plugin-opener
/// on other platforms or if the portal fails.
///
/// On macOS and Windows, this uses tauri-plugin-opener directly.
#[command]
pub async fn open_url(url: String) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        // Try XDG Desktop Portal first (works in Flatpak, immutable distros, etc.)
        match open_url_via_portal(&url).await {
            Ok(_) => {
                tracing::debug!("Opened URL via XDG portal: {}", url);
                return Ok(());
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to open URL via portal ({}), falling back to opener plugin: {}",
                    e,
                    url
                );
                // Fall through to opener plugin fallback
            }
        }
    }

    // Fallback for non-Linux platforms or if portal fails
    // This uses tauri-plugin-opener which calls xdg-open on Linux
    tauri::async_runtime::spawn(async move {
        if let Err(e) = tauri_plugin_opener::open_url(url.clone(), None::<&str>) {
            tracing::error!("Failed to open URL with opener plugin: {} - {}", url, e);
        }
    });

    Ok(())
}

#[cfg(target_os = "linux")]
async fn open_url_via_portal(url: &str) -> Result<(), ashpd::Error> {
    use ashpd::desktop::open_uri::OpenFileRequest;

    // Parse URL and send it via the portal
    let uri = ashpd::Uri::parse(url).map_err(|_| {
        ashpd::Error::Response(ashpd::desktop::ResponseError::Other)
    })?;

    OpenFileRequest::default().send_uri(&uri).await?;

    Ok(())
}
