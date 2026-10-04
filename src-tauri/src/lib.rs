pub mod crypto;
pub mod db;
pub mod discovery;
pub mod pairing;
pub mod settings;
pub mod sync;
pub mod transfer;

use crate::db::{Database, TransferRecord};
use crate::discovery::mdns::{
    get_network_interfaces, Device, DiscoveryService, NetworkDiagnostics, NetworkInterface,
};
use crate::transfer::TransferManager;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tauri::Manager;

use crate::crypto::security::SecurityService;
use crate::settings::{Settings, SettingsManager};
use crate::sync::SyncState;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TransferStatus {
    Pending,
    InProgress,
    Paused,
    Cancelled,
    Completed,
    PartialSuccess,
    Failed,
}

pub type TransferRegistry = Arc<RwLock<HashMap<String, TransferStatus>>>;

/// QUIC port every ProxiShare instance listens on.
pub const APP_PORT: u16 = 51731;
pub type GenericError = Box<dyn std::error::Error + Send + Sync>;

pub struct AppState {
    pub discovery: Arc<RwLock<Option<Arc<DiscoveryService>>>>,
    pub transfer: Arc<RwLock<Option<Arc<TransferManager>>>>,
    pub sync: Arc<RwLock<SyncState>>,
    pub security: Arc<RwLock<SecurityService>>,
    pub database: Arc<RwLock<Option<Database>>>,
    pub transfers: TransferRegistry,
    pub renames: Arc<RwLock<HashMap<String, String>>>,
    pub settings: Arc<RwLock<SettingsManager>>,
    pub pairing: Arc<RwLock<pairing::PairingState>>,
}

