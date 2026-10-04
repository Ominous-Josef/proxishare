use chrono::Utc;
use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

/// How long before a device is considered stale (5 minutes)
/// mDNS doesn't continuously announce, so we need a longer timeout
pub const DEVICE_TIMEOUT_SECS: i64 = 300;

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

use parking_lot::RwLock as SyncRwLock;

const SERVICE_TYPE: &str = "_proxishare._tcp.local.";

/// First 8 characters of a device id, used in mDNS instance names.
/// Never panics, even on short or non-ASCII ids.
pub fn short_id(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

pub fn is_valid_device_id(id: &str) -> bool {
    uuid::Uuid::parse_str(id).is_ok()
}

pub struct DiscoveryService {
    device_id: String,
    device_name: SyncRwLock<String>,
    port: u16,
    mdns: ServiceDaemon,
    discovered_devices: Arc<RwLock<HashMap<String, Device>>>,
    /// Full mDNS name of our registered service, if we're currently discoverable.
    registered: SyncRwLock<Option<String>>,
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
            device_name: SyncRwLock::new(device_name),
            port,
            mdns,
            discovered_devices: Arc::new(RwLock::new(HashMap::new())),
            registered: SyncRwLock::new(None),
        })
    }

    pub fn start_broadcasting(&self) -> Result<(), crate::GenericError> {
        let service_type = SERVICE_TYPE;
        let current_name = self.device_name.read().clone();
        let instance_name = format!("{}_{}", current_name, short_id(&self.device_id));

        // Get all local IPs to register with mDNS
        let local_ips = get_local_ips();
        let ip_str = local_ips.first().cloned().unwrap_or_default();

        println!("[mDNS] Broadcasting on interfaces: {:?}", local_ips);

        let mut properties = HashMap::new();
        properties.insert("id".to_string(), self.device_id.clone());
        properties.insert("name".to_string(), current_name);
        // Store all IPs in properties for cross-interface discovery
        properties.insert("ips".to_string(), local_ips.join(","));

        // Create a safe hostname (no spaces or special chars)
        let safe_hostname: String = instance_name.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
            .collect();
        // Remove consecutive dashes and trailing/leading dashes
        let safe_hostname = safe_hostname.replace("--", "-").trim_matches('-').to_string();

        // Passing the actual IP is crucial for mDNS to advertise correctly on Linux.
        let service_info = ServiceInfo::new(
            service_type,
            &instance_name,
            &format!("{}.local.", safe_hostname),
            &ip_str, 
            self.port,
            Some(properties),
        )?;

        let fullname = service_info.get_fullname().to_string();
        self.mdns.register(service_info)?;
        *self.registered.write() = Some(fullname);
        println!(
            "[mDNS] Service registered: {} on port {}",
            instance_name, self.port
        );
        Ok(())
    }

    /// Unregisters our mDNS service so other devices stop seeing us.
    pub fn stop_broadcasting(&self) {
        if let Some(fullname) = self.registered.write().take() {
            println!("[mDNS] Unregistering service: {}", fullname);
            let _ = self.mdns.unregister(&fullname);
        }
    }

    pub fn set_discoverable(&self, discoverable: bool) -> Result<(), crate::GenericError> {
        let is_registered = self.registered.read().is_some();
        if discoverable && !is_registered {
            self.start_broadcasting()?;
        } else if !discoverable && is_registered {
            self.stop_broadcasting();
        }
        Ok(())
    }

    pub fn update_name(&self, new_name: String) -> Result<(), crate::GenericError> {
        if *self.device_name.read() == new_name {
            return Ok(());
        }

        let was_registered = self.registered.read().is_some();
        self.stop_broadcasting();
        *self.device_name.write() = new_name;
        if was_registered {
            self.start_broadcasting()?;
        }
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
        let service_type = SERVICE_TYPE;
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
                                let id = match info.get_property_val_str("id") {
                                    Some(id) if is_valid_device_id(id) => id.to_string(),
                                    _ => {
                                        println!(
                                            "[mDNS] Ignoring service without a valid id: {}",
                                            info.get_fullname()
                                        );
                                        continue;
                                    }
                                };
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
                                    .find(|(_, d)| name.contains(short_id(&d.id)))
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

        // Periodically re-query so devices that stay up keep refreshing `last_seen`.
        // Stale-device cleanup lives in lib.rs, where the QUIC transport is available.
        let mdns_for_requery = self.mdns.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(REQUERY_INTERVAL_SECS)).await;
                let _ = mdns_for_requery.browse(SERVICE_TYPE);
            }
        });

        Ok(())
    }

    /// Trigger an immediate mDNS scan
    pub fn trigger_scan(&self) {
        println!("[mDNS] Manual scan triggered...");
        let _ = self.mdns.browse(SERVICE_TYPE);
    }

    pub async fn get_devices(&self) -> Vec<Device> {
        let devices = self.discovered_devices.read().await;
        devices.values().cloned().collect()
    }

    pub fn get_my_id(&self) -> String {
        self.device_id.clone()
    }
    
    pub fn get_my_name(&self) -> String {
        self.device_name.read().clone()
    }

    /// Devices not seen for at least `timeout_secs`, as (id, ip, port).
    pub async fn stale_devices(&self, timeout_secs: i64) -> Vec<(String, String, u16)> {
        let now = Utc::now().timestamp();
        self.discovered_devices
            .read()
            .await
            .values()
            .filter(|d| now - d.last_seen >= timeout_secs)
            .map(|d| (d.id.clone(), d.ip.clone(), d.port))
            .collect()
    }

    pub async fn touch(&self, id: &str) {
        if let Some(device) = self.discovered_devices.write().await.get_mut(id) {
            device.last_seen = Utc::now().timestamp();
        }
    }

    pub async fn remove(&self, id: &str) {
        self.discovered_devices.write().await.remove(id);
    }

    /// Adds a device that contacted us directly, but never overwrites one we
    /// already know: otherwise anyone could redirect a known device id to their IP.
    pub async fn add_if_unknown(&self, id: String, name: String, ip: String, port: u16) {
        if !is_valid_device_id(&id) || id == self.device_id {
            return;
        }
        let mut devices = self.discovered_devices.write().await;
        devices.entry(id.clone()).or_insert(Device {
            id,
            name,
            ip: ip.clone(),
            all_ips: vec![ip],
            port,
            last_seen: Utc::now().timestamp(),
        });
    }

    pub async fn add_manual_device(&self, id: String, name: String, ip: String, port: u16) {
        let mut devices = self.discovered_devices.write().await;
        devices.insert(
            id.clone(),
            Device {
                id,
                name,
                ip: ip.clone(),
                all_ips: vec![ip],
                port,
                last_seen: Utc::now().timestamp(),
            },
        );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_id_never_panics() {
        assert_eq!(short_id("0123456789"), "01234567");
        assert_eq!(short_id("unknown"), "unknown");
        assert_eq!(short_id(""), "");
        // A multi-byte character straddling byte 8 must not panic.
        assert_eq!(short_id("abcdefgé"), "abcdefgé");
    }

    #[test]
    fn device_ids_must_be_uuids() {
        assert!(is_valid_device_id("6f1c0f9e-2b7c-4a7e-9d43-1b1f0d3c9a10"));
        assert!(!is_valid_device_id("unknown"));
        assert!(!is_valid_device_id(""));
    }
}
