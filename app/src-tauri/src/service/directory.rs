use crate::service::ServiceCommand;
use baras_core::directory_watcher::DirectoryEvent;

/// Convert a DirectoryEvent into a ServiceCommand (if action needed)
pub fn translate_event(event: DirectoryEvent) -> Option<ServiceCommand> {
    match event {
        DirectoryEvent::NewFile(path) => {
            println!("New log file detected: {}", path.display());
            Some(ServiceCommand::FileDetected(path))
        }
        DirectoryEvent::FileRemoved(path) => {
            println!("Log file removed: {}", path.display());
            Some(ServiceCommand::FileRemoved(path))
        }
        DirectoryEvent::Message(msg) => {
            println!("{}", msg);
            None
        }
        DirectoryEvent::Error(err) => {
            eprintln!("Directory watcher error: {}", err);
            None
        }
        DirectoryEvent::DirectoryIndexed { .. } => None,
    }
}
