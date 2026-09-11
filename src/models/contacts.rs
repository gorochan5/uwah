use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlx::{types::chrono, FromRow, SqlitePool, sqlite::SqliteQueryResult};
use tracing::info;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contact_instantiation() {
        let _c = Contact {
            session_id: "s".to_string(),
            jid: "j".to_string(),
            lid_jid: None,
            phone_number: None,
            push_name: None,
            status_text: None,
            profile_pic_url: None,
            is_business: false,
            username: None,
            devices: None,
            verified_name: None,
            updated_at: chrono::Utc::now(),
        };
    }
}
