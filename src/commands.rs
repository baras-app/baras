use std::time::Instant;

use crate::app_state::AppState;
use crate::reader::{read_log_file, tail_log_file};
use std::io::Write;
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn parse_file(path: &str, state: Arc<RwLock<AppState>>) {
    let mut s = state.write().await;
    s.set_active_file(path);
    let timer = Instant::now();
    let active_path = s.active_file.as_ref().expect("a");
    let (events, end_pos) = read_log_file(active_path).expect("failed to parse log file {path}");
    let ms = timer.elapsed().as_millis();
    {
        println!("parsed {} events in {}ms", events.len(), ms);
        s.current_byte = Some(end_pos);
        s.events = events.clone();
    }

    let state_clone = Arc::clone(&state);
    let resolved_path = s.active_file.clone().expect("invalid file path");
    drop(s);
    println!("tailing file: {}", resolved_path.to_str().unwrap());
    tokio::spawn(async move {
        tail_log_file(resolved_path, state_clone).await.ok();
    });
}

pub async fn show_settings(state: Arc<RwLock<AppState>>) {
    let s = state.read().await;

    println!(
        "Current file: {}",
        s.active_file
            .as_ref()
            .map_or("No active file".to_string(), |p| p.display().to_string())
    );

    println!(
        "Game session start date: {}",
        s.game_session_date
            .as_ref()
            .map_or("None".to_string(), |d| d.to_string())
    );
}

pub async fn file_info(state: Arc<RwLock<AppState>>) {
    let s = state.read().await;
    println!("total events: {}", s.events.len());
}

pub fn exit() {
    write!(std::io::stdout(), "quitting...").expect("error exiting");
    std::io::stdout().flush().expect("error flushing stdout");
}
