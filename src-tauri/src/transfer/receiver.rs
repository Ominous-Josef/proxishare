use crate::transfer::protocol::MessageType;
use quinn::{Connection, RecvStream, SendStream};
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use tauri::Emitter;

pub struct FileReceiver {
    save_directory: PathBuf,
    connection: Connection,
    app_handle: tauri::AppHandle,
}

impl FileReceiver {
    pub fn new(
        save_directory: PathBuf,
        connection: Connection,
        app_handle: tauri::AppHandle,
    ) -> Self {
        Self {
            save_directory,
            connection,
            app_handle,
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
        let mut _bytes_received: u64 = 0;

        loop {
            let msg = Self::read_message(&mut recv_stream).await?;
            match msg {
                MessageType::FileOffer {
                    transfer_id: _,
                    metadata,
                } => {
                    self.check_disk_space(metadata.size)?;

                    let path = self.save_directory.join(&metadata.name);

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
                        _bytes_received += data.len() as u64;
                    }
                }
                MessageType::TransferComplete { transfer_id } => {
                    if let Some(mut f) = file.take() {
                        f.flush().await?;
                    }
                    // Send acknowledgment on the same stream
                    Self::write_message(
                        &mut send_stream,
                        &MessageType::TransferCompleteAck { transfer_id },
                    )
                    .await?;
                    send_stream.finish()?;
                    break;
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
                            "code": pairing_code
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
