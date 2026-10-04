use crate::transfer::protocol::{
    read_message, write_message, FileMetadata, MessageType, SyncedTransfer, TransferOutcome,
    MAX_CHUNK_SIZE,
};
use crate::transfer::sender::TransferProgress;
use crate::crypto::security::PeerTrust;
use crate::pairing::{commitment, new_nonce, pairing_code, Decision, PairParty, PAIRING_TTL};
use crate::transfer::session::{
    resolve_destination, unique_name, FileVerdict, Phase, ReceiveSession,
};
use crate::TransferStatus;
use quinn::{Connection, RecvStream, SendStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, oneshot, RwLock};

/// How long to wait for the user to accept or decline (the sender gives up at 300s).
const DECISION_TIMEOUT: Duration = Duration::from_secs(310);
/// Minimum interval between progress events while data is flowing.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
/// Upper bound on history records accepted from a peer in one sync.
const MAX_SYNC_RECORDS: usize = 100;

/// What the background reader hands to the transfer loop. Reading happens in its
/// own task so the loop can `select!` on it without losing half-read frames.
enum Incoming {
    Message(MessageType),
    Chunk { transfer_id: String, data: Vec<u8> },
}

/// How a transfer ended without a protocol or IO error.
enum End {
    Completed(TransferOutcome),
    Declined,
    Cancelled,
}

struct RxState {
    session: ReceiveSession,
    file: Option<File>,
    last_status: TransferStatus,
    /// Path of the current file as the sender named it, for the UI's folder tree.
    current_display: Option<String>,
    last_emit: Instant,
}

pub struct FileReceiver {
    save_directory: PathBuf,
    connection: Connection,
    app_handle: tauri::AppHandle,
    database: Arc<RwLock<Option<crate::db::Database>>>,
    transfers: crate::TransferRegistry,
    security: Arc<RwLock<crate::crypto::security::SecurityService>>,
    renames: Arc<RwLock<std::collections::HashMap<String, String>>>,
    settings: Arc<RwLock<crate::settings::SettingsManager>>,
    pairing: Arc<RwLock<crate::pairing::PairingState>>,
    /// Fingerprint of our own key.
    my_fingerprint: String,
    /// Fingerprint of the key the peer proved it holds in the handshake.
    peer_fingerprint: String,
}

