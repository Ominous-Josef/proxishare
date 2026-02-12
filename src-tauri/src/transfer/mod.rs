pub mod protocol;
pub mod sender;
pub mod receiver;

use std::sync::Arc;
use quinn::{Endpoint, ServerConfig, ClientConfig};
use crate::crypto::encryption::CertificateManager;
use crate::transfer::sender::FileSender;
use crate::transfer::receiver::FileReceiver;
use std::path::PathBuf;

pub struct TransferManager {
    endpoint: Endpoint,
}

impl TransferManager {
    pub fn new(port: u16) -> Result<Self, Box<dyn std::error::Error>> {
        let cert_manager = CertificateManager::generate_self_signed()?;
        
        let server_config = ServerConfig::with_crypto(Arc::new(quinn::crypto::rustls::QuicServerConfig::try_from(
            cert_manager.get_server_config()?
        )?));
        
        let client_config = ClientConfig::with_crypto(Arc::new(quinn::crypto::rustls::QuicClientConfig::try_from(
            cert_manager.get_client_config()?
        )?));
        
        let addr = format!("0.0.0.0:{}", port).parse()?;
        let mut endpoint = Endpoint::server(server_config, addr)?;
        endpoint.set_default_client_config(client_config);

        Ok(Self { endpoint })
    }

    pub async fn start_listening(&self, save_dir: PathBuf) {
        while let Some(conn) = self.endpoint.accept().await {
            let save_dir = save_dir.clone();
            tokio::spawn(async move {
                if let Ok(connection) = conn.await {
                    let receiver = FileReceiver::new(save_dir, connection);
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
        progress_tx: tokio::sync::mpsc::Sender<u64>
    ) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("{}:{}", target_ip, target_port).parse()?;
        let connection = self.endpoint.connect(addr, "proxishare.local")?.await?;
        
        let sender = FileSender::new(connection);
        let transfer_id = uuid::Uuid::new_v4().to_string();
        
        sender.send_file(transfer_id, file_path, progress_tx).await
    }
}
