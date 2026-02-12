pub mod crypto;
pub mod discovery;
pub mod sync;
pub mod transfer;

use crate::discovery::mdns::{Device, DiscoveryService};
use crate::transfer::TransferManager;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::RwLock;

use crate::crypto::security::SecurityService;
use crate::sync::SyncState;

pub struct AppState {
    pub discovery: Arc<RwLock<Option<DiscoveryService>>>,
    pub transfer: Arc<RwLock<Option<Arc<TransferManager>>>>,
    pub sync: Arc<RwLock<SyncState>>,
    pub security: Arc<RwLock<SecurityService>>,
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
    let discovery_lock = state.discovery.read().await;
    if let Some(discovery) = &*discovery_lock {
        Ok(discovery.get_devices().await)
    } else {
        Ok(vec![])
    }
}

#[tauri::command]
async fn send_file(
    state: tauri::State<'_, AppState>,
    ip: String,
    port: u16,
    path: String,
) -> Result<(), String> {
    let transfer_lock = state.transfer.read().await;
    if let Some(tm) = &*transfer_lock {
        let (tx, _rx) = tokio::sync::mpsc::channel(100);
        tm.send_file(ip, port, PathBuf::from(path), tx)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("Transfer manager not initialized".to_string())
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
async fn request_pairing(
    _state: tauri::State<'_, AppState>,
    _device_id: String,
    _ip: String,
    _port: u16,
) -> Result<(), String> {
    Ok(())
}

#[tauri::command]
async fn accept_pairing(
    device_id: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let mut security = state.security.write().await;
    security.add_trusted(device_id).map_err(|e| e.to_string())?;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
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
            let security = SecurityService::new(app_data_dir);

            // Initialize Transfer Manager
            let port = 51731;
            let transfer_manager = Arc::new(TransferManager::new(port, app_handle.clone())?);
            let tm_clone = Arc::clone(&transfer_manager);
            let ds_downloads_dir = downloads_dir.clone();
            tokio::spawn(async move {
                tm_clone.start_listening(ds_downloads_dir).await;
            });

            // Initialize Discovery Service
            let discovery = DiscoveryService::new("ProxiNode".to_string(), port)?;
            let _ = discovery.start_broadcasting();
            let _ = discovery.start_discovery();

            app.manage(AppState {
                discovery: Arc::new(RwLock::new(Some(discovery))),
                transfer: Arc::new(RwLock::new(Some(transfer_manager))),
                sync: Arc::new(RwLock::new(SyncState::new())),
                security: Arc::new(RwLock::new(security)),
            });

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
            request_pairing,
            accept_pairing,
            set_sync_folder,
            get_sync_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
