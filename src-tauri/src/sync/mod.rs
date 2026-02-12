pub mod watcher;
pub mod manager;

pub struct SyncState {
    pub shared_folder: Option<std::path::PathBuf>,
}
