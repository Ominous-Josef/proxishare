pub mod crypto;
pub mod db;
pub mod discovery;
pub mod sync;
pub mod transfer;

use crate::db::{Database, TransferRecord};
use crate::discovery::mdns::{
    get_network_interfaces, Device, DiscoveryService, NetworkDiagnostics, NetworkInterface,
};
use crate::transfer::TransferManager;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::RwLock;

use crate::crypto::security::SecurityService;
use crate::sync::SyncState;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TransferStatus {
    InProgress,
    Paused,
    Cancelled,
    Completed,
    Failed,
}

pub type TransferRegistry = Arc<RwLock<HashMap<String, TransferStatus>>>;

pub struct AppState {
    pub discovery: Arc<RwLock<Option<Arc<DiscoveryService>>>>,
    pub transfer: Arc<RwLock<Option<Arc<TransferManager>>>>,
    pub sync: Arc<RwLock<SyncState>>,
    pub security: Arc<RwLock<SecurityService>>,
    pub database: Arc<RwLock<Option<Database>>>,
    pub transfers: TransferRegistry,
}

#[tauri::command]
async fn start_discovery(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let discovery_lock = state.discovery.read().await;
    if let Some(discovery) = &*discovery_lock {
        discovery.start_broadcasting().map_err(|e| e.to_string())?;
        discovery.start_discovery().map_err(|e| e.to_string())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
async fn get_discovered_devices(state: tauri::State<'_, AppState>) -> Result<Vec<Device>, String> {
    let discovery = state.discovery.read().await.clone();
    if let Some(ds) = discovery {
        Ok(ds.get_devices().await)
    } else {
        Ok(vec![])
    }
}

#[tauri::command]
async fn send_file(
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
    let file_size = std::fs::metadata(&path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);

    let transfer_id = uuid::Uuid::new_v4().to_string();

    // Track transfer in registry
    {
        let mut transfers = state.transfers.write().await;
        transfers.insert(transfer_id.clone(), TransferStatus::InProgress);
    }

    // Record the transfer start in database
    {
        let db_lock = state.database.read().await;
        if let Some(db) = &*db_lock {
            let _ = db
                .record_transfer(
                    &transfer_id,
                    &device_id,
                    &file_name,
                    &path,
                    file_size,
                    "send",
                    "", // Hash will be calculated during transfer
                )
                .await;
        }
    }

    let tm_opt = state.transfer.read().await.clone();
    if let Some(tm) = tm_opt {
        // Convert result to Send-compatible type immediately
        let send_result: Result<(), String> = tm
            .send_file(
                transfer_id.clone(),
                ip.clone(),
                port,
                file_path,
                state.transfers.clone(),
            )
            .await
            .map_err(|e| e.to_string());

        // Update transfer status
        {
            let db_lock = state.database.read().await;
            if let Some(db) = &*db_lock {
                let status = match &send_result {
                    Ok(_) => "completed",
                    Err(e) if e.contains("cancelled") => "cancelled",
                    Err(_) => "failed",
                };
                let _ = db
                    .update_transfer_status(&transfer_id, status, file_size)
                    .await;
            }
        }

        match send_result {
            Ok(_) => {
                println!("[Command] send_file completed successfully");
                Ok(())
            }
            Err(e) => {
                let error_msg = format!("Failed to send file: {}", e);
                println!("[Command] {}", error_msg);
                Err(error_msg)
            }
        }
    } else {
        let error_msg = "Transfer manager not initialized".to_string();
        println!("[Command] {}", error_msg);
        Err(error_msg)
    }
}

#[tauri::command]
async fn get_trusted_devices(state: tauri::State<'_, AppState>) -> Result<Vec<String>, String> {
    let security = state.security.read().await;
    Ok(security.trusted_devices.iter().cloned().collect())
}

#[tauri::command]
async fn is_device_trusted(
    device_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let security = state.security.read().await;
    Ok(security.is_trusted(&device_id))
}

#[tauri::command]
async fn test_device_connectivity(
    ip: String,
    port: u16,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let discovery = state.discovery.read().await.clone();
    if let Some(ds) = discovery {
        Ok(ds.test_connectivity(&ip, port).await)
    } else {
        Ok(false)
    }
}

#[tauri::command]
async fn find_reachable_device_ip(
    device_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<Option<String>, String> {
    let discovery = state.discovery.read().await.clone();
    if let Some(ds) = discovery {
        let devices = ds.get_devices().await;
        if let Some(device) = devices.iter().find(|d| d.id == device_id) {
            Ok(ds.find_reachable_ip(device).await)
        } else {
            Ok(None)
        }
    } else {
        Ok(None)
    }
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
            app_port: 51731,
            subnet_info: "Unknown".to_string(),
        })
    }
}

#[tauri::command]
fn get_local_network_interfaces() -> Vec<NetworkInterface> {
    get_network_interfaces()
}

