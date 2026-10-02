use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct OutboxEvent {
    pub id: String,
    pub session_id: String,
    pub event_type: String,
    pub aggregate_id: Option<String>,
    pub payload: String,
    pub status: String,
    pub retry_count: i64,
    pub next_retry_at: i64,
    pub lease_until: i64,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct WebhookDlqEntry {
    pub id: String,
    pub session_id: String,
    pub event_type: String,
    pub aggregate_id: Option<String>,
    pub payload: String,
    pub retry_count: i64,
    pub error_reason: String,
    pub failed_at: DateTime<Utc>,
}
