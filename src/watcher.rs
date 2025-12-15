use crate::app_state::AppState;
use crate::commands;
use crate::directory_index::LogFileIndex;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc::{self, Receiver};
use tokio::sync::RwLock;

pub struct DirectoryWatcher {
    _watcher: RecommendedWatcher,
    rx: Receiver<notify::Result<Event>>,
}

impl DirectoryWatcher {
    pub fn new(path: &Path) -> notify::Result<Self> {
        let (tx, rx) = mpsc::channel(100);

        let mut watcher = RecommendedWatcher::new(
            move |res| {
                let _ = tx.blocking_send(res);
            },
            Config::default(),
        )?;

        watcher.watch(path, RecursiveMode::NonRecursive)?;

        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    pub async fn next_event(&mut self) -> Option<notify::Result<Event>> {
        self.rx.recv().await
    }
}

/// Main watcher loop - spawned as a tokio task
pub async fn run_watcher(state: Arc<RwLock<AppState>>) {
    let dir = {
        let s = state.read().await;
        PathBuf::from(&s.config.log_directory)
    };

    if !dir.exists() {
        println!("Warning: Log directory {} does not exist", dir.display());
        return;
    }

    let mut watcher = match DirectoryWatcher::new(&dir) {
        Ok(w) => w,
        Err(e) => {
            println!("Failed to start directory watcher: {}", e);
            return;
        }
    };

    println!("Watching directory: {}", dir.display());

    while let Some(event_result) = watcher.next_event().await {
        match event_result {
            Ok(event) => handle_event(event, Arc::clone(&state)).await,
            Err(e) => println!("Watch error: {}", e),
        }
    }
}

async fn handle_event(event: Event, state: Arc<RwLock<AppState>>) {
    match event.kind {
        EventKind::Create(_) => {
            for path in event.paths {
                if is_combat_log(&path) {
                    handle_new_file(path, Arc::clone(&state)).await;
                }
            }
        }
        EventKind::Remove(_) => {
            for path in event.paths {
                if is_combat_log(&path) {
                    handle_removed_file(path, Arc::clone(&state)).await;
                }
            }
        }
        _ => {}
    }
}

fn is_combat_log(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with("combat_") && n.ends_with(".txt"))
        .unwrap_or(false)
}

async fn handle_new_file(path: PathBuf, state: Arc<RwLock<AppState>>) {
    println!("New log file detected: {}", path.display());

    // Add to index
    {
        let mut s = state.write().await;
        if let Some(index) = &mut s.file_index {
            index.add_file(&path);
        }
    }

    // Auto-switch to new file
    let path_str = path.to_string_lossy().to_string();
    commands::parse_file(&path_str, state).await;
}

async fn handle_removed_file(path: PathBuf, state: Arc<RwLock<AppState>>) {
    let mut s = state.write().await;
    if let Some(index) = &mut s.file_index {
        index.remove_file(&path);
    }
}

/// Initialize the file index and start the watcher
pub async fn init_watcher(state: Arc<RwLock<AppState>>) -> Option<tokio::task::JoinHandle<()>> {
    let dir = {
        let s = state.read().await;
        PathBuf::from(&s.config.log_directory)
    };

    // Build initial index
    match LogFileIndex::build_index(&dir) {
        Ok(index) => {
            let file_count = index.len();
            let newest = index.newest_file().map(|f| f.path.clone());

            {
                let mut s = state.write().await;
                s.file_index = Some(index);
            }

            println!("Indexed {} log files", file_count);

            // Auto-load newest file if available
            if let Some(newest_path) = newest {
                let path_str = newest_path.to_string_lossy().to_string();
                commands::parse_file(&path_str, Arc::clone(&state)).await;
            }
        }
        Err(e) => {
            println!("Failed to build file index: {}", e);
        }
    }

    // Spawn watcher task
    let watcher_state = Arc::clone(&state);
    let handle = tokio::spawn(async move {
        run_watcher(watcher_state).await;
    });

    Some(handle)
}
