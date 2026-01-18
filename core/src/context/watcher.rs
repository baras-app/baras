use crate::context::DirectoryIndex;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc::{self, Receiver};
use tokio::time::{Instant, sleep};

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

    pub async fn next_event(&mut self) -> Option<DirectoryEvent> {
        while let Some(event_result) = self.rx.recv().await {
            match event_result {
                Ok(event) => {
                    if let Some(watcher_event) = self.process_event(event).await {
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

    async fn process_event(&mut self, event: Event) -> Option<DirectoryEvent> {
        match event.kind {
            EventKind::Create(_) => {
                for path in event.paths {
                    if is_combat_log(&path) {
                        return Some(self.handle_new_file(path).await);
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

    async fn handle_new_file(&self, path: PathBuf) -> DirectoryEvent {
        const NEW_FILE_TIMEOUT: Duration = Duration::from_secs(60);
        const NEW_FILE_POLL_INTERVAL: Duration = Duration::from_millis(500);

        // Wait for file to have content (DisciplineChanged event)
        let start = Instant::now();
        let mut has_content = false;

        while start.elapsed() < NEW_FILE_TIMEOUT {
            if path.metadata().map(|m| m.len()).unwrap_or(0) > 0 {
                has_content = true;
                break;
            } else {
                // File might still be locked/being created, keep waiting
                sleep(NEW_FILE_POLL_INTERVAL).await;
            }
        }

        if !has_content {
            return DirectoryEvent::Message(format!(
                "Warning: Timed out waiting for content in {}",
                path.display()
            ));
        }

        DirectoryEvent::NewFile(path)
    }
}

fn is_combat_log(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with("combat_") && n.ends_with(".txt"))
        .unwrap_or(false)
}

pub fn build_index(dir: &Path) -> Result<(DirectoryIndex, Option<PathBuf>), String> {
    let index = DirectoryIndex::build_index(dir)
        .map_err(|e| format!("Failed to build file index: {}", e))?;

    let newest = index.newest_file().map(|f| f.path.clone());
    Ok((index, newest))
}
