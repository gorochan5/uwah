use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, SqlitePool, sqlite::SqliteQueryResult};
use tracing::info;

type Contact = crate::models::contacts::Contact;

pub async fn get_all() -> Result<Vec<Contact>> {
    info!("Fetching all contacts from DB");

    let pool = crate::database::get();
    sqlx::query_as!(
        Contact,
        r#"
        SELECT
            session_id,
            jid,
            lid_jid,
            phone_number,
            push_name,
            status_text,
            profile_pic_url,
            is_business,
            username,
            devices,
            verified_name,
            updated_at as "updated_at: chrono::DateTime<chrono::Utc>"
        FROM contacts
        "#
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch contacts from DB")
}

pub async fn get_one(id: &str, session_id: &str) -> Result<Option<Contact>> {
    info!(
        contact_id = id,
        "Fetching contact from DB"
    );
    
    let pool = crate::database::get();
    sqlx::query_as!(
        Contact,
        r#"
        SELECT
            session_id,
            jid,
            lid_jid,
            phone_number,
            push_name,
            status_text,
            profile_pic_url,
            is_business,
            username,
            devices,
            verified_name,
            updated_at as "updated_at: chrono::DateTime<chrono::Utc>"
        FROM contacts
        WHERE jid = $1
        AND session_id = $2
        "#,
        id,
        session_id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to fetch contact from DB")
}

pub async fn create(id: &str, session_id: &str) -> Result<SqliteQueryResult> {
    info!(
        contact_id = id,
        "Creating contact in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        INSERT INTO contacts ( 
                jid,
                session_id
            )
        VALUES (
                $1,
                $2
            )
        "#,
        id,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to create new contact on DB")
}

pub async fn delete(id: &str, session_id: &str) -> Result<SqliteQueryResult>  {
    info!(
        contact_id = id,
        "Deleting contact in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        DELETE FROM contacts
        WHERE jid = $1
        AND session_id = $2
        "#,
        id,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to delete contact from DB")
}
