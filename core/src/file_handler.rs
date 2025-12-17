use crate::app_state::AppState;
use crate::reader::Reader;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ParseResult {
    pub events_count: usize,
    pub elapsed_ms: u128,
    pub reader: Reader, // caller can use this to tail
    pub end_pos: u64,
}

pub async fn parse_file(path: &str, state: Arc<RwLock<AppState>>) -> Result<ParseResult, String> {
    let timer = std::time::Instant::now();

    {
        let mut s = state.write().await;
        s.set_active_file(path);
    }

    let active_path = {
        let s = state.read().await;
        s.active_file.clone().ok_or("invalid file given")?
    };

    let reader = Reader::from(active_path, Arc::clone(&state));

    let (events, end_pos) = reader
        .read_log_file()
        .await
        .map_err(|e| format!("failed to parse log file: {}", e))?;

    let events_count = events.len();
    let elapsed_ms = timer.elapsed().as_millis();

    {
        let mut s = state.write().await;
        s.current_byte = Some(end_pos);
        s.process_events(events);
    }

    Ok(ParseResult {
        events_count,
        elapsed_ms,
        reader,
        end_pos,
    })
}
