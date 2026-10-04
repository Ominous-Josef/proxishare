pub mod protocol;
pub mod receiver;
pub mod sender;
pub mod session;

use crate::crypto::encryption::CertificateManager;
use crate::transfer::protocol::{read_message, write_message, MessageType};
use crate::transfer::receiver::FileReceiver;
use crate::transfer::sender::{FileSender, TransferResult};
use quinn::{ClientConfig, Endpoint, ServerConfig};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct TransferManager {
    endpoint: Endpoint,
    app_handle: tauri::AppHandle,
    database: Arc<RwLock<Option<crate::db::Database>>>,
    transfers: crate::TransferRegistry,
    device_id: String,
    settings: Arc<RwLock<crate::settings::SettingsManager>>,
    security: Arc<RwLock<crate::crypto::security::SecurityService>>,
    renames: Arc<RwLock<std::collections::HashMap<String, String>>>,
    pairing: Arc<RwLock<crate::pairing::PairingState>>,
}

impl TransferManager {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        port: u16,
        app_handle: tauri::AppHandle,
        database: Arc<RwLock<Option<crate::db::Database>>>,
        transfers: crate::TransferRegistry,
        device_id: String,
        settings: Arc<RwLock<crate::settings::SettingsManager>>,
        security: Arc<RwLock<crate::crypto::security::SecurityService>>,
        renames: Arc<RwLock<std::collections::HashMap<String, String>>>,
        pairing: Arc<RwLock<crate::pairing::PairingState>>,
    ) -> Result<Self, crate::GenericError> {
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
            database,
            transfers,
            device_id,
            settings,
            security,
            renames,
            pairing,
        })
    }

    pub async fn start_listening(&self) {
        println!(
            "[Transfer] Server listening on port"
        );
        let app_handle = self.app_handle.clone();
        while let Some(conn) = self.endpoint.accept().await {
            println!("[Transfer] Incoming connection accepted");
            let save_dir_str = self.settings.read().await.get_settings().download_dir;
            let save_dir = PathBuf::from(save_dir_str);
            if !save_dir.exists() {
                let _ = std::fs::create_dir_all(&save_dir);
            }
            
            let app_handle = app_handle.clone();
            let database = self.database.clone();
            let transfers = self.transfers.clone();
            let security = self.security.clone();
            let renames = self.renames.clone();
            let settings = self.settings.clone();
            let pairing = self.pairing.clone();
            tauri::async_runtime::spawn(async move {
                match conn.await {
                    Ok(connection) => {
                        println!("[Transfer] Connection established from remote peer");
                        let receiver = FileReceiver::new(
                            save_dir, connection, app_handle, database, transfers, security,
                            renames, settings, pairing,
                        );
                        if let Err(e) = receiver.handle_transfer().await {
                            println!("[Transfer] Incoming connection ended with error: {}", e);
                        }
                    }
                    Err(e) => {
                        println!("[Transfer] Failed to establish connection: {:?}", e);
                    }
                }
            });
        }
    }

    pub async fn send_message(
        &self,
        target_ip: String,
        target_port: u16,
        message: MessageType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!(
            "[Transfer] Sending message to {}:{}",
            target_ip, target_port
        );

        let addr = format!("{}:{}", target_ip, target_port).parse()?;
        let connecting = self.endpoint.connect(addr, "proxishare.local")?;

        let connection =
            match tokio::time::timeout(std::time::Duration::from_secs(5), connecting).await {
                Ok(Ok(conn)) => conn,
                Ok(Err(e)) => return Err(format!("Connection failed: {}", e).into()),
                Err(_) => return Err("Connection timed out".into()),
            };

        let (mut send_stream, _) = connection.open_bi().await?;
        write_message(&mut send_stream, &message).await?;

        send_stream.finish()?;

        // Give it a moment to send
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        connection.close(quinn::VarInt::from_u32(0), b"message sent");

        Ok(())
    }

    pub async fn ping_device(
        &self,
        target_ip: &str,
        target_port: u16,
        my_id: String,
        my_name: String,
    ) -> Result<(String, String), crate::GenericError> {
        let addr = format!("{}:{}", target_ip, target_port).parse()?;
        let connecting = self.endpoint.connect(addr, "proxishare.local")?;

        let connection = match tokio::time::timeout(std::time::Duration::from_secs(5), connecting).await {
            Ok(Ok(conn)) => conn,
            Ok(Err(e)) => return Err(format!("Connection failed: {}", e).into()),
            Err(_) => return Err("Connection timed out".into()),
        };

        let (mut send_stream, mut recv_stream) = connection.open_bi().await?;

        let msg = MessageType::Hello {
            device_id: my_id,
            device_name: my_name,
        };
        write_message(&mut send_stream, &msg).await?;

        // An unresponsive or older peer must not hang the caller.
        let ack_msg = match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            read_message(&mut recv_stream),
        )
        .await
        {
            Ok(res) => res?,
            Err(_) => return Err("Timed out waiting for HelloAck".into()),
        };

        let result = match ack_msg {
            MessageType::HelloAck { device_id, device_name } => Ok((device_id, device_name)),
            _ => Err("Invalid response from device".into()),
        };

        send_stream.finish()?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        connection.close(quinn::VarInt::from_u32(0), b"ping complete");

        result
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn send_file(
        &self,
        transfer_id: String,
        device_id: String,
        target_ip: String,
        target_port: u16,
        file_path: PathBuf,
        transfers: crate::TransferRegistry,
        is_dir: bool,
    ) -> Result<TransferResult, crate::GenericError> {
        println!(
            "[Transfer] Attempting to send file {:?} to {}:{}",
            file_path, target_ip, target_port
        );

        let addr = format!("{}:{}", target_ip, target_port).parse()?;
        println!("[Transfer] Connecting to {:?}...", addr);

        let connecting = self.endpoint.connect(addr, "proxishare.local")?;
        println!("[Transfer] Connection initiated, waiting for handshake...");

        let connection =
            match tokio::time::timeout(std::time::Duration::from_secs(10), connecting).await {
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

        let sender_name = self.settings.read().await.get_settings().device_name;

        let sender = FileSender::new(
            connection,
            self.app_handle.clone(),
            self.device_id.clone(),
            sender_name,
            device_id.clone(),
        );
        println!("[Transfer] Starting file transfer with ID: {}", transfer_id);

        match sender
            .send_file(transfer_id.clone(), file_path.clone(), transfers, is_dir)
            .await
        {
            Ok(result) => {
                println!(
                    "[Transfer] File {:?} sent, receiver reported {:?}",
                    file_path, result.outcome
                );
                Ok(result)
            }
            Err(e) => {
                println!("[Transfer] Failed to send file: {:?}", e);
                Err(e)
            }
        }
    }
}
