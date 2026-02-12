use crate::discovery::mdns::{Device, DiscoveryService};
use crate::transfer::TransferManager;
use std::sync::Arc;
use std::path::PathBuf;
use tokio::sync::RwLock;
use tauri::Manager;

use crate::sync::SyncState;
use crate::crypto::security::SecurityService;

pub struct AppState {
    pub discovery: Arc<RwLock<Option<DiscoveryService>>>,
    pub transfer: Arc<RwLock<Option<Arc<TransferManager>>>>,
    pub sync: Arc<RwLock<SyncState>>,
    pub security: Arc<RwLock<SecurityService>>,
}

#[tauri::command]
async fn start_discovery(state: tauri::State<'_, AppState>, app: tauri::AppHandle) -> Result<(), String> {
    let mut discovery_lock = state.discovery.write().await;
    let mut transfer_lock = state.transfer.write().await;

    if discovery_lock.is_none() {
        let port = 51000;
        
        // Use user's Downloads folder as default save dir
        let downloads_dir = app.path().download_dir().unwrap_or_else(|_| PathBuf::from("./downloads"));
        if !downloads_dir.exists() {
            let _ = std::fs::create_dir_all(&downloads_dir);
        }

        // Initialize Transfer Manager
        let transfer_manager = Arc::new(TransferManager::new(port, app.clone()).map_err(|e| e.to_string())?);
        let tm_clone = Arc::clone(&transfer_manager);
        tokio::spawn(async move {
            tm_clone.start_listening(downloads_dir).await;
        });
        *transfer_lock = Some(transfer_manager);

        // Initialize Discovery
        let discovery = DiscoveryService::new("Desktop User".to_string(), port)
            .map_err(|e| e.to_string())?;
        
        discovery.start_broadcasting().map_err(|e| e.to_string())?;
        discovery.start_discovery().map_err(|e| e.to_string())?;
        
        *discovery_lock = Some(discovery);
    }
    Ok(())
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
        // In a real app, we'd handle _rx to update progress in AppState/Frontend
        tm.send_file(ip, port, PathBuf::from(path), tx)
            .await
            .map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("Transfer manager not initialized".to_string())
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


#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle().clone();
            let downloads_dir = app_handle.path().download_dir().unwrap_or_else(|_| PathBuf::from("./downloads"));
            if !downloads_dir.exists() {
                let _ = std::fs::create_dir_all(&downloads_dir);
            }

            // Initialize Security Service
            let app_data_dir = app_handle.path().app_data_dir().unwrap_or_else(|_| PathBuf::from("./data"));
            if !app_data_dir.exists() {
                let _ = std::fs::create_dir_all(&app_data_dir);
            }
            let security = SecurityService::new(app_data_dir);

            // Initialize Transfer Manager
            let port = 51731;
            let transfer_manager = Arc::new(TransferManager::new(port, app_handle.clone()).map_err(|e| e.to_string())?);
            let tm_clone = Arc::clone(&transfer_manager);
            tokio::spawn(async move {
                tm_clone.start_listening(downloads_dir).await;
            });

            // Initialize Discovery Service
            let discovery = DiscoveryService::new(port, "ProxiNode".to_string())?;
            discovery.start()?;

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
            set_sync_folder,
            get_sync_status,
            get_trusted_devices,
            is_device_trusted,
            request_pairing,
            accept_pairing
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
