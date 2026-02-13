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
        println!("[Transfer] Server listening on port, save dir: {:?}", save_dir);
        let app_handle = self.app_handle.clone();
        while let Some(conn) = self.endpoint.accept().await {
            println!("[Transfer] Incoming connection accepted");
            let save_dir = save_dir.clone();
            let app_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                match conn.await {
                    Ok(connection) => {
                        println!("[Transfer] Connection established from remote peer");
                        let receiver = FileReceiver::new(save_dir, connection, app_handle);
                        match receiver.handle_transfer().await {
                            Ok(_) => println!("[Transfer] File received successfully"),
                            Err(e) => println!("[Transfer] Error receiving file: {:?}", e),
                        }
                    }
                    Err(e) => {
                        println!("[Transfer] Failed to establish connection: {:?}", e);
                    }
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
        println!("[Transfer] Attempting to send file {:?} to {}:{}", file_path, target_ip, target_port);
        
        let addr = format!("{}:{}", target_ip, target_port).parse()?;
        println!("[Transfer] Connecting to {:?}...", addr);
        
        let connecting = self.endpoint.connect(addr, "proxishare.local")?;
        println!("[Transfer] Connection initiated, waiting for handshake...");
        
        let connection = match tokio::time::timeout(
            std::time::Duration::from_secs(10),
            connecting
        ).await {
            Ok(Ok(conn)) => {
                println!("[Transfer] Connection established!");
                conn
            }
            Ok(Err(e)) => {
                println!("[Transfer] Connection failed: {:?}", e);
                return Err(format!("Connection failed: {}", e).into());
            }
            Err(_) => {
                println!("[Transfer] Connection timed out after 10 seconds");
                return Err("Connection timed out".into());
            }
        };

        let sender = FileSender::new(connection);
        let transfer_id = uuid::Uuid::new_v4().to_string();
        println!("[Transfer] Starting file transfer with ID: {}", transfer_id);

        match sender.send_file(transfer_id.clone(), file_path.clone(), progress_tx).await {
            Ok(_) => {
                println!("[Transfer] File {:?} sent successfully!", file_path);
                Ok(())
            }
            Err(e) => {
                println!("[Transfer] Failed to send file: {:?}", e);
                Err(e)
            }
        }
    }
}
