use crate::transfer::protocol::{
    read_message, write_message, FileEntry, FileMetadata, MessageType, TransferOutcome,
    BASE_CHUNK_SIZE, MAX_CHUNK_SIZE,
};
use crate::TransferStatus;
use quinn::{Connection, SendStream};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc;

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct TransferProgress {
    pub transfer_id: String,
    pub device_id: String,
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

/// What the receiver confirmed at the end of a transfer.
pub struct TransferResult {
    pub outcome: TransferOutcome,
    pub bytes_sent: u64,
}

pub struct FileSender {
    pub connection: Connection,
    pub app_handle: tauri::AppHandle,
    pub device_id: String, // local device id
    pub device_name: String,
    pub remote_device_id: String, // remote device id
}

const ACCEPT_TIMEOUT: Duration = Duration::from_secs(300);
const ACK_TIMEOUT: Duration = Duration::from_secs(30);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

fn calculate_chunk_size(file_size: u64) -> usize {
    let mut chunk_size = BASE_CHUNK_SIZE;
    if file_size > 1024 * 1024 * 1024 {
        chunk_size = MAX_CHUNK_SIZE;
    } else if file_size > 100 * 1024 * 1024 {
        chunk_size = 2 * 1024 * 1024;
    }
    chunk_size
}

/// Messages from the receiver while data is flowing.
enum Control {
    Pause,
    Resume,
    Cancelled,
    Ack(TransferOutcome),
    Error(crate::GenericError),
}

#[derive(Default)]
struct SendProgress {
    total: u64,
    sent: u64,
}

struct DirScan {
    files: Vec<FileEntry>,
    subfolder_count: u32,
    top_extensions: Vec<String>,
}

/// Walks a folder and builds the manifest. Paths always use `/` so the
/// receiver sees the same layout regardless of the sender's OS.
fn scan_directory(root: &Path) -> DirScan {
    let mut files = Vec::new();
    let mut subfolder_count = 0;
    let mut ext_map: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

    for entry in walkdir::WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if let Some(ext) = entry.path().extension().and_then(|s| s.to_str()) {
                *ext_map.entry(format!(".{}", ext.to_lowercase())).or_insert(0) += 1;
            }
            let rel = entry.path().strip_prefix(root).unwrap_or(entry.path());
            let relative_path = rel
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            files.push(FileEntry {
                relative_path,
                size,
            });
        } else if entry.file_type().is_dir() && entry.path() != root {
            subfolder_count += 1;
        }
    }

    let mut ext_vec: Vec<_> = ext_map.into_iter().collect();
    ext_vec.sort_by_key(|e| std::cmp::Reverse(e.1));
    let top_extensions = ext_vec.into_iter().take(3).map(|(ext, _)| ext).collect();

    DirScan {
        files,
        subfolder_count,
        top_extensions,
    }
}

impl FileSender {
    pub fn new(
        connection: Connection,
        app_handle: tauri::AppHandle,
        device_id: String,
        device_name: String,
        remote_device_id: String,
    ) -> Self {
        Self {
            connection,
            app_handle,
            device_id,
            device_name,
            remote_device_id,
        }
    }

    fn emit(
        &self,
        transfer_id: &str,
        file_name: &str,
        progress: &SendProgress,
        status: &str,
        current: Option<(&str, u64, u64)>,
    ) {
        let _ = self.app_handle.emit(
            "transfer-progress",
            TransferProgress {
                transfer_id: transfer_id.to_string(),
                device_id: self.remote_device_id.clone(),
                file_name: file_name.to_string(),
                bytes_sent: progress.sent,
                total_bytes: progress.total,
                direction: "send".to_string(),
                status: status.to_string(),
                current_file_path: current.map(|c| c.0.to_string()),
                current_file_sent: current.map(|c| c.1),
                current_file_total: current.map(|c| c.2),
            },
        );
    }

    pub async fn send_file(
        &self,
        transfer_id: String,
        path: PathBuf,
        transfers: crate::TransferRegistry,
        is_dir: bool,
    ) -> Result<TransferResult, crate::GenericError> {
        let file_name = path
            .file_name()
            .ok_or("Cannot send a path without a file name")?
            .to_string_lossy()
            .to_string();

        let mut progress = SendProgress::default();
        let result = self
            .run(&transfer_id, &path, &file_name, &transfers, is_dir, &mut progress)
            .await;

        match &result {
            Ok(res) => {
                if res.outcome == TransferOutcome::Completed {
                    progress.sent = progress.total;
                }
                self.emit(&transfer_id, &file_name, &progress, res.outcome.as_status(), None);
            }
            Err(e) => {
                let status = if e.to_string().contains("cancelled") {
                    "cancelled"
                } else {
                    "failed"
                };
                self.emit(&transfer_id, &file_name, &progress, status, None);
            }
        }
        let _ = self.app_handle.emit("history-updated", ());
        result
    }