#[tauri::command]
async fn request_pairing(
    state: tauri::State<'_, AppState>,
    device_id: String,
    ip: String,
    port: u16,
) -> Result<String, String> {
    // Generate a random 6-digit pairing code
    let pairing_code = {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        format!("{:06}", rng.gen_range(0..1_000_000))
    };

    // Get our device info for the pairing request
    let discovery_lock = state.discovery.read().await;
    let my_id = discovery_lock
        .as_ref()
        .map(|d| d.get_my_id())
        .unwrap_or_else(|| "unknown".to_string());
    drop(discovery_lock);

    // Get hostname as device name
    let my_name = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "ProxiNode".to_string());

    // Send pairing request message
    let tm_opt = state.transfer.read().await.clone();
    if let Some(tm) = tm_opt {
        let _ = tm
            .send_message(
                ip.clone(),
                port,
                crate::transfer::protocol::MessageType::PairRequest {
                    device_id: my_id,
                    device_name: my_name,
                    pairing_code: pairing_code.clone(),
                },
            )
            .await
            .map_err(|e| e.to_string())?;
    }

    // For now, just trust the device directly (simplified pairing)
    let mut security = state.security.write().await;
    security
        .add_trusted(device_id.clone())
        .map_err(|e| e.to_string())?;

    println!("[Pairing] Device {} added to trusted devices", device_id);
    Ok(pairing_code)
}

#[tauri::command]
async fn pause_transfer(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
    let mut transfers = state.transfers.write().await;
    if transfers.contains_key(&transfer_id) {
        transfers.insert(transfer_id, TransferStatus::Paused);
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
    if transfers.contains_key(&transfer_id) {
        transfers.insert(transfer_id, TransferStatus::InProgress);
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
    if transfers.contains_key(&transfer_id) {
        transfers.insert(transfer_id, TransferStatus::Cancelled);
        Ok(())
    } else {
        Err("Transfer not found".to_string())
    }
}

#[tauri::command]
async fn accept_pairing(
    device_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    println!("[Pairing] Accepting pairing for device: {}", device_id);
    let mut security = state.security.write().await;
    security
        .add_trusted(device_id.clone())
        .map_err(|e| e.to_string())?;
    println!("[Pairing] Device {} is now trusted", device_id);
    Ok(())
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
) -> Result<Vec<TransferRecord>, String> {
    let db_lock = state.database.read().await;
    if let Some(db) = &*db_lock {
        db.get_transfer_history(limit.unwrap_or(100))
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
) -> Result<Vec<TransferRecord>, String> {
    let db_lock = state.database.read().await;
    if let Some(db) = &*db_lock {
        db.get_device_transfers(&device_id, limit.unwrap_or(50))
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            println!("Setup hook started");
            let app_handle = app.handle().clone();
            let downloads_dir = app_handle
                .path()
                .download_dir()
                .unwrap_or_else(|_| PathBuf::from("./downloads"));
            if !downloads_dir.exists() {
                let _ = std::fs::create_dir_all(&downloads_dir);
            }

            // Initialize Security Service
            let app_data_dir = app_handle
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| PathBuf::from("./data"));
            if !app_data_dir.exists() {
                let _ = std::fs::create_dir_all(&app_data_dir);
            }
            let security = SecurityService::new(app_data_dir.clone());

            // Initialize Database
            let db_path = app_data_dir.join("proxishare.db");
            let database = tauri::async_runtime::block_on(async {
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

            // Initialize Device ID and Name
            let device_id_path = app_data_dir.join("device_id.txt");
            let device_id = if device_id_path.exists() {
                std::fs::read_to_string(&device_id_path)
                    .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string())
            } else {
                let id = uuid::Uuid::new_v4().to_string();
                let _ = std::fs::write(&device_id_path, &id);
                id
            };

            let device_name = hostname::get()
                .ok()
                .and_then(|h| h.into_string().ok())
                .unwrap_or_else(|| "ProxiNode".to_string());

            println!("Initializing services with block_on");
            let (discovery, transfer_manager) = tauri::async_runtime::block_on(async {
                println!("Inside block_on: Initializing TransferManager");
                // Initialize Transfer Manager
                let port = 51731;
                let tm = TransferManager::new(port, app_handle.clone())?;
                println!("Inside block_on: TransferManager initialized");

                println!("Inside block_on: Initializing DiscoveryService");
                // Initialize Discovery Service
                let ds = DiscoveryService::new(device_id, device_name, port)?;
                println!("Inside block_on: DiscoveryService initialized");

                Ok::<(Arc<DiscoveryService>, Arc<TransferManager>), Box<dyn std::error::Error>>((
                    Arc::new(ds),
                    Arc::new(tm),
                ))
            })?;

            println!("Starting listening and broadcasting");
            let tm_clone = Arc::clone(&transfer_manager);
            let ds_downloads_dir = downloads_dir.clone();
            tauri::async_runtime::spawn(async move {
                tm_clone.start_listening(ds_downloads_dir).await;
            });

            let _ = discovery.start_broadcasting();
            let _ = discovery.start_discovery();

            app.manage(AppState {
                discovery: Arc::new(RwLock::new(Some(discovery))),
                transfer: Arc::new(RwLock::new(Some(transfer_manager))),
                sync: Arc::new(RwLock::new(SyncState::new())),
                security: Arc::new(RwLock::new(security)),
                database: Arc::new(RwLock::new(database)),
                transfers: Arc::new(RwLock::new(HashMap::new())),
            });

            println!("Setup hook finished");
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            start_discovery,
            get_discovered_devices,
            send_file,
            get_trusted_devices,
            is_device_trusted,
            test_device_connectivity,
            find_reachable_device_ip,
            get_network_diagnostics,
            get_local_network_interfaces,
            request_pairing,
            accept_pairing,
            set_sync_folder,
            get_sync_status,
            get_transfer_history,
            get_device_transfers,
            clear_transfer_history,
            pause_transfer,
            resume_transfer,
            cancel_transfer
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
