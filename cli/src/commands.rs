use baras_core::context::resolve_log_path;
use baras_core::file_handler;
use chrono::offset;
use std::io::Write;
use std::path::Path;

use crate::dir_watcher;
use crate::CliContext;

pub async fn parse_file(path: &str, ctx: &CliContext) {
    // Resolve the path using config
    let resolved = {
        let config = ctx.config.read().await;
        resolve_log_path(&config, Path::new(path))
    };

    // Stop any current tailing task
    {
        let mut tasks = ctx.tasks.lock().await;
        if let Some(active_tail) = tasks.log_tail.take() {
            active_tail.abort();
        }
    }

    // Start a new session for this file
    let session_handle = ctx.start_session(resolved.clone()).await;

    // Parse the file
    let result = match file_handler::parse_file(session_handle.clone()).await {
        Ok(r) => r,
        Err(e) => {
            println!("Error parsing file: {}", e);
            return;
        }
    };

    println!(
        "parsed {} events in {}ms",
        result.events_count, result.elapsed_ms
    );

    println!("Beginning file tail: {}", resolved.display());
    let handle = tokio::spawn(async move {
        result.reader.tail_log_file().await.ok();
    });

    ctx.tasks.lock().await.log_tail = Some(handle);
}

pub async fn show_settings(ctx: &CliContext) {
    let session = match ctx.session().await {
        Some(s) => s,
        None => {
            println!("No active session");
            return;
        }
    };
    let s = session.read().await;
    if let Some(cache) = &s.session_cache {
        cache.print_metadata();
    } else {
        println!("No session cache available");
    }
}

pub async fn show_stats(ctx: &CliContext) {
    let session = match ctx.session().await {
        Some(s) => s,
        None => {
            println!("No active session");
            return;
        }
    };
    let s = session.read().await;
    let enc = s
        .session_cache
        .as_ref()
        .and_then(|c| c.last_combat_encounter());

    match enc {
        Some(e) => e.show_dps(),
        None => println!("No combat encounters found"),
    }
}

pub fn exit() {
    write!(std::io::stdout(), "quitting...").expect("error exiting");
    std::io::stdout().flush().expect("error flushing stdout");
}

pub async fn list_files(ctx: &CliContext) {
    let index_guard = ctx.file_index.read().await;
    let index = match &*index_guard {
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

pub async fn delete_old_files(ctx: &CliContext, days: u32) {
    let today = offset::Local::now().date_naive();

    let files_to_delete: Vec<_> = {
        let index_guard = ctx.file_index.read().await;
        let index = match &*index_guard {
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
                let mut index_guard = ctx.file_index.write().await;
                if let Some(index) = &mut *index_guard {
                    index.remove_file(path);
                }
            }
            Err(e) => println!("Failed to delete {}: {}", path.display(), e),
        }
    }

    println!("Deleted {} files", deleted);
}

pub async fn clean_empty_files(ctx: &CliContext) {
    let files_to_delete: Vec<_> = {
        let index_guard = ctx.file_index.read().await;
        let index = match &*index_guard {
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
                let mut index_guard = ctx.file_index.write().await;
                if let Some(index) = &mut *index_guard {
                    index.remove_file(path);
                }
            }
            Err(e) => println!("Failed to delete {}: {}", path.display(), e),
        }
    }

    println!("Deleted {} empty files", deleted);
}

pub async fn set_directory(new_directory: &str, ctx: &CliContext) {
    let filepath = std::path::PathBuf::from(new_directory);
    if !(filepath.exists() && filepath.is_dir()) {
        println!("Update failed. Invalid directory name given.");
        return;
    }

    // Check if already configured to this directory
    {
        let config = ctx.config.read().await;
        if new_directory == config.log_directory {
            println!("Log directory already configured to {}", new_directory);
            return;
        }
    }

    // Abort existing tasks
    {
        let mut tasks = ctx.tasks.lock().await;
        tasks.abort_all().await;
    }

    // Clear session and update config
    ctx.clear_session().await;
    {
        let mut config = ctx.config.write().await;
        config.log_directory = new_directory.to_string();
    }
    {
        let mut index_guard = ctx.file_index.write().await;
        *index_guard = None;
    }

    // Initialize new watcher
    if let Some(handle) = dir_watcher::init_watcher(ctx).await {
        ctx.tasks.lock().await.watcher = Some(handle);
    }
}
