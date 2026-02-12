pub mod protocol;
pub mod receiver;
pub mod sender;

use crate::crypto::encryption::CertificateManager;
use crate::transfer::receiver::FileReceiver;
use crate::transfer::sender::FileSender;
use quinn::{ClientConfig, Endpoint, ServerConfig};
use std::path::PathBuf;
use std::sync::Arc;

pub struct TransferManager {
    endpoint: Endpoint,
    app_handle: tauri::AppHandle,
}

impl TransferManager {
    pub fn new(
        port: u16,
        app_handle: tauri::AppHandle,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let cert_manager = CertificateManager::generate_self_signed()?;

        let server_crypto = Arc::new(quinn::crypto::rustls::QuicServerConfig::try_from(
            cert_manager.get_server_config()?,
        )?);

        // Fix: Create server config with just the crypto config
        let server_config = ServerConfig::with_crypto(server_crypto);

        let client_crypto = Arc::new(quinn::crypto::rustls::QuicClientConfig::try_from(
            cert_manager.get_client_config()?,
        )?);
        let client_config = ClientConfig::new(client_crypto);

        let addr = format!("0.0.0.0:{}", port).parse()?;
        let mut endpoint = Endpoint::server(server_config, addr)?;
        endpoint.set_default_client_config(client_config);

        Ok(Self {
            endpoint,
            app_handle,
        })
    }

    pub async fn start_listening(&self, save_dir: PathBuf) {
        let app_handle = self.app_handle.clone();
        while let Some(conn) = self.endpoint.accept().await {
            let save_dir = save_dir.clone();
            let app_handle = app_handle.clone();
            tokio::spawn(async move {
                if let Ok(connection) = conn.await {
                    let receiver = FileReceiver::new(save_dir, connection, app_handle);
                    let _ = receiver.handle_transfer().await;
                }
            });
        }
    }

    pub async fn send_file(
        &self,
        target_ip: String,
        target_port: u16,
        file_path: PathBuf,
        progress_tx: tokio::sync::mpsc::Sender<u64>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("{}:{}", target_ip, target_port).parse()?;
        let connection = self.endpoint.connect(addr, "proxishare.local")?.await?;

        let sender = FileSender::new(connection);
        let transfer_id = uuid::Uuid::new_v4().to_string();

        sender.send_file(transfer_id, file_path, progress_tx).await
    }
}
