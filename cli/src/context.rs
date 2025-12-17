use baras_core::context::{AppConfig, BackgroundTasks, ParsingSession};
use baras_core::context::DirectoryIndex;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// Shared handle to a parsing session that can be passed to file_handler/reader.
pub type SessionHandle = Arc<RwLock<ParsingSession>>;

/// Holds all shared state for the CLI application.
/// This is a lightweight container - logic lives in the individual state types.
#[derive(Clone)]
pub struct CliContext {
    pub config: Arc<RwLock<AppConfig>>,
    /// The active parsing session. None if no file is loaded.
    /// When a file is loaded, this is swapped with a new SessionHandle.
    session: Arc<RwLock<Option<SessionHandle>>>,
    pub tasks: Arc<Mutex<BackgroundTasks>>,
    pub file_index: Arc<RwLock<Option<DirectoryIndex>>>,
}

impl CliContext {
    pub fn new() -> Self {
        Self {
            config: Arc::new(RwLock::new(AppConfig::load())),
            session: Arc::new(RwLock::new(None)),
            tasks: Arc::new(Mutex::new(BackgroundTasks::default())),
            file_index: Arc::new(RwLock::new(None)),
        }
    }

    /// Start a new parsing session for the given file path.
    /// Returns the session handle to pass to file_handler.
    pub async fn start_session(&self, path: PathBuf) -> SessionHandle {
        let session = ParsingSession::new(path);
        let handle = Arc::new(RwLock::new(session));
        *self.session.write().await = Some(Arc::clone(&handle));
        handle
    }

    /// Get the current session handle, if one exists.
    pub async fn session(&self) -> Option<SessionHandle> {
        self.session.read().await.clone()
    }

    /// Clear the current session.
    pub async fn clear_session(&self) {
        *self.session.write().await = None;
    }
}
