use std::path::PathBuf;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use crate::transfer::protocol::MessageType;
use fs2::FileExt;
use quinn::Connection;

pub struct FileReceiver {
    save_directory: PathBuf,
    connection: Connection,
}

impl FileReceiver {
    pub fn new(save_directory: PathBuf, connection: Connection) -> Self {
        Self { save_directory, connection }
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
        let mut file: Option<File> = None;
        let mut file_size: u64 = 0;
        let mut bytes_received: u64 = 0;

        loop {
            let msg = self.receive_message().await?;
            match msg {
                MessageType::FileOffer { transfer_id, metadata } => {
                    self.check_disk_space(metadata.size)?;
                    file_size = metadata.size;
                    
                    let path = self.save_directory.join(&metadata.name);
                    let f = OpenOptions::new()
                        .write(true)
                        .create(true)
                        .open(&path)
                        .await?;
                    
                    // Pre-allocate (synchronous block)
                    let std_file = f.try_clone().await?.into_std();
                    let _ = std_file.allocate(metadata.size);
                    file = Some(f);
                }
                MessageType::ChunkData { transfer_id, chunk_index, data, chunk_hash } => {
                    if let Some(ref mut f) = file {
                        // Verify chunk
                        let actual_hash = blake3::hash(&data).to_hex().to_string();
                        if actual_hash != chunk_hash {
                            return Err("Chunk hash mismatch".into());
                        }

                        // Write at correct offset
                        // For simplicity, we assume sequential chunks here, but in a real app
                        // we'd use seek for random access during resumption.
                        f.write_all(&data).await?;
                        bytes_received += data.len() as u64;

                        // Acknowledge
                        self.send_message(&MessageType::ChunkAck { transfer_id, chunk_index }).await?;
                    }
                }
                MessageType::TransferComplete { .. } => {
                    if let Some(mut f) = file.take() {
                        f.flush().await?;
                    }
                    break;
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn receive_message(&self) -> Result<MessageType, Box<dyn std::error::Error>> {
        let (_, mut recv) = self.connection.accept_bi().await?;
        let mut len_buf = [0u8; 4];
        recv.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;
        
        let mut data = vec![0u8; len];
        recv.read_exact(&mut data).await?;
        
        let msg = bincode::deserialize(&data)?;
        Ok(msg)
    }

    async fn send_message(&self, msg: &MessageType) -> Result<(), Box<dyn std::error::Error>> {
        let (mut send, _) = self.connection.open_bi().await?;
        let data = bincode::serialize(msg)?;
        let len = data.len() as u32;
        send.write_all(&len.to_be_bytes()).await?;
        send.write_all(&data).await?;
        send.finish().await?;
        Ok(())
    }
}
