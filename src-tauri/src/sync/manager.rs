use std::path::{Path, PathBuf};
use std::fs;
use std::time::SystemTime;
use crate::transfer::protocol::FileMetadata;

pub struct SyncManager {
    shared_folder: PathBuf,
    backup_folder: PathBuf,
}

impl SyncManager {
    pub fn new(shared_folder: PathBuf) -> Self {
        let backup_folder = shared_folder.join(".proxishare/backups");
        if !backup_folder.exists() {
            let _ = fs::create_dir_all(&backup_folder);
        }
        Self { shared_folder, backup_folder }
    }

    /// Resolves a conflict between a local file and a remote modification.
    /// Returns true if the remote change should overwrite the local file.
    pub fn should_overwrite(
        &self,
        relative_path: &Path,
        remote_metadata: &FileMetadata,
        remote_timestamp: u64,
    ) -> bool {
        let local_path = self.shared_folder.join(relative_path);
        if !local_path.exists() {
            return true;
        }

        let local_metadata = match fs::metadata(&local_path) {
            Ok(m) => m,
            Err(_) => return true,
        };

        let local_timestamp = local_metadata
            .modified()
            .unwrap_or(SystemTime::UNIX_EPOCH)
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // LWW Strategy
        remote_timestamp > local_timestamp
    }

    /// Moves a file to the backup directory before it is overwritten.
    pub fn backup_file(&self, relative_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        let source = self.shared_folder.join(relative_path);
        if !source.exists() {
            return Ok(());
        }

        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_secs();

        let filename = source.file_name().ok_or("Invalid filename")?;
        let backup_name = format!("{}.{}.bak", filename.to_string_lossy(), timestamp);
        let destination = self.backup_folder.join(backup_name);

        fs::rename(source, destination)?;
        Ok(())
    }

    /// Renames a conflicting file instead of overwriting.
    pub fn handle_rename_conflict(&self, relative_path: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let original = self.shared_folder.join(relative_path);
        if !original.exists() {
            return Ok(original);
        }

        let extension = original.extension().and_then(|e| e.to_str()).unwrap_or("");
        let file_stem = original.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
        
        let mut index = 1;
        let mut new_path = original.clone();
        
        while new_path.exists() {
            let new_name = if extension.is_empty() {
                format!("{}.conflict.{}", file_stem, index)
            } else {
                format!("{}.conflict.{}.{}", file_stem, index, extension)
            };
            new_path = original.with_file_name(new_name);
            index += 1;
        }

        Ok(new_path)
    }
}
