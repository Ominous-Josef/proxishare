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
    /// Fingerprint of the device's key, pinned at pairing. `None` for pairings
    /// made before keys were verified (ProxiShare 1.1 and older).
    #[serde(default)]
    pub fingerprint: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TrustStore {
    pub trusted_devices: HashMap<String, TrustedDevice>, // Map of device ID to device metadata
}

/// How a connection relates to the trust store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerTrust {
    /// Paired, and the peer holds the pinned key.
    Verified,
    /// Paired before keys were pinned; must pair again.
    NeedsRepair,
    /// Paired, but the peer holds a different key: possible impersonation.
    KeyMismatch,
    /// Not paired.
    Unknown,
}

impl PeerTrust {
    /// Message for rejecting anything that isn't `Verified`.
    pub fn rejection(&self) -> &'static str {
        match self {
            PeerTrust::Verified => "",
            PeerTrust::NeedsRepair => "Device needs to be paired again",
            PeerTrust::KeyMismatch => "Device key does not match the paired device",
            PeerTrust::Unknown => "Device not trusted",
        }
    }
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

    /// Checks a claimed device id against the key the peer proved it holds.
    pub fn verify_peer(&self, device_id: &str, fingerprint: &str) -> PeerTrust {
        match self.trusted_devices.get(device_id) {
            None => PeerTrust::Unknown,
            Some(device) => match &device.fingerprint {
                None => PeerTrust::NeedsRepair,
                Some(pinned) if pinned == fingerprint => PeerTrust::Verified,
                Some(_) => PeerTrust::KeyMismatch,
            },
        }
    }

    /// Pairing state of a device, without a connection (for the UI).
    pub fn pairing_state(&self, device_id: &str) -> PeerTrust {
        match self.trusted_devices.get(device_id) {
            None => PeerTrust::Unknown,
            Some(device) if device.fingerprint.is_none() => PeerTrust::NeedsRepair,
            Some(_) => PeerTrust::Verified,
        }
    }

    pub fn pinned_fingerprint(&self, device_id: &str) -> Option<&str> {
        self.trusted_devices
            .get(device_id)
            .and_then(|d| d.fingerprint.as_deref())
    }

    /// Whether pairing `device_id` with this key is allowed. A device that is
    /// already pinned to a different key must be forgotten first, so a key
    /// change is always a deliberate user action.
    pub fn can_pair(&self, device_id: &str, fingerprint: &str) -> Result<(), &'static str> {
        match self.verify_peer(device_id, fingerprint) {
            PeerTrust::KeyMismatch => Err(
                "This device is already paired with a different key. Forget it first if it was reinstalled.",
            ),
            _ => Ok(()),
        }
    }

    pub fn add_trusted(&mut self, device: TrustedDevice) -> Result<(), Box<dyn std::error::Error>> {
        self.trusted_devices.insert(device.id.clone(), device);
        self.save()
    }

    pub fn remove_trusted(&mut self, device_id: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.trusted_devices.remove(device_id);
        self.save()
    }

    /// Records a fresh address for a verified device.
    pub fn update_address(&mut self, device_id: &str, ip: &str, port: u16) {
        if let Some(device) = self.trusted_devices.get_mut(device_id) {
            if device.last_ip != ip || device.last_port != port {
                device.last_ip = ip.to_string();
                device.last_port = port;
                device.last_seen = chrono::Utc::now().timestamp();
                let _ = self.save();
            }
        }
    }

    fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let content = serde_json::to_string(&self.trusted_devices)?;
        fs::write(&self.store_path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(json: &str) -> SecurityService {
        let dir = std::env::temp_dir().join(format!("proxishare-trust-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("trust_store.json"), json).unwrap();
        SecurityService::new(dir, "me".into())
    }

    const STORE: &str = r#"{
        "legacy": {"id":"legacy","name":"Old","last_ip":"10.0.0.2","last_port":51731,"last_seen":0},
        "pinned": {"id":"pinned","name":"New","last_ip":"10.0.0.3","last_port":51731,"last_seen":0,"fingerprint":"abc"}
    }"#;

    #[test]
    fn legacy_pairings_load_and_need_repair() {
        let s = service(STORE);
        assert_eq!(s.trusted_devices.len(), 2);
        assert_eq!(s.verify_peer("legacy", "anything"), PeerTrust::NeedsRepair);
        assert_eq!(s.pairing_state("legacy"), PeerTrust::NeedsRepair);
    }

    #[test]
    fn pinned_key_must_match() {
        let s = service(STORE);
        assert_eq!(s.verify_peer("pinned", "abc"), PeerTrust::Verified);
        assert_eq!(s.verify_peer("pinned", "evil"), PeerTrust::KeyMismatch);
        assert_eq!(s.verify_peer("stranger", "abc"), PeerTrust::Unknown);
    }

    #[test]
    fn key_change_requires_forgetting_first() {
        let s = service(STORE);
        assert!(s.can_pair("pinned", "evil").is_err());
        assert!(s.can_pair("pinned", "abc").is_ok());
        assert!(s.can_pair("legacy", "anything").is_ok());
        assert!(s.can_pair("stranger", "anything").is_ok());
    }
}
