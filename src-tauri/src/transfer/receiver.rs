use crate::transfer::protocol::MessageType;
use crate::transfer::sender::TransferProgress;
use quinn::Connection;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

use tauri::Emitter;

pub struct FileReceiver {
    save_directory: PathBuf,
    connection: Connection,
    app_handle: tauri::AppHandle,
    database: Arc<tokio::sync::RwLock<Option<crate::db::Database>>>,
    transfers: crate::TransferRegistry,
    security: Arc<tokio::sync::RwLock<crate::crypto::security::SecurityService>>,
}

impl FileReceiver {
    pub fn new(
        save_directory: PathBuf,
        connection: Connection,
        app_handle: tauri::AppHandle,
        database: Arc<tokio::sync::RwLock<Option<crate::db::Database>>>,
        transfers: crate::TransferRegistry,
        security: Arc<tokio::sync::RwLock<crate::crypto::security::SecurityService>>,
    ) -> Self {
        Self {
            save_directory,
            connection,
            app_handle,
            database,
            transfers,
            security,
        }
    }

    fn check_disk_space(&self, required_bytes: u64) -> Result<(), crate::GenericError> {
        let parent = self.save_directory.parent().unwrap_or(&self.save_directory);
        let space = fs2::available_space(parent)?;
        if space < required_bytes {
            return Err("Insufficient disk space".into());
        }
        Ok(())
    }

