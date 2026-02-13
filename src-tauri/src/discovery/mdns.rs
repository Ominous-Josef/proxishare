use chrono::Utc;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use uuid::Uuid;

/// How long before a device is considered stale (30 seconds)
const DEVICE_TIMEOUT_SECS: i64 = 30;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub ip: String,
    /// All discovered IP addresses for this device (for multi-interface scenarios)
    pub all_ips: Vec<String>,
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

        tauri::async_runtime::spawn(async move {
            while let Ok(event) = receiver.recv_async().await {
                match event {
                    ServiceEvent::ServiceResolved(info) => {
                        let id = info
                            .get_property_val_str("id")
                            .unwrap_or("unknown")
                            .to_string();
                        if id == own_device_id {
                            continue;
                        }

                        let name = info
                            .get_property_val_str("name")
                            .unwrap_or("Unknown Device")
                            .to_string();
                        
                        // Collect all IP addresses for multi-interface support
                        let all_ips: Vec<String> = info
                            .get_addresses()
                            .iter()
                            .map(|ip| ip.to_string())
                            .collect();
                        
                        // Select the best IP (prefer IPv4, then local network ranges)
                        let ip = select_best_ip(info.get_addresses())
                            .unwrap_or_else(|| all_ips.first().cloned().unwrap_or_default());
                        
                        let port = info.get_port();

                        let mut devices = discovered_devices.write().await;
                        devices.insert(
                            id.clone(),
                            Device {
                                id,
                                name,
                                ip,
                                all_ips,
                                port,
                                last_seen: Utc::now().timestamp(),
                            },
                        );
                    }
                    ServiceEvent::ServiceRemoved(_type, name) => {
                        // Remove device when service is explicitly removed
                        let mut devices = discovered_devices.write().await;
                        // Try to find and remove by matching the instance name prefix
                        let id_to_remove: Option<String> = devices.iter()
                            .find(|(_, d)| name.contains(&d.id[..8]))
                            .map(|(id, _)| id.clone());
                        if let Some(id) = id_to_remove {
                            devices.remove(&id);
                            println!("Device removed: {}", name);
                        }
                    }
                    _ => {}
                }
            }
        });

        // Start a background task to clean up stale devices
        let cleanup_devices = Arc::clone(&self.discovered_devices);
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                let now = Utc::now().timestamp();
                let mut devices = cleanup_devices.write().await;
                devices.retain(|_, device| {
                    now - device.last_seen < DEVICE_TIMEOUT_SECS
                });
            }
        });

        Ok(())
    }
    
    /// Test connectivity to a device by attempting a TCP connection
    pub async fn test_connectivity(&self, ip: &str, port: u16) -> bool {
        use std::net::SocketAddr;
        let addr: SocketAddr = match format!("{}:{}", ip, port).parse() {
            Ok(a) => a,
            Err(_) => return false,
        };
        
        match tokio::time::timeout(
            Duration::from_millis(500),
            tokio::net::TcpStream::connect(addr),
        ).await {
            Ok(Ok(_)) => true,
            _ => false,
        }
    }
    
    /// Find a reachable IP for a device from its list of addresses
    pub async fn find_reachable_ip(&self, device: &Device) -> Option<String> {
        // First try the primary IP
        if self.test_connectivity(&device.ip, device.port).await {
            return Some(device.ip.clone());
        }
        
        // Try other IPs
        for ip in &device.all_ips {
            if ip != &device.ip && self.test_connectivity(ip, device.port).await {
                return Some(ip.clone());
            }
        }
        
        None
    }

    pub async fn get_devices(&self) -> Vec<Device> {
        let devices = self.discovered_devices.read().await;
        devices.values().cloned().collect()
    }

    pub fn get_my_id(&self) -> String {
        self.device_id.clone()
    }
}

/// Select the best IP address from a set of addresses
/// Priority: IPv4 private ranges > IPv4 > IPv6 link-local > IPv6
fn select_best_ip(addresses: &std::collections::HashSet<IpAddr>) -> Option<String> {
    let mut ipv4_private: Option<&IpAddr> = None;
    let mut ipv4_other: Option<&IpAddr> = None;
    let mut ipv6_link_local: Option<&IpAddr> = None;
    let mut ipv6_other: Option<&IpAddr> = None;

    for ip in addresses {
        match ip {
            IpAddr::V4(v4) => {
                if v4.is_private() {
                    ipv4_private = Some(ip);
                } else if ipv4_other.is_none() {
                    ipv4_other = Some(ip);
                }
            }
            IpAddr::V6(v6) => {
                // Check for link-local (fe80::/10)
                let segments = v6.segments();
                if segments[0] & 0xffc0 == 0xfe80 {
                    ipv6_link_local = Some(ip);
                } else if ipv6_other.is_none() {
                    ipv6_other = Some(ip);
                }
            }
        }
    }

    // Return in priority order
    ipv4_private
        .or(ipv4_other)
        .or(ipv6_link_local)
        .or(ipv6_other)
        .map(|ip| ip.to_string())
}
