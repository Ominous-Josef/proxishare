use crate::transfer::protocol::{FileMetadata, MessageType};
use bincode;
use quinn::Connection;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

/// Maximum chunk size (4MB) - used for large files
const MAX_CHUNK_SIZE: usize = 4 * 1024 * 1024;
/// Minimum chunk size (64KB) - used for small files or slow connections
const MIN_CHUNK_SIZE: usize = 64 * 1024;
/// Default chunk size (1MB) - balanced for most scenarios
const DEFAULT_CHUNK_SIZE: usize = 1 * 1024 * 1024;

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
}

impl FileSender {
    pub fn new(connection: Connection) -> Self {
        Self { connection }
    }

    pub async fn calculate_hash(
        &self,
        path: &PathBuf,
    ) -> Result<String, Box<dyn std::error::Error>> {
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
        progress_tx: tokio::sync::mpsc::Sender<u64>,
    ) -> Result<(), Box<dyn std::error::Error>> {
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
                name: file_name,
                size: file_size,
                hash: file_hash,
                chunk_size: chunk_size as u32,
            },
        };
        self.send_message(&offer).await?;

        // 3. Send Chunks
        let mut buffer = vec![0u8; chunk_size];
        let mut chunk_index = 0;
        let mut total_sent = 0;

        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
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

            self.send_message(&chunk_msg).await?;

            total_sent += n as u64;
            chunk_index += 1;
            let _ = progress_tx.send(total_sent).await;
        }

        // 4. Send Completion
        self.send_message(&MessageType::TransferComplete { transfer_id })
            .await?;

        Ok(())
    }

    async fn send_message(&self, msg: &MessageType) -> Result<(), Box<dyn std::error::Error>> {
        let (mut send, _) = self.connection.open_bi().await?;
        let data = bincode::serialize(msg)?;
        let len = data.len() as u32;
        send.write_all(&len.to_be_bytes()).await?;
        send.write_all(&data).await?;
        send.finish().map_err(|e| e.to_string())?;
        Ok(())
    }
}