#[tauri::command]
async fn start_discovery(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    // The actual start_broadcasting and start_discovery background loops are already 
    // initialized in the setup hook. This command just triggers a fresh manual scan.
    let discovery_lock = state.discovery.read().await;
    if let Some(discovery) = &*discovery_lock {
        discovery.trigger_scan();
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
async fn add_device_manually(ip: String, state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let port = APP_PORT;
    let tm = state
        .transfer
        .read()
        .await
        .clone()
        .ok_or("Transfer manager not initialized")?;

    let reply = tm.ping_device(&ip, port).await.map_err(|e| {
        println!("[mDNS] Failed to connect manually to {}: {:?}", ip, e);
        format!("Could not connect to {}: {}", ip, e)
    })?;
    if !crate::discovery::mdns::is_valid_device_id(&reply.device_id) {
        return Err(format!("{} answered with an invalid device id", ip));
    }
    // A paired device must answer with its pinned key, or it isn't that device.
    if state
        .security
        .read()
        .await
        .pinned_fingerprint(&reply.device_id)
        .is_some_and(|pinned| pinned != reply.fingerprint)
    {
        return Err(format!(
            "{} claims to be a paired device but holds a different key",
            ip
        ));
    }

    if let Some(discovery) = state.discovery.read().await.as_ref() {
        discovery
            .add_manual_device(reply.device_id.clone(), reply.device_name.clone(), ip.clone(), port)
            .await;
        println!(
            "[mDNS] Manually added device: {} ({}) at {}",
            reply.device_name, reply.device_id, ip
        );
    }
    Ok(true)
}

/// A device as the UI sees it: discovery data plus its pairing state.
#[derive(serde::Serialize)]
struct DeviceView {
    #[serde(flatten)]
    device: Device,
    /// "paired", "needs_repair" or "none".
    trust: &'static str,
}

fn trust_label(trust: crate::crypto::security::PeerTrust) -> &'static str {
    use crate::crypto::security::PeerTrust;
    match trust {
        PeerTrust::Verified => "paired",
        PeerTrust::NeedsRepair => "needs_repair",
        PeerTrust::KeyMismatch | PeerTrust::Unknown => "none",
    }
}

#[tauri::command]
async fn get_discovered_devices(state: tauri::State<'_, AppState>) -> Result<Vec<DeviceView>, String> {
    let mut devices = if let Some(ds) = state.discovery.read().await.as_ref() {
        ds.get_devices().await
    } else {
        vec![]
    };

    // Inject offline trusted devices into the discovery response
    let security = state.security.read().await;
    for td in security.trusted_devices.values() {
        // Only add if not already currently discovered (online)
        if !devices.iter().any(|d| d.id == td.id) {
            devices.push(Device {
                id: td.id.clone(),
                name: td.name.clone(),
                ip: td.last_ip.clone(),
                all_ips: vec![td.last_ip.clone()],
                port: td.last_port,
                last_seen: td.last_seen,
            });
        }
    }

    Ok(devices
        .into_iter()
        .map(|device| DeviceView {
            trust: trust_label(security.pairing_state(&device.id)),
            device,
        })
        .collect())
}

#[tauri::command]
async fn send_file(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    device_id: String,
    ip: String,
    port: u16,
    path: String,
) -> Result<(), String> {
    println!("[Command] send_file called: {} to {}:{}", path, ip, port);

    let file_path = PathBuf::from(&path);
    let file_name = file_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Get file size for logging
    let metadata = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    let is_dir = metadata.is_dir();
    let file_size = if is_dir { 0 } else { metadata.len() as i64 }; // size calculated later for dirs

    let transfer_id = uuid::Uuid::new_v4().to_string();

    // Track transfer in registry
    {
        let mut transfers = state.transfers.write().await;
        transfers.insert(transfer_id.clone(), TransferStatus::Pending);
    }

    // Record the transfer start in database
    {
        let db_lock = state.database.read().await;
        if let Some(db) = &*db_lock {
            if let Err(e) = db
                .record_transfer(crate::db::TransferRecordArgs {
                    id: &transfer_id,
                    device_id: &device_id,
                    file_name: &file_name,
                    file_path: &path,
                    total_size: file_size,
                    direction: "send",
                    file_hash: "", // Hash will be calculated during transfer
                    is_dir,
                    folder_manifest: None,
                })
                .await
            {
                println!("[Database] Failed to record transfer: {:?}", e);
            }
        }
    }

    let tm_opt = state.transfer.read().await.clone();
    if let Some(tm) = tm_opt {
        let app_clone = app.clone();
        let db_clone = state.database.clone();
        let transfers_clone = state.transfers.clone();
        
        tokio::spawn(async move {
            let send_result = tm
                .send_file(
                    transfer_id.clone(),
                    device_id.clone(),
                    ip.clone(),
                    port,
                    file_path.clone(),
                    transfers_clone.clone(),
                    is_dir,
                )
                .await
                .map_err(|e| e.to_string());

            // Record what actually happened, as confirmed by the receiver
            {
                let db_lock = db_clone.read().await;
                if let Some(db) = &*db_lock {
                    let update = match &send_result {
                        Ok(result) => {
                            db.update_transfer_status(
                                &transfer_id,
                                result.outcome.as_status(),
                                result.bytes_sent as i64,
                            )
                            .await
                        }
                        Err(e) if e.contains("cancelled") => {
                            db.update_status_only(&transfer_id, "cancelled").await
                        }
                        Err(_) => db.update_status_only(&transfer_id, "failed").await,
                    };
                    if let Err(e) = update {
                        println!("[Database] Failed to update transfer status: {:?}", e);
                    }
                }
            }
            transfers_clone.write().await.remove(&transfer_id);

            // Notify frontend that history changed
            use tauri::Emitter;
            let _ = app_clone.emit("history-updated", ());

            match send_result {
                Ok(result) => println!("[Command] send_file finished: {:?}", result.outcome),
                Err(e) => println!("[Command] send_file failed: {}", e),
            }
        });

        Ok(())
    } else {
        let error_msg = "Transfer manager not initialized".to_string();
        println!("[Command] {}", error_msg);
        Err(error_msg)
    }
}

#[tauri::command]
async fn get_trusted_devices(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    let security = state.security.read().await;
    Ok(security.trusted_devices.keys().cloned().collect())
}

#[tauri::command]
async fn is_device_trusted(
    device_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let security = state.security.read().await;
    Ok(security.pairing_state(&device_id) == crate::crypto::security::PeerTrust::Verified)
}

/// Removes a pairing. History with the device is kept.
#[tauri::command]
async fn forget_device(device_id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state
        .security
        .write()
        .await
        .remove_trusted(&device_id)
        .map_err(|e| e.to_string())?;
    println!("[Pairing] Forgot device {}", device_id);
    Ok(())
}

#[tauri::command]
async fn test_device_connectivity(
    ip: String,
    port: u16,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let tm = state.transfer.read().await.clone();
    match tm {
        // Errors are expected during the 15s background polling; just report unreachable.
        Some(tm) => Ok(tm.ping_device(&ip, port).await.is_ok()),
        None => Ok(false),
    }
}

#[tauri::command]
async fn is_dir(path: String) -> Result<bool, String> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_dir()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
async fn find_reachable_device_ip(
    device_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, String> {
    let discovery = state.discovery.read().await.clone();
    let tm = state.transfer.read().await.clone();

    if let (Some(ds), Some(tm)) = (discovery, tm) {
        let devices = ds.get_devices().await;
        if let Some(device) = devices.iter().find(|d| d.id == device_id) {
            // Primary IP first, then the others; the device must answer with its own key.
            let candidates = std::iter::once(&device.ip)
                .chain(device.all_ips.iter().filter(|ip| *ip != &device.ip));
            for ip in candidates {
                if tm.ping_matches(&device_id, ip, device.port).await {
                    return Ok(Some(ip.clone()));
                }
            }
        }
    }
    Ok(None)
}

#[tauri::command]
async fn get_network_diagnostics(
    state: tauri::State<'_, AppState>,
) -> Result<NetworkDiagnostics, String> {
    let discovery_lock = state.discovery.read().await;
    if let Some(discovery) = &*discovery_lock {
        Ok(discovery.get_diagnostics())
    } else {
        // Return basic diagnostics even without discovery service
        Ok(NetworkDiagnostics {
            interfaces: get_network_interfaces(),
            local_ips: crate::discovery::mdns::get_local_ips(),
            mdns_port: 5353,
            app_port: APP_PORT,
            subnet_info: "Unknown".to_string(),
        })
    }
}

#[tauri::command]
fn get_local_network_interfaces() -> Vec<NetworkInterface> {
    get_network_interfaces()
}

/// Starts pairing with a device. Returns the code to show; the other user must
/// type it. The pairing finishes in the background and emits `pairing-result`.
#[tauri::command]
async fn request_pairing(
    state: tauri::State<'_, AppState>,
    device_id: String,
    ip: String,
    port: u16,
) -> Result<String, String> {
    let tm = state
        .transfer
        .read()
        .await
        .clone()
        .ok_or("Transfer manager not initialized")?;

    let (code, pairing) = tm
        .start_pairing(&device_id, &ip, port)
        .await
        .map_err(|e| e.to_string())?;

    let task = tauri::async_runtime::spawn(async move {
        tm.finish_pairing(pairing).await;
    });
    state
        .pairing
        .write()
        .await
        .add_outgoing(&device_id, task.inner().abort_handle());

    Ok(code)
}

/// Cancels a pairing we started (the user closed the code dialog).
#[tauri::command]
async fn cancel_pairing(device_id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.pairing.write().await.cancel_outgoing(&device_id);
    Ok(())
}

#[tauri::command]
async fn accept_file_offer(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
    new_name: Option<String>,
) -> Result<(), String> {
    if let Some(name) = new_name {
        let name = crate::transfer::session::sanitize_file_name(&name)
            .ok_or("Invalid file name: use a plain name without folders or special path characters")?;
        state.renames.write().await.insert(transfer_id.clone(), name);
    }
    let mut transfers = state.transfers.write().await;
    if let std::collections::hash_map::Entry::Occupied(mut e) = transfers.entry(transfer_id) {
        e.insert(TransferStatus::InProgress);
        Ok(())
    } else {
        Err("Transfer not found".to_string())
    }
}

#[tauri::command]
async fn reject_file_offer(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    let mut transfers = state.transfers.write().await;
    if let std::collections::hash_map::Entry::Occupied(mut e) = transfers.entry(transfer_id) {
        e.insert(TransferStatus::Cancelled);
        Ok(())
    } else {
        Err("Transfer not found".to_string())
    }
}

#[tauri::command]
async fn pause_transfer(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    let mut transfers = state.transfers.write().await;
    if let std::collections::hash_map::Entry::Occupied(mut e) = transfers.entry(transfer_id.clone()) {
        e.insert(TransferStatus::Paused);
        // Sync to DB
        let db_lock = state.database.read().await;
        if let Some(db) = &*db_lock {
            let _ = db.update_status_only(&transfer_id, "paused").await;
        }
        Ok(())
    } else {
        Err("Transfer not found".to_string())
    }
}

#[tauri::command]
async fn resume_transfer(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    let mut transfers = state.transfers.write().await;
    if let std::collections::hash_map::Entry::Occupied(mut e) = transfers.entry(transfer_id.clone()) {
        e.insert(TransferStatus::InProgress);
        // Sync to DB
        let db_lock = state.database.read().await;
        if let Some(db) = &*db_lock {
            let _ = db.update_status_only(&transfer_id, "in_progress").await;
        }
        Ok(())
    } else {
        Err("Transfer not found".to_string())
    }
}

#[tauri::command]
async fn cancel_transfer(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    let mut transfers = state.transfers.write().await;
    if let std::collections::hash_map::Entry::Occupied(mut e) = transfers.entry(transfer_id.clone()) {
        e.insert(TransferStatus::Cancelled);
        // Sync to DB
        let db_lock = state.database.read().await;
        if let Some(db) = &*db_lock {
            let _ = db.update_status_only(&transfer_id, "cancelled").await;
        }
        Ok(())
    } else {
        Err("Transfer not found".to_string())
    }
}

#[tauri::command]
async fn accept_pairing(
    device_id: String,
    code: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let pending = state
        .pairing
        .write()
        .await
        .verify_incoming(&device_id, &code)
        .map_err(|e| e.to_string())?;

    println!(
        "[Pairing] Code confirmed for device {} at {}:{}",
        device_id, pending.ip, pending.port
    );

    // Hand the decision to the task holding the pairing stream and wait for it
    // to finish the handshake (it pins the key once the other side confirms).
    let (done_tx, done_rx) = tokio::sync::oneshot::channel();
    pending
        .responder
        .send(crate::pairing::Decision {
            accepted: true,
            done: done_tx,
        })
        .map_err(|_| "The pairing request is no longer open".to_string())?;

    match tokio::time::timeout(std::time::Duration::from_secs(20), done_rx).await {
        Ok(Ok(Ok(()))) => {}
        Ok(Ok(Err(e))) => return Err(e),
        _ => return Err("Pairing did not complete".to_string()),
    }

    if let Err(e) = push_history(&state, &device_id, pending.ip, APP_PORT).await {
        println!("[Sync] History sync after pairing failed: {}", e);
    }
    Ok(())
}

#[tauri::command]
async fn reject_pairing(device_id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    println!("[Pairing] Rejecting pairing for device: {}", device_id);
    if let Some(pending) = state.pairing.write().await.remove_incoming(&device_id) {
        let (done_tx, _done_rx) = tokio::sync::oneshot::channel();
        let _ = pending.responder.send(crate::pairing::Decision {
            accepted: false,
            done: done_tx,
        });
    }
    Ok(())
}

/// Shares our history with a paired device, limited to transfers with that device.
async fn push_history(
    state: &AppState,
    device_id: &str,
    ip: String,
    port: u16,
) -> Result<(), String> {
    if state.security.read().await.pairing_state(device_id)
        != crate::crypto::security::PeerTrust::Verified
    {
        return Err("Device is not paired".to_string());
    }
    println!(
        "[Sync] Syncing history with device: {} at {}:{}",
        device_id, ip, port
    );

    let records = {
        let db_lock = state.database.read().await;
        let db = db_lock.as_ref().ok_or("Database not initialized")?;
        db.get_device_transfers(device_id, 100, 0)
            .await
            .map_err(|e| e.to_string())?
            .iter()
            .map(|r| r.to_synced())
            .collect()
    };
    let sender_id = state.security.read().await.get_device_id().to_string();

    let tm = state
        .transfer
        .read()
        .await
        .clone()
        .ok_or("Transfer manager not initialized")?;
    tm.send_verified_message(
        device_id,
        &ip,
        port,
        crate::transfer::protocol::MessageType::HistorySync { sender_id, records },
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn set_sync_folder(path: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut sync = state.sync.write().await;
    sync.shared_folder = Some(PathBuf::from(path));
    Ok(())
}

#[tauri::command]
async fn get_sync_status(state: tauri::State<'_, AppState>) -> Result<Option<String>, String> {
    let sync = state.sync.read().await;
    Ok(sync
        .shared_folder
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned()))
}

#[tauri::command]
async fn get_transfer_history(
    state: tauri::State<'_, AppState>,
    limit: Option<i32>,
    offset: Option<i32>,
) -> Result<Vec<TransferRecord>, String> {
    let db_lock = state.database.read().await;
    if let Some(db) = &*db_lock {
        db.get_transfer_history(limit.unwrap_or(100), offset.unwrap_or(0))
            .await
            .map_err(|e| e.to_string())
    } else {
        Ok(vec![])
    }
}

#[tauri::command]
async fn get_device_transfers(
    state: tauri::State<'_, AppState>,
    device_id: String,
    limit: Option<i32>,
    offset: Option<i32>,
) -> Result<Vec<TransferRecord>, String> {
    let db_lock = state.database.read().await;
    if let Some(db) = &*db_lock {
        db.get_device_transfers(&device_id, limit.unwrap_or(50), offset.unwrap_or(0))
            .await
            .map_err(|e| e.to_string())
    } else {
        Ok(vec![])
    }
}

#[tauri::command]
async fn clear_transfer_history(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let db_lock = state.database.read().await;
    if let Some(db) = &*db_lock {
        db.clear_history().await.map_err(|e| e.to_string())
    } else {
        Ok(())
    }
}

#[tauri::command]
async fn get_settings(state: tauri::State<'_, AppState>) -> Result<Settings, String> {
    let settings = state.settings.read().await;
    Ok(settings.get_settings())
}

#[tauri::command]
async fn update_settings(
    settings: Settings,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut settings_manager = state.settings.write().await;
    
    // Check if device name changed to update discovery service
    let old = settings_manager.get_settings();
    settings_manager.update_settings(settings.clone())?;

    let discovery_lock = state.discovery.read().await;
    if let Some(discovery) = &*discovery_lock {
        if old.device_name != settings.device_name {
            if let Err(e) = discovery.update_name(settings.device_name.clone()) {
                println!("[Settings] Failed to update discovery name: {}", e);
            }
        }
        if old.is_discoverable != settings.is_discoverable {
            if let Err(e) = discovery.set_discoverable(settings.is_discoverable) {
                println!("[Settings] Failed to update discoverability: {}", e);
            }
        }
    }
    
    Ok(())
}

/// Drops devices that haven't been seen for a while, unless they still answer
/// a QUIC ping as themselves (with their pinned key, when paired).
fn spawn_stale_device_cleanup(discovery: Arc<DiscoveryService>, tm: Arc<TransferManager>) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            let stale = discovery
                .stale_devices(crate::discovery::mdns::DEVICE_TIMEOUT_SECS)
                .await;
            for (id, ip, port) in stale {
                if tm.ping_matches(&id, &ip, port).await {
                    discovery.touch(&id).await;
                } else {
                    println!("[mDNS] Removing stale device: {}", id);
                    discovery.remove(&id).await;
                }
            }
        }
    });
}

