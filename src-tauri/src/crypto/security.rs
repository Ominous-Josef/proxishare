use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TrustStore {
    pub trusted_devices: HashSet<String>, // Set of device IDs
}

pub struct SecurityService {
    store_path: PathBuf,
    pub trusted_devices: HashSet<String>,
}

impl SecurityService {
    pub fn new(app_dir: PathBuf) -> Self {
        let store_path = app_dir.join("trust_store.json");
        let trusted_devices = if store_path.exists() {
            let content = fs::read_to_string(&store_path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_else(|_| HashSet::new())
        } else {
            HashSet::new()
        };

        Self {
            store_path,
            trusted_devices,
        }
    }

    pub fn is_trusted(&self, device_id: &str) -> bool {
        self.trusted_devices.contains(device_id)
    }

    pub fn add_trusted(&mut self, device_id: String) -> Result<(), Box<dyn std::error::Error>> {
        self.trusted_devices.insert(device_id);
        self.save()
    }

    pub fn remove_trusted(&mut self, device_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.trusted_devices.remove(device_id);
        self.save()
    }

    fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let content = serde_json::to_string(&self.trusted_devices)?;
        fs::write(&self.store_path, content)?;
        Ok(())
    }
}
