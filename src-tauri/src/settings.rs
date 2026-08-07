use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub device_name: String,
    pub download_dir: String,
    pub is_discoverable: bool,
    pub auto_accept: bool,
    pub theme: String,
}

pub struct SettingsManager {
    file_path: PathBuf,
    settings: Settings,
}

impl SettingsManager {
    pub fn new(app_data_dir: PathBuf, default_device_name: String, default_download_dir: String) -> Self {
        let file_path = app_data_dir.join("settings.json");
        
        let settings = if file_path.exists() {
            match fs::read_to_string(&file_path) {
                Ok(content) => match serde_json::from_str(&content) {
                    Ok(s) => s,
                    Err(_) => Self::default_settings(default_device_name, default_download_dir),
                },
                Err(_) => Self::default_settings(default_device_name, default_download_dir),
            }
        } else {
            Self::default_settings(default_device_name, default_download_dir)
        };
        
        let manager = Self {
            file_path,
            settings,
        };
        
        // Save initial settings if it didn't exist
        manager.save().unwrap_or(());
        
        manager
    }

    fn default_settings(device_name: String, download_dir: String) -> Settings {
        Settings {
            device_name,
            download_dir,
            is_discoverable: true,
            auto_accept: false,
            theme: "system".to_string(),
        }
    }

    pub fn get_settings(&self) -> Settings {
        self.settings.clone()
    }

    pub fn update_settings(&mut self, new_settings: Settings) -> Result<(), String> {
        self.settings = new_settings;
        self.save()
    }

    fn save(&self) -> Result<(), String> {
        let content = serde_json::to_string_pretty(&self.settings).map_err(|e| e.to_string())?;
        fs::write(&self.file_path, content).map_err(|e| e.to_string())?;
        Ok(())
    }
}
