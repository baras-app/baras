use std::time::Instant;

use crate::app_state::AppState;
use crate::reader::Reader;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn parse_file(path: &str, state: Arc<RwLock<AppState>>) {
    let timer = Instant::now();
    let mut s = state.write().await;
    s.set_active_file(path);
    let active_path = s.active_file.clone().expect("invalid file given");

    let state_clone = Arc::clone(&state);
    if let Some(active_tail) = s.log_tail_task.take() {
        active_tail.abort();
    }

    drop(s);
    let reader = Reader::from(active_path.clone(), state_clone);

    let (events, end_pos) = reader
        .read_log_file()
        .expect("failed to parse log file {path}");

    println!(
        "parsed {} events in {}ms",
        events.len(),
        timer.elapsed().as_millis()
    );

    {
        let mut s = state.write().await;
        s.current_byte = Some(end_pos);
        s.process_events(events);
    }

    println!("tailing file: {}", active_path.display());
    let handle = tokio::spawn(async move {
        reader.tail_log_file().await.ok();
    });
    state.write().await.log_tail_task = Some(handle);
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
