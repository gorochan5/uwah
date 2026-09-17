use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Serialize, Deserialize, FromRow, Debug, Clone)]
pub struct Contact {
    pub session_id: String,
    pub jid: String,
    pub lid_jid: Option<String>,
    pub phone_number: Option<String>,
    pub push_name: Option<String>,
    pub status_text: Option<String>,
    pub profile_pic_url: Option<String>,
    pub is_business: bool,
    pub username: Option<String>,
    pub devices: Option<String>,
    pub verified_name: Option<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