    async fn run(
        &self,
        transfer_id: &str,
        path: &Path,
        file_name: &str,
        transfers: &crate::TransferRegistry,
        is_dir: bool,
        progress: &mut SendProgress,
    ) -> Result<TransferResult, crate::GenericError> {
        // Open a single bidirectional stream for the entire transfer
        let (mut send_stream, mut recv_stream) = self.connection.open_bi().await?;

        // 1. Work out what we're sending
        let mut manifest_files = Vec::new();
        let mut metadata = FileMetadata {
            name: file_name.to_string(),
            size: 0,
            is_dir,
            file_count: None,
            subfolder_count: None,
            top_extensions: None,
        };

        if is_dir {
            self.emit(transfer_id, file_name, progress, "preparing", None);

            let root = path.to_path_buf();
            let scan = tokio::task::spawn_blocking(move || scan_directory(&root)).await?;
            if scan.files.is_empty() {
                return Err("Directory contains no files to transfer.".into());
            }

            metadata.size = scan.files.iter().map(|f| f.size).sum();
            metadata.file_count = Some(scan.files.len() as u32);
            metadata.subfolder_count = Some(scan.subfolder_count);
            metadata.top_extensions = Some(scan.top_extensions);
            manifest_files = scan.files;

            // Save manifest to database
            let state = self.app_handle.state::<crate::AppState>();
            if let Some(db) = &*state.database.read().await {
                if let Ok(manifest_json) = serde_json::to_string(&manifest_files) {
                    let _ = db.update_folder_manifest(transfer_id, &manifest_json).await;
                }
            }
            let _ = self.app_handle.emit(
                "folder-manifest",
                serde_json::json!({ "transfer_id": transfer_id, "files": manifest_files }),
            );
        } else {
            metadata.size = std::fs::metadata(path)?.len();
        }
        progress.total = metadata.size;
        let chunk_size = calculate_chunk_size(metadata.size);

        // 2. Offer and wait for the user on the other side
        write_message(
            &mut send_stream,
            &MessageType::FileOffer {
                transfer_id: transfer_id.to_string(),
                metadata,
                sender_id: self.device_id.clone(),
                sender_name: self.device_name.clone(),
            },
        )
        .await?;
        self.emit(transfer_id, file_name, progress, "pending", None);

        println!("[Transfer] Waiting for receiver to accept file...");
        let reply = {
            let reply = read_message(&mut recv_stream);
            tokio::pin!(reply);
            let deadline = tokio::time::sleep(ACCEPT_TIMEOUT);
            tokio::pin!(deadline);
            loop {
                tokio::select! {
                    r = &mut reply => break r,
                    _ = &mut deadline => {
                        let _ = write_message(&mut send_stream, &MessageType::TransferCancel {
                            transfer_id: transfer_id.to_string(),
                        }).await;
                        return Err("Transfer cancelled: receiver did not respond in time".into());
                    }
                    _ = tokio::time::sleep(Duration::from_millis(500)) => {
                        let status = transfers.read().await.get(transfer_id).cloned();
                        if status == Some(TransferStatus::Cancelled) {
                            let _ = write_message(&mut send_stream, &MessageType::TransferCancel {
                                transfer_id: transfer_id.to_string(),
                            }).await;
                            tokio::time::sleep(Duration::from_millis(100)).await;
                            return Err("Transfer cancelled by user".into());
                        }
                    }
                }
            }
        };

        match reply {
            Ok(MessageType::FileAccept { transfer_id: ack_id }) if ack_id == transfer_id => {
                println!("[Transfer] Receiver accepted the file. Starting chunks...");
            }
            Ok(MessageType::FileReject { reason, .. }) => {
                return Err(format!("Transfer rejected: {}", reason).into());
            }
            Ok(MessageType::TransferError { message, .. }) => {
                return Err(format!("Transfer error: {}", message).into());
            }
            Ok(_) => return Err("Unexpected message while waiting for file acceptance".into()),
            Err(e) => return Err(format!("Failed to receive file acceptance: {}", e).into()),
        }
        transfers
            .write()
            .await
            .insert(transfer_id.to_string(), TransferStatus::InProgress);

        if is_dir {
            write_message(
                &mut send_stream,
                &MessageType::DirectoryManifest {
                    transfer_id: transfer_id.to_string(),
                    files: manifest_files.clone(),
                },
            )
            .await?;
        }

        // 3. Listen for pause/cancel/ack from the receiver in the background
        let (tx, mut rx) = mpsc::channel::<Control>(10);
        tokio::spawn(async move {
            loop {
                let control = match read_message(&mut recv_stream).await {
                    Ok(MessageType::TransferPause { .. }) => Control::Pause,
                    Ok(MessageType::TransferResume { .. }) => Control::Resume,
                    Ok(MessageType::TransferCancel { .. }) => Control::Cancelled,
                    Ok(MessageType::TransferCompleteAck { outcome, .. }) => Control::Ack(outcome),
                    Ok(MessageType::TransferError { message, .. }) => {
                        Control::Error(format!("Receiver reported an error: {}", message).into())
                    }
                    Ok(_) => continue,
                    Err(e) => Control::Error(e),
                };
                let done = !matches!(control, Control::Pause | Control::Resume);
                if tx.send(control).await.is_err() || done {
                    break;
                }
            }
        });

        // 4. Stream the files
        let paths_to_send: Vec<(PathBuf, String, u64)> = if is_dir {
            manifest_files
                .into_iter()
                .map(|f| (path.join(&f.relative_path), f.relative_path, f.size))
                .collect()
        } else {
            vec![(path.to_path_buf(), file_name.to_string(), progress.total)]
        };

        let mut buffer = vec![0u8; chunk_size];
        let mut last_status = TransferStatus::InProgress;
        let mut last_emit = Instant::now();

        for (file_path, rel_path, size) in paths_to_send {
            // Open before announcing, so an unreadable file is simply skipped and
            // the receiver reports the transfer as partial instead of a zero-filled file.
            let mut file = match File::open(&file_path).await {
                Ok(f) => f,
                Err(e) => {
                    println!("[Sender] Skipping {:?}: {}", file_path, e);
                    continue;
                }
            };

            self.send_msg(
                &mut send_stream,
                &mut rx,
                &MessageType::FileStart {
                    transfer_id: transfer_id.to_string(),
                    relative_path: rel_path.clone(),
                    size,
                },
            )
            .await?;

            let mut hasher = blake3::Hasher::new();
            let mut file_sent: u64 = 0;

            loop {
                self.checkpoint(
                    transfer_id,
                    file_name,
                    transfers,
                    &mut send_stream,
                    &mut rx,
                    &mut last_status,
                    progress,
                    Some((&rel_path, file_sent, size)),
                )
                .await?;

                // Never send more than announced, even if the file grew meanwhile.
                let remaining = size - file_sent;
                if remaining == 0 {
                    break;
                }
                let want = remaining.min(buffer.len() as u64) as usize;
                let n = match file.read(&mut buffer[..want]).await {
                    Ok(n) => n,
                    Err(e) => {
                        println!("[Sender] Error reading file {:?}: {}", file_path, e);
                        break;
                    }
                };
                if n == 0 {
                    break; // File shrank; the receiver will flag it.
                }

                let chunk = &buffer[..n];
                hasher.update(chunk);
                self.send_msg(
                    &mut send_stream,
                    &mut rx,
                    &MessageType::ChunkData {
                        transfer_id: transfer_id.to_string(),
                        chunk_size: n as u32,
                    },
                )
                .await?;
                if let Err(e) = send_stream.write_all(chunk).await {
                    return Err(explain_failure(&mut rx, e.into()).await);
                }

                file_sent += n as u64;
                progress.sent += n as u64;

                if last_emit.elapsed() >= PROGRESS_INTERVAL {
                    last_emit = Instant::now();
                    self.emit(
                        transfer_id,
                        file_name,
                        progress,
                        "in_progress",
                        Some((&rel_path, file_sent, size)),
                    );
                }
            }

            self.send_msg(
                &mut send_stream,
                &mut rx,
                &MessageType::FileEnd {
                    transfer_id: transfer_id.to_string(),
                    bytes: file_sent,
                    hash: hasher.finalize().to_hex().to_string(),
                },
            )
            .await?;
        }

        // 5. Finish and wait for the receiver's verdict
        self.send_msg(
            &mut send_stream,
            &mut rx,
            &MessageType::TransferComplete {
                transfer_id: transfer_id.to_string(),
            },
        )
        .await?;
        send_stream.finish()?;

        loop {
            match tokio::time::timeout(ACK_TIMEOUT, rx.recv()).await {
                Ok(Some(Control::Ack(outcome))) => {
                    return Ok(TransferResult {
                        outcome,
                        bytes_sent: progress.sent,
                    })
                }
                Ok(Some(Control::Pause | Control::Resume)) => continue,
                Ok(Some(Control::Cancelled)) => {
                    return Err("Transfer cancelled by receiver".into())
                }
                Ok(Some(Control::Error(e))) => return Err(e),
                Ok(None) => {
                    return Err("Receiver closed the connection without confirming".into())
                }
                Err(_) => return Err("No confirmation from receiver".into()),
            }
        }
    }

