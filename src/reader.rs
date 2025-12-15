use crate::app_state::AppState;
use crate::{CombatEvent, parse_line};
use memchr::memchr_iter;
use memmap2::Mmap;
use rayon::prelude::*;
use std::fs;
use std::io::Result;
use std::io::SeekFrom;
use std::path::Path;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt, AsyncSeekExt, BufReader};
use tokio::sync::RwLock;
use tokio::time::{Duration, sleep};

// processing full log file, don't want to always write to session cache
pub fn read_log_file<P: AsRef<Path>>(path: P) -> Result<(Vec<CombatEvent>, u64)> {
    let file = fs::File::open(path)?;
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

    let events: Vec<CombatEvent> = line_ranges
        .par_iter()
        .enumerate()
        .filter_map(|(idx, &(start, end))| {
            let line = unsafe { std::str::from_utf8_unchecked(&bytes[start..end]) };
            parse_line(idx as u64 + 1, line)
        })
        .collect();

    Ok((events, end_pos))
}

//tailing live log file always write to session cache
pub async fn tail_log_file<P: AsRef<Path>>(path: P, state: Arc<RwLock<AppState>>) -> Result<()> {
    let file = File::open(&path).await?;
    let mut reader = BufReader::new(file);
    let mut line_number = 0u64;
    let pos = state.read().await.current_byte.unwrap_or(0);

    reader.seek(SeekFrom::Start(pos)).await?;

    let mut line = String::new();

    loop {
        match reader.read_line(&mut line).await {
            Ok(0) => {
                sleep(Duration::from_millis(100)).await;
                continue;
            }
            Ok(_) => {
                if let Some(event) = parse_line(line_number, &line) {
                    state.write().await.process_event(event);
                }
                line.clear();
                line_number += 1;
            }
            Err(_) => break,
        }
    }
    Ok(())
}
