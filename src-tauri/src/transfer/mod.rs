pub mod protocol;
pub mod receiver;
pub mod sender;
pub mod session;

use crate::crypto::encryption::{peer_fingerprint, DeviceIdentity};
use crate::crypto::security::PeerTrust;
use crate::pairing::{commitment, new_nonce, pairing_code, PairParty, PAIRING_TTL};
use crate::transfer::protocol::{read_message, write_message, MessageType};
use crate::transfer::receiver::FileReceiver;
use crate::transfer::sender::{FileSender, TransferResult};
use quinn::{ClientConfig, Connection, Endpoint, RecvStream, SendStream, ServerConfig};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::Emitter;
use tokio::sync::RwLock;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const TRANSFER_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long the initiator waits for the other side's challenge.
const CHALLENGE_TIMEOUT: Duration = Duration::from_secs(30);

/// What a device answered to a ping, and the key it proved it holds.
pub struct PingReply {
    pub device_id: String,
    pub device_name: String,
    pub fingerprint: String,
}

/// A pairing we started that is waiting for the other user to type the code.
pub struct OutgoingPairing {
    connection: Connection,
    send_stream: SendStream,
    recv_stream: RecvStream,
    pub device_id: String,
    pub device_name: String,
    fingerprint: String,
    ip: String,
    port: u16,
}

pub struct TransferManager {
    endpoint: Endpoint,
    app_handle: tauri::AppHandle,
    database: Arc<RwLock<Option<crate::db::Database>>>,
    transfers: crate::TransferRegistry,
    device_id: String,
    /// Fingerprint of our own key.
    fingerprint: String,
    settings: Arc<RwLock<crate::settings::SettingsManager>>,
    security: Arc<RwLock<crate::crypto::security::SecurityService>>,
    renames: Arc<RwLock<std::collections::HashMap<String, String>>>,
    pairing: Arc<RwLock<crate::pairing::PairingState>>,
}

