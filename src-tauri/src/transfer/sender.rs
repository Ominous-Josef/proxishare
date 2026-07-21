use crate::transfer::protocol::{FileMetadata, MessageType};
use bincode;
use quinn::{Connection, RecvStream, SendStream};
use serde::Serialize;
use std::path::PathBuf;
use tauri::{Emitter, Manager};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

/// Maximum chunk size (4MB) - used for large files
const MAX_CHUNK_SIZE: usize = 4 * 1024 * 1024;
/// Minimum chunk size (64KB) - used for small files or slow connections
const MIN_CHUNK_SIZE: usize = 64 * 1024;
/// Default chunk size (1MB) - balanced for most scenarios
const DEFAULT_CHUNK_SIZE: usize = 1024 * 1024;

#[derive(Clone, Serialize)]
pub struct TransferProgress {
    pub transfer_id: String,
    pub file_name: String,
    pub bytes_sent: u64,
    pub total_bytes: u64,
    pub direction: String,
    pub status: String,
}

/// Calculate optimal chunk size based on file size
/// Smaller files use smaller chunks to reduce overhead
/// Larger files use larger chunks for efficiency
fn calculate_chunk_size(file_size: u64) -> usize {
    if file_size < 1024 * 1024 {
        // Files < 1MB: use 64KB chunks
        MIN_CHUNK_SIZE
    } else if file_size < 100 * 1024 * 1024 {
        // Files < 100MB: use 1MB chunks
        DEFAULT_CHUNK_SIZE
    } else {
        // Large files: use 4MB chunks
        MAX_CHUNK_SIZE
    }
}

pub struct FileSender {
    connection: Connection,
    app_handle: tauri::AppHandle,
    device_id: String,
    device_name: String,
}

impl FileSender {
    pub fn new(
        connection: Connection,
        app_handle: tauri::AppHandle,
        device_id: String,
        device_name: String,
    ) -> Self {
        Self {
            connection,
            app_handle,
            device_id,
            device_name,
        }
    }

    pub async fn calculate_hash(&self, path: &PathBuf) -> Result<String, crate::GenericError> {
        let mut file = File::open(path).await?;
        let mut hasher = blake3::Hasher::new();
        let mut buffer = vec![0u8; 64 * 1024];

        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        Ok(hasher.finalize().to_hex().to_string())
    }

