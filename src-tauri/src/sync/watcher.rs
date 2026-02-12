use notify::{Watcher, RecursiveMode, Config, Event};
use std::path::PathBuf;
use tokio::sync::mpsc;

pub struct FolderWatcher {
    watcher: notify::RecommendedWatcher,
}

impl FolderWatcher {
    pub fn new(
        path: PathBuf,
        event_tx: mpsc::Sender<Event>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<Event>| {
            if let Ok(event) = res {
                let _ = event_tx.blocking_send(event);
            }
        })?;

        watcher.watch(&path, RecursiveMode::Recursive)?;

        Ok(Self { watcher })
    }
}
