use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool, sqlite::SqliteQueryResult};
use tracing::info;

#[derive(Serialize, Deserialize, FromRow, Debug, Clone)]
pub struct Message {
    pub id: String,
    pub session_id: String,
    pub chat_jid: String,
    pub sender_jid: String,
    pub quoted_message_id: Option<String>,
    pub is_from_me: bool,
    pub message_type: String,
    pub body: Option<String>,
    pub cdn_url: Option<String>,
    pub media_key: Option<Vec<u8>>,
    pub file_enc_sha256: Option<String>,
    pub media_sha256: Option<String>,
    pub delivery_status: String,
    pub is_edited: bool,
    pub is_deleted: bool,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}
