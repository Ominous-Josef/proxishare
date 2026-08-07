pub mod crypto;
pub mod db;
pub mod discovery;
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
use tauri::Manager;
use tokio::sync::RwLock;

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
pub type GenericError = Box<dyn std::error::Error + Send + Sync>;

pub struct AppState {
    pub discovery: Arc<RwLock<Option<Arc<DiscoveryService>>>>,
    pub transfer: Arc<RwLock<Option<Arc<TransferManager>>>>,
    pub sync: Arc<RwLock<SyncState>>,
    pub security: Arc<RwLock<SecurityService>>,
    pub database: Arc<RwLock<Option<Database>>>,
    pub transfers: TransferRegistry,
    pub settings: Arc<RwLock<SettingsManager>>,
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
    let port = 51731; // Default port
    
    // Get our device info
    let my_id = state.security.read().await.get_device_id().to_string();
    let my_name = state.settings.read().await.get_settings().device_name.clone();

    // Get transfer manager to ping the device
    let transfer = state.transfer.read().await.clone();
    if let Some(tm) = transfer {
        match tm.ping_device(&ip, port, my_id, my_name).await {
            Ok((device_id, device_name)) => {
                // If ping succeeds, inject it into discovery
                if let Some(discovery) = state.discovery.read().await.as_ref() {
                    discovery.add_manual_device(device_id.clone(), device_name.clone(), ip.clone(), port).await;
                    println!("[mDNS] Manually added device: {} ({}) at {}", device_name, device_id, ip);
                    return Ok(true);
                }
            },
            Err(e) => {
                println!("[mDNS] Failed to connect manually to {}: {:?}", ip, e);
                return Err(format!("Could not connect to {}: {:?}", ip, e));
            }
        }
    }
    Err("Transfer manager not initialized".to_string())
}

