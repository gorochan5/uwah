use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, FromRow, ToSchema, Debug, Clone)]
pub struct GroupParticipant {
    pub session_id: String,
    pub group_jid: String,
    pub user_jid: String,
    pub role: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
}
