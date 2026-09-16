use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, SqlitePool, sqlite::SqliteQueryResult};
use tracing::info;

type Message = crate::models::messages::Message;

pub async fn get_all() -> Result<Vec<Message>> {
    info!("Fetching all messages from DB");
    
    let pool = crate::database::get();
    sqlx::query_as!(
        Message,
        r#"
        SELECT
            id,
            session_id,
            chat_jid,
            sender_jid,
            quoted_message_id,
            is_from_me,
            message_type,
            body,
            cdn_url,
            media_key,
            file_enc_sha256,
            media_sha256,
            delivery_status,
            is_edited,
            is_deleted,
            timestamp AS "timestamp: chrono::DateTime<chrono::Utc>"
        FROM messages
        "#
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch messages from DB")
}

pub async fn get_one(id: &str) -> Result<Option<Message>> {
    info!(
        message_id = id,
        "Fetching message from DB"
    );
    
    let pool = crate::database::get();
    sqlx::query_as!(
        Message,
        r#"
        SELECT
            id,
            session_id,
            chat_jid,
            sender_jid,
            quoted_message_id,
            is_from_me,
            message_type,
            body,
            cdn_url,
            media_key,
            file_enc_sha256,
            media_sha256,
            delivery_status,
            is_edited,
            is_deleted,
            timestamp AS "timestamp: chrono::DateTime<chrono::Utc>"
        FROM messages
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to fetch message from DB")
}

pub async fn create(id: &str) -> Result<SqliteQueryResult> {
    info!(
        message_id = id,
        "Creating message in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        INSERT INTO messages ( 
                id
            )
        VALUES (
                $1
            )
        "#,
        id
    )
    .execute(pool)
    .await
    .context("Failed to create new message on DB")
}

pub async fn delete(id: &str) -> Result<SqliteQueryResult>  {
    info!(
        message_id = id,
        "Deleting message in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        DELETE FROM messages
        WHERE id = $1
        "#,
        id,
    )
    .execute(pool)
    .await
    .context("Failed to delete message from DB")
}
