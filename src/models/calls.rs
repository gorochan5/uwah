use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Serialize, Deserialize, FromRow, Debug, Clone)]
pub struct Call {
    pub session_id: String,
    pub id: String,
    pub chat_jid: String,
    pub caller_jid: String,
    pub is_video: bool,
    pub is_group_call: bool,
    pub status: String,
    pub duration_seconds: Option<i64>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub ended_at: Option<chrono::DateTime<chrono::Utc>>,
}
