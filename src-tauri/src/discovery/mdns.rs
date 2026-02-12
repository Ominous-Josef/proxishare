use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use chrono::Utc;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub ip: String,
    pub port: u16,
    pub last_seen: i64,
}

pub struct DiscoveryService {
    device_id: String,
    device_name: String,
    port: u16,
    mdns: ServiceDaemon,
    discovered_devices: Arc<RwLock<HashMap<String, Device>>>,
}

impl DiscoveryService {
    pub fn new(device_name: String, port: u16) -> Result<Self, Box<dyn std::error::Error>> {
        let device_id = Uuid::new_v4().to_string();
        let mdns = ServiceDaemon::new()?;

        Ok(Self {
            device_id,
            device_name,
            port,
            mdns,
            discovered_devices: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn start_broadcasting(&self) -> Result<(), Box<dyn std::error::Error>> {
        let service_type = "_proxishare._tcp.local.";
        let instance_name = format!("{}_{}", self.device_name, &self.device_id[..8]);
        
        // In a real app, we'd get the actual local IP. 
        // For now, we'll let mdns-sd handle it or use a placeholder if needed.
        // mdns-sd usually picks up the interface IPs.
        
        let mut properties = HashMap::new();
        properties.insert("id".to_string(), self.device_id.clone());
        properties.insert("name".to_string(), self.device_name.clone());

        let service_info = ServiceInfo::new(
            service_type,
            &instance_name,
            &format!("{}.local.", instance_name),
            "",
            self.port,
            Some(properties),
        )?;

        self.mdns.register(service_info)?;
        Ok(())
    }

    pub fn start_discovery(&self) -> Result<(), Box<dyn std::error::Error>> {
        let service_type = "_proxishare._tcp.local.";
        let receiver = self.mdns.browse(service_type)?;

        let discovered_devices = Arc::clone(&self.discovered_devices);
        let own_device_id = self.device_id.clone();

        tokio::spawn(async move {
            while let Ok(event) = receiver.recv_async().await {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let id = info.get_property_val_str("id").unwrap_or("unknown").to_string();
                        if id == own_device_id {
                            continue;
                        }

                        let name = info.get_property_val_str("name").unwrap_or("Unknown Device").to_string();
                        let ip = info.get_addresses().iter().next().map(|ip| ip.to_string()).unwrap_or_default();
                        let port = info.get_port();

                        let mut devices = discovered_devices.write().await;
                        devices.insert(id.clone(), Device {
                            id,
                            name,
                            ip,
                            port,
                            last_seen: Utc::now().timestamp(),
                        });
                    }
                    ServiceEvent::ServiceRemoved(_type, name) => {
                        // We'd need a way to map instance name back to ID if we want to remove immediately,
                        // or just rely on the poll/timeout. For now, let's keep it simple.
                        println!("Service removed: {}", name);
                    }
                    _ => {}
                }
            }
        });

        Ok(())
    }

    pub async fn get_devices(&self) -> Vec<Device> {
        let devices = self.discovered_devices.read().await;
        devices.values().cloned().collect()
    }

    pub fn get_my_id(&self) -> String {
        self.device_id.clone()
    }
}
