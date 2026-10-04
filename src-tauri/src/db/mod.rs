pub mod schema;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, FromRow, Pool, Sqlite};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TransferRecord {
    pub id: String,
    pub device_id: String,
    #[sqlx(default)]
    pub device_name: Option<String>,
    pub file_name: String,
    pub file_path: String,
    pub total_size: i64,
    pub direction: String, // "send" or "receive"
    pub status: String,    // "pending", "in_progress", "completed", "failed"
    pub bytes_transferred: i64,
    pub file_hash: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[sqlx(default)]
    pub is_dir: bool,
    #[sqlx(default)]
    pub folder_manifest: Option<String>,
    #[sqlx(default)]
    pub file_exists: Option<bool>,
}

impl TransferRecord {
    pub fn to_synced(&self) -> crate::transfer::protocol::SyncedTransfer {
        crate::transfer::protocol::SyncedTransfer {
            id: self.id.clone(),
            file_name: self.file_name.clone(),
            total_size: self.total_size,
            direction: self.direction.clone(),
            status: self.status.clone(),
            bytes_transferred: self.bytes_transferred,
            created_at: self.created_at,
            is_dir: self.is_dir,
        }
    }
}

pub struct TransferRecordArgs<'a> {
    pub id: &'a str,
    pub device_id: &'a str,
    pub file_name: &'a str,
    pub file_path: &'a str,
    pub total_size: i64,
    pub direction: &'a str,
    pub file_hash: &'a str,
    pub is_dir: bool,
    pub folder_manifest: Option<&'a str>,
}

pub struct Database {
    pool: Pool<Sqlite>,
}

impl Database {
    pub async fn new(db_path: &Path) -> Result<Self, crate::GenericError> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await?;

        // Run base migrations
        sqlx::query(schema::SCHEMA).execute(&pool).await?;

        // Migrate older databases by adding missing columns
        let _ = sqlx::query("ALTER TABLE transfers ADD COLUMN bytes_transferred INTEGER DEFAULT 0")
            .execute(&pool)
            .await;
        
        let _ = sqlx::query("ALTER TABLE transfers ADD COLUMN device_name TEXT")
            .execute(&pool)
            .await;

        let _ = sqlx::query("ALTER TABLE transfers ADD COLUMN is_dir BOOLEAN DEFAULT FALSE")
            .execute(&pool)
            .await;
            
        let _ = sqlx::query("ALTER TABLE transfers ADD COLUMN folder_manifest TEXT")
            .execute(&pool)
            .await;

        println!("[Database] Initialized and migrations run");

        Ok(Self { pool })
    }

    pub async fn record_transfer(
        &self,
        args: TransferRecordArgs<'_>,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp();
        println!(
            "[Database] Recording transfer: id={}, direction={}, file={}",
            args.id, args.direction, args.file_name
        );

        sqlx::query(
            r#"
            INSERT OR REPLACE INTO transfers (id, device_id, file_name, file_path, total_size, direction, status, bytes_transferred, file_hash, created_at, updated_at, is_dir, folder_manifest)
            VALUES (?, ?, ?, ?, ?, ?, COALESCE((SELECT status FROM transfers WHERE id = ?), 'pending'), COALESCE((SELECT bytes_transferred FROM transfers WHERE id = ?), 0), ?, ?, ?, ?, ?)
            "#,
        )
        .bind(args.id)
        .bind(args.device_id)
        .bind(args.file_name)
        .bind(args.file_path)
        .bind(args.total_size)
        .bind(args.direction)
        .bind(args.id)
        .bind(args.id)
        .bind(args.file_hash)
        .bind(now)
        .bind(now)
        .bind(args.is_dir)
        .bind(args.folder_manifest)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_transfer_status(
        &self,
        id: &str,
        status: &str,
        bytes_transferred: i64,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp();
        println!(
            "[Database] Updating transfer status: id={}, status={}",
            id, status
        );

        sqlx::query(
            r#"
            UPDATE transfers 
            SET status = ?, bytes_transferred = ?, updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(status)
        .bind(bytes_transferred)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_status_only(
        &self,
        id: &str,
        status: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp();
        sqlx::query(
            "UPDATE transfers SET status = ?, updated_at = ? WHERE id = ?"
        )
        .bind(status)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_file_location(
        &self,
        id: &str,
        file_name: &str,
        file_path: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE transfers SET file_name = ?, file_path = ? WHERE id = ?")
            .bind(file_name)
            .bind(file_path)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn update_folder_manifest(
        &self,
        id: &str,
        manifest: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE transfers SET folder_manifest = ? WHERE id = ?")
            .bind(manifest)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn get_transfer_history(
        &self,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<TransferRecord>, sqlx::Error> {
        let mut records = sqlx::query_as::<_, TransferRecord>(
            r#"
            SELECT 
                id, device_id, NULL as device_name, file_name, file_path, 
                total_size, direction, status, bytes_transferred, file_hash, 
                created_at, updated_at, is_dir, folder_manifest
            FROM transfers 
            ORDER BY created_at DESC 
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        for record in &mut records {
            record.file_exists = Some(std::path::Path::new(&record.file_path).exists());
        }

        Ok(records)
    }

    pub async fn get_device_transfers(
        &self,
        device_id: &str,
        limit: i32,
        offset: i32,
    ) -> Result<Vec<TransferRecord>, sqlx::Error> {
        let mut records = sqlx::query_as::<_, TransferRecord>(
            r#"
            SELECT 
                id, device_id, NULL as device_name, file_name, file_path, 
                total_size, direction, status, bytes_transferred, file_hash, 
                created_at, updated_at, is_dir, folder_manifest
            FROM transfers 
            WHERE device_id = ?
            ORDER BY created_at DESC 
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(device_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        for record in &mut records {
            record.file_exists = Some(std::path::Path::new(&record.file_path).exists());
        }

        Ok(records)
    }

    /// Stores a record shared by a paired device, seen from our side: the peer
    /// becomes the device, the direction is flipped, and there is no local path.
    /// Existing local records always win (INSERT OR IGNORE).
    pub async fn import_peer_record(
        &self,
        peer_id: &str,
        record: &crate::transfer::protocol::SyncedTransfer,
    ) -> Result<(), sqlx::Error> {
        let direction = match record.direction.as_str() {
            "send" => "receive",
            "receive" => "send",
            _ => return Ok(()),
        };
        const KNOWN_STATUSES: &[&str] = &[
            "pending",
            "in_progress",
            "paused",
            "cancelled",
            "completed",
            "partial_success",
            "failed",
        ];
        if !KNOWN_STATUSES.contains(&record.status.as_str()) {
            return Ok(());
        }

        sqlx::query(
            r#"
            INSERT OR IGNORE INTO transfers (id, device_id, file_name, file_path, total_size, direction, status, bytes_transferred, file_hash, created_at, updated_at, is_dir, folder_manifest)
            VALUES (?, ?, ?, '', ?, ?, ?, ?, '', ?, ?, ?, NULL)
            "#,
        )
        .bind(&record.id)
        .bind(peer_id)
        .bind(&record.file_name)
        .bind(record.total_size)
        .bind(direction)
        .bind(&record.status)
        .bind(record.bytes_transferred)
        .bind(record.created_at)
        .bind(Utc::now().timestamp())
        .bind(record.is_dir)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn clear_history(&self) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM transfers")
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
