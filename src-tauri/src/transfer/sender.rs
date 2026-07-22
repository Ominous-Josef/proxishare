use crate::transfer::protocol::{FileMetadata, MessageType};
use quinn::Connection;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

#[derive(Clone, Serialize, Deserialize)]
pub struct TransferProgress {
    pub transfer_id: String,
    pub file_name: String,
    pub bytes_sent: u64,
    pub total_bytes: u64,
    pub direction: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_file_sent: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_file_total: Option<u64>,
}

impl Default for TransferProgress {
    fn default() -> Self {
        Self {
            transfer_id: String::new(),
            file_name: String::new(),
            bytes_sent: 0,
            total_bytes: 0,
            direction: String::new(),
            status: String::new(),
            current_file_path: None,
            current_file_sent: None,
            current_file_total: None,
        }
    }
}

pub struct FileSender {
    connection: Connection,
    app_handle: tauri::AppHandle,
    device_id: String,
    device_name: String,
}

const BASE_CHUNK_SIZE: usize = 128 * 1024; // 128KB minimum
const MAX_CHUNK_SIZE: usize = 8 * 1024 * 1024; // 8MB maximum

fn calculate_chunk_size(file_size: u64) -> usize {
    let mut chunk_size = BASE_CHUNK_SIZE;
    if file_size > 1024 * 1024 * 1024 {
        chunk_size = MAX_CHUNK_SIZE;
    } else if file_size > 100 * 1024 * 1024 {
        chunk_size = 2 * 1024 * 1024;
    }
    chunk_size
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
        is_dir: bool,
    ) -> Result<(), crate::GenericError> {
        // Open a single bidirectional stream for the entire transfer
        let (mut send_stream, mut recv_stream) = self.connection.open_bi().await?;

        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        
        let mut file_size: u64 = 0;
        let mut file_hash = String::new();
        let mut manifest_files = Vec::new();

        if is_dir {
            for entry in walkdir::WalkDir::new(&path).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() {
                    let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    file_size += size;
                    let rel_path = entry.path().strip_prefix(&path).unwrap_or(entry.path());
                    manifest_files.push(crate::transfer::protocol::FileEntry {
                        relative_path: rel_path.to_string_lossy().to_string(),
                        size,
                    });
                }
            }
            let _ = self.app_handle.emit("folder-manifest", serde_json::json!({
                "transfer_id": transfer_id,
                "files": manifest_files
            }));
        } else {
            let metadata = std::fs::metadata(&path)?;
            file_size = metadata.len();
            file_hash = self.calculate_hash(&path).await?;
        }

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
                is_dir: Some(is_dir),
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
            
                    ..Default::default()
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

                if is_dir {
                    let manifest = MessageType::DirectoryManifest {
                        transfer_id: transfer_id.clone(),
                        files: manifest_files.clone(),
                    };
                    Self::write_message(&mut send_stream, &manifest).await?;
                }
            }
            Ok(Ok(MessageType::FileReject { transfer_id: _, reason })) => {
                println!("[Transfer] Receiver rejected the file: {}", reason);
                let _ = self.app_handle.emit("transfer-progress", TransferProgress {
                    transfer_id: transfer_id.clone(),
                    file_name: file_name.clone(),
                    bytes_sent: 0,
                    total_bytes: file_size,
                    direction: "send".to_string(),
                    status: "failed".to_string(),
                
                    ..Default::default()
                });
                let mut registry = transfers.write().await;
                registry.insert(transfer_id.clone(), crate::TransferStatus::Failed);
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
                
                    ..Default::default()
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
                    ..Default::default()
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
        let tx_bg = tx.clone();

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
                                let _ = tx_bg.send(SenderTaskMessage::Error("Transfer cancelled by receiver".into())).await;
                                break;
                            }
                            MessageType::TransferCompleteAck { .. } => {
                                let _ = tx_bg.send(SenderTaskMessage::AckReceived).await;
                                break;
                            }
                            MessageType::HistorySync { records } => {
                                let _ = tx_bg.send(SenderTaskMessage::HistorySync(records)).await;
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        let _ = tx_bg.send(SenderTaskMessage::Error(e)).await;
                        break;
                    }
                }
            }
        });

        let paths_to_send: Vec<(PathBuf, String, u64)> = if is_dir {
            manifest_files.into_iter().map(|f| (path.join(&f.relative_path), f.relative_path, f.size)).collect()
        } else {
            vec![(path.clone(), file_name.clone(), file_size)]
        };

        let mut partial_success = false;
        for (file_path, rel_path, size) in paths_to_send {
            if is_dir {
                 let start_msg = MessageType::FileStart {
                     transfer_id: transfer_id.clone(),
                     relative_path: rel_path.clone(),
                     size,
                 };
                 if let Err(e) = Self::write_message(&mut send_stream, &start_msg).await {
                     let _ = tx.send(SenderTaskMessage::Error(e)).await;
                 }
            }
            
            let mut file = match File::open(&file_path).await {
                Ok(f) => f,
                Err(e) => {
                    println!("[Sender] Error opening file {:?}: {}", file_path, e);
                    file_size -= size;
                    partial_success = true;
                    continue;
                }
            };
            
            let mut current_file_sent: u64 = 0;
            
            loop {
                // Check for background task messages (e.g. cancellation)
                if let Ok(SenderTaskMessage::Error(e)) = rx.try_recv() {
                    let status_str = if e.to_string().contains("cancelled") { "cancelled" } else { "failed" };
                    let _ = self.app_handle.emit("transfer-progress", TransferProgress {
                        transfer_id: transfer_id.clone(),
                        file_name: file_name.clone(),
                        bytes_sent: total_sent,
                        total_bytes: file_size,
                        direction: "send".to_string(),
                        status: status_str.to_string(),
                        current_file_path: Some(rel_path.clone()),
                        current_file_sent: Some(current_file_sent),
                        current_file_total: Some(size),
                    });
                    return Err(e);
                }
                
                // Check status for pause/cancel
                {
                    let mut status = {
                        let registry = transfers.read().await;
                        registry.get(&transfer_id).cloned().unwrap_or(crate::TransferStatus::InProgress)
                    };

                    if status != last_status {
                        let _ = self.app_handle.emit("transfer-progress", TransferProgress {
                            transfer_id: transfer_id.clone(), file_name: file_name.clone(),
                            bytes_sent: total_sent, total_bytes: file_size, direction: "send".to_string(),
                            status: match status { crate::TransferStatus::Paused => "paused", crate::TransferStatus::Cancelled => "cancelled", _ => "in_progress" }.to_string(),
                            current_file_path: Some(rel_path.clone()),
                            current_file_sent: Some(current_file_sent),
                            current_file_total: Some(size),
                        });
                        match status {
                            crate::TransferStatus::Cancelled => {
                                let _ = Self::write_message(&mut send_stream, &MessageType::TransferCancel { transfer_id: transfer_id.clone() }).await;
                                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                                return Err("Transfer cancelled by user".into());
                            }
                            crate::TransferStatus::Paused => {
                                let _ = Self::write_message(&mut send_stream, &MessageType::TransferPause { transfer_id: transfer_id.clone() }).await;
                            }
                            crate::TransferStatus::InProgress if last_status == crate::TransferStatus::Paused => {
                                let _ = Self::write_message(&mut send_stream, &MessageType::TransferResume { transfer_id: transfer_id.clone() }).await;
                            }
                            _ => {}
                        }
                        last_status = status;
                    }

                    while status == crate::TransferStatus::Paused {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        let registry = transfers.read().await;
                        status = registry.get(&transfer_id).cloned().unwrap_or(crate::TransferStatus::InProgress);

                        if status == crate::TransferStatus::Cancelled {
                            let _ = Self::write_message(&mut send_stream, &MessageType::TransferCancel { transfer_id: transfer_id.clone() }).await;
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                            return Err("Transfer cancelled by user".into());
                        }

                        if status == crate::TransferStatus::InProgress {
                            let _ = self.app_handle.emit("transfer-progress", TransferProgress {
                                transfer_id: transfer_id.clone(), file_name: file_name.clone(),
                                bytes_sent: total_sent, total_bytes: file_size, direction: "send".to_string(), status: "in_progress".to_string(),
                                current_file_path: Some(rel_path.clone()),
                                current_file_sent: Some(current_file_sent),
                                current_file_total: Some(size),
                            });
                            let _ = Self::write_message(&mut send_stream, &MessageType::TransferResume { transfer_id: transfer_id.clone() }).await;
                            last_status = status;
                        }
                    }
                }

                let n = match file.read(&mut buffer).await {
                    Ok(n) => n,
                    Err(e) => {
                        println!("[Sender] Error reading file {:?}: {}", file_path, e);
                        file_size -= size.saturating_sub(current_file_sent);
                        partial_success = true;
                        break;
                    }
                };
                
                if n == 0 {
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
                    let final_err = if let Ok(SenderTaskMessage::Error(bg_err)) = rx.try_recv() { bg_err } else { e };
                    let status_str = if final_err.to_string().contains("cancelled") { "cancelled" } else { "failed" };
                    let _ = self.app_handle.emit("transfer-progress", TransferProgress {
                        transfer_id: transfer_id.clone(), file_name: file_name.clone(),
                        bytes_sent: total_sent, total_bytes: file_size, direction: "send".to_string(), status: status_str.to_string(),
                    
                    ..Default::default()
                });
                    return Err(final_err);
                }
                if let Err(e) = send_stream.write_all(chunk_data).await {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    let final_err = if let Ok(SenderTaskMessage::Error(bg_err)) = rx.try_recv() { bg_err } else { e.into() };
                    let status_str = if final_err.to_string().contains("cancelled") { "cancelled" } else { "failed" };
                    let _ = self.app_handle.emit("transfer-progress", TransferProgress {
                        transfer_id: transfer_id.clone(), file_name: file_name.clone(),
                        bytes_sent: total_sent, total_bytes: file_size, direction: "send".to_string(), status: status_str.to_string(),
                    
                    ..Default::default()
                });
                    return Err(final_err);
                }

                total_sent += n as u64;
                current_file_sent += n as u64;
                chunk_index += 1;

                let _ = self.app_handle.emit("transfer-progress", TransferProgress {
                    transfer_id: transfer_id.clone(), file_name: file_name.clone(),
                    bytes_sent: total_sent, total_bytes: file_size, direction: "send".to_string(), status: "in_progress".to_string(),
                    current_file_path: Some(rel_path.clone()),
                    current_file_sent: Some(current_file_sent),
                    current_file_total: Some(size),
                });
            }
        }

        // Mark as completed in registry
        let mut registry = transfers.write().await;
        let final_registry_status = if partial_success { crate::TransferStatus::PartialSuccess } else { crate::TransferStatus::Completed };
        registry.insert(transfer_id.clone(), final_registry_status);

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
                }
                Ok(Some(SenderTaskMessage::Error(e))) => {
                    println!("[Transfer] Sender task error while waiting for ack: {:?}", e);
                    // Just log and continue, we are done sending anyway.
                }
                Ok(None) => {
                    println!("[Transfer] Background sender task channel closed unexpectedly");
                    break;
                }
                Err(_) => {
                    println!("[Transfer] Timeout waiting for transfer complete ack");
                    break; // Just complete it anyway on our side
                }
            }
        }

        // Final UI update
        let _ = self.app_handle.emit(
            "transfer-progress",
            TransferProgress {
                transfer_id: transfer_id.clone(),
                file_name: file_name.clone(),
                bytes_sent: file_size,
                total_bytes: file_size,
                direction: "send".to_string(),
                status: "completed".to_string(),
            
                    ..Default::default()
                },
        );
        let _ = self.app_handle.emit("history-updated", ());

        Ok(())
    }

    pub async fn write_message(
        send_stream: &mut quinn::SendStream,
        message: &MessageType,
    ) -> Result<(), crate::GenericError> {
        let data = bincode::serialize(message)?;
        let len = data.len() as u32;
        send_stream.write_all(&len.to_be_bytes()).await?;
        send_stream.write_all(&data).await?;
        Ok(())
    }

    pub async fn read_message(
        recv_stream: &mut quinn::RecvStream,
    ) -> Result<MessageType, crate::GenericError> {
        let mut len_buf = [0u8; 4];
        recv_stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > 10 * 1024 * 1024 {
            return Err("Message too large".into());
        }

        let mut data = vec![0u8; len];
        recv_stream.read_exact(&mut data).await?;

        let msg = bincode::deserialize(&data)?;
        Ok(msg)
    }
}
