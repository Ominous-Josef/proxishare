use crate::transfer::protocol::MessageType;
use crate::transfer::sender::TransferProgress;
use quinn::{Connection, RecvStream, SendStream};
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
}

impl FileReceiver {
    pub fn new(
        save_directory: PathBuf,
        connection: Connection,
        app_handle: tauri::AppHandle,
        database: Arc<tokio::sync::RwLock<Option<crate::db::Database>>>,
    ) -> Self {
        Self {
            save_directory,
            connection,
            app_handle,
            database,
        }
    }

    pub fn check_disk_space(&self, required_bytes: u64) -> Result<(), String> {
        let free_space = fs2::available_space(&self.save_directory)
            .map_err(|e| format!("Failed to check disk space: {}", e))?;

        if free_space < required_bytes {
            return Err(format!(
                "Insufficient disk space. Required: {} bytes, available: {} bytes",
                required_bytes, free_space
            ));
        }
        Ok(())
    }

    pub async fn handle_transfer(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Accept the single bidirectional stream from the sender
        let (mut send_stream, mut recv_stream) = self.connection.accept_bi().await?;

        let mut file: Option<File> = None;
        let mut bytes_received: u64 = 0;
        let mut current_transfer_id = String::new();
        let mut current_file_name = String::new();
        let mut current_file_size: u64 = 0;

        loop {
            let msg = Self::read_message(&mut recv_stream).await?;
            match msg {
                MessageType::FileOffer {
                    transfer_id,
                    metadata,
                    sender_id,
                    sender_name: _,
                } => {
                    self.check_disk_space(metadata.size)?;

                    let path = self.save_directory.join(&metadata.name);
                    current_transfer_id = transfer_id.clone();
                    current_file_name = metadata.name.clone();
                    current_file_size = metadata.size;

                    // Record the transfer start in database
                    {
                        let db_lock = self.database.read().await;
                        if let Some(db) = &*db_lock {
                            if let Err(e) = db
                                .record_transfer(
                                    &current_transfer_id,
                                    &sender_id,
                                    &current_file_name,
                                    &path.to_string_lossy(),
                                    current_file_size as i64,
                                    "receive",
                                    &metadata.hash,
                                )
                                .await
                            {
                                println!("[Database] Failed to record transfer: {:?}", e);
                            }
                        }
                    }

                    // Use std::fs to create and allocate to avoid tokio/fs2 complexity
                    let std_file = std::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .open(&path)?;

                    use fs2::FileExt;
                    let _ = std_file.allocate(metadata.size);

                    // Then convert to tokio file
                    file = Some(File::from_std(std_file));
                }
                MessageType::ChunkData {
                    transfer_id: _,
                    chunk_index: _,
                    data,
                    chunk_hash,
                } => {
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
                                file_name: current_file_name.clone(),
                                bytes_sent: bytes_received,
                                total_bytes: current_file_size,
                                direction: "receive".to_string(),
                            },
                        );
                    }
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
                                println!("[Database] Failed to update transfer status: {:?}", e);
                            }
                        }
                    }

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
                            // We use record_transfer but might need a "merge" or "upsert" logic
                            // For now, let's just record it if it doesn't exist?
                            // Actually, record_transfer uses INSERT.
                            // Let's just record it and handle errors or implement an upsert in db.rs later if needed.
                            let _ = db
                                .record_transfer(
                                    &record.id,
                                    &record.device_id,
                                    &record.file_name,
                                    &record.file_path,
                                    record.total_size,
                                    &record.direction,
                                    &record.file_hash,
                                )
                                .await;
                            // Also update status to the incoming status
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
                _ => {}
            }
        }
        Ok(())
    }

    async fn read_message(
        stream: &mut RecvStream,
    ) -> Result<MessageType, Box<dyn std::error::Error>> {
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        let mut data = vec![0u8; len];
        stream.read_exact(&mut data).await?;

        let msg = bincode::deserialize(&data)?;
        Ok(msg)
    }

    async fn write_message(
        stream: &mut SendStream,
        msg: &MessageType,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let data = bincode::serialize(msg)?;
        let len = data.len() as u32;
        stream.write_all(&len.to_be_bytes()).await?;
        stream.write_all(&data).await?;
        Ok(())
    }
}
