use super::error::ReaderError;
use crate::context::ParsingSession;
use crate::{CombatEvent, LogParser};
use encoding_rs::WINDOWS_1252;
use memchr::memchr_iter;
use memmap2::Mmap;
use std::fs;
use std::io::Result;
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};
use tokio::sync::RwLock;
use tokio::time::{Duration, sleep};

const TAIL_SLEEP_DURATION: Duration = Duration::from_millis(30);
const MAX_BACKOFF: Duration = Duration::from_secs(5);

/// Calculate exponential backoff duration based on consecutive error count.
/// Starts at TAIL_SLEEP_DURATION (30ms), doubles each time, caps at MAX_BACKOFF (5s).
fn calculate_backoff(consecutive_errors: u32) -> Duration {
    let base_ms = TAIL_SLEEP_DURATION.as_millis() as u64;
    let multiplier = 2u64.saturating_pow(consecutive_errors.min(10));
    let ms = (base_ms * multiplier).min(MAX_BACKOFF.as_millis() as u64);
    Duration::from_millis(ms)
}

pub struct Reader {
    path: PathBuf,
    state: Arc<RwLock<ParsingSession>>,
}

impl Reader {
    pub fn from(file_path: PathBuf, state: Arc<RwLock<ParsingSession>>) -> Self {
        Reader { path: file_path, state }
    }

    /// Stream-parse log file, calling `on_event` for each parsed event.
    ///
    /// This avoids allocating a giant Vec of all events, keeping memory stable.
    /// Returns the final byte position and event count.
    ///
    /// Note: `session_date` must be passed in to avoid deadlock when caller holds session lock.
    pub fn read_log_file_streaming<F>(&self, session_date: chrono::NaiveDateTime, on_event: F) -> Result<(u64, usize)>
    where
        F: FnMut(CombatEvent),
    {
        self.read_log_file_streaming_from(session_date, 0, 0, on_event)
            .map(|(end_pos, _, event_count)| (end_pos, event_count))
    }

    /// Stream-parse from `start_byte`, numbering lines from `start_line`
    /// (same resume contract as `tail_log_file`: `start_line` is the number
    /// given to the first line read). Returns the final byte position, the
    /// next line number, and the event count. Used to consume a parse-worker
    /// handoff region without going through the live tail loop.
    pub fn read_log_file_streaming_from<F>(
        &self,
        session_date: chrono::NaiveDateTime,
        start_byte: u64,
        start_line: u64,
        mut on_event: F,
    ) -> Result<(u64, u64, usize)>
    where
        F: FnMut(CombatEvent),
    {
        let file = fs::File::open(&self.path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        let end_pos = mmap.len() as u64;
        if start_byte >= end_pos {
            return Ok((end_pos, start_line, 0));
        }
        let bytes = &mmap.as_ref()[start_byte as usize..];

        let parser = LogParser::new(session_date);
        let mut event_count = 0usize;
        let mut line_number = start_line;
        let mut start = 0usize;

        // Parse line by line using memchr for fast newline detection
        for end in memchr_iter(b'\n', bytes) {
            if end > start {
                let (line, _, _) = WINDOWS_1252.decode(&bytes[start..end]);
                if let Some(event) = parser.parse_line(line_number, &line) {
                    on_event(event);
                    event_count += 1;
                }
                line_number += 1;
            }
            start = end + 1;
        }

        // Handle final line without trailing newline
        if start < bytes.len() {
            let (line, _, _) = WINDOWS_1252.decode(&bytes[start..]);
            if let Some(event) = parser.parse_line(line_number, &line) {
                on_event(event);
                event_count += 1;
            }
            line_number += 1;
        }

        Ok((end_pos, line_number, event_count))
    }

    /// Tail a live log file, processing new lines as they are written.
    ///
    /// This method is designed to be "immortal" - it will never exit due to transient
    /// I/O errors. Instead, it logs errors and retries with exponential backoff.
    /// The only way this loop exits is via task abort (when stop_tailing is called)
    /// or if the file cannot be opened/seeked initially.
    pub async fn tail_log_file(self) -> std::result::Result<(), ReaderError> {
        const CRLF: &[u8] = b"\r\n";
        let file = File::open(&self.path).await.map_err(|source| ReaderError::OpenFile {
            path: self.path.clone(),
            source,
        })?;
        let mut reader = BufReader::new(file);
        // Initialize line number from subprocess parse result (if available)
        // This ensures correct line numbering for events processed during tailing
        let mut line_number = self.state.read().await.current_line.unwrap_or(0);
        let pos = self.state.read().await.current_byte.unwrap_or(0);

        let session_date = self
            .state
            .read()
            .await
            .game_session_date
            .ok_or(ReaderError::SessionDateMissing)?;

        reader
            .seek(SeekFrom::Start(pos))
            .await
            .map_err(|source| ReaderError::Seek {
                path: self.path.clone(),
                source,
            })?;

        let parser = LogParser::new(session_date);
        let mut buf = Vec::new();
        let mut consecutive_errors = 0u32;

        // Immortal loop - only exits via task abort
        loop {
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) => {
                    // No new data - tick combat state for wall-clock timeout
                    consecutive_errors = 0; // Not an error, just no data yet
                    self.state.write().await.tick();
                    sleep(TAIL_SLEEP_DURATION).await;
                }
                Ok(_) => {
                    consecutive_errors = 0; // Reset on successful read
                    // Only process if line is complete (ends with CRLF)
                    if buf.ends_with(CRLF) {
                        let (line, _, _) = WINDOWS_1252.decode(&buf);
                        if let Some(event) = parser.parse_line(line_number, &line) {
                            self.state.write().await.process_event(event);
                        }
                        buf.clear();
                        line_number += 1;
                    }
                    // Otherwise keep partial data, next read will append to it
                }
                Err(e) => {
                    // Log the error but NEVER break - let the watcher handle file deletion
                    consecutive_errors += 1;
                    let backoff = calculate_backoff(consecutive_errors);
                    tracing::warn!(
                        error = %e,
                        path = %self.path.display(),
                        line_number,
                        consecutive_errors,
                        backoff_ms = backoff.as_millis() as u64,
                        "Read error during tailing, will retry"
                    );
                    sleep(backoff).await;
                    // Continue - the loop is immortal
                }
            }
        }
    }
}
