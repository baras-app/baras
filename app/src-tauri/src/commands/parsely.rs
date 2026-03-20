//! Parsely.io upload commands

use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::write::GzEncoder;
use reqwest::multipart::{Form, Part};
use tauri::State;

use crate::service::ServiceHandle;

const PARSELY_URL: &str = "https://parsely.io/api/upload2";
const USER_AGENT: &str = "BARAS v0.1.0";

/// Extra seconds of trailing log lines to include beyond the encounter's end_line
/// when uploading to Parsely. Ensures late-arriving damage/death events from SWTOR's
/// log buffering are captured for accurate classification.
const PARSELY_TRAILING_SECS: u32 = 5;

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
    visibility: u8,
    notes: Option<String>,
    guild_log: bool,
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

    let mut form = Form::new()
        .part("file", file_part)
        .text("public", visibility.to_string());

    // Add notes if provided
    if let Some(ref note) = notes {
        if !note.is_empty() {
            form = form.text("notes", note.clone());
        }
    }

    let config = handle.config().await;
    if !config.parsely.username.is_empty() && !config.parsely.password.is_empty() {
        form = form.text("username", config.parsely.username.clone());
        form = form.text("password", config.parsely.password.clone());
        if !config.parsely.guild.is_empty() {
            form = form.text("guild", config.parsely.guild.clone());
        }
    }

    if guild_log {
        form = form.text("guild-log", "1");
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

/// Upload a specific encounter (line range) to Parsely.io
#[tauri::command]
pub async fn upload_encounter_to_parsely(
    path: PathBuf,
    start_line: u64,
    end_line: u64,
    area_entered_line: Option<u64>,
    visibility: u8,
    notes: Option<String>,
    guild_log: bool,
    handle: State<'_, ServiceHandle>,
) -> Result<ParselyUploadResponse, String> {
    // Extract and compress the relevant lines
    let compressed = extract_and_compress_lines(&path, start_line, end_line, area_entered_line)
        .map_err(|e| format!("Failed to extract lines: {}", e))?;

    if compressed.is_empty() {
        return Ok(ParselyUploadResponse {
            success: false,
            link: None,
            error: Some("No lines extracted".to_string()),
        });
    }

    // Build filename with encounter info
    let base_filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("combat.txt");
    let filename = format!("{}_lines_{}-{}.txt", base_filename, start_line, end_line);

    let file_part = Part::bytes(compressed)
        .file_name(filename)
        .mime_str("text/html")
        .map_err(|e| format!("Failed to create file part: {}", e))?;

    let mut form = Form::new()
        .part("file", file_part)
        .text("public", visibility.to_string());

    // Add notes if provided
    if let Some(ref note) = notes {
        if !note.is_empty() {
            form = form.text("notes", note.clone());
        }
    }

    let config = handle.config().await;
    if !config.parsely.username.is_empty() && !config.parsely.password.is_empty() {
        form = form.text("username", config.parsely.username.clone());
        form = form.text("password", config.parsely.password.clone());
        if !config.parsely.guild.is_empty() {
            form = form.text("guild", config.parsely.guild.clone());
        }
    }

    if guild_log {
        form = form.text("guild-log", "1");
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

    parse_parsely_response(&response_text)
}

/// Extract specific lines from a file, optionally prepending the area entered line,
/// and gzip compress the result.
///
/// Includes a trailing window of [`PARSELY_TRAILING_SECS`] seconds beyond `end_line`
/// so that late-arriving damage/death events are captured for Parsely classification.
///
/// Note: Log files are Windows-1252 encoded, so we read raw bytes and split by newlines
/// without attempting UTF-8 conversion.
fn extract_and_compress_lines(
    path: &Path,
    start_line: u64,
    end_line: u64,
    area_entered_line: Option<u64>,
) -> std::io::Result<Vec<u8>> {
    use std::io::Read;

    // Read entire file as raw bytes (preserves Windows-1252 encoding)
    let mut file = std::fs::File::open(path)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;

    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    let mut area_line_content: Option<&[u8]> = None;

    // If area_entered_line is before start_line, we need to capture it separately
    let capture_area_line = area_entered_line
        .map(|l| l < start_line)
        .unwrap_or(false);

    // Split by newlines, keeping track of line numbers (1-indexed)
    let mut line_num: u64 = 0;
    let mut start = 0;
    let mut end_line_secs: Option<u32> = None;

    for (i, &byte) in contents.iter().enumerate() {
        if byte == b'\n' {
            line_num += 1;
            let line_end = if i > 0 && contents[i - 1] == b'\r' {
                i - 1 // Strip \r from \r\n
            } else {
                i
            };
            let line = &contents[start..line_end];

            // Capture area entered line if it's before our range
            if capture_area_line && Some(line_num) == area_entered_line {
                area_line_content = Some(line);
            }

            if line_num >= start_line && line_num <= end_line {
                // If we have a captured area line, write it first (once)
                if let Some(area_line) = area_line_content.take() {
                    encoder.write_all(area_line)?;
                    encoder.write_all(b"\n")?;
                }
                encoder.write_all(line)?;
                encoder.write_all(b"\n")?;

                // Capture the timestamp of end_line for the trailing window
                if line_num == end_line {
                    end_line_secs = extract_timestamp_secs(line);
                }
            } else if line_num > end_line {
                // Trailing window: include lines within PARSELY_TRAILING_SECS of end_line
                if let Some(end_secs) = end_line_secs {
                    if let Some(line_secs) = extract_timestamp_secs(line) {
                        // Handle midnight wraparound (line timestamp < end timestamp)
                        let elapsed = if line_secs >= end_secs {
                            line_secs - end_secs
                        } else {
                            break; // Timestamp went backwards, stop
                        };
                        if elapsed <= PARSELY_TRAILING_SECS {
                            encoder.write_all(line)?;
                            encoder.write_all(b"\n")?;
                        } else {
                            break;
                        }
                    } else {
                        break; // Unparseable line, stop
                    }
                } else {
                    break; // No reference timestamp, stop
                }
            }

            start = i + 1;
        }
    }

    // Handle last line if file doesn't end with newline
    if start < contents.len() {
        line_num += 1;
        let line = &contents[start..];

        if capture_area_line && Some(line_num) == area_entered_line {
            area_line_content = Some(line);
        }

        if line_num >= start_line && line_num <= end_line {
            if let Some(area_line) = area_line_content.take() {
                encoder.write_all(area_line)?;
                encoder.write_all(b"\n")?;
            }
            encoder.write_all(line)?;
            encoder.write_all(b"\n")?;
        }
    }

    encoder.finish()
}

/// Extract total seconds from a SWTOR log line timestamp.
/// Format: `[HH:MM:SS.mmm] ...` — returns HH*3600 + MM*60 + SS.
fn extract_timestamp_secs(line: &[u8]) -> Option<u32> {
    // Minimum: [HH:MM:SS.mmm] = 14 bytes, first byte must be '['
    if line.len() < 14 || line[0] != b'[' {
        return None;
    }
    let h = digit2(line[1], line[2])?;
    let m = digit2(line[4], line[5])?;
    let s = digit2(line[7], line[8])?;
    Some(h * 3600 + m * 60 + s)
}

#[inline]
fn digit2(a: u8, b: u8) -> Option<u32> {
    if a.is_ascii_digit() && b.is_ascii_digit() {
        Some((a - b'0') as u32 * 10 + (b - b'0') as u32)
    } else {
        None
    }
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
