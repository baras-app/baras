use crate::commands;
use crate::CliContext;
use baras_core::directory_watcher::{self as core_watcher, DirectoryEvent, DirectoryWatcher};
use std::path::PathBuf;
use tokio::task::JoinHandle;

/// Initialize the file index and start the watcher
pub async fn init_watcher(ctx: &CliContext) -> Option<JoinHandle<()>> {
    let dir = {
        let config = ctx.config.read().await;
        PathBuf::from(&config.log_directory)
    };

    if !dir.exists() {
        println!("Warning: Log directory {} does not exist", dir.display());
        return None;
    }

    // Build initial index using core
    match core_watcher::build_index(&dir) {
        Ok((index, newest)) => {
            let file_count = index.len();

            {
                let mut index_guard = ctx.file_index.write().await;
                *index_guard = Some(index);
            }

            println!("Indexed {} log files", file_count);

            // Auto-load newest file if available
            if let Some(newest_path) = newest {
                let path_str = newest_path.to_string_lossy().to_string();
                commands::parse_file(&path_str, ctx).await;
            }
        }
        Err(e) => {
            println!("{}", e);
        }
    }

    // Create watcher
    let mut watcher = match DirectoryWatcher::new(&dir) {
        Ok(w) => w,
        Err(e) => {
            println!("Failed to start directory watcher: {}", e);
            return None;
        }
    };

    println!("Watching directory: {}", dir.display());

    // Clone context for the spawned task
    let watcher_ctx = ctx.clone();
    let handle = tokio::spawn(async move {
        while let Some(event) = watcher.next_event().await {
            handle_watcher_event(event, &watcher_ctx).await;
        }
    });

    Some(handle)
}

async fn handle_watcher_event(event: DirectoryEvent, ctx: &CliContext) {
    match event {
        DirectoryEvent::NewFile(path) => {
            println!("New log file detected: {}", path.display());

            // Add to index
            let is_latest_file = {
                let mut index_guard = ctx.file_index.write().await;
                if let Some(index) = &mut *index_guard {
                    index.add_file(&path);
                    index.newest_file().map(|f| f.path == path).unwrap_or(false)
                } else {
                    false
                }
            };

            if is_latest_file {
                let path_str = path.to_string_lossy().to_string();
                commands::parse_file(&path_str, ctx).await;
            }
        }

        DirectoryEvent::FileRemoved(path) => {
            let next_file = {
                // Remove from index
                {
                    let mut index_guard = ctx.file_index.write().await;
                    if let Some(index) = &mut *index_guard {
                        index.remove_file(&path);
                    }
                }

                // Check if removed file was the active file
                let was_active = {
                    if let Some(session) = ctx.session().await {
                        let s = session.read().await;
                        s.active_file.as_ref().map(|p| p == &path).unwrap_or(false)
                    } else {
                        false
                    }
                };

                if was_active {
                    // Abort tail task
                    {
                        let mut tasks = ctx.tasks.lock().await;
                        if let Some(tail) = tasks.log_tail.take() {
                            tail.abort();
                        }
                    }

                    // Clear session
                    ctx.clear_session().await;

                    // Get newest file to switch to
                    let index_guard = ctx.file_index.read().await;
                    index_guard
                        .as_ref()
                        .and_then(|idx| idx.newest_file())
                        .map(|f| f.path.clone())
                } else {
                    None
                }
            };

            // Switch to new file outside of lock
            if let Some(new_path) = next_file {
                println!("Active file removed, switching to: {}", new_path.display());
                let path_str = new_path.to_string_lossy().to_string();
                commands::parse_file(&path_str, ctx).await;
            }
        }

        DirectoryEvent::Message(msg) => {
            println!("{}", msg);
        }

        DirectoryEvent::Error(err) => {
            println!("Error: {}", err);
        }

        DirectoryEvent::DirectoryIndexed { .. } => {
            // Handled during init, not expected here
        }
    }
}
