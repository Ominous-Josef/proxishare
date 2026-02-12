pub const SCHEMA: &str = r#"
-- Active and historical transfers
CREATE TABLE IF NOT EXISTS transfers (
    id TEXT PRIMARY KEY,
    device_id TEXT NOT NULL,
    file_name TEXT NOT NULL,
    file_path TEXT NOT NULL,
    total_size INTEGER NOT NULL,
    direction TEXT NOT NULL, -- 'send' or 'receive'
    status TEXT NOT NULL, -- 'pending', 'in_progress', 'completed', 'failed', 'paused'
    bytes_transferred INTEGER DEFAULT 0,
    file_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

-- Chunk progress for resumable receiving
CREATE TABLE IF NOT EXISTS transfer_chunks (
    transfer_id TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    received BOOLEAN DEFAULT TRUE,
    PRIMARY KEY (transfer_id, chunk_index),
    FOREIGN KEY (transfer_id) REFERENCES transfers(id) ON DELETE CASCADE
);

-- Indices
CREATE INDEX IF NOT EXISTS idx_transfers_status ON transfers(status);
CREATE INDEX IF NOT EXISTS idx_transfers_device ON transfers(device_id);
"#;
