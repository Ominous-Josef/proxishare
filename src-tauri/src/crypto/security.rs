use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TrustedDevice {
    pub id: String,
    pub name: String,
    pub last_ip: String,
    pub last_port: u16,
    pub last_seen: i64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TrustStore {
    pub trusted_devices: HashMap<String, TrustedDevice>, // Map of device ID to device metadata
}

pub struct SecurityService {
    store_path: PathBuf,
    pub trusted_devices: HashMap<String, TrustedDevice>,
    my_id: String,
}

impl SecurityService {
    pub fn new(app_dir: PathBuf, my_id: String) -> Self {
        let store_path = app_dir.join("trust_store.json");
        let trusted_devices = if store_path.exists() {
            let content = fs::read_to_string(&store_path).unwrap_or_default();
            serde_json::from_str(&content).unwrap_or_else(|_| HashMap::new())
        } else {
            HashMap::new()
        };

        Self {
            store_path,
            trusted_devices,
            my_id,
        }
    }

    pub fn get_device_id(&self) -> &String {
        &self.my_id
    }

    pub fn is_trusted(&self, device_id: &str) -> bool {
        self.trusted_devices.contains_key(device_id)
    }

    pub fn add_trusted(&mut self, device: TrustedDevice) -> Result<(), Box<dyn std::error::Error>> {
        self.trusted_devices.insert(device.id.clone(), device);
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