    /// Applies pause/resume/cancel from either side before the next chunk.
    /// Blocks while the transfer is paused.
    #[allow(clippy::too_many_arguments)]
    async fn checkpoint(
        &self,
        transfer_id: &str,
        file_name: &str,
        transfers: &crate::TransferRegistry,
        send_stream: &mut SendStream,
        rx: &mut mpsc::Receiver<Control>,
        last_status: &mut TransferStatus,
        progress: &SendProgress,
        current: Option<(&str, u64, u64)>,
    ) -> Result<(), crate::GenericError> {
        loop {
            // The receiver's requests are applied without echoing them back.
            while let Ok(control) = rx.try_recv() {
                match control {
                    Control::Pause | Control::Resume => {
                        let (status, label) = if matches!(control, Control::Pause) {
                            println!("[Sender] Receiver paused the transfer");
                            (TransferStatus::Paused, "paused")
                        } else {
                            println!("[Sender] Receiver resumed the transfer");
                            (TransferStatus::InProgress, "in_progress")
                        };
                        transfers
                            .write()
                            .await
                            .insert(transfer_id.to_string(), status);
                        *last_status = status;
                        self.emit(transfer_id, file_name, progress, label, current);
                    }
                    Control::Cancelled => return Err("Transfer cancelled by receiver".into()),
                    Control::Error(e) => return Err(e),
                    Control::Ack(_) => {
                        return Err("Protocol violation: completion ack before completion".into())
                    }
                }
            }

            let status = transfers
                .read()
                .await
                .get(transfer_id)
                .cloned()
                .unwrap_or(TransferStatus::InProgress);

            if status != *last_status {
                let message = match status {
                    TransferStatus::Cancelled => {
                        let _ = write_message(
                            send_stream,
                            &MessageType::TransferCancel {
                                transfer_id: transfer_id.to_string(),
                            },
                        )
                        .await;
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        return Err("Transfer cancelled by user".into());
                    }
                    TransferStatus::Paused => Some(MessageType::TransferPause {
                        transfer_id: transfer_id.to_string(),
                    }),
                    TransferStatus::InProgress if *last_status == TransferStatus::Paused => {
                        Some(MessageType::TransferResume {
                            transfer_id: transfer_id.to_string(),
                        })
                    }
                    _ => None,
                };
                if let Some(message) = message {
                    write_message(send_stream, &message).await?;
                    let label = if status == TransferStatus::Paused {
                        "paused"
                    } else {
                        "in_progress"
                    };
                    self.emit(transfer_id, file_name, progress, label, current);
                }
                *last_status = status;
            }

            if *last_status != TransferStatus::Paused {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }

    async fn send_msg(
        &self,
        send_stream: &mut SendStream,
        rx: &mut mpsc::Receiver<Control>,
        msg: &MessageType,
    ) -> Result<(), crate::GenericError> {
        match write_message(send_stream, msg).await {
            Ok(()) => Ok(()),
            Err(e) => Err(explain_failure(rx, e).await),
        }
    }
}

/// A write usually fails because the receiver cancelled or errored; prefer its
/// explanation over the bare stream error when one arrives shortly after.
async fn explain_failure(
    rx: &mut mpsc::Receiver<Control>,
    error: crate::GenericError,
) -> crate::GenericError {
    tokio::time::sleep(Duration::from_millis(50)).await;
    match rx.try_recv() {
        Ok(Control::Cancelled) => "Transfer cancelled by receiver".into(),
        Ok(Control::Error(e)) => e,
        _ => error,
    }
}
