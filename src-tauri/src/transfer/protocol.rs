use bincode::Options;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Wire protocol version. Bump whenever `MessageType` or framing changes in a
/// way older builds can't understand. v1 (ProxiShare 1.0.0) had no version byte,
/// v2 was 1.1.0, v3 (1.2.0) added key-bound pairing.
pub const PROTOCOL_VERSION: u8 = 3;

/// Upper bound for a single framed message (manifests for large folders are the biggest).
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

pub const BASE_CHUNK_SIZE: usize = 128 * 1024; // 128KB minimum
pub const MAX_CHUNK_SIZE: usize = 8 * 1024 * 1024; // 8MB maximum

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileMetadata {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub file_count: Option<u32>,
    pub subfolder_count: Option<u32>,
    pub top_extensions: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileEntry {
    pub relative_path: String,
    pub size: u64,
}

/// Final result of a transfer as judged by the receiver.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferOutcome {
    Completed,
    PartialSuccess,
    Failed,
}

impl TransferOutcome {
    pub fn as_status(&self) -> &'static str {
        match self {
            TransferOutcome::Completed => "completed",
            TransferOutcome::PartialSuccess => "partial_success",
            TransferOutcome::Failed => "failed",
        }
    }
}

/// History record shared between paired devices. Deliberately carries no local
/// file paths: the peer only learns what it already took part in.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SyncedTransfer {
    pub id: String,
    pub file_name: String,
    pub total_size: i64,
    pub direction: String,
    pub status: String,
    pub bytes_transferred: i64,
    pub created_at: i64,
    pub is_dir: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MessageType {
    // Handshake
    Hello {
        device_id: String,
        device_name: String,
    },
    HelloAck {
        device_id: String,
        device_name: String,
    },

    // File transfer negotiation
    FileOffer {
        transfer_id: String,
        metadata: FileMetadata,
        sender_id: String,
        sender_name: String,
    },
    FileAccept {
        transfer_id: String,
    },
    FileReject {
        transfer_id: String,
        reason: String,
    },

    // Data transfer
    DirectoryManifest {
        transfer_id: String,
        files: Vec<FileEntry>,
    },
    FileStart {
        transfer_id: String,
        relative_path: String,
        size: u64,
    },
    /// Followed on the stream by exactly `chunk_size` raw bytes.
    ChunkData {
        transfer_id: String,
        chunk_size: u32,
    },
    /// Sent after the last chunk of each file; `hash` is the blake3 of the whole file.
    FileEnd {
        transfer_id: String,
        bytes: u64,
        hash: String,
    },

    // Completion
    TransferComplete {
        transfer_id: String,
    },
    TransferCompleteAck {
        transfer_id: String,
        outcome: TransferOutcome,
    },
    TransferError {
        transfer_id: String,
        message: String,
    },
    TransferPause {
        transfer_id: String,
    },
    TransferResume {
        transfer_id: String,
    },
    TransferCancel {
        transfer_id: String,
    },

    // Pairing: one stream, commit-then-reveal. The initiator (A) commits to its
    // nonce before seeing the responder's (B) nonce, so neither side can steer
    // the resulting code. See `crate::pairing` for the code derivation.
    /// A -> B
    PairRequest {
        device_id: String,
        device_name: String,
        commitment: [u8; 32],
    },
    /// B -> A
    PairChallenge {
        device_id: String,
        device_name: String,
        nonce: [u8; 32],
    },
    /// A -> B: reveals the nonce behind `commitment`.
    PairReveal {
        nonce: [u8; 32],
    },
    /// B -> A, once B's user has typed the code (or declined).
    PairResult {
        accepted: bool,
        reason: String,
    },
    /// A -> B: A has stored the pairing, so B can store it too.
    PairConfirmed,

    // History shared between paired devices
    HistorySync {
        sender_id: String,
        records: Vec<SyncedTransfer>,
    },
}

fn bincode_options() -> impl Options {
    bincode::DefaultOptions::new().with_limit(MAX_MESSAGE_SIZE as u64)
}

/// Writes one frame: `[u8 version][u32 BE length][bincode payload]`.
pub async fn write_message<W: AsyncWrite + Unpin>(
    writer: &mut W,
    msg: &MessageType,
) -> Result<(), crate::GenericError> {
    let data = bincode_options().serialize(msg)?;
    if data.len() > MAX_MESSAGE_SIZE {
        return Err("Message too large".into());
    }

    let mut frame = Vec::with_capacity(5 + data.len());
    frame.push(PROTOCOL_VERSION);
    frame.extend_from_slice(&(data.len() as u32).to_be_bytes());
    frame.extend_from_slice(&data);

    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(())
}

