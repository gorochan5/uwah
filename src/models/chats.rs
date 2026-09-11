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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chat_instantiation() {
        let _c = Chat {
            session_id: "s".to_string(),
            jid: "j".to_string(),
            name: None,
            kind: "direct".to_string(), // Must be String, not Option
            unread_count: 0i64,
            is_archived: false,
            is_pinned: false,
            muted_until: Some(123i64), // Must be Option<i64>
            last_message_at: Some(123i64), // Must be Option<i64>
            subject_owner: None, // New field
            subject_time: None,  // New field
            created_at: None,
            updated_at: None,
        };
    }
}
