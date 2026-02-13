pub mod schema;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::SqlitePoolOptions, Pool, Sqlite};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRecord {
    pub id: String,
    pub device_id: String,
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
}

pub struct Database {
    pool: Pool<Sqlite>,
}

impl Database {
    pub async fn new(db_path: &Path) -> Result<Self, sqlx::Error> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
        
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&db_url)
            .await?;

        // Run migrations
        sqlx::query(schema::SCHEMA)
            .execute(&pool)
            .await?;

        Ok(Self { pool })
    }

    pub async fn record_transfer(
        &self,
        id: &str,
        device_id: &str,
        file_name: &str,
        file_path: &str,
        total_size: i64,
        direction: &str,
        file_hash: &str,
    ) -> Result<(), sqlx::Error> {
        let now = Utc::now().timestamp();
        
        sqlx::query(
            r#"
            INSERT INTO transfers (id, device_id, file_name, file_path, total_size, direction, status, bytes_transferred, file_hash, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, 'in_progress', 0, ?, ?, ?)
            "#,
        )
        .bind(id)
        .bind(device_id)
        .bind(file_name)
        .bind(file_path)
        .bind(total_size)
        .bind(direction)
        .bind(file_hash)
        .bind(now)
        .bind(now)
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

    pub async fn get_transfer_history(
        &self,
        limit: i32,
    ) -> Result<Vec<TransferRecord>, sqlx::Error> {
        let records = sqlx::query_as!(
            TransferRecord,
            r#"
            SELECT 
                id, device_id, NULL as "device_name: _", file_name, file_path, 
                total_size, direction, status, bytes_transferred, file_hash, 
                created_at, updated_at
            FROM transfers 
            ORDER BY created_at DESC 
            LIMIT ?
            "#,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(records)
    }

    pub async fn get_device_transfers(
        &self,
        device_id: &str,
        limit: i32,
    ) -> Result<Vec<TransferRecord>, sqlx::Error> {
        let records = sqlx::query_as!(
            TransferRecord,
            r#"
            SELECT 
                id, device_id, NULL as "device_name: _", file_name, file_path, 
                total_size, direction, status, bytes_transferred, file_hash, 
                created_at, updated_at
            FROM transfers 
            WHERE device_id = ?
            ORDER BY created_at DESC 
            LIMIT ?
            "#,
            device_id,
            limit
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(records)
    }

    pub async fn clear_history(&self) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM transfers")
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
