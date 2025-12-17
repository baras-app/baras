use tokio::task::JoinHandle;

#[derive(Default)]
pub struct BackgroundTasks {
    pub watcher: Option<JoinHandle<()>>,
    pub log_tail: Option<JoinHandle<()>>,
}

impl BackgroundTasks {
    pub async fn abort_all(&mut self) {
        if let Some(handle) = self.log_tail.take() {
            handle.abort();
        }
        if let Some(handle) = self.watcher.take() {
            handle.abort();
        }
    }
}
