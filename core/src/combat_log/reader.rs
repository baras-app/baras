use crate::context::ParsingSession;
use crate::{CombatEvent, LogParser};
use encoding_rs::WINDOWS_1252;
use memchr::memchr_iter;
use memmap2::Mmap;
use rayon::prelude::*;
use std::fs;
use std::io::Result;
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};
use tokio::sync::RwLock;
use tokio::time::{Duration, sleep};

const TAIL_SLEEP_DURATION: Duration = Duration::from_millis(100);

pub struct Reader {
    path: PathBuf,
    state: Arc<RwLock<ParsingSession>>,
}

impl Reader {
    pub fn from(file_path: PathBuf, state: Arc<RwLock<ParsingSession>>) -> Self {
        Reader {
            path: file_path,
            state,
        }
    }

    // processing full log file, don't want to always write to session cache
    pub async fn read_log_file(&self) -> Result<(Vec<CombatEvent>, u64)> {
        let file = fs::File::open(&self.path)?;
        let mmap = unsafe { Mmap::map(&file)? };
        let bytes = mmap.as_ref();
        let end_pos = bytes.len() as u64;

        // Find all line boundaries
        let mut line_ranges: Vec<(usize, usize)> = Vec::new();
        let mut start = 0;
        for end in memchr_iter(b'\n', bytes) {
            if end > start {
                line_ranges.push((start, end));
            }
            start = end + 1;
        }
        if start < bytes.len() {
            line_ranges.push((start, bytes.len()));
        }
        let date = &self
            .state
            .read()
            .await
            .game_session_date
            .unwrap_or_default();

        let parser = LogParser::new(*date);
        let events: Vec<CombatEvent> = line_ranges
            .par_iter()
            .enumerate()
            .filter_map(|(idx, &(start, end))| {
                let (line, _, _) = WINDOWS_1252.decode(&bytes[start..end]);
                parser.parse_line(idx as u64 + 1, &line)
            })
            .collect();

        Ok((events, end_pos))
    }

    //tailing live log file always write to session cache
    pub async fn tail_log_file(self) -> Result<()> {
        const CRLF: &[u8] = b"\r\n";
        let file = File::open(&self.path).await?;
        let mut reader = BufReader::new(file);
        let mut line_number = 0u64;
        let pos = self.state.read().await.current_byte.unwrap_or(0);

        let session_date = self
            .state
            .read()
            .await
            .game_session_date
            .expect("failed to find game_session_date");

        reader.seek(SeekFrom::Start(pos)).await?;

        let parser = LogParser::new(session_date);
        let mut buf = Vec::new();

        loop {
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) => {
                    sleep(TAIL_SLEEP_DURATION).await;
                    continue;
                }
                Ok(_) => {
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
                Err(_) => break,
            }
        }
        Ok(())
    }
}
