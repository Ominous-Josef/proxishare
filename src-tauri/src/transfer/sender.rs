use crate::transfer::protocol::{FileMetadata, MessageType};
use bincode;
use quinn::{Connection, RecvStream, SendStream};
use serde::Serialize;
use std::path::PathBuf;
use tauri::Emitter;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

/// Maximum chunk size (4MB) - used for large files
const MAX_CHUNK_SIZE: usize = 4 * 1024 * 1024;
/// Minimum chunk size (64KB) - used for small files or slow connections
const MIN_CHUNK_SIZE: usize = 64 * 1024;
/// Default chunk size (1MB) - balanced for most scenarios
const DEFAULT_CHUNK_SIZE: usize = 1 * 1024 * 1024;

#[derive(Clone, Serialize)]
pub struct TransferProgress {
    pub transfer_id: String,
    pub file_name: String,
    pub bytes_sent: u64,
    pub total_bytes: u64,
    pub direction: String,
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
}

impl FileSender {
    pub fn new(connection: Connection, app_handle: tauri::AppHandle) -> Self {
        Self {
            connection,
            app_handle,
        }
    }

    pub async fn calculate_hash(
        &self,
        path: &PathBuf,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
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
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
        };
        Self::write_message(&mut send_stream, &offer).await?;

        // 2. Send Chunks
        let mut buffer = vec![0u8; chunk_size];
        let mut chunk_index = 0;
        let mut total_sent: u64 = 0;

        loop {
            // Check status for pause/cancel
            {
                let mut status = {
                    let registry = transfers.read().await;
                    registry
                        .get(&transfer_id)
                        .cloned()
                        .unwrap_or(crate::TransferStatus::InProgress)
                };

                if status == crate::TransferStatus::Cancelled {
                    println!("[Transfer] Transfer {} cancelled", transfer_id);
                    return Err("Transfer cancelled by user".into());
                }

                while status == crate::TransferStatus::Paused {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let registry = transfers.read().await;
                    status = registry
                        .get(&transfer_id)
                        .cloned()
                        .unwrap_or(crate::TransferStatus::InProgress);

                    if status == crate::TransferStatus::Cancelled {
                        println!("[Transfer] Transfer {} cancelled while paused", transfer_id);
                        return Err("Transfer cancelled by user".into());
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
                data: chunk_data.to_vec(),
                chunk_hash,
            };

            Self::write_message(&mut send_stream, &chunk_msg).await?;

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

        // 5. Wait for acknowledgment from receiver before closing connection
        match tokio::time::timeout(
            std::time::Duration::from_secs(30),
            Self::read_message(&mut recv_stream),
        )
        .await
        {
            Ok(Ok(MessageType::TransferCompleteAck {
                transfer_id: ack_id,
            })) if ack_id == transfer_id => {
                // Give the receiver a moment to finish its side cleanly
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                Ok(())
            }
            Ok(Ok(_)) => Err("Unexpected message while waiting for completion ack".into()),
            Ok(Err(e)) => Err(format!("Failed to receive completion ack: {}", e).into()),
            Err(_) => Err("Timeout waiting for transfer completion acknowledgment".into()),
        }
    }

    async fn write_message(
        stream: &mut SendStream,
        msg: &MessageType,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let data = bincode::serialize(msg)?;
        let len = data.len() as u32;
        stream.write_all(&len.to_be_bytes()).await?;
        stream.write_all(&data).await?;
        Ok(())
    }

    async fn read_message(
        stream: &mut RecvStream,
    ) -> Result<MessageType, Box<dyn std::error::Error + Send + Sync>> {
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        let mut data = vec![0u8; len];
        stream.read_exact(&mut data).await?;

        let msg = bincode::deserialize(&data)?;
        Ok(msg)
    }
}