/// Reads one frame written by [`write_message`]. The length is checked before
/// anything is allocated, so a hostile peer can't make us reserve gigabytes.
pub async fn read_message<R: AsyncRead + Unpin>(
    reader: &mut R,
) -> Result<MessageType, crate::GenericError> {
    let mut header = [0u8; 5];
    reader.read_exact(&mut header).await?;

    if header[0] != PROTOCOL_VERSION {
        return Err(format!(
            "Incompatible protocol version (peer v{}, us v{}). Update ProxiShare on both devices.",
            header[0], PROTOCOL_VERSION
        )
        .into());
    }

    let len = u32::from_be_bytes([header[1], header[2], header[3], header[4]]) as usize;
    if len > MAX_MESSAGE_SIZE {
        return Err(format!("Message too large ({} bytes)", len).into());
    }

    let mut data = vec![0u8; len];
    reader.read_exact(&mut data).await?;

    Ok(bincode_options().deserialize(&data)?)
}

/// How long the side that sent a conversation's last message waits for the peer
/// to close the connection.
pub const CLOSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// Ends a conversation in which we sent the last message. The peer closes the
/// connection once it has read that message (its handler returns and drops the
/// connection), so we wait for that instead of guessing with a sleep: closing
/// first could discard data the peer has received but not yet read. Returns
/// false if the peer didn't close within `CLOSE_TIMEOUT`.
pub async fn finish_and_await_close(stream: &mut quinn::SendStream, connection: &quinn::Connection) -> bool {
    let _ = stream.finish();
    tokio::time::timeout(CLOSE_TIMEOUT, connection.closed()).await.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn frame_round_trip() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        let msg = MessageType::FileEnd {
            transfer_id: "t1".into(),
            bytes: 42,
            hash: "abc".into(),
        };
        write_message(&mut a, &msg).await.unwrap();
        match read_message(&mut b).await.unwrap() {
            MessageType::FileEnd {
                transfer_id,
                bytes,
                hash,
            } => {
                assert_eq!(transfer_id, "t1");
                assert_eq!(bytes, 42);
                assert_eq!(hash, "abc");
            }
            other => panic!("unexpected message: {:?}", other),
        }
    }

    #[tokio::test]
    async fn rejects_old_protocol_version() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        // A v1 frame starts directly with a big-endian length, so the first byte is 0.
        a.write_all(&[0, 0, 0, 0, 4, 1, 2, 3, 4]).await.unwrap();
        let err = read_message(&mut b).await.unwrap_err().to_string();
        assert!(err.contains("Incompatible protocol version"), "{}", err);
    }

    #[tokio::test]
    async fn rejects_oversized_length_without_allocating() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        let mut header = vec![PROTOCOL_VERSION];
        header.extend_from_slice(&u32::MAX.to_be_bytes());
        a.write_all(&header).await.unwrap();
        let err = read_message(&mut b).await.unwrap_err().to_string();
        assert!(err.contains("too large"), "{}", err);
    }

    /// The sender closes as soon as `finish_and_await_close` returns. A peer
    /// that has received the message but is slow to read it must still get
    /// all of it.
    #[tokio::test]
    async fn last_message_survives_closing_after_the_wait() {
        use crate::crypto::encryption::DeviceIdentity;
        use quinn::crypto::rustls::{QuicClientConfig, QuicServerConfig};
        use std::sync::Arc;
        use std::time::Duration;

        let server_identity = DeviceIdentity::generate_ephemeral().unwrap();
        let client_identity = DeviceIdentity::generate_ephemeral().unwrap();
        let server = quinn::Endpoint::server(
            quinn::ServerConfig::with_crypto(Arc::new(
                QuicServerConfig::try_from(server_identity.get_server_config().unwrap()).unwrap(),
            )),
            "127.0.0.1:0".parse().unwrap(),
        )
        .unwrap();
        let mut client = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
        client.set_default_client_config(quinn::ClientConfig::new(Arc::new(
            QuicClientConfig::try_from(client_identity.get_client_config().unwrap()).unwrap(),
        )));

        let reader = tokio::spawn({
            let server = server.clone();
            async move {
                let connection = server.accept().await.unwrap().await.unwrap();
                let (_send, mut recv) = connection.accept_bi().await.unwrap();
                // Busy before reading, so the message sits received but unread.
                tokio::time::sleep(Duration::from_millis(300)).await;
                read_message(&mut recv).await.map_err(|e| e.to_string())
                // Returning drops the connection, which closes it.
            }
        });

        // Small enough to fit the receive window, so it is fully received
        // (and acknowledged) before the reader reads any of it.
        let name = "x".repeat(256 * 1024);
        let connection = client
            .connect(server.local_addr().unwrap(), "proxishare.local")
            .unwrap()
            .await
            .unwrap();
        let (mut send, _recv) = connection.open_bi().await.unwrap();
        write_message(
            &mut send,
            &MessageType::Hello {
                device_id: "a".into(),
                device_name: name.clone(),
            },
        )
        .await
        .unwrap();
        assert!(finish_and_await_close(&mut send, &connection).await);
        connection.close(0u32.into(), b"done");

        let received = tokio::time::timeout(Duration::from_secs(5), reader).await.unwrap().unwrap();
        match received {
            Ok(MessageType::Hello { device_name, .. }) => assert_eq!(device_name, name),
            other => panic!("message lost: {:?}", other.map(|_| ())),
        }
    }
}
