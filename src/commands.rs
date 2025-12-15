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

pub async fn list_files(state: Arc<RwLock<AppState>>) {
    let s = state.read().await;
    let index = match &s.file_index {
        Some(idx) => idx,
        None => {
            println!("No file index available");
            return;
        }
    };

    if index.is_empty() {
        println!("No log files found");
        return;
    }

    println!("{:<50} {:<20} {}", "Character", "Date", "Session");
    println!("{}", "-".repeat(80));

    for entry in index.entries() {
        let char_name = entry.character_name.as_deref().unwrap_or("Unknown");
        let empty_marker = if entry.is_empty { " (empty)" } else { "" };
        println!(
            "{:<50} {:<20} {}{}",
            char_name, entry.date, entry.session_number, empty_marker
        );
    }

    println!("\nTotal: {} files", index.len());
}

pub async fn delete_old_files(state: Arc<RwLock<AppState>>, days: u32) {
    let today = time::OffsetDateTime::now_utc().date();

    let files_to_delete: Vec<_> = {
        let s = state.read().await;
        let index = match &s.file_index {
            Some(idx) => idx,
            None => {
                println!("No file index available");
                return;
            }
        };

        index
            .entries_older_than(days, today)
            .iter()
            .map(|e| e.path.clone())
            .collect()
    };

    if files_to_delete.is_empty() {
        println!("No files older than {} days", days);
        return;
    }

    println!("Deleting {} files older than {} days...", files_to_delete.len(), days);

    let mut deleted = 0;
    for path in &files_to_delete {
        match std::fs::remove_file(path) {
            Ok(_) => {
                deleted += 1;
                let mut s = state.write().await;
                if let Some(index) = &mut s.file_index {
                    index.remove_file(path);
                }
            }
            Err(e) => println!("Failed to delete {}: {}", path.display(), e),
        }
    }

    println!("Deleted {} files", deleted);
}

pub async fn clean_empty_files(state: Arc<RwLock<AppState>>) {
    let files_to_delete: Vec<_> = {
        let s = state.read().await;
        let index = match &s.file_index {
            Some(idx) => idx,
            None => {
                println!("No file index available");
                return;
            }
        };

        index.empty_files().iter().map(|e| e.path.clone()).collect()
    };

    if files_to_delete.is_empty() {
        println!("No empty files to delete");
        return;
    }

    println!("Deleting {} empty files...", files_to_delete.len());

    let mut deleted = 0;
    for path in &files_to_delete {
        match std::fs::remove_file(path) {
            Ok(_) => {
                deleted += 1;
                let mut s = state.write().await;
                if let Some(index) = &mut s.file_index {
                    index.remove_file(path);
                }
            }
            Err(e) => println!("Failed to delete {}: {}", path.display(), e),
        }
    }

    println!("Deleted {} empty files", deleted);
}
