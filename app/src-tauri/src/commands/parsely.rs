//! Parsely.io upload commands

use std::io::Write;
use std::path::PathBuf;

use encoding_rs::WINDOWS_1252;
use flate2::write::GzEncoder;
use flate2::Compression;
use reqwest::multipart::{Form, Part};
use tauri::State;

use crate::service::ServiceHandle;

const PARSELY_URL: &str = "https://parsely.io/api/upload2";
const USER_AGENT: &str = "BARAS v0.1.0";

/// Response from Parsely upload
#[derive(Debug, serde::Serialize)]
pub struct ParselyUploadResponse {
    pub success: bool,
    pub link: Option<String>,
    pub error: Option<String>,
}

/// Upload a log file to Parsely.io
#[tauri::command]
pub async fn upload_to_parsely(
    path: PathBuf,
    handle: State<'_, ServiceHandle>,
) -> Result<ParselyUploadResponse, String> {
    // Read file as bytes (SWTOR logs are Windows-1252 encoded, not UTF-8)
    let file_bytes = std::fs::read(&path)
        .map_err(|e| format!("Failed to read log file: {}", e))?;

    if file_bytes.is_empty() {
        return Ok(ParselyUploadResponse {
            success: false,
            link: None,
            error: Some("File is empty".to_string()),
        });
    }

    // Decode as Windows-1252 for content inspection
    let (log_content, _, had_errors) = WINDOWS_1252.decode(&file_bytes);

    if had_errors {
        return Ok(ParselyUploadResponse {
            success: false,
            link: None,
            error: Some("File appears to be corrupted".to_string()),
        });
    }

    // Check if file has combat data (look for combat log markers)
    let has_combat = log_content.contains("EnterCombat")
        || log_content.contains("ExitCombat")
        || log_content.contains("ApplyEffect");

    if !has_combat {
        return Ok(ParselyUploadResponse {
            success: false,
            link: None,
            error: Some("File has no combat encounters".to_string()),
        });
    }

    // Gzip compress the original bytes (already in Windows-1252)
    let compressed = gzip_compress(&file_bytes)
        .map_err(|e| format!("Failed to compress: {}", e))?;

    // Get filename for the upload
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("combat.txt")
        .to_string();

    // Build multipart form
    let file_part = Part::bytes(compressed)
        .file_name(filename)
        .mime_str("text/html")
        .map_err(|e| format!("Failed to create file part: {}", e))?;

    let mut form = Form::new()
        .part("file", file_part)
        .text("public", "1");

    // Add credentials if configured
    let config = handle.config().await;
    if !config.parsely.username.is_empty() {
        form = form.text("username", config.parsely.username.clone());
        form = form.text("password", config.parsely.password.clone());
        if !config.parsely.guild.is_empty() {
            form = form.text("guild", config.parsely.guild.clone());
        }
    }

    // Send request
    let client = reqwest::Client::new();
    let response = client
        .post(PARSELY_URL)
        .header("User-Agent", USER_AGENT)
        .multipart(form)
        .timeout(std::time::Duration::from_secs(300))
        .send()
        .await
        .map_err(|e| format!("Upload failed: {}", e))?;

    let response_text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    // Parse XML response
    parse_parsely_response(&response_text)
}

/// Gzip compress data
fn gzip_compress(data: &[u8]) -> std::io::Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(data)?;
    encoder.finish()
}

/// Parse Parsely XML response
fn parse_parsely_response(xml: &str) -> Result<ParselyUploadResponse, String> {
    // Check for error status: <status>error</status>
    if xml.contains("<status>error</status>") {
        // Extract error message from <error>...</error>
        let error_msg = extract_xml_element(xml, "error")
            .unwrap_or_else(|| "Unknown error".to_string());
        return Ok(ParselyUploadResponse {
            success: false,
            link: None,
            error: Some(error_msg),
        });
    }

    // Check for legacy error format
    if xml.contains("NOT OK") {
        return Ok(ParselyUploadResponse {
            success: false,
            link: None,
            error: Some("Upload rejected by server".to_string()),
        });
    }

    // Extract link from <file> element
    if let Some(link) = extract_xml_element(xml, "file") {
        return Ok(ParselyUploadResponse {
            success: true,
            link: Some(link),
            error: None,
        });
    }

    Ok(ParselyUploadResponse {
        success: false,
        link: None,
        error: Some(format!("Unexpected response: {}", xml)),
    })
}

/// Extract content from an XML element: <tag>content</tag>
fn extract_xml_element(xml: &str, tag: &str) -> Option<String> {
    let open_tag = format!("<{}>", tag);
    let close_tag = format!("</{}>", tag);

    if let Some(start) = xml.find(&open_tag)
        && let Some(end) = xml.find(&close_tag) {
            let content_start = start + open_tag.len();
            if content_start < end {
                return Some(xml[content_start..end].to_string());
            }
    }
    None
}
