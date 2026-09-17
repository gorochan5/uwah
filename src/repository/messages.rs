use anyhow::{Context, Result};
use sqlx::sqlite::SqliteQueryResult;
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
        ORDER BY timestamp DESC
        "#
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch messages from DB")
}

pub async fn get_by_chat(session_id: &str, chat_jid: &str) -> Result<Vec<Message>> {
    info!(
        session_id = session_id,
        chat_jid = chat_jid,
        "Fetching messages for chat from DB"
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
        WHERE session_id = $1
        AND chat_jid = $2
        ORDER BY timestamp ASC
        "#,
        session_id,
        chat_jid
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch chat messages from DB")
}

pub async fn get_one(id: &str, session_id: &str) -> Result<Option<Message>> {
    info!(
        message_id = id,
        session_id = session_id,
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
        AND session_id = $2
        "#,
        id,
        session_id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to fetch message from DB")
}

pub async fn create(message: &Message) -> Result<SqliteQueryResult> {
    info!(
        message_id = message.id,
        session_id = message.session_id,
        "Inserting message into DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        INSERT INTO messages ( 
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
            timestamp
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16
        )
        "#,
        message.id,
        message.session_id,
        message.chat_jid,
        message.sender_jid,
        message.quoted_message_id,
        message.is_from_me,
        message.message_type,
        message.body,
        message.cdn_url,
        message.media_key,
        message.file_enc_sha256,
        message.media_sha256,
        message.delivery_status,
        message.is_edited,
        message.is_deleted,
        message.timestamp
    )
    .execute(pool)
    .await
    .context("Failed to insert message into DB")
}

pub async fn delete(id: &str, session_id: &str) -> Result<SqliteQueryResult> {
    info!(
        message_id = id,
        session_id = session_id,
        "Deleting message in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        DELETE FROM messages
        WHERE id = $1
        AND session_id = $2
        "#,
        id,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to delete message from DB")
}