    pub async fn handle_transfer(&self) -> Result<(), crate::GenericError> {
        // Accept the single bidirectional stream from the sender
        let (mut send_stream, mut recv_stream) = self.connection.accept_bi().await?;

        let mut file: Option<File> = None;
        let mut bytes_received: u64 = 0;
        let mut current_transfer_id = String::new();
        let mut current_device_id = String::new();
        let mut current_file_name = String::new();
        let mut current_file_size: u64 = 0;
        let mut last_status = crate::TransferStatus::InProgress;
        let mut is_dir = false;

        let mut status_ticker = tokio::time::interval(std::time::Duration::from_millis(500));

        loop {
            // 1. Check local status changes
            if !current_transfer_id.is_empty() {
                let status = {
                    let registry = self.transfers.read().await;
                    registry
                        .get(&current_transfer_id)
                        .cloned()
                        .unwrap_or(last_status)
                };

                if status != last_status {
                    // Sync status to database
                    {
                        let db_lock = self.database.read().await;
                        if let Some(db) = &*db_lock {
                            let status_str = match status {
                                crate::TransferStatus::Pending => "pending",
                                crate::TransferStatus::InProgress => "in_progress",
                                crate::TransferStatus::Paused => "paused",
                                crate::TransferStatus::Cancelled => "cancelled",
                                crate::TransferStatus::Completed => "completed",
                                crate::TransferStatus::PartialSuccess => "partial_success",
                                crate::TransferStatus::Failed => "failed",
                            };
                            let _ = db
                                .update_transfer_status(
                                    &current_transfer_id,
                                    status_str,
                                    bytes_received as i64,
                                )
                                .await;
                        }
                    }

                    match status {
                        crate::TransferStatus::InProgress
                            if last_status == crate::TransferStatus::Pending =>
                        {
                            println!("[Receiver] User accepted file. Sending FileAccept...");
                            let _ = Self::write_message(
                                &mut send_stream,
                                &MessageType::FileAccept {
                                    transfer_id: current_transfer_id.clone(),
                                },
                            )
                            .await;

                            if !is_dir {
                                let path = self.save_directory.join(&current_file_name);
                                if let Some(parent) = path.parent() {
                                    let _ = tokio::fs::create_dir_all(parent).await;
                                }
                                let std_file = std::fs::OpenOptions::new()
                                    .write(true)
                                    .create(true)
                                    .truncate(true)
                                    .open(&path)
                                    .unwrap(); // Or handle error

                                use fs2::FileExt;
                                let _ = std_file.allocate(current_file_size);

                                file = Some(File::from_std(std_file));
                            } else {
                                let path = self.save_directory.join(&current_file_name);
                                let _ = tokio::fs::create_dir_all(&path).await;
                            }
                        }
                        crate::TransferStatus::Cancelled
                            if last_status == crate::TransferStatus::Pending =>
                        {
                            println!("[Receiver] User declined file. Sending FileReject...");
                            let _ = Self::write_message(
                                &mut send_stream,
                                &MessageType::FileReject {
                                    transfer_id: current_transfer_id.clone(),
                                    reason: "User declined".to_string(),
                                },
                            )
                            .await;
                            let _ = send_stream.finish();
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            return Err("Transfer declined by receiver".into());
                        }
                        crate::TransferStatus::Cancelled => {
                            println!("[Receiver] Sending TransferCancel to sender...");
                            let _ = Self::write_message(
                                &mut send_stream,
                                &MessageType::TransferCancel {
                                    transfer_id: current_transfer_id.clone(),
                                },
                            )
                            .await;
                            // Emit final cancelled status
                            let _ = self.app_handle.emit(
                                "transfer-progress",
                                TransferProgress {
                                    transfer_id: current_transfer_id.clone(),
                                    device_id: current_device_id.clone(),
                                    file_name: current_file_name.clone(),
                                    bytes_sent: bytes_received,
                                    total_bytes: current_file_size,
                                    direction: "receive".to_string(),
                                    status: "cancelled".to_string(),
                                    ..Default::default()
                                },
                            );
                            // Give sender time to read the message before closing socket
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            return Err("Transfer cancelled by receiver".into());
                        }
                        crate::TransferStatus::Paused => {
                            println!("[Receiver] Sending TransferPause to sender...");
                            let _ = Self::write_message(
                                &mut send_stream,
                                &MessageType::TransferPause {
                                    transfer_id: current_transfer_id.clone(),
                                },
                            )
                            .await;
                        }
                        crate::TransferStatus::InProgress
                            if last_status == crate::TransferStatus::Paused =>
                        {
                            println!("[Receiver] Sending TransferResume to sender...");
                            let _ = Self::write_message(
                                &mut send_stream,
                                &MessageType::TransferResume {
                                    transfer_id: current_transfer_id.clone(),
                                },
                            )
                            .await;
                        }
                        _ => {}
                    }

                    // Emit progress update when local status changes
                    let _ = self.app_handle.emit(
                        "transfer-progress",
                        TransferProgress {
                            transfer_id: current_transfer_id.clone(),
                            device_id: current_device_id.clone(),
                            file_name: current_file_name.clone(),
                            bytes_sent: bytes_received,
                            total_bytes: current_file_size,
                            direction: "receive".to_string(),
                            status: match status {
                                crate::TransferStatus::Paused => "paused",
                                crate::TransferStatus::Cancelled => "cancelled",
                                _ => "in_progress",
                            }
                            .to_string(),
                            ..Default::default()
                        },
                    );

                    last_status = status;
                }
            }

            // 2. Listen for network messages
            let msg_result = if last_status == crate::TransferStatus::Pending
                || last_status == crate::TransferStatus::Paused
                || current_transfer_id.is_empty()
            {
                tokio::select! {
                    res = Self::read_message(&mut recv_stream) => Some(res),
                    _ = status_ticker.tick() => None,
                }
            } else {
                Some(Self::read_message(&mut recv_stream).await)
            };

            if let Some(msg_result) = msg_result {
                let msg = match msg_result {
                    Ok(m) => m,
                    Err(e) => {
                        println!("[Receiver] Error reading from stream: {:?}", e);
                        if !current_transfer_id.is_empty() {
                            let mut transfers = self.transfers.write().await;
                            transfers
                                .insert(current_transfer_id.clone(), crate::TransferStatus::Failed);

                            let db_lock = self.database.read().await;
                            if let Some(db) = &*db_lock {
                                let _ = db
                                    .update_transfer_status(
                                        &current_transfer_id,
                                        "failed",
                                        bytes_received as i64,
                                    )
                                    .await;
                            }

                            let _ = self.app_handle.emit(
                                "transfer-progress",
                                TransferProgress {
                                    transfer_id: current_transfer_id.clone(),
                                    device_id: current_device_id.clone(),
                                    file_name: current_file_name.clone(),
                                    bytes_sent: bytes_received,
                                    total_bytes: current_file_size,
                                    direction: "receive".to_string(),
                                    status: "failed".to_string(),
                                    ..Default::default()
                                },
                            );
                        }
                        return Err(e);
                    }
                };

                match msg {
                    MessageType::Hello {
                        device_id,
                        device_name,
                    } => {
                        use tauri::Manager;
                        let state = self.app_handle.state::<crate::AppState>();

                        // 1. Get our own details
                        let my_id = {
                            let security = self.security.read().await;
                            security.get_device_id().to_string()
                        };
                        let my_name = {
                            let settings = state.settings.read().await;
                            settings.get_settings().device_name.clone()
                        };

                        // 2. Respond with HelloAck
                        let _ = Self::write_message(
                            &mut send_stream,
                            &MessageType::HelloAck {
                                device_id: my_id,
                                device_name: my_name,
                            },
                        )
                        .await;

                        // 3. Inject sender into discovery list so we can see them too!
                        if let Some(discovery) = state.discovery.read().await.as_ref() {
                            let ip = self.connection.remote_address().ip().to_string();
                            discovery
                                .add_manual_device(
                                    device_id.clone(),
                                    device_name.clone(),
                                    ip,
                                    51731,
                                )
                                .await;
                        }

                        println!(
                            "[Receiver] Handled Hello ping from {}: {}",
                            device_name, device_id
                        );

                        // Finish stream and wait to ensure QUIC delivers the ACK
                        let _ = send_stream.finish();
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

                        return Err("Ping connection closed".into());
                    }
                    MessageType::FileOffer {
                        transfer_id,
                        metadata,
                        sender_id,
                        sender_name,
                    } => {
                        // 1. Verify Unauthenticated connection
                        {
                            let security = self.security.read().await;
                            if !security.is_trusted(&sender_id) {
                                println!(
                                    "[Receiver] Rejecting connection from untrusted sender: {}",
                                    sender_id
                                );
                                let _ = Self::write_message(
                                    &mut send_stream,
                                    &MessageType::TransferError {
                                        transfer_id: transfer_id.clone(),
                                        message: "Device not trusted".to_string(),
                                    },
                                )
                                .await;
                                return Err("Untrusted device".into());
                            }
                        }

                        self.check_disk_space(metadata.size)?;

                        // 2. Fix Path Traversal
                        let file_name = std::path::Path::new(&metadata.name)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("unnamed_file");

                        let path = self.save_directory.join(file_name);

                        // Abort if path escapes directory (double check)
                        if !path.starts_with(&self.save_directory) {
                            return Err("Path traversal detected".into());
                        }

                        current_transfer_id = transfer_id.clone();
                        current_device_id = sender_id.clone();
                        current_file_name = file_name.to_string();
                        current_file_size = metadata.size;
                        is_dir = metadata.is_dir.unwrap_or(false);

                        // Update registry as Pending
                        {
                            let mut transfers = self.transfers.write().await;
                            transfers.insert(
                                current_transfer_id.clone(),
                                crate::TransferStatus::Pending,
                            );
                        }
                        last_status = crate::TransferStatus::Pending;

                        // Record the transfer start in database
                        {
                            let db_lock = self.database.read().await;
                            if let Some(db) = &*db_lock {
                                if let Err(e) = db
                                    .record_transfer(crate::db::TransferRecordArgs {
                                        id: &current_transfer_id,
                                        device_id: &sender_id,
                                        file_name: &current_file_name,
                                        file_path: &path.to_string_lossy(),
                                        total_size: current_file_size as i64,
                                        direction: "receive",
                                        file_hash: &metadata.hash,
                                    })
                                    .await
                                {
                                    println!("[Database] Failed to record transfer: {:?}", e);
                                }
                            }
                        }

                        // Emit event to UI asking for acceptance
                        let _ = self.app_handle.emit(
                            "file-offer-received",
                            serde_json::json!({
                                "transferId": current_transfer_id,
                                "fileName": current_file_name,
                                "fileSize": current_file_size,
                                "senderId": sender_id,
                                "senderName": sender_name,
                            }),
                        );
                    }
                    MessageType::DirectoryManifest { transfer_id, files } => {
                        if transfer_id == current_transfer_id {
                            let _ = self.app_handle.emit(
                                "folder-manifest",
                                serde_json::json!({
                                    "transfer_id": transfer_id,
                                    "files": files
                                }),
                            );
                            let base_path = self.save_directory.join(&current_file_name);
                            for file_entry in files {
                                // Protect against path traversal again
                                let rel_path = std::path::Path::new(&file_entry.relative_path);
                                let mut safe = true;
                                for comp in rel_path.components() {
                                    if matches!(
                                        comp,
                                        std::path::Component::ParentDir
                                            | std::path::Component::RootDir
                                    ) {
                                        safe = false;
                                    }
                                }
                                if safe {
                                    let full_path = base_path.join(rel_path);
                                    if let Some(parent) = full_path.parent() {
                                        let _ = tokio::fs::create_dir_all(parent).await;
                                    }
                                }
                            }
                        }
                    }
                    MessageType::FileStart {
                        transfer_id,
                        relative_path,
                        size,
                    } => {
                        if transfer_id == current_transfer_id {
                            // Close previous file if any
                            if let Some(mut f) = file.take() {
                                let _ = f.flush().await;
                            }

                            let rel_path = std::path::Path::new(&relative_path);
                            // Check traversal
                            let mut safe = true;
                            for comp in rel_path.components() {
                                if matches!(
                                    comp,
                                    std::path::Component::ParentDir | std::path::Component::RootDir
                                ) {
                                    safe = false;
                                }
                            }
                            if safe {
                                let base_path = if is_dir {
                                    self.save_directory.join(&current_file_name)
                                } else {
                                    self.save_directory.clone()
                                };
                                let full_path = base_path.join(rel_path);

                                if let Some(parent) = full_path.parent() {
                                    let _ = tokio::fs::create_dir_all(parent).await;
                                }

                                let std_file = std::fs::OpenOptions::new()
                                    .write(true)
                                    .create(true)
                                    .truncate(true)
                                    .open(&full_path)?;
                                use fs2::FileExt;
                                let _ = std_file.allocate(size);
                                file = Some(File::from_std(std_file));
                            }
                        }
                    }
                    MessageType::ChunkData {
                        transfer_id: _,
                        chunk_index: _,
                        chunk_size,
                        chunk_hash,
                    } => {
                        let mut data = vec![0u8; chunk_size as usize];
                        recv_stream.read_exact(&mut data).await?;

                        if let Some(ref mut f) = file {
                            // Verify chunk
                            let actual_hash = blake3::hash(&data).to_hex().to_string();
                            if actual_hash != chunk_hash {
                                return Err("Chunk hash mismatch".into());
                            }

                            f.write_all(&data).await?;
                            bytes_received += data.len() as u64;

                            // Emit progress event
                            let _ = self.app_handle.emit(
                                "transfer-progress",
                                TransferProgress {
                                    transfer_id: current_transfer_id.clone(),
                                    device_id: current_device_id.clone(),
                                    file_name: current_file_name.clone(),
                                    bytes_sent: bytes_received,
                                    total_bytes: current_file_size,
                                    direction: "receive".to_string(),
                                    status: match last_status {
                                        crate::TransferStatus::Paused => "paused",
                                        crate::TransferStatus::Cancelled => "cancelled",
                                        _ => "in_progress",
                                    }
                                    .to_string(),

                                    ..Default::default()
                                },
                            );
                        }
                    }
                    MessageType::TransferPause { transfer_id: _ } => {
                        println!("[Receiver] Transfer paused by sender");
                        {
                            let mut transfers = self.transfers.write().await;
                            transfers
                                .insert(current_transfer_id.clone(), crate::TransferStatus::Paused);
                        }
                        last_status = crate::TransferStatus::Paused;
                        // Emit progress event
                        let _ = self.app_handle.emit(
                            "transfer-progress",
                            TransferProgress {
                                transfer_id: current_transfer_id.clone(),
                                device_id: current_device_id.clone(),
                                file_name: current_file_name.clone(),
                                bytes_sent: bytes_received,
                                total_bytes: current_file_size,
                                direction: "receive".to_string(),
                                status: "paused".to_string(),

                                ..Default::default()
                            },
                        );
                    }
                    MessageType::TransferResume { transfer_id: _ } => {
                        println!("[Receiver] Transfer resumed by sender");
                        {
                            let mut transfers = self.transfers.write().await;
                            transfers.insert(
                                current_transfer_id.clone(),
                                crate::TransferStatus::InProgress,
                            );
                        }
                        last_status = crate::TransferStatus::InProgress;
                        // Emit progress event
                        let _ = self.app_handle.emit(
                            "transfer-progress",
                            TransferProgress {
                                transfer_id: current_transfer_id.clone(),
                                device_id: current_device_id.clone(),
                                file_name: current_file_name.clone(),
                                bytes_sent: bytes_received,
                                total_bytes: current_file_size,
                                direction: "receive".to_string(),
                                status: match last_status {
                                    crate::TransferStatus::Paused => "paused",
                                    crate::TransferStatus::Cancelled => "cancelled",
                                    _ => "in_progress",
                                }
                                .to_string(),

                                ..Default::default()
                            },
                        );
                    }
                    MessageType::TransferCancel { transfer_id: _ } => {
                        println!("[Receiver] Transfer cancelled by sender");
                        {
                            let mut transfers = self.transfers.write().await;
                            transfers.insert(
                                current_transfer_id.clone(),
                                crate::TransferStatus::Cancelled,
                            );
                        }
                        // Update database
                        {
                            let db_lock = self.database.read().await;
                            if let Some(db) = &*db_lock {
                                let _ = db
                                    .update_transfer_status(
                                        &current_transfer_id,
                                        "cancelled",
                                        bytes_received as i64,
                                    )
                                    .await;
                            }
                        }
                        // Emit progress event
                        let _ = self.app_handle.emit(
                            "transfer-progress",
                            TransferProgress {
                                transfer_id: current_transfer_id.clone(),
                                device_id: current_device_id.clone(),
                                file_name: current_file_name.clone(),
                                bytes_sent: bytes_received,
                                total_bytes: current_file_size,
                                direction: "receive".to_string(),
                                status: "cancelled".to_string(),

                                ..Default::default()
                            },
                        );
                        let _ = self.app_handle.emit("history-updated", ());
                        return Err("Transfer cancelled by sender".into());
                    }
                    MessageType::TransferComplete { transfer_id } => {
                        println!("[Transfer] Received TransferComplete, flushing file...");
                        if let Some(mut f) = file.take() {
                            f.flush().await?;
                        }
                        // Update status in database
                        {
                            let db_lock = self.database.read().await;
                            if let Some(db) = &*db_lock {
                                if let Err(e) = db
                                    .update_transfer_status(
                                        &transfer_id,
                                        "completed",
                                        current_file_size as i64,
                                    )
                                    .await
                                {
                                    println!(
                                        "[Database] Failed to update transfer status: {:?}",
                                        e
                                    );
                                }

                                // Automatic History Sync after completion
                                println!("[Transfer] Preparing automatic history sync...");
                                if let Ok(records) = db.get_transfer_history(50).await {
                                    println!(
                                        "[Transfer] Sending {} history records to sender...",
                                        records.len()
                                    );
                                    let _ = Self::write_message(
                                        &mut send_stream,
                                        &MessageType::HistorySync { records },
                                    )
                                    .await;
                                }
                            }
                        }

                        // Notify frontend that history changed
                        let _ = self.app_handle.emit("history-updated", ());
                        let _ = self.app_handle.emit(
                            "transfer-progress",
                            TransferProgress {
                                transfer_id: transfer_id.clone(),
                                device_id: current_device_id.clone(),
                                file_name: current_file_name.clone(),
                                bytes_sent: current_file_size,
                                total_bytes: current_file_size,
                                direction: "receive".to_string(),
                                status: "completed".to_string(),

                                ..Default::default()
                            },
                        );

                        println!("[Transfer] Sending TransferCompleteAck...");
                        // Send acknowledgment on the same stream
                        Self::write_message(
                            &mut send_stream,
                            &MessageType::TransferCompleteAck { transfer_id },
                        )
                        .await?;

                        println!("[Transfer] Finishing send stream...");
                        send_stream.finish()?;

                        // Give QUIC time to flush the ACK bytes over the wire
                        // before we return and the connection gets dropped
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                        // Explicitly close the connection gracefully
                        self.connection
                            .close(quinn::VarInt::from_u32(0), b"transfer complete");

                        println!("[Transfer] Transfer complete, breaking loop");
                        break;
                    }
                    MessageType::HistorySync { records } => {
                        println!(
                            "[Transfer] Received HistorySync with {} records",
                            records.len()
                        );
                        let db_lock = self.database.read().await;
                        if let Some(db) = &*db_lock {
                            for record in records {
                                let _ = db
                                    .record_transfer(crate::db::TransferRecordArgs {
                                        id: &record.id,
                                        device_id: &record.device_id,
                                        file_name: &record.file_name,
                                        file_path: &record.file_path,
                                        total_size: record.total_size,
                                        direction: &record.direction,
                                        file_hash: &record.file_hash,
                                    })
                                    .await;
                                let _ = db
                                    .update_transfer_status(
                                        &record.id,
                                        &record.status,
                                        record.bytes_transferred,
                                    )
                                    .await;
                            }
                        }
                        // Notify frontend that history changed
                        let _ = self.app_handle.emit("history-updated", ());
                    }
                    MessageType::PairRequest {
                        device_id,
                        device_name,
                        pairing_code,
                    } => {
                        let _ = self.app_handle.emit(
                            "pairing-request",
                            serde_json::json!({
                                "device": { "id": device_id, "name": device_name },
                                "code": pairing_code,
                                "ip": self.connection.remote_address().ip().to_string(),
                                "port": self.connection.remote_address().port()
                            }),
                        );
                    }
                    MessageType::PairResponse {
                        accepted,
                        device_id,
                        device_name,
                    } if accepted => {
                        let mut security = self.security.write().await;
                        let trusted_device = crate::crypto::security::TrustedDevice {
                            id: device_id.clone(),
                            name: device_name.clone(),
                            last_ip: self.connection.remote_address().ip().to_string(),
                            last_port: self.connection.remote_address().port(),
                            last_seen: std::time::UNIX_EPOCH.elapsed().unwrap().as_secs() as i64,
                        };
                        let _ = security.add_trusted(trusted_device);
                        println!("[Pairing] Device {} is now trusted", device_id);
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    async fn read_message(
        recv: &mut quinn::RecvStream,
    ) -> Result<MessageType, crate::GenericError> {
        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        let mut data = vec![0u8; len];
        recv.read_exact(&mut data).await?;

        let msg = bincode::deserialize(&data)?;
        Ok(msg)
    }

    async fn write_message(
        send: &mut quinn::SendStream,
        msg: &MessageType,
    ) -> Result<(), crate::GenericError> {
        let data = bincode::serialize(msg)?;
        let len = data.len() as u32;
        send.write_all(&len.to_be_bytes()).await?;
        send.write_all(&data).await?;
        use tokio::io::AsyncWriteExt;
        send.flush().await?;
        Ok(())
    }
}
