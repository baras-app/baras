use crate::service::ServiceCommand;
use baras_core::directory_watcher::DirectoryEvent;
use tracing::{debug, error, info};

/// Convert a DirectoryEvent into a ServiceCommand (if action needed)
pub fn translate_event(event: DirectoryEvent) -> Option<ServiceCommand> {
    match event {
        DirectoryEvent::NewFile(path) => {
            info!(path = %path.display(), "New log file detected");
            Some(ServiceCommand::FileDetected(path))
        }
        DirectoryEvent::FileRemoved(path) => {
            info!(path = %path.display(), "Log file removed");
            Some(ServiceCommand::FileRemoved(path))
        }
        DirectoryEvent::Message(msg) => {
            debug!(message = %msg, "Directory watcher message");
            None
        }
        DirectoryEvent::Error(err) => {
            error!(error = %err, "Directory watcher error");
            None
        }
        DirectoryEvent::DirectoryIndexed { .. } => None,
    }
}