    pub async fn send_file(
        &self,
        transfer_id: String,
        path: PathBuf,
        transfers: crate::TransferRegistry,
    ) -> Result<(), crate::GenericError> {
        // Open a single bidirectional stream for the entire transfer
        let (mut send_stream, mut recv_stream) = self.connection.open_bi().await?;

        let mut file = File::open(&path).await?;
        let metadata = file.metadata().await?;
        let file_size = metadata.len();
        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        let file_hash = self.calculate_hash(&path).await?;

        // Calculate optimal chunk size based on file size
        let chunk_size = calculate_chunk_size(file_size);

        // 1. Send File Offer
        let offer = MessageType::FileOffer {
            transfer_id: transfer_id.clone(),
            metadata: FileMetadata {
                name: file_name.clone(),
                size: file_size,
                hash: file_hash,
                chunk_size: chunk_size as u32,
            },
            sender_id: self.device_id.clone(),
            sender_name: self.device_name.clone(),
        };
        Self::write_message(&mut send_stream, &offer).await?;

        // Emit initial progress so UI knows we're waiting
        let _ = self.app_handle.emit(
            "transfer-progress",
            TransferProgress {
                transfer_id: transfer_id.clone(),
                file_name: file_name.clone(),
                bytes_sent: 0,
                total_bytes: file_size,
                direction: "send".to_string(),
                status: "pending".to_string(),
            },
        );

        // Wait for FileAccept or FileReject
        println!("[Transfer] Waiting for receiver to accept file...");
        let acceptance_timeout = std::time::Duration::from_secs(300); // 5 minutes
        match tokio::time::timeout(
            acceptance_timeout,
            Self::read_message(&mut recv_stream),
        ).await {
            Ok(Ok(MessageType::FileAccept { transfer_id: ack_id })) if ack_id == transfer_id => {
                println!("[Transfer] Receiver accepted the file. Starting chunks...");
                let mut registry = transfers.write().await;
                registry.insert(transfer_id.clone(), crate::TransferStatus::InProgress);
            }
            Ok(Ok(MessageType::FileReject { transfer_id: _, reason })) => {
                println!("[Transfer] Receiver rejected the file: {}", reason);
                let _ = self.app_handle.emit("transfer-progress", TransferProgress {
                    transfer_id: transfer_id.clone(),
                    file_name: file_name.clone(),
                    bytes_sent: 0,
                    total_bytes: file_size,
                    direction: "send".to_string(),
                    status: "cancelled".to_string(),
                });
                let mut registry = transfers.write().await;
                registry.insert(transfer_id.clone(), crate::TransferStatus::Cancelled);
                return Err(format!("Transfer rejected: {}", reason).into());
            }
            Ok(Ok(MessageType::TransferError { transfer_id: _, message })) => {
                println!("[Transfer] Transfer Error: {}", message);
                let _ = self.app_handle.emit("transfer-progress", TransferProgress {
                    transfer_id: transfer_id.clone(),
                    file_name: file_name.clone(),
                    bytes_sent: 0,
                    total_bytes: file_size,
                    direction: "send".to_string(),
                    status: "failed".to_string(),
                });
                let mut registry = transfers.write().await;
                registry.insert(transfer_id.clone(), crate::TransferStatus::Failed);
                return Err(format!("Transfer error: {}", message).into());
            }
            Ok(Ok(_)) => return Err("Unexpected message while waiting for file acceptance".into()),
            Ok(Err(e)) => return Err(format!("Failed to receive file acceptance: {}", e).into()),
            Err(_) => {
                println!("[Transfer] Timeout waiting for receiver to accept");
                let _ = self.app_handle.emit("transfer-progress", TransferProgress {
                    transfer_id: transfer_id.clone(),
                    file_name: file_name.clone(),
                    bytes_sent: 0,
                    total_bytes: file_size,
                    direction: "send".to_string(),
                    status: "cancelled".to_string(),
                });
                let mut registry = transfers.write().await;
                registry.insert(transfer_id.clone(), crate::TransferStatus::Cancelled);
                return Err("Timeout waiting for receiver to accept file".into());
            }
        }

        // 2. Send Chunks
        let mut buffer = vec![0u8; chunk_size];
        let mut chunk_index = 0;
        let mut total_sent: u64 = 0;

        let mut last_status = crate::TransferStatus::InProgress;

        enum SenderTaskMessage {
            AckReceived,
            HistorySync(Vec<crate::db::TransferRecord>),
            Error(crate::GenericError),
        }

        let (tx, mut rx) = tokio::sync::mpsc::channel::<SenderTaskMessage>(10);
        let transfer_id_clone = transfer_id.clone();
        let transfers_clone = transfers.clone();
        
        let mut recv_stream = recv_stream;

        tokio::spawn(async move {
            loop {
                match Self::read_message(&mut recv_stream).await {
                    Ok(msg) => {
                        match msg {
                            MessageType::TransferPause { .. } => {
                                println!("[Sender] Receiver paused the transfer");
                                let mut registry = transfers_clone.write().await;
                                registry.insert(transfer_id_clone.clone(), crate::TransferStatus::Paused);
                            }
                            MessageType::TransferResume { .. } => {
                                println!("[Sender] Receiver resumed the transfer");
                                let mut registry = transfers_clone.write().await;
                                registry.insert(transfer_id_clone.clone(), crate::TransferStatus::InProgress);
                            }
                            MessageType::TransferCancel { .. } => {
                                println!("[Sender] Receiver cancelled the transfer");
                                let mut registry = transfers_clone.write().await;
                                registry.insert(transfer_id_clone.clone(), crate::TransferStatus::Cancelled);
                                let _ = tx.send(SenderTaskMessage::Error("Transfer cancelled by receiver".into())).await;
                                break;
                            }
                            MessageType::TransferCompleteAck { .. } => {
                                let _ = tx.send(SenderTaskMessage::AckReceived).await;
                                break;
                            }
                            MessageType::HistorySync { records } => {
                                let _ = tx.send(SenderTaskMessage::HistorySync(records)).await;
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(SenderTaskMessage::Error(e)).await;
                        break;
                    }
                }
            }
        });

        loop {
            // Check for background task messages (e.g. cancellation)
            if let Ok(SenderTaskMessage::Error(e)) = rx.try_recv() {
                let _ = self.app_handle.emit("transfer-progress", TransferProgress {
                    transfer_id: transfer_id.clone(),
                    file_name: file_name.clone(),
                    bytes_sent: total_sent,
                    total_bytes: file_size,
                    direction: "send".to_string(),
                    status: "failed".to_string(),
                });
                return Err(e);
            }
            // Check status for pause/cancel
            {
                let mut status = {
                    let registry = transfers.read().await;
                    registry
                        .get(&transfer_id)
                        .cloned()
                        .unwrap_or(crate::TransferStatus::InProgress)
                };

                // Notify receiver if status changed
                if status != last_status {
                    // Emit progress update so UI sees the change instantly
                    let _ = self.app_handle.emit(
                        "transfer-progress",
                        TransferProgress {
                            transfer_id: transfer_id.clone(),
                            file_name: file_name.clone(),
                            bytes_sent: total_sent,
                            total_bytes: file_size,
                            direction: "send".to_string(),
                            status: match status {
                                crate::TransferStatus::Paused => "paused",
                                crate::TransferStatus::Cancelled => "cancelled",
                                _ => "in_progress",
                            }.to_string(),
                        },
                    );

                    // Sync status to database locally if we have access (wait, sender doesn't have DB here)
                    match status {
                        crate::TransferStatus::Cancelled => {
                            println!("[Transfer] Sending TransferCancel to receiver...");
                            let _ = Self::write_message(
                                &mut send_stream,
                                &MessageType::TransferCancel {
                                    transfer_id: transfer_id.clone(),
                                },
                            )
                            .await;
                            println!("[Transfer] Transfer {} cancelled by sender", transfer_id);
                            // Give receiver time to read the message before closing socket
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            return Err("Transfer cancelled by user".into());
                        }
                        crate::TransferStatus::Paused => {
                            println!("[Transfer] Sending TransferPause to receiver...");
                            let _ = Self::write_message(
                                &mut send_stream,
                                &MessageType::TransferPause {
                                    transfer_id: transfer_id.clone(),
                                },
                            )
                            .await;
                        }
                        crate::TransferStatus::InProgress
                            if last_status == crate::TransferStatus::Paused =>
                        {
                            println!("[Transfer] Sending TransferResume to receiver...");
                            let _ = Self::write_message(
                                &mut send_stream,
                                &MessageType::TransferResume {
                                    transfer_id: transfer_id.clone(),
                                },
                            )
                            .await;
                        }
                        _ => {}
                    }
                    last_status = status;
                }

                while status == crate::TransferStatus::Paused {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let registry = transfers.read().await;
                    status = registry
                        .get(&transfer_id)
                        .cloned()
                        .unwrap_or(crate::TransferStatus::InProgress);

                    if status == crate::TransferStatus::Cancelled {
                        println!("[Transfer] Sending TransferCancel to receiver while paused...");
                        let _ = Self::write_message(
                            &mut send_stream,
                            &MessageType::TransferCancel {
                                transfer_id: transfer_id.clone(),
                            },
                        )
                        .await;
                        // Give receiver time to read the message before closing socket
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        return Err("Transfer cancelled by user".into());
                    }

                    if status == crate::TransferStatus::InProgress {
                        println!("[Transfer] Resuming, sending TransferResume...");
                        // Emit progress update for resume
                        let _ = self.app_handle.emit(
                            "transfer-progress",
                            TransferProgress {
                                transfer_id: transfer_id.clone(),
                                file_name: file_name.clone(),
                                bytes_sent: total_sent,
                                total_bytes: file_size,
                                direction: "send".to_string(),
                                status: "in_progress".to_string(),
                            },
                        );
                        let _ = Self::write_message(
                            &mut send_stream,
                            &MessageType::TransferResume {
                                transfer_id: transfer_id.clone(),
                            },
                        )
                        .await;
                        last_status = status;
                    }
                }
            }

            let n = file.read(&mut buffer).await?;
            if n == 0 {
                // Mark as completed in registry
                let mut registry = transfers.write().await;
                registry.insert(transfer_id.clone(), crate::TransferStatus::Completed);
                break;
            }

            let chunk_data = &buffer[..n];
            let chunk_hash = blake3::hash(chunk_data).to_hex().to_string();

            let chunk_msg = MessageType::ChunkData {
                transfer_id: transfer_id.clone(),
                chunk_index,
                chunk_size: n as u32,
                chunk_hash,
            };

            if let Err(e) = Self::write_message(&mut send_stream, &chunk_msg).await {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                if let Ok(SenderTaskMessage::Error(bg_err)) = rx.try_recv() {
                    return Err(bg_err);
                }
                return Err(e);
            }
            if let Err(e) = send_stream.write_all(chunk_data).await {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                if let Ok(SenderTaskMessage::Error(bg_err)) = rx.try_recv() {
                    return Err(bg_err);
                }
                return Err(e.into());
            }

            total_sent += n as u64;
            chunk_index += 1;

            // Emit progress event
            let _ = self.app_handle.emit(
                "transfer-progress",
                TransferProgress {
                    transfer_id: transfer_id.clone(),
                    file_name: file_name.clone(),
                    bytes_sent: total_sent,
                    total_bytes: file_size,
                    direction: "send".to_string(),
                    status: "in_progress".to_string(),
                },
            );
        }

        // 3. Send Completion
        Self::write_message(
            &mut send_stream,
            &MessageType::TransferComplete {
                transfer_id: transfer_id.clone(),
            },
        )
        .await?;

        // 4. Signal that we're done sending data (but keep stream open for reading ACK)
        send_stream.finish()?;

        // 5. Wait for acknowledgment or history sync from receiver task
        let mut completion_received = false;
        while !completion_received {
            match tokio::time::timeout(
                std::time::Duration::from_secs(30),
                rx.recv(),
            )
            .await
            {
                Ok(Some(SenderTaskMessage::AckReceived)) => {
                    completion_received = true;
                }
                Ok(Some(SenderTaskMessage::HistorySync(records))) => {
                    println!(
                        "[Transfer] Received HistorySync ({} records) during completion",
                        records.len()
                    );
                    let app_state = self.app_handle.state::<crate::AppState>();
                    let db_lock = app_state.database.read().await;
                    if let Some(db) = &*db_lock {
                        for record in records {
                            // Update local database with synced records
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
                    let _ = self.app_handle.emit("history-updated", ());
                }
                Ok(Some(SenderTaskMessage::Error(e))) => {
                    return Err(e);
                }
                Ok(None) => return Err("Receiver disconnected before acknowledging completion".into()),
                Err(_) => {
                    println!("[Transfer] Timeout waiting for transfer completion ACK");
                    let _ = self.app_handle.emit("transfer-progress", TransferProgress {
                        transfer_id: transfer_id.clone(),
                        file_name: file_name.clone(),
                        bytes_sent: file_size,
                        total_bytes: file_size,
                        direction: "send".to_string(),
                        status: "failed".to_string(),
                    });
                    return Err("Timeout waiting for transfer completion ACK".into());
                }
            }
        }

        // Emit final progress as completed
        let _ = self.app_handle.emit(
            "transfer-progress",
            TransferProgress {
                transfer_id: transfer_id.clone(),
                file_name: file_name.clone(),
                bytes_sent: file_size,
                total_bytes: file_size,
                direction: "send".to_string(),
                status: "completed".to_string(),
            },
        );
        let _ = self.app_handle.emit("history-updated", ());

        // Give the receiver a moment to finish its side cleanly
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        Ok(())
    }

    async fn write_message(
        stream: &mut SendStream,
        msg: &MessageType,
    ) -> Result<(), crate::GenericError> {
        let data = bincode::serialize(msg)?;
        let len = data.len() as u32;
        stream.write_all(&len.to_be_bytes()).await?;
        stream.write_all(&data).await?;
        Ok(())
    }

    async fn read_message(stream: &mut RecvStream) -> Result<MessageType, crate::GenericError> {
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        let mut data = vec![0u8; len];
        stream.read_exact(&mut data).await?;

        let msg = bincode::deserialize(&data)?;
        Ok(msg)
    }
}
