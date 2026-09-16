use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{types::chrono, FromRow, SqlitePool, sqlite::SqliteQueryResult};
use tracing::info;

#[derive(Serialize, Deserialize, FromRow, Debug, Clone)]
pub struct Chat {
    pub session_id: String,
    pub jid: String,
    pub name: Option<String>,
    pub kind: String,
    pub unread_count: i64,
    pub is_archived: bool,
    pub is_pinned: bool,
    pub muted_until: Option<i64>,
    pub last_message_at: Option<i64>,
    pub subject_owner: Option<String>,
    pub subject_time: Option<i64>,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: Option<chrono::DateTime<chrono::Utc>>,
}