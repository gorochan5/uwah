use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, FromRow, ToSchema, Debug, Clone)]
pub struct Session {
    pub id: String,
    pub phone_number: Option<String>,
    pub push_name: Option<String>,
    pub status: String,
    pub qr_payload: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