/// Loads this device's key. If the key file is damaged, it is set aside and a
/// new key is made: paired devices will then report a key change and need to
/// forget and re-pair this device, which beats refusing to start.
fn load_identity(
    app_data_dir: &std::path::Path,
) -> Result<crate::crypto::encryption::DeviceIdentity, Box<dyn std::error::Error>> {
    use crate::crypto::encryption::DeviceIdentity;
    match DeviceIdentity::load_or_create(app_data_dir) {
        Ok(identity) => Ok(identity),
        Err(e) => {
            println!("[Identity] {}. Creating a new device key.", e);
            let key_path = app_data_dir.join("identity.key");
            let backup = app_data_dir.join(format!(
                "identity.key.damaged-{}",
                chrono::Utc::now().timestamp()
            ));
            std::fs::rename(&key_path, &backup)?;
            DeviceIdentity::load_or_create(app_data_dir).map_err(|e| e as Box<dyn std::error::Error>)
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            println!("Setup hook started");
            let app_handle = app.handle().clone();
            let default_downloads_dir = app_handle
                .path()
                .download_dir()
                .unwrap_or_else(|_| PathBuf::from("./downloads"))
                .to_string_lossy()
                .to_string();

            // Initialize Transfer Registry (Status tracking)
            let transfers: TransferRegistry = Arc::new(RwLock::new(HashMap::new()));

            // Initialize Security Service
            let app_data_dir = app_handle
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("./data"));
            if !app_data_dir.exists() {
                let _ = std::fs::create_dir_all(&app_data_dir);
            }

            // Reuse the stored id only if it's a valid UUID; otherwise start fresh.
            let device_id_path = app_data_dir.join("device_id.txt");
            let device_id = match std::fs::read_to_string(&device_id_path) {
                Ok(id) if crate::discovery::mdns::is_valid_device_id(id.trim()) => {
                    id.trim().to_string()
                }
                _ => {
                    let id = uuid::Uuid::new_v4().to_string();
                    let _ = std::fs::write(&device_id_path, &id);
                    id
                }
            };

            let default_device_name = hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "ProxiNode".to_string());

            let settings_manager = SettingsManager::new(
                app_data_dir.clone(),
                default_device_name,
                default_downloads_dir,
            );
            let initial_settings = settings_manager.get_settings();
            let settings = Arc::new(RwLock::new(settings_manager));

            let security = Arc::new(RwLock::new(SecurityService::new(app_data_dir.clone(), device_id.clone())));

            // Initialize Database
            let db_path = app_data_dir.join("proxishare.db");
            let database_opt = tauri::async_runtime::block_on(async {
                match Database::new(&db_path).await {
                    Ok(db) => {
                        println!("Database initialized at {:?}", db_path);
                        Some(db)
                    }
                    Err(e) => {
                        println!("Failed to initialize database: {:?}", e);
                        None
                    }
                }
            });
            let database = Arc::new(RwLock::new(database_opt));

            // Initialize Device ID and Name
            let device_name = initial_settings.device_name;

            let renames = Arc::new(RwLock::new(HashMap::new()));
            let pairing = Arc::new(RwLock::new(pairing::PairingState::default()));
            let identity = load_identity(&app_data_dir)?;
            println!("[Identity] Device key fingerprint: {}", identity.fingerprint);

            println!("Initializing services with block_on");
            let (discovery, transfer_manager) = tauri::async_runtime::block_on(async {
                println!("Inside block_on: Initializing TransferManager");
                // Initialize Transfer Manager
                let port = APP_PORT;
                let tm = TransferManager::new(
                    port,
                    &identity,
                    app_handle.clone(),
                    database.clone(),
                    transfers.clone(),
                    device_id.clone(),
                    settings.clone(),
                    security.clone(),
                    renames.clone(),
                    pairing.clone(),
                )?;
                println!("Inside block_on: TransferManager initialized");

                println!("Inside block_on: Initializing DiscoveryService");
                // Initialize Discovery Service
                let ds = DiscoveryService::new(device_id.clone(), device_name.clone(), port)?;
                println!("Inside block_on: DiscoveryService initialized");

                Ok::<(Arc<DiscoveryService>, Arc<TransferManager>), GenericError>((
                    Arc::new(ds),
                    Arc::new(tm),
                ))
            })
            .map_err(|e| e as Box<dyn std::error::Error>)?;

            println!("Starting listening and broadcasting");
            let tm_clone = Arc::clone(&transfer_manager);
            tauri::async_runtime::spawn(async move {
                tm_clone.start_listening().await;
            });

            if initial_settings.is_discoverable {
                if let Err(e) = discovery.start_broadcasting() {
                    println!("Error starting broadcasting: {:?}", e);
                }
            }
            if let Err(e) = discovery.start_discovery() {
                println!("Error starting discovery: {:?}", e);
            }
            spawn_stale_device_cleanup(discovery.clone(), transfer_manager.clone());
            
            let app_state = AppState {
                discovery: Arc::new(RwLock::new(Some(discovery))),
                transfer: Arc::new(RwLock::new(Some(transfer_manager))),
                sync: Arc::new(RwLock::new(SyncState::new())),
                security,
                database: database.clone(),
                transfers,
                renames,
                settings,
                pairing,
            };
            app.manage(app_state);

            println!("Setup hook finished");
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            start_discovery,
            get_discovered_devices,
            add_device_manually,
            send_file,
            get_trusted_devices,
            is_device_trusted,
            test_device_connectivity,
            is_dir,
            find_reachable_device_ip,
            get_network_diagnostics,
            get_local_network_interfaces,
            request_pairing,
            cancel_pairing,
            accept_pairing,
            reject_pairing,
            forget_device,
            set_sync_folder,
            get_sync_status,
            get_transfer_history,
            get_device_transfers,
            clear_transfer_history,
            pause_transfer,
            resume_transfer,
            cancel_transfer,
            accept_file_offer,
            reject_file_offer,
            get_settings,
            update_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
