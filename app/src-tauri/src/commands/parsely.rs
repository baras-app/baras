//! Parsely.io upload commands

use std::io::BufReader;
use std::path::PathBuf;

use flate2::Compression;
use flate2::write::GzEncoder;
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
    // Quick metadata check before reading
    let metadata = std::fs::metadata(&path).map_err(|e| format!("Failed to read file: {}", e))?;
    if metadata.len() == 0 {
        return Ok(ParselyUploadResponse {
            success: false,
            link: None,
            error: Some("File is empty".to_string()),
        });
    }

    let compressed = gzip_compress_file(&path).map_err(|e| format!("Failed to compress: {}", e))?;

    // Build Handle
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("combat.txt")
        .to_string();

    let file_part = Part::bytes(compressed)
        .file_name(filename)
        .mime_str("text/html")
        .map_err(|e| format!("Failed to create file part: {}", e))?;

    let mut form = Form::new().part("file", file_part).text("public", "1");

    let config = handle.config().await;
    if !config.parsely.username.is_empty() && !config.parsely.password.is_empty() {
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

fn gzip_compress_file(path: &std::path::Path) -> std::io::Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    std::io::copy(&mut reader, &mut encoder)?;
    encoder.finish()
}

/// Parse Parsely XML response
fn parse_parsely_response(xml: &str) -> Result<ParselyUploadResponse, String> {
    // Check for error status: <status>error</status>
    if xml.contains("<status>error</status>") {
        // Extract error message from <error>...</error>
        let error_msg =
            extract_xml_element(xml, "error").unwrap_or_else(|| "Unknown error".to_string());
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
        && let Some(end) = xml.find(&close_tag)
    {
        let content_start = start + open_tag.len();
        if content_start < end {
            return Some(xml[content_start..end].to_string());
        }
    }
    None
}