impl TransferManager {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        port: u16,
        identity: &DeviceIdentity,
        app_handle: tauri::AppHandle,
        database: Arc<RwLock<Option<crate::db::Database>>>,
        transfers: crate::TransferRegistry,
        device_id: String,
        settings: Arc<RwLock<crate::settings::SettingsManager>>,
        security: Arc<RwLock<crate::crypto::security::SecurityService>>,
        renames: Arc<RwLock<std::collections::HashMap<String, String>>>,
        pairing: Arc<RwLock<crate::pairing::PairingState>>,
    ) -> Result<Self, crate::GenericError> {
        // Keep idle connections alive, so a pairing survives the user typing the code.
        let mut transport = quinn::TransportConfig::default();
        transport.keep_alive_interval(Some(Duration::from_secs(10)));
        let transport = Arc::new(transport);

        let server_crypto = Arc::new(quinn::crypto::rustls::QuicServerConfig::try_from(
            identity.get_server_config()?,
        )?);
        let mut server_config = ServerConfig::with_crypto(server_crypto);
        server_config.transport_config(transport.clone());

        let client_crypto = Arc::new(quinn::crypto::rustls::QuicClientConfig::try_from(
            identity.get_client_config()?,
        )?);
        let mut client_config = ClientConfig::new(client_crypto);
        client_config.transport_config(transport);

        let addr = format!("0.0.0.0:{}", port).parse()?;
        let mut endpoint = Endpoint::server(server_config, addr)?;
        endpoint.set_default_client_config(client_config);

        Ok(Self {
            endpoint,
            app_handle,
            database,
            transfers,
            device_id,
            fingerprint: identity.fingerprint.clone(),
            settings,
            security,
            renames,
            pairing,
        })
    }

    pub async fn start_listening(&self) {
        println!("[Transfer] Server listening on port");
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
            let my_fingerprint = self.fingerprint.clone();
            tauri::async_runtime::spawn(async move {
                match conn.await {
                    Ok(connection) => {
                        println!("[Transfer] Connection established from remote peer");
                        let receiver = match FileReceiver::new(
                            save_dir, connection, app_handle, database, transfers, security,
                            renames, settings, pairing, my_fingerprint,
                        ) {
                            Ok(receiver) => receiver,
                            Err(e) => {
                                println!("[Transfer] Rejected connection: {}", e);
                                return;
                            }
                        };
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

    async fn connect(
        &self,
        ip: &str,
        port: u16,
        timeout: Duration,
    ) -> Result<Connection, crate::GenericError> {
        let addr = format!("{}:{}", ip, port).parse()?;
        let connecting = self.endpoint.connect(addr, "proxishare.local")?;
        match tokio::time::timeout(timeout, connecting).await {
            Ok(Ok(conn)) => Ok(conn),
            Ok(Err(e)) => Err(format!("Connection failed: {}", e).into()),
            Err(_) => Err("Connection timed out".into()),
        }
    }

    /// Connects to a paired device and checks it holds the pinned key before
    /// anything is sent. A different key means someone else answered.
    async fn connect_verified(
        &self,
        device_id: &str,
        ip: &str,
        port: u16,
        timeout: Duration,
    ) -> Result<Connection, crate::GenericError> {
        let connection = self.connect(ip, port, timeout).await?;
        let fingerprint = peer_fingerprint(&connection)?;
        let trust = self.security.read().await.verify_peer(device_id, &fingerprint);
        if trust == PeerTrust::Verified {
            return Ok(connection);
        }

        connection.close(quinn::VarInt::from_u32(0), b"identity check failed");
        Err(match trust {
            PeerTrust::KeyMismatch => {
                "Device identity mismatch: the device at this address does not hold the paired key (possible impersonation)".into()
            }
            other => other.rejection().into(),
        })
    }

    /// Sends one message to a paired device over a verified connection.
    pub async fn send_verified_message(
        &self,
        device_id: &str,
        ip: &str,
        port: u16,
        message: MessageType,
    ) -> Result<(), crate::GenericError> {
        println!("[Transfer] Sending message to {} at {}:{}", device_id, ip, port);
        let connection = self.connect_verified(device_id, ip, port, CONNECT_TIMEOUT).await?;

        let (mut send_stream, _) = connection.open_bi().await?;
        write_message(&mut send_stream, &message).await?;
        send_stream.finish()?;

        // Give it a moment to send
        tokio::time::sleep(Duration::from_millis(500)).await;
        connection.close(quinn::VarInt::from_u32(0), b"message sent");
        Ok(())
    }

    pub async fn ping_device(
        &self,
        target_ip: &str,
        target_port: u16,
    ) -> Result<PingReply, crate::GenericError> {
        let connection = self.connect(target_ip, target_port, CONNECT_TIMEOUT).await?;
        let fingerprint = peer_fingerprint(&connection)?;
        let (mut send_stream, mut recv_stream) = connection.open_bi().await?;

        let my_name = self.settings.read().await.get_settings().device_name;
        let msg = MessageType::Hello {
            device_id: self.device_id.clone(),
            device_name: my_name,
        };
        write_message(&mut send_stream, &msg).await?;

        // An unresponsive or older peer must not hang the caller.
        let ack_msg = match tokio::time::timeout(CONNECT_TIMEOUT, read_message(&mut recv_stream)).await
        {
            Ok(res) => res?,
            Err(_) => return Err("Timed out waiting for HelloAck".into()),
        };

        let result = match ack_msg {
            MessageType::HelloAck {
                device_id,
                device_name,
            } => Ok(PingReply {
                device_id,
                device_name,
                fingerprint,
            }),
            _ => Err("Invalid response from device".into()),
        };

        send_stream.finish()?;
        tokio::time::sleep(Duration::from_millis(100)).await;
        connection.close(quinn::VarInt::from_u32(0), b"ping complete");

        result
    }

    /// True if `device_id` answers at this address, holding its pinned key
    /// when it is paired.
    pub async fn ping_matches(&self, device_id: &str, ip: &str, port: u16) -> bool {
        let Ok(reply) = self.ping_device(ip, port).await else {
            return false;
        };
        if reply.device_id != device_id {
            return false;
        }
        match self.security.read().await.pinned_fingerprint(device_id) {
            Some(pinned) => pinned == reply.fingerprint,
            None => true,
        }
    }

    /// Runs our half of the pairing handshake up to the point where the code
    /// is known. Returns the code to show and the open pairing to finish.
    pub async fn start_pairing(
        &self,
        device_id: &str,
        ip: &str,
        port: u16,
    ) -> Result<(String, OutgoingPairing), crate::GenericError> {
        let connection = self.connect(ip, port, CONNECT_TIMEOUT).await?;
        let fingerprint = peer_fingerprint(&connection)?;
        self.security.read().await.can_pair(device_id, &fingerprint)?;

        let (mut send_stream, mut recv_stream) = connection.open_bi().await?;
        let my_name = self.settings.read().await.get_settings().device_name;
        let my_nonce = new_nonce();
        write_message(
            &mut send_stream,
            &MessageType::PairRequest {
                device_id: self.device_id.clone(),
                device_name: my_name,
                commitment: commitment(&my_nonce),
            },
        )
        .await?;

        let challenge = match tokio::time::timeout(CHALLENGE_TIMEOUT, read_message(&mut recv_stream)).await
        {
            Ok(res) => res?,
            Err(_) => return Err("The other device did not respond to the pairing request".into()),
        };
        let (peer_id, peer_name, peer_nonce) = match challenge {
            MessageType::PairChallenge {
                device_id,
                device_name,
                nonce,
            } => (device_id, device_name, nonce),
            MessageType::PairResult { reason, .. } => return Err(reason.into()),
            _ => return Err("Unexpected reply to the pairing request".into()),
        };
        if peer_id != device_id {
            return Err("The device at this address is not the one you selected".into());
        }

        write_message(&mut send_stream, &MessageType::PairReveal { nonce: my_nonce }).await?;

        let code = pairing_code(
            &PairParty {
                device_id: &self.device_id,
                fingerprint: &self.fingerprint,
            },
            &PairParty {
                device_id: &peer_id,
                fingerprint: &fingerprint,
            },
            &my_nonce,
            &peer_nonce,
        );

        Ok((
            code,
            OutgoingPairing {
                connection,
                send_stream,
                recv_stream,
                device_id: peer_id,
                device_name: peer_name,
                fingerprint,
                ip: ip.to_string(),
                port,
            },
        ))
    }

    /// Waits for the other user to type the code, then pins their key and
    /// tells the UI. Trust is only granted on this stream, which we opened.
    pub async fn finish_pairing(&self, mut pairing: OutgoingPairing) {
        let reply = tokio::time::timeout(
            PAIRING_TTL + Duration::from_secs(10),
            read_message(&mut pairing.recv_stream),
        )
        .await;

        let outcome: Result<(), String> = match reply {
            Ok(Ok(MessageType::PairResult { accepted: true, .. })) => {
                let mut security = self.security.write().await;
                match security.can_pair(&pairing.device_id, &pairing.fingerprint) {
                    Err(e) => Err(e.to_string()),
                    Ok(()) => security
                        .add_trusted(crate::crypto::security::TrustedDevice {
                            id: pairing.device_id.clone(),
                            name: pairing.device_name.clone(),
                            last_ip: pairing.ip.clone(),
                            last_port: pairing.port,
                            last_seen: chrono::Utc::now().timestamp(),
                            fingerprint: Some(pairing.fingerprint.clone()),
                        })
                        .map_err(|e| e.to_string()),
                }
            }
            Ok(Ok(MessageType::PairResult { reason, .. })) => Err(reason),
            Ok(Ok(_)) => Err("Unexpected reply during pairing".to_string()),
            Ok(Err(_)) => Err("The other device ended the pairing".to_string()),
            Err(_) => Err("Pairing timed out".to_string()),
        };

        if outcome.is_ok() {
            println!("[Pairing] Device {} is now trusted", pairing.device_id);
            let _ = write_message(&mut pairing.send_stream, &MessageType::PairConfirmed).await;
            let _ = pairing.send_stream.finish();
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        pairing
            .connection
            .close(quinn::VarInt::from_u32(0), b"pairing finished");

        let _ = self.app_handle.emit(
            "pairing-result",
            serde_json::json!({
                "deviceId": pairing.device_id,
                "deviceName": pairing.device_name,
                "accepted": outcome.is_ok(),
                "reason": outcome.err(),
            }),
        );
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

        // Nothing is sent unless the device proves it holds the paired key.
        let connection = match self
            .connect_verified(&device_id, &target_ip, target_port, TRANSFER_CONNECT_TIMEOUT)
            .await
        {
            Ok(connection) => connection,
            Err(e) => {
                let _ = self.app_handle.emit(
                    "send-failed",
                    serde_json::json!({
                        "deviceId": device_id,
                        "fileName": file_path.file_name().map(|n| n.to_string_lossy().to_string()),
                        "message": e.to_string(),
                    }),
                );
                return Err(e);
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