impl FileReceiver {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        save_directory: PathBuf,
        connection: Connection,
        app_handle: tauri::AppHandle,
        database: Arc<RwLock<Option<crate::db::Database>>>,
        transfers: crate::TransferRegistry,
        security: Arc<RwLock<crate::crypto::security::SecurityService>>,
        renames: Arc<RwLock<std::collections::HashMap<String, String>>>,
        settings: Arc<RwLock<crate::settings::SettingsManager>>,
        pairing: Arc<RwLock<crate::pairing::PairingState>>,
        my_fingerprint: String,
    ) -> Result<Self, crate::GenericError> {
        let peer_fingerprint = crate::crypto::encryption::peer_fingerprint(&connection)?;
        Ok(Self {
            save_directory,
            connection,
            app_handle,
            database,
            transfers,
            security,
            renames,
            settings,
            pairing,
            my_fingerprint,
            peer_fingerprint,
        })
    }

    async fn peer_trust(&self, device_id: &str) -> PeerTrust {
        self.security
            .read()
            .await
            .verify_peer(device_id, &self.peer_fingerprint)
    }

    fn check_disk_space(&self, required_bytes: u64) -> Result<(), crate::GenericError> {
        let space = fs2::available_space(&self.save_directory)?;
        if space < required_bytes {
            return Err("Insufficient disk space".into());
        }
        Ok(())
    }

    fn remote_ip(&self) -> String {
        self.connection.remote_address().ip().to_string()
    }

    /// Each connection carries exactly one purpose, decided by its first message.
    /// Control messages (ping, pairing, history) are never accepted mid-transfer.
    pub async fn handle_transfer(&self) -> Result<(), crate::GenericError> {
        let (mut send_stream, recv_stream) = self.connection.accept_bi().await?;
        let mut recv_stream = recv_stream;

        match read_message(&mut recv_stream).await? {
            MessageType::Hello {
                device_id,
                device_name,
            } => self.handle_hello(&mut send_stream, device_id, device_name).await,
            MessageType::PairRequest {
                device_id,
                device_name,
                commitment,
            } => {
                self.handle_pair_request(
                    send_stream,
                    recv_stream,
                    device_id,
                    device_name,
                    commitment,
                )
                .await
            }
            MessageType::HistorySync { sender_id, records } => {
                self.handle_history_sync(sender_id, records).await
            }
            MessageType::FileOffer {
                transfer_id,
                metadata,
                sender_id,
                sender_name,
            } => {
                self.receive(
                    send_stream,
                    recv_stream,
                    transfer_id,
                    metadata,
                    sender_id,
                    sender_name,
                )
                .await
            }
            _ => Err("Protocol violation: unexpected first message".into()),
        }
    }

    async fn handle_hello(
        &self,
        send_stream: &mut SendStream,
        device_id: String,
        device_name: String,
    ) -> Result<(), crate::GenericError> {
        let my_id = self.security.read().await.get_device_id().to_string();
        let my_name = self.settings.read().await.get_settings().device_name;

        write_message(
            send_stream,
            &MessageType::HelloAck {
                device_id: my_id,
                device_name: my_name,
            },
        )
        .await?;

        // Make the caller visible to us too. A paired device that proves its key
        // may update its address; anyone else may only add an id we don't know yet.
        let verified = self.peer_trust(&device_id).await == PeerTrust::Verified;
        if verified {
            self.security
                .write()
                .await
                .update_address(&device_id, &self.remote_ip(), crate::APP_PORT);
        }
        if let Some(state) = self.app_handle.try_state::<crate::AppState>() {
            if let Some(discovery) = state.discovery.read().await.as_ref() {
                if verified {
                    discovery
                        .add_manual_device(
                            device_id.clone(),
                            device_name.clone(),
                            self.remote_ip(),
                            crate::APP_PORT,
                        )
                        .await;
                } else {
                    discovery
                        .add_if_unknown(
                            device_id.clone(),
                            device_name.clone(),
                            self.remote_ip(),
                            crate::APP_PORT,
                        )
                        .await;
                }
            }
        }

        println!(
            "[Receiver] Handled Hello ping from {}: {}",
            device_name, device_id
        );

        // Finish stream and wait to ensure QUIC delivers the ACK
        let _ = send_stream.finish();
        tokio::time::sleep(Duration::from_millis(200)).await;
        Ok(())
    }

    /// Responder side of pairing. Keeps the stream open until our user types
    /// the code shown on the other device, declines, or the request expires.
    async fn handle_pair_request(
        &self,
        mut send_stream: SendStream,
        mut recv_stream: RecvStream,
        device_id: String,
        device_name: String,
        their_commitment: [u8; 32],
    ) -> Result<(), crate::GenericError> {
        if !crate::discovery::mdns::is_valid_device_id(&device_id) {
            return Err("Malformed pairing request".into());
        }
        if let Err(reason) = self
            .security
            .read()
            .await
            .can_pair(&device_id, &self.peer_fingerprint)
        {
            self.send_pair_result(&mut send_stream, false, reason).await;
            return Err(reason.into());
        }

        let my_id = self.security.read().await.get_device_id().to_string();
        let my_name = self.settings.read().await.get_settings().device_name;
        let my_nonce = new_nonce();
        write_message(
            &mut send_stream,
            &MessageType::PairChallenge {
                device_id: my_id.clone(),
                device_name: my_name,
                nonce: my_nonce,
            },
        )
        .await?;

        let their_nonce = match tokio::time::timeout(
            Duration::from_secs(30),
            read_message(&mut recv_stream),
        )
        .await
        {
            Ok(Ok(MessageType::PairReveal { nonce })) => nonce,
            Ok(Ok(_)) => return Err("Protocol violation: expected PairReveal".into()),
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err("Pairing request timed out".into()),
        };
        if commitment(&their_nonce) != their_commitment {
            return Err("Pairing nonce does not match its commitment".into());
        }

        let code = pairing_code(
            &PairParty {
                device_id: &device_id,
                fingerprint: &self.peer_fingerprint,
            },
            &PairParty {
                device_id: &my_id,
                fingerprint: &self.my_fingerprint,
            },
            &their_nonce,
            &my_nonce,
        );

        let ip = self.remote_ip();
        let port = self.connection.remote_address().port();
        let (decision_tx, decision_rx) = oneshot::channel::<Decision>();
        let added = self.pairing.write().await.add_incoming(
            &device_id,
            &device_name,
            &ip,
            port,
            &code,
            decision_tx,
        );
        if !added {
            self.send_pair_result(&mut send_stream, false, "A pairing request from this device is already open")
                .await;
            return Err("Duplicate pairing request".into());
        }

        // The code is never sent to the UI: the user must type what the other screen shows.
        let _ = self.app_handle.emit(
            "pairing-request",
            serde_json::json!({ "device": { "id": device_id, "name": device_name } }),
        );

        let cancelled = |reason: &str| {
            let _ = self.app_handle.emit(
                "pairing-cancelled",
                serde_json::json!({ "deviceId": device_id, "reason": reason }),
            );
        };

        tokio::select! {
            decision = decision_rx => match decision {
                Ok(Decision { accepted: true, done }) => {
                    let result = self
                        .complete_pairing(&mut send_stream, &mut recv_stream, &device_id, &device_name, &ip, port)
                        .await;
                    let _ = done.send(result.clone());
                    result.map_err(|e| e.into())
                }
                Ok(Decision { accepted: false, done }) => {
                    self.send_pair_result(&mut send_stream, false, "Pairing was declined").await;
                    let _ = done.send(Ok(()));
                    Ok(())
                }
                Err(_) => {
                    // Too many wrong codes, or the request expired.
                    self.send_pair_result(&mut send_stream, false, "The pairing code was not confirmed").await;
                    cancelled("The pairing code was not confirmed");
                    Ok(())
                }
            },
            _ = read_message(&mut recv_stream) => {
                // The other side only speaks again after our result, so anything
                // here (usually the stream closing) means it gave up.
                self.pairing.write().await.remove_incoming(&device_id);
                cancelled("The other device cancelled pairing");
                Ok(())
            }
            _ = tokio::time::sleep(PAIRING_TTL) => {
                self.pairing.write().await.remove_incoming(&device_id);
                self.send_pair_result(&mut send_stream, false, "Pairing timed out").await;
                cancelled("Pairing timed out");
                Ok(())
            }
        }
    }

    /// Our user typed the right code: tell the initiator, wait until it has
    /// stored the pairing, then pin its key on our side too.
    async fn complete_pairing(
        &self,
        send_stream: &mut SendStream,
        recv_stream: &mut RecvStream,
        device_id: &str,
        device_name: &str,
        ip: &str,
        port: u16,
    ) -> Result<(), String> {
        write_message(
            send_stream,
            &MessageType::PairResult {
                accepted: true,
                reason: String::new(),
            },
        )
        .await
        .map_err(|e| format!("The other device is no longer reachable: {}", e))?;

        match tokio::time::timeout(Duration::from_secs(10), read_message(recv_stream)).await {
            Ok(Ok(MessageType::PairConfirmed)) => {}
            _ => return Err("The other device did not confirm the pairing".to_string()),
        }

        let mut security = self.security.write().await;
        security.can_pair(device_id, &self.peer_fingerprint)?;
        security
            .add_trusted(crate::crypto::security::TrustedDevice {
                id: device_id.to_string(),
                name: device_name.to_string(),
                last_ip: ip.to_string(),
                last_port: port,
                last_seen: chrono::Utc::now().timestamp(),
                fingerprint: Some(self.peer_fingerprint.clone()),
            })
            .map_err(|e| e.to_string())?;
        println!("[Pairing] Device {} is now trusted", device_id);
        Ok(())
    }

    async fn send_pair_result(&self, send_stream: &mut SendStream, accepted: bool, reason: &str) {
        let _ = write_message(
            send_stream,
            &MessageType::PairResult {
                accepted,
                reason: reason.to_string(),
            },
        )
        .await;
        let _ = send_stream.finish();
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    async fn handle_history_sync(
        &self,
        sender_id: String,
        records: Vec<SyncedTransfer>,
    ) -> Result<(), crate::GenericError> {
        if self.peer_trust(&sender_id).await != PeerTrust::Verified {
            return Err("Ignoring history from an unverified device".into());
        }

        println!(
            "[Transfer] Received HistorySync with {} records from {}",
            records.len(),
            sender_id
        );
        if let Some(db) = &*self.database.read().await {
            for record in records.iter().take(MAX_SYNC_RECORDS) {
                if let Err(e) = db.import_peer_record(&sender_id, record).await {
                    println!("[Database] Failed to import synced record: {:?}", e);
                }
            }
        }
        let _ = self.app_handle.emit("history-updated", ());
        Ok(())
    }

    async fn reject_offer(&self, send_stream: &mut SendStream, transfer_id: &str, message: &str) {
        let _ = write_message(
            send_stream,
            &MessageType::TransferError {
                transfer_id: transfer_id.to_string(),
                message: message.to_string(),
            },
        )
        .await;
        let _ = send_stream.finish();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    async fn receive(
        &self,
        mut send_stream: SendStream,
        recv_stream: RecvStream,
        transfer_id: String,
        metadata: FileMetadata,
        sender_id: String,
        sender_name: String,
    ) -> Result<(), crate::GenericError> {
        let trust = self.peer_trust(&sender_id).await;
        if trust != PeerTrust::Verified {
            println!(
                "[Receiver] Rejecting offer from {}: {:?}",
                sender_id, trust
            );
            self.reject_offer(&mut send_stream, &transfer_id, trust.rejection())
                .await;
            return Err(trust.rejection().into());
        }

        let mut session = ReceiveSession::new();
        if let Err(e) = session.on_offer(&transfer_id, &sender_id, &metadata) {
            self.reject_offer(&mut send_stream, &transfer_id, &e.to_string())
                .await;
            return Err(e.into());
        }

        // Auto-accept only ever applies to verified devices, and never overwrites.
        let auto_accept = self.settings.read().await.get_settings().auto_accept;
        if auto_accept {
            let name = session.offer().unwrap().name.clone();
            let free_name = unique_name(&self.save_directory, &name);
            if free_name != name {
                session.rename(&free_name)?;
            }
        }
        if self.check_disk_space(metadata.size).is_err() {
            self.reject_offer(
                &mut send_stream,
                &transfer_id,
                "Insufficient disk space on receiver",
            )
            .await;
            return Err("Insufficient disk space".into());
        }

        {
            let mut registry = self.transfers.write().await;
            if registry.contains_key(&transfer_id) {
                drop(registry);
                self.reject_offer(&mut send_stream, &transfer_id, "Duplicate transfer id")
                    .await;
                return Err("Duplicate transfer id".into());
            }
            // Auto-accepted offers start as accepted; the transfer loop picks that up.
            let initial = if auto_accept {
                TransferStatus::InProgress
            } else {
                TransferStatus::Pending
            };
            registry.insert(transfer_id.clone(), initial);
        }

        let result = self
            .run_session(send_stream, recv_stream, session, sender_name, &metadata, auto_accept)
            .await;

        self.transfers.write().await.remove(&transfer_id);
        self.renames.write().await.remove(&transfer_id);
        result
    }

    async fn run_session(
        &self,
        mut send_stream: SendStream,
        recv_stream: RecvStream,
        session: ReceiveSession,
        sender_name: String,
        metadata: &FileMetadata,
        auto_accepted: bool,
    ) -> Result<(), crate::GenericError> {
        let offer = session.offer().cloned().expect("offer validated");
        let path = self.save_directory.join(&offer.name);

        if let Some(db) = &*self.database.read().await {
            if let Err(e) = db
                .record_transfer(crate::db::TransferRecordArgs {
                    id: &offer.transfer_id,
                    device_id: &offer.sender_id,
                    file_name: &offer.name,
                    file_path: &path.to_string_lossy(),
                    total_size: offer.total_size as i64,
                    direction: "receive",
                    file_hash: "",
                    is_dir: offer.is_dir,
                    folder_manifest: None,
                })
                .await
            {
                println!("[Database] Failed to record transfer: {:?}", e);
            }
        }

        if auto_accepted {
            let _ = self.app_handle.emit(
                "file-auto-accepted",
                serde_json::json!({
                    "transferId": offer.transfer_id,
                    "fileName": offer.name,
                    "senderId": offer.sender_id,
                    "senderName": sender_name,
                }),
            );
        } else {
            // Ask the user
            let _ = self.app_handle.emit(
                "file-offer-received",
                serde_json::json!({
                "transferId": offer.transfer_id,
                "fileName": offer.name,
                "fileSize": offer.total_size,
                "senderId": offer.sender_id,
                "senderName": sender_name,
                "isDir": offer.is_dir,
                "fileCount": metadata.file_count,
                "subfolderCount": metadata.subfolder_count,
                "topExtensions": metadata.top_extensions,
                "fileExists": path.exists(),
            }),
            );
        }

        let (tx, mut rx) = mpsc::channel(4);
        let reader = tokio::spawn(read_loop(recv_stream, tx));

        let mut st = RxState {
            session,
            file: None,
            last_status: TransferStatus::Pending,
            current_display: None,
            last_emit: Instant::now(),
        };

        let result = self.drive(&mut send_stream, &mut rx, &mut st).await;
        reader.abort();

        let bytes = st.session.bytes_received() as i64;
        match result {
            Ok(End::Completed(outcome)) => {
                self.set_db_status(&offer.transfer_id, outcome.as_status(), bytes)
                    .await;
                self.emit_progress(&st, outcome.as_status());
                let _ = self.app_handle.emit("history-updated", ());
                Ok(())
            }
            Ok(End::Declined) | Ok(End::Cancelled) => {
                self.discard_current(&mut st).await;
                self.set_db_status(&offer.transfer_id, "cancelled", bytes)
                    .await;
                self.emit_progress(&st, "cancelled");
                let _ = self.app_handle.emit("history-updated", ());
                Ok(())
            }
            Err(e) => {
                println!("[Receiver] Transfer {} failed: {}", offer.transfer_id, e);
                self.discard_current(&mut st).await;
                let _ = write_message(
                    &mut send_stream,
                    &MessageType::TransferError {
                        transfer_id: offer.transfer_id.clone(),
                        message: e.to_string(),
                    },
                )
                .await;
                let _ = send_stream.finish();
                self.set_db_status(&offer.transfer_id, "failed", bytes).await;
                self.emit_progress(&st, "failed");
                let _ = self.app_handle.emit("history-updated", ());
                tokio::time::sleep(Duration::from_millis(100)).await;
                Err(e)
            }
        }
    }

    async fn drive(
        &self,
        send_stream: &mut SendStream,
        rx: &mut mpsc::Receiver<Result<Incoming, crate::GenericError>>,
        st: &mut RxState,
    ) -> Result<End, crate::GenericError> {
        let transfer_id = st.session.offer().unwrap().transfer_id.clone();
        let decision_deadline = Instant::now() + DECISION_TIMEOUT;
        let mut ticker = tokio::time::interval(Duration::from_millis(500));

        loop {
            tokio::select! {
                incoming = rx.recv() => {
                    let incoming = match incoming {
                        Some(Ok(incoming)) => incoming,
                        Some(Err(e)) => return Err(e),
                        None => return Err("Connection closed by sender".into()),
                    };
                    if let Some(end) = self.handle_incoming(incoming, send_stream, st).await? {
                        return Ok(end);
                    }
                }
                _ = ticker.tick() => {
                    if st.session.phase() == Phase::AwaitingDecision && Instant::now() > decision_deadline {
                        st.session.on_reject();
                        let _ = write_message(send_stream, &MessageType::FileReject {
                            transfer_id: transfer_id.clone(),
                            reason: "No response from receiver".to_string(),
                        }).await;
                        let _ = send_stream.finish();
                        return Ok(End::Declined);
                    }

                    let status = self
                        .transfers
                        .read()
                        .await
                        .get(&transfer_id)
                        .cloned()
                        .unwrap_or(st.last_status);
                    if status != st.last_status {
                        let previous = st.last_status;
                        st.last_status = status;
                        if let Some(end) = self.handle_local_status(status, previous, send_stream, st).await? {
                            return Ok(end);
                        }
                    }
                }
            }
        }
    }

    /// Reacts to the local user accepting, declining, pausing, resuming or cancelling.
    async fn handle_local_status(
        &self,
        status: TransferStatus,
        previous: TransferStatus,
        send_stream: &mut SendStream,
        st: &mut RxState,
    ) -> Result<Option<End>, crate::GenericError> {
        let transfer_id = st.session.offer().unwrap().transfer_id.clone();

        match (status, previous) {
            (TransferStatus::InProgress, TransferStatus::Pending) => {
                if let Some(new_name) = self.renames.write().await.remove(&transfer_id) {
                    st.session.rename(&new_name)?;
                }
                st.session.on_accept()?;

                let offer = st.session.offer().unwrap().clone();
                if offer.is_dir {
                    let root = resolve_destination(&self.save_directory, Path::new(&offer.name))?;
                    tokio::fs::create_dir_all(&root).await?;
                }
                if let Some(db) = &*self.database.read().await {
                    let path = self.save_directory.join(&offer.name);
                    let _ = db
                        .update_file_location(&transfer_id, &offer.name, &path.to_string_lossy())
                        .await;
                    let _ = db.update_status_only(&transfer_id, "in_progress").await;
                }

                println!("[Receiver] Offer accepted. Sending FileAccept...");
                write_message(send_stream, &MessageType::FileAccept { transfer_id }).await?;
                self.emit_progress(st, "in_progress");
                Ok(None)
            }
            (TransferStatus::Cancelled, TransferStatus::Pending) => {
                println!("[Receiver] User declined file. Sending FileReject...");
                st.session.on_reject();
                let _ = write_message(
                    send_stream,
                    &MessageType::FileReject {
                        transfer_id,
                        reason: "User declined".to_string(),
                    },
                )
                .await;
                let _ = send_stream.finish();
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok(Some(End::Declined))
            }
            (TransferStatus::Cancelled, _) => {
                println!("[Receiver] Sending TransferCancel to sender...");
                let _ = write_message(send_stream, &MessageType::TransferCancel { transfer_id })
                    .await;
                // Give sender time to read the message before closing socket
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok(Some(End::Cancelled))
            }
            (TransferStatus::Paused, _) => {
                println!("[Receiver] Sending TransferPause to sender...");
                write_message(send_stream, &MessageType::TransferPause { transfer_id }).await?;
                self.emit_progress(st, "paused");
                Ok(None)
            }
            (TransferStatus::InProgress, TransferStatus::Paused) => {
                println!("[Receiver] Sending TransferResume to sender...");
                write_message(send_stream, &MessageType::TransferResume { transfer_id }).await?;
                self.emit_progress(st, "in_progress");
                Ok(None)
            }
            _ => Ok(None),
        }
    }

    /// Applies one message from the sender. Anything the session doesn't allow
    /// in the current state is an error and ends the transfer.
    async fn handle_incoming(
        &self,
        incoming: Incoming,
        send_stream: &mut SendStream,
        st: &mut RxState,
    ) -> Result<Option<End>, crate::GenericError> {
        let msg = match incoming {
            Incoming::Chunk { transfer_id, data } => {
                st.session.on_chunk(&transfer_id, data.len())?;
                let file = st.file.as_mut().ok_or("Chunk received without an open file")?;
                file.write_all(&data).await?;
                st.session.absorb(&data);

                if st.last_emit.elapsed() >= PROGRESS_INTERVAL {
                    st.last_emit = Instant::now();
                    let status = if st.last_status == TransferStatus::Paused {
                        "paused"
                    } else {
                        "in_progress"
                    };
                    self.emit_progress(st, status);
                }
                return Ok(None);
            }
            Incoming::Message(msg) => msg,
        };

        match msg {
            MessageType::DirectoryManifest { transfer_id, files } => {
                let paths = st.session.on_manifest(&transfer_id, &files)?;
                for rel in &paths {
                    resolve_destination(&self.save_directory, rel)?;
                }

                if let Ok(manifest_json) = serde_json::to_string(&files) {
                    if let Some(db) = &*self.database.read().await {
                        let _ = db.update_folder_manifest(&transfer_id, &manifest_json).await;
                    }
                }
                let _ = self.app_handle.emit(
                    "folder-manifest",
                    serde_json::json!({ "transfer_id": transfer_id, "files": files }),
                );
            }
            MessageType::FileStart {
                transfer_id,
                relative_path,
                size,
            } => {
                let rel = st.session.on_file_start(&transfer_id, &relative_path, size)?;
                let full_path = resolve_destination(&self.save_directory, &rel)?;

                let std_file = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&full_path)?;
                use fs2::FileExt;
                let _ = std_file.allocate(size);
                st.file = Some(File::from_std(std_file));
                st.current_display = Some(relative_path);
                self.emit_progress(st, "in_progress");
            }
            MessageType::FileEnd {
                transfer_id,
                bytes,
                hash,
            } => {
                let result = st.session.on_file_end(&transfer_id, bytes, &hash)?;
                if let Some(mut file) = st.file.take() {
                    file.flush().await?;
                    // Drop the zero padding left by pre-allocation if the file came up short.
                    file.set_len(result.received).await?;
                }
                if let FileVerdict::Corrupt(reason) = &result.verdict {
                    println!(
                        "[Receiver] Discarding {:?}: {}",
                        result.path, reason
                    );
                    let _ = tokio::fs::remove_file(self.save_directory.join(&result.path)).await;
                }
                self.emit_progress(st, "in_progress");
            }
            MessageType::TransferPause { transfer_id } => {
                st.session.on_remote_control(&transfer_id, false)?;
                println!("[Receiver] Transfer paused by sender");
                self.transfers
                    .write()
                    .await
                    .insert(transfer_id.clone(), TransferStatus::Paused);
                st.last_status = TransferStatus::Paused;
                if let Some(db) = &*self.database.read().await {
                    let _ = db.update_status_only(&transfer_id, "paused").await;
                }
                self.emit_progress(st, "paused");
            }
            MessageType::TransferResume { transfer_id } => {
                st.session.on_remote_control(&transfer_id, false)?;
                println!("[Receiver] Transfer resumed by sender");
                self.transfers
                    .write()
                    .await
                    .insert(transfer_id.clone(), TransferStatus::InProgress);
                st.last_status = TransferStatus::InProgress;
                if let Some(db) = &*self.database.read().await {
                    let _ = db.update_status_only(&transfer_id, "in_progress").await;
                }
                self.emit_progress(st, "in_progress");
            }
            MessageType::TransferCancel { transfer_id } => {
                st.session.on_remote_control(&transfer_id, true)?;
                println!("[Receiver] Transfer cancelled by sender");
                return Ok(Some(End::Cancelled));
            }
            MessageType::TransferComplete { transfer_id } => {
                println!("[Transfer] Received TransferComplete");
                let (outcome, unfinished) = st.session.on_complete(&transfer_id)?;
                if let Some(rel) = unfinished {
                    st.file.take();
                    let _ = tokio::fs::remove_file(self.save_directory.join(rel)).await;
                }

                write_message(
                    send_stream,
                    &MessageType::TransferCompleteAck {
                        transfer_id,
                        outcome,
                    },
                )
                .await?;
                send_stream.finish()?;

                // Give QUIC time to flush the ACK bytes over the wire
                // before we return and the connection gets dropped
                tokio::time::sleep(Duration::from_millis(500)).await;
                self.connection
                    .close(quinn::VarInt::from_u32(0), b"transfer complete");
                return Ok(Some(End::Completed(outcome)));
            }
            _ => return Err("Protocol violation: unexpected message during transfer".into()),
        }
        Ok(None)
    }

    /// Closes and deletes the file that was being written, if any.
    async fn discard_current(&self, st: &mut RxState) {
        st.file.take();
        if let Some(rel) = st.session.current_path() {
            let _ = tokio::fs::remove_file(self.save_directory.join(rel)).await;
        }
    }

    async fn set_db_status(&self, transfer_id: &str, status: &str, bytes: i64) {
        if let Some(db) = &*self.database.read().await {
            if let Err(e) = db.update_transfer_status(transfer_id, status, bytes).await {
                println!("[Database] Failed to update transfer status: {:?}", e);
            }
        }
    }

    fn emit_progress(&self, st: &RxState, status: &str) {
        let Some(offer) = st.session.offer() else {
            return;
        };
        let (current_sent, current_total) = match st.session.current_progress() {
            Some((sent, total)) => (Some(sent), Some(total)),
            None => (None, None),
        };
        let _ = self.app_handle.emit(
            "transfer-progress",
            TransferProgress {
                transfer_id: offer.transfer_id.clone(),
                device_id: offer.sender_id.clone(),
                file_name: offer.name.clone(),
                bytes_sent: st.session.bytes_received(),
                total_bytes: offer.total_size,
                direction: "receive".to_string(),
                status: status.to_string(),
                current_file_path: st.current_display.clone(),
                current_file_sent: current_sent,
                current_file_total: current_total,
            },
        );
    }
}

/// Reads frames (and chunk payloads) off the stream and forwards them. Chunk
/// sizes are bounded before allocating; state checks happen in the session.
async fn read_loop(
    mut recv_stream: RecvStream,
    tx: mpsc::Sender<Result<Incoming, crate::GenericError>>,
) {
    loop {
        let item: Result<Incoming, crate::GenericError> = match read_message(&mut recv_stream).await
        {
            Ok(MessageType::ChunkData {
                transfer_id,
                chunk_size,
            }) => {
                let len = chunk_size as usize;
                if len == 0 || len > MAX_CHUNK_SIZE {
                    Err(format!("Protocol violation: invalid chunk size {}", len).into())
                } else {
                    let mut data = vec![0u8; len];
                    match recv_stream.read_exact(&mut data).await {
                        Ok(()) => Ok(Incoming::Chunk { transfer_id, data }),
                        Err(e) => Err(e.into()),
                    }
                }
            }
            Ok(msg) => Ok(Incoming::Message(msg)),
            Err(e) => Err(e),
        };

        let stop = item.is_err();
        if tx.send(item).await.is_err() || stop {
            break;
        }
    }
}
