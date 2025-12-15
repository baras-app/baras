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
        s.process_events(events);
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
    s.session_cache.as_ref().expect("no cache").print_metadata();
}

pub async fn session_info(state: Arc<RwLock<AppState>>) {
    let s = state.read().await;

    let enc_state = s
        .session_cache
        .as_ref()
        .unwrap()
        .current_encounter()
        .unwrap()
        .state
        .clone();

    println!("{:?}", enc_state);

    println!(
        "Current Player {:?}",
        s.session_cache
            .as_ref()
            .expect("no session initialized")
            .player
            .id
    );

    println!(
        "Current Area {}",
        s.session_cache
            .as_ref()
            .expect("not sesssion")
            .current_area
            .area_name
    );

    println!("{}", s.session_cache.as_ref().unwrap().session_date);
}

pub fn exit() {
    write!(std::io::stdout(), "quitting...").expect("error exiting");
    std::io::stdout().flush().expect("error flushing stdout");
}
