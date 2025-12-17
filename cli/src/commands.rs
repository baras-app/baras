use baras_core::app_state::AppState;
use baras_core::file_handler;
use chrono::offset;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::dir_watcher;

pub async fn parse_file(path: &str, state: Arc<RwLock<AppState>>) {
    // Set new path
    let mut s = state.write().await;
    s.set_active_file(path);
    let active_path = s.active_file.clone().expect("invalid file given");

    // Stop any current tailing task
    if let Some(active_tail) = s.log_tail_task.take() {
        active_tail.abort();
    }
    drop(s);

    let state_clone = Arc::clone(&state);
    let result = file_handler::parse_file(path, state_clone).await.expect("");

    println!(
        "parsed {} events in {}ms",
        result.events_count, result.elapsed_ms
    );

    println!("Beginning file tail: {}", active_path.display());
    let handle = tokio::spawn(async move {
        result.reader.tail_log_file().await.ok();
    });
    state.write().await.log_tail_task = Some(handle);
}

pub async fn show_settings(state: Arc<RwLock<AppState>>) {
    let s = state.read().await;
    s.session_cache.as_ref().expect("no cache").print_metadata();
}

pub async fn show_stats(state: Arc<RwLock<AppState>>) {
    let s = state.read().await;
    let enc = s
        .session_cache
        .as_ref()
        .unwrap()
        .last_combat_encounter()
        .unwrap();
    enc.show_dps();
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

    println!("{:<50} {:<20} Session", "Character", "Date");
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
    let today = offset::Local::now().date_naive();

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

    println!(
        "Deleting {} files older than {} days...",
        files_to_delete.len(),
        days
    );

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

pub async fn set_directory(new_directory: &str, state: Arc<RwLock<AppState>>) {
    // update state
    let filepath = PathBuf::from(&new_directory);
    if !(filepath.exists() && filepath.is_dir()) {
        println!("Update failed. Invalid directory name given.");
        return;
    }

    {
        let mut s = state.write().await;
        if new_directory == s.config.log_directory {
            println!("Log directory already configured to {}", new_directory);
            return;
        }

        if let Some(log_tail) = &mut s.log_tail_task {
            log_tail.abort();
        }
        if let Some(directory_watcher) = &mut s.watcher_task {
            directory_watcher.abort();
        }
        if let Some(file_index) = &mut s.file_index {
            file_index.empty_files();
        }
        s.session_cache = None;
        s.config.log_directory = new_directory.to_string();
        s.active_file = None;
    }

    //initiate new watcher task
    if let Some(handle) = dir_watcher::init_watcher(Arc::clone(&state)).await {
        state.write().await.watcher_task = Some(handle);
    }
}
