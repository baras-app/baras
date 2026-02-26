use crate::context::DirectoryIndex;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use tokio::sync::mpsc::{self, Receiver};

pub enum DirectoryEvent {
    NewFile(PathBuf),
    /// File was modified (grew in size) - useful for re-checking character on empty files
    FileModified(PathBuf),
    FileRemoved(PathBuf),
    DirectoryIndexed {
        file_count: usize,
        newest: Option<PathBuf>,
    },
    Message(String),
    Error(String),
}

pub struct DirectoryWatcher {
    _watcher: RecommendedWatcher,
    rx: Receiver<notify::Result<Event>>,
}

impl DirectoryWatcher {
    pub fn new(path: &Path) -> notify::Result<Self> {
        let (tx, rx) = mpsc::channel(100);

        let mut watcher = RecommendedWatcher::new(
            move |res| {
                if tx.try_send(res).is_err() {
                    tracing::error!(
                        "Watcher channel full, filesystem event dropped - this should not happen"
                    );
                }
            },
            Config::default(),
        )?;

        watcher.watch(path, RecursiveMode::NonRecursive)?;

        Ok(Self {
            _watcher: watcher,
            rx,
        })
    }

    pub async fn next_event(&mut self) -> Option<DirectoryEvent> {
        while let Some(event_result) = self.rx.recv().await {
            match event_result {
                Ok(event) => {
                    if let Some(watcher_event) = self.process_event(event) {
                        return Some(watcher_event);
                    }
                }
                Err(e) => {
                    return Some(DirectoryEvent::Error(format!(
                        "Directory watcher error: {}",
                        e
                    )));
                }
            }
        }
        None
    }

    /// Process a filesystem event and convert to DirectoryEvent if relevant.
    /// This method is intentionally non-blocking - it immediately returns without
    /// waiting for file content or any other condition.
    fn process_event(&self, event: Event) -> Option<DirectoryEvent> {
        match event.kind {
            EventKind::Create(_) => {
                for path in event.paths {
                    if is_combat_log(&path) {
                        return Some(DirectoryEvent::NewFile(path));
                    }
                }
            }
            EventKind::Modify(_) => {
                // File was modified - emit event so service can re-check character
                // on files that were previously empty or missing character data
                for path in event.paths {
                    if is_combat_log(&path) {
                        tracing::debug!(path = %path.display(), "Log file modified");
                        return Some(DirectoryEvent::FileModified(path));
                    }
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    if is_combat_log(&path) {
                        return Some(DirectoryEvent::FileRemoved(path));
                    }
                }
            }
            _ => {}
        }
        None
    }
}

fn is_combat_log(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with("combat_") && n.ends_with(".txt"))
        .unwrap_or(false)
}

/// Build the index using a disk cache. Only re-opens files that are new or changed.
pub fn build_index_cached(
    dir: &Path,
    cache_path: &Path,
) -> Result<(DirectoryIndex, Option<PathBuf>), String> {
    let index = DirectoryIndex::build_index_cached(dir, cache_path)
        .map_err(|e| format!("Failed to build cached file index: {}", e))?;

    let newest = index.newest_file().map(|f| f.path.clone());
    Ok((index, newest))
}

/// Quick-index only the newest file in the directory.
pub fn build_index_newest(dir: &Path) -> Result<(DirectoryIndex, Option<PathBuf>), String> {
    DirectoryIndex::build_index_newest(dir)
        .map_err(|e| format!("Failed to quick-index newest file: {}", e))
}
