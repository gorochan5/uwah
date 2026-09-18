use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Serialize, Deserialize, FromRow, ToSchema, Debug, Clone)]
pub struct MediaCache {
    pub sha256: String,
    pub disk_path: String,
    pub mime_type: String,
    pub file_size: i64,
    pub ref_count: i64,
    pub last_accessed_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}
