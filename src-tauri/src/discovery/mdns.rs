use chrono::Utc;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// How long before a device is considered stale (5 minutes)
/// mDNS doesn't continuously announce, so we need a longer timeout
const DEVICE_TIMEOUT_SECS: i64 = 300;

/// How often to re-query for devices (seconds)
const REQUERY_INTERVAL_SECS: u64 = 30;

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
    pub fn new(
        device_id: String,
        device_name: String,
        port: u16,
    ) -> Result<Self, crate::GenericError> {
        let mdns = ServiceDaemon::new()?;

        Ok(Self {
            device_id,
            device_name,
            port,
            mdns,
            discovered_devices: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    pub fn start_broadcasting(&self) -> Result<(), crate::GenericError> {
        let service_type = "_proxishare._tcp.local.";
        let instance_name = format!("{}_{}", self.device_name, &self.device_id[..8]);

        // Get all local IPs to register with mDNS
        let local_ips = get_local_ips();
        let ip_str = local_ips.first().map(|s| s.as_str()).unwrap_or("");

        println!("[mDNS] Broadcasting on interfaces: {:?}", local_ips);

        let mut properties = HashMap::new();
        properties.insert("id".to_string(), self.device_id.clone());
        properties.insert("name".to_string(), self.device_name.clone());
        // Store all IPs in properties for cross-interface discovery
        properties.insert("ips".to_string(), local_ips.join(","));

        let service_info = ServiceInfo::new(
            service_type,
            &instance_name,
            &format!("{}.local.", instance_name),
            ip_str,
            self.port,
            Some(properties),
        )?;

        self.mdns.register(service_info)?;
        println!(
            "[mDNS] Service registered: {} on port {}",
            instance_name, self.port
        );
        Ok(())
    }

    /// Get network diagnostics for troubleshooting
    pub fn get_diagnostics(&self) -> NetworkDiagnostics {
        let interfaces = get_network_interfaces();
        let local_ips = get_local_ips();

        // Determine subnet info
        let subnet_info = if let Some(ip) = local_ips.first() {
            let parts: Vec<&str> = ip.split('.').collect();
            if parts.len() == 4 {
                format!("{}.{}.{}.x", parts[0], parts[1], parts[2])
            } else {
                "Unknown".to_string()
            }
        } else {
            "No network".to_string()
        };

        NetworkDiagnostics {
            interfaces,
            local_ips,
            mdns_port: 5353,
            app_port: self.port,
            subnet_info,
        }
    }

    pub fn start_discovery(&self) -> Result<(), crate::GenericError> {
        let service_type = "_proxishare._tcp.local.";
        let receiver = self.mdns.browse(service_type)?;

        println!(
            "[mDNS] Discovery started, listening for {} services",
            service_type
        );

        let discovered_devices = Arc::clone(&self.discovered_devices);
        let own_device_id = self.device_id.clone();

        tauri::async_runtime::spawn(async move {
            println!("[mDNS] Event loop started");
            loop {
                match receiver.recv_async().await {
                    Ok(event) => {
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

                                // Collect all IP addresses from mDNS response
                                let mut all_ips: Vec<String> = info
                                    .get_addresses()
                                    .iter()
                                    .map(|ip| ip.to_string())
                                    .collect();

                                // Also include IPs from properties (for cross-interface discovery)
                                if let Some(ips_str) = info.get_property_val_str("ips") {
                                    for ip in ips_str.split(',') {
                                        let ip = ip.trim().to_string();
                                        if !ip.is_empty() && !all_ips.contains(&ip) {
                                            all_ips.push(ip);
                                        }
                                    }
                                }

                                // Select the best IP (prefer IPv4, then local network ranges)
                                let ip =
                                    select_best_ip(info.get_addresses()).unwrap_or_else(|| {
                                        all_ips.first().cloned().unwrap_or_default()
                                    });

                                let port = info.get_port();

                                println!(
                                    "[mDNS] Discovered device: {} ({}) - IPs: {:?}",
                                    name, id, all_ips
                                );

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
                                let id_to_remove: Option<String> = devices
                                    .iter()
                                    .find(|(_, d)| name.contains(&d.id[..8]))
                                    .map(|(id, _)| id.clone());
                                if let Some(id) = id_to_remove {
                                    devices.remove(&id);
                                    println!("[mDNS] Device removed: {}", name);
                                }
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        println!("[mDNS] Event loop error: {:?}, continuing...", e);
                        // Small delay before continuing to avoid busy loop on persistent errors
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        });

        // Start a background task to clean up stale devices and re-query
        let cleanup_devices = Arc::clone(&self.discovered_devices);
        let mdns_for_requery = self.mdns.clone();
        tauri::async_runtime::spawn(async move {
            let mut requery_counter = 0u64;
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;

                // Clean up stale devices
                let now = Utc::now().timestamp();
                let mut devices = cleanup_devices.write().await;
                let _before_count = devices.len();
                devices.retain(|id, device| {
                    let keep = now - device.last_seen < DEVICE_TIMEOUT_SECS;
                    if !keep {
                        println!(
                            "[mDNS] Removing stale device: {} (last seen {}s ago)",
                            id,
                            now - device.last_seen
                        );
                    }
                    keep
                });
                drop(devices);

                // Re-query every REQUERY_INTERVAL_SECS to refresh device list
                requery_counter += 10;
                if requery_counter >= REQUERY_INTERVAL_SECS {
                    requery_counter = 0;
                    println!("[mDNS] Re-querying for devices...");
                    // Trigger a new query by browsing again (mdns-sd handles deduplication)
                    let _ = mdns_for_requery.browse("_proxishare._tcp.local.");
                }
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
        )
        .await
        {
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

/// Network interface information for diagnostics
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub ip: String,
    pub is_loopback: bool,
}

/// Get all network interfaces with their IP addresses
pub fn get_network_interfaces() -> Vec<NetworkInterface> {
    let mut interfaces = Vec::new();

    if let Ok(addrs) = if_addrs::get_if_addrs() {
        for iface in addrs {
            // Skip IPv6 for now, focus on IPv4 for discovery
            if let IpAddr::V4(ipv4) = iface.addr.ip() {
                interfaces.push(NetworkInterface {
                    name: iface.name.clone(),
                    ip: ipv4.to_string(),
                    is_loopback: ipv4.is_loopback(),
                });
            }
        }
    }

    interfaces
}

/// Get all local IPv4 addresses (non-loopback)
pub fn get_local_ips() -> Vec<String> {
    get_network_interfaces()
        .into_iter()
        .filter(|iface| !iface.is_loopback)
        .map(|iface| iface.ip)
        .collect()
}

/// Network diagnostics result
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct NetworkDiagnostics {
    pub interfaces: Vec<NetworkInterface>,
    pub local_ips: Vec<String>,
    pub mdns_port: u16,
    pub app_port: u16,
    pub subnet_info: String,
}