#[tauri::command]
async fn get_discovered_devices(state: tauri::State<'_, AppState>) -> Result<Vec<Device>, String> {
    let mut devices = if let Some(ds) = state.discovery.read().await.as_ref() {
        ds.get_devices().await
    } else {
        vec![]
    };

    // Inject offline trusted devices into the discovery response
    let security = state.security.read().await;
    for (_, td) in &security.trusted_devices {
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

    Ok(devices)
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
            let send_result: Result<(), String> = tm
                .send_file(
                    transfer_id.clone(),
                    ip.clone(),
                    port,
                    file_path.clone(),
                    transfers_clone.clone(),
                    is_dir,
                )
                .await
                .map_err(|e| e.to_string());

            // Update transfer status
            {
                let db_lock = db_clone.read().await;
                if let Some(db) = &*db_lock {
                    let status = match &send_result {
                        Ok(_) => "completed",
                        Err(e) if e.contains("cancelled") => "cancelled",
                        Err(_) => "failed",
                    };
                    if let Err(e) = db
                        .update_transfer_status(&transfer_id, status, file_size)
                        .await
                    {
                        println!("[Database] Failed to update transfer status: {:?}", e);
                    }
                }
            }
            
            // Notify frontend that history changed
            use tauri::Emitter;
            let _ = app_clone.emit("history-updated", ());

            match send_result {
                Ok(_) => {
                    println!("[Command] send_file completed successfully");
                }
                Err(e) => {
                    let error_msg = if e.contains("cancelled") {
                        "Transfer cancelled".to_string()
                    } else {
                        format!("Failed to send file: {}", e)
                    };
                    println!("[Command] {}", error_msg);
                }
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
    Ok(security.is_trusted(&device_id))
}

#[tauri::command]
async fn test_device_connectivity(
    ip: String,
    port: u16,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    let tm = state.transfer.read().await.clone();
    if let Some(tm) = tm {
        let security = state.security.read().await;
        let my_id = security.get_device_id().clone();
        drop(security);
        
        let settings = state.settings.read().await;
        let my_name = settings.get_settings().device_name;
        drop(settings);

        match tm.ping_device(&ip, port, my_id, my_name).await {
            Ok(_) => Ok(true),
            Err(e) => {
                println!("[API] test_device_connectivity to {}:{} failed: {:?}", ip, port, e);
                Ok(false)
            }
        }
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
    let tm = state.transfer.read().await.clone();
    
    if let (Some(ds), Some(tm)) = (discovery, tm) {
        let devices = ds.get_devices().await;
        if let Some(device) = devices.iter().find(|d| d.id == device_id) {
            let security = state.security.read().await;
            let my_id = security.get_device_id().clone();
            drop(security);
            
            let settings = state.settings.read().await;
            let my_name = settings.get_settings().device_name;
            drop(settings);
            
            // Try primary IP
            if tm.ping_device(&device.ip, device.port, my_id.clone(), my_name.clone()).await.is_ok() {
                return Ok(Some(device.ip.clone()));
            }
            
            // Try other IPs
            for ip in &device.all_ips {
                if ip != &device.ip && tm.ping_device(ip, device.port, my_id.clone(), my_name.clone()).await.is_ok() {
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
    _device_id: String,
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
        tm
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

    Ok(pairing_code)
}

#[tauri::command]
async fn accept_file_offer(
    state: tauri::State<'_, AppState>,
    transfer_id: String,
) -> Result<(), String> {
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
    device_name: String,
    ip: String,
    port: u16,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    println!(
        "[Pairing] Accepting pairing for device: {} at {}:{}",
        device_id, ip, port
    );
    let settings = state.settings.read().await;
    let my_name = settings.get_settings().device_name;
    drop(settings);

    let mut security = state.security.write().await;
    let trusted_device = crate::crypto::security::TrustedDevice {
        id: device_id.clone(),
        name: device_name.clone(),
        last_ip: ip.clone(),
        last_port: port,
        last_seen: std::time::UNIX_EPOCH.elapsed().unwrap().as_secs() as i64,
    };
    security
        .add_trusted(trusted_device)
        .map_err(|e| e.to_string())?;
    println!("[Pairing] Device {} is now trusted", device_id);
    let my_id = security.get_device_id().clone();
    drop(security);

    // Send PairResponse to the device
    let tm_opt = state.transfer.read().await.clone();
    if let Some(tm) = tm_opt {
        let _ = tm
            .send_message(
                ip.clone(),
                port,
                crate::transfer::protocol::MessageType::PairResponse {
                    accepted: true,
                    device_id: my_id,
                    device_name: my_name,
                },
            )
            .await;
    }

    // Trigger history sync
    let _ = sync_history(state, device_id, ip, port).await;

    Ok(())
}

#[tauri::command]
async fn reject_pairing(
    device_id: String,
    ip: String,
    port: u16,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    println!("[Pairing] Rejecting pairing for device: {}", device_id);

    // Send PairResponse to the device
    let tm_opt = state.transfer.read().await.clone();
    if let Some(tm) = tm_opt {
        let _ = tm
            .send_message(
                ip.clone(),
                port,
                crate::transfer::protocol::MessageType::PairResponse {
                    accepted: false,
                    device_id: "".to_string(), // They don't need our ID if rejected
                    device_name: "".to_string(), // They don't need our name if rejected
                },
            )
            .await;
    }

    Ok(())
}

#[tauri::command]
async fn sync_history(
    state: tauri::State<'_, AppState>,
    device_id: String,
    ip: String,
    port: u16,
) -> Result<(), String> {
    println!(
        "[Sync] Syncing history with device: {} at {}:{}",
        device_id, ip, port
    );

    let records = {
        let db_lock = state.database.read().await;
        if let Some(db) = &*db_lock {
            db.get_transfer_history(100)
                .await
                .map_err(|e| e.to_string())?
        } else {
            return Err("Database not initialized".to_string());
        }
    };

    let tm_opt = state.transfer.read().await.clone();
    if let Some(tm) = tm_opt {
        tm.send_message(
            ip,
            port,
            crate::transfer::protocol::MessageType::HistorySync { records },
        )
        .await
        .map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("Transfer manager not initialized".to_string())
    }
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
    let old_name = settings_manager.get_settings().device_name;
    settings_manager.update_settings(settings.clone())?;
    
    if old_name != settings.device_name {
        let discovery_lock = state.discovery.read().await;
        if let Some(discovery) = &*discovery_lock {
            if let Err(e) = discovery.update_name(settings.device_name.clone()) {
                println!("[Settings] Failed to update discovery name: {}", e);
            }
        }
    }
    
    Ok(())
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

            let device_id_path = app_data_dir.join("device_id.txt");
            let device_id = if device_id_path.exists() {
                std::fs::read_to_string(&device_id_path)
                    .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string())
            } else {
                let id = uuid::Uuid::new_v4().to_string();
                let _ = std::fs::write(&device_id_path, &id);
                id
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

            println!("Initializing services with block_on");
            let (discovery, transfer_manager) = tauri::async_runtime::block_on(async {
                println!("Inside block_on: Initializing TransferManager");
                // Initialize Transfer Manager
                let port = 51731;
                let tm = TransferManager::new(
                    port,
                    app_handle.clone(),
                    database.clone(),
                    transfers.clone(),
                    device_id.clone(),
                    settings.clone(),
                    security.clone(),
                )?;
                println!("Inside block_on: TransferManager initialized");

                println!("Inside block_on: Initializing DiscoveryService");
                // Initialize Discovery Service
                let ds = DiscoveryService::new(device_id, device_name, port)?;
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

            if let Err(e) = discovery.start_broadcasting() {
                println!("Error starting broadcasting: {:?}", e);
            }
            if let Err(e) = discovery.start_discovery() {
                println!("Error starting discovery: {:?}", e);
            }

            let app_state = AppState {
                discovery: Arc::new(RwLock::new(Some(discovery))),
                transfer: Arc::new(RwLock::new(Some(transfer_manager))),
                sync: Arc::new(RwLock::new(SyncState::new())),
                security,
                database: database.clone(),
                transfers,
                settings,
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
            find_reachable_device_ip,
            get_network_diagnostics,
            get_local_network_interfaces,
            request_pairing,
            accept_pairing,
            reject_pairing,
            set_sync_folder,
            get_sync_status,
            get_transfer_history,
            get_device_transfers,
            clear_transfer_history,
            pause_transfer,
            resume_transfer,
            cancel_transfer,
            sync_history,
            accept_file_offer,
            reject_file_offer,
            get_settings,
            update_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
