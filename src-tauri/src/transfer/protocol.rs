use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileMetadata {
    pub name: String,
    pub size: u64,
    pub hash: String,
    pub chunk_size: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum MessageType {
    // Handshake
    Hello {
        device_id: String,
        device_name: String,
    },
    HelloAck,

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
    ChunkData {
        transfer_id: String,
        chunk_index: u32,
        data: Vec<u8>,
        chunk_hash: String,
    },
    ChunkAck {
        transfer_id: String,
        chunk_index: u32,
    },

    // Resumability
    ResumeRequest {
        transfer_id: String,
        last_chunk_index: u32,
    },

    // Completion
    TransferComplete {
        transfer_id: String,
    },
    TransferCompleteAck {
        transfer_id: String,
    },
    TransferError {
        transfer_id: String,
        message: String,
    },

    // Pairing
    PairRequest {
        device_id: String,
        device_name: String,
        pairing_code: String,
    },
    PairResponse {
        accepted: bool,
        device_id: String,
    },

    // Sync (placeholder for Phase 4)
    HistorySync {
        records: Vec<crate::db::TransferRecord>,
    },
    SyncRequest {
        folder_path: String,
        files: Vec<FileMetadata>,
    },
    SyncResponse {
        missing_files: Vec<String>,
    },
}
