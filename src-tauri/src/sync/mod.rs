pub mod watcher;
pub mod manager;

use std::path::PathBuf;
use std::sync::Arc;
use crate::sync::manager::SyncManager;

pub struct SyncState {
    pub shared_folder: Option<PathBuf>,
    pub manager: Option<Arc<SyncManager>>,
}

impl SyncState {
    pub fn new() -> Self {
        Self {
            shared_folder: None,
            manager: None,
        }
    }
}
