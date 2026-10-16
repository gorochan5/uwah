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

pub async fn get_by_chat_paginated(
    session_id: &str,
    chat_jid: &str,
    limit: i64,
    before: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Vec<Message>> {
    let pool = crate::database::get();
    let limit = limit.clamp(1, 100);

    match before {
        Some(before_ts) => sqlx::query_as!(
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
                  AND timestamp < $3
                ORDER BY timestamp DESC
                LIMIT $4
                "#,
            session_id,
            chat_jid,
            before_ts,
            limit
        )
        .fetch_all(pool)
        .await
        .context("Failed to fetch paginated chat messages from DB"),
        None => sqlx::query_as!(
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
                ORDER BY timestamp DESC
                LIMIT $3
                "#,
            session_id,
            chat_jid,
            limit
        )
        .fetch_all(pool)
        .await
        .context("Failed to fetch chat messages from DB"),
    }
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
        ON CONFLICT(id, session_id) DO UPDATE SET
            delivery_status = CASE
                WHEN messages.delivery_status = 'read' THEN 'read'
                WHEN messages.delivery_status = 'delivered' AND excluded.delivery_status IN ('pending', 'sent', 'received') THEN messages.delivery_status
                ELSE excluded.delivery_status
            END,
            body = CASE
                WHEN excluded.is_edited = 1 THEN excluded.body
                WHEN messages.is_edited = 1 THEN messages.body
                ELSE COALESCE(excluded.body, messages.body)
            END,
            is_edited = messages.is_edited OR excluded.is_edited,
            is_deleted = messages.is_deleted OR excluded.is_deleted,
            cdn_url = COALESCE(excluded.cdn_url, messages.cdn_url),
            media_key = COALESCE(excluded.media_key, messages.media_key),
            file_enc_sha256 = COALESCE(excluded.file_enc_sha256, messages.file_enc_sha256),
            media_sha256 = COALESCE(excluded.media_sha256, messages.media_sha256)
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

pub async fn create_batch(messages: &[Message]) -> Result<()> {
    if messages.is_empty() {
        return Ok(());
    }
    let pool = crate::database::get();

    for chunk in messages.chunks(200) {
        let mut builder = sqlx::QueryBuilder::new(
            "INSERT INTO messages (
                id, session_id, chat_jid, sender_jid, quoted_message_id,
                is_from_me, message_type, body, cdn_url, media_key,
                file_enc_sha256, media_sha256, delivery_status, is_edited,
                is_deleted, timestamp
            ) ",
        );

        builder.push_values(chunk, |mut b, msg| {
            b.push_bind(&msg.id)
                .push_bind(&msg.session_id)
                .push_bind(&msg.chat_jid)
                .push_bind(&msg.sender_jid)
                .push_bind(&msg.quoted_message_id)
                .push_bind(msg.is_from_me)
                .push_bind(&msg.message_type)
                .push_bind(&msg.body)
                .push_bind(&msg.cdn_url)
                .push_bind(&msg.media_key)
                .push_bind(&msg.file_enc_sha256)
                .push_bind(&msg.media_sha256)
                .push_bind(&msg.delivery_status)
                .push_bind(msg.is_edited)
                .push_bind(msg.is_deleted)
                .push_bind(msg.timestamp);
        });

        builder.push(
            " ON CONFLICT(id, session_id) DO UPDATE SET
                delivery_status = CASE
                    WHEN messages.delivery_status = 'read' THEN 'read'
                    WHEN messages.delivery_status = 'delivered' AND excluded.delivery_status IN ('pending', 'sent', 'received') THEN messages.delivery_status
                    ELSE excluded.delivery_status
                END,
                body = CASE
                    WHEN excluded.is_edited = 1 THEN excluded.body
                    WHEN messages.is_edited = 1 THEN messages.body
                    ELSE COALESCE(excluded.body, messages.body)
                END,
                is_edited = messages.is_edited OR excluded.is_edited,
                is_deleted = messages.is_deleted OR excluded.is_deleted,
                cdn_url = COALESCE(excluded.cdn_url, messages.cdn_url),
                media_key = COALESCE(excluded.media_key, messages.media_key),
                file_enc_sha256 = COALESCE(excluded.file_enc_sha256, messages.file_enc_sha256),
                media_sha256 = COALESCE(excluded.media_sha256, messages.media_sha256)",
        );

        builder
            .build()
            .execute(pool)
            .await
            .context("Failed to insert message batch into DB")?;
    }

    Ok(())
}

pub async fn update_delivery_status(
    session_id: &str,
    message_ids: &[String],
    status: &str,
) -> Result<()> {
    if message_ids.is_empty() {
        return Ok(());
    }

    let pool = crate::database::get();
    let mut builder = sqlx::QueryBuilder::new(
        "UPDATE messages SET delivery_status = CASE \
            WHEN delivery_status = 'read' THEN 'read' \
            WHEN delivery_status = 'delivered' AND ",
    );
    builder.push_bind(status);
    builder.push(" IN ('pending', 'sent', 'received') THEN delivery_status ELSE ");
    builder.push_bind(status);
    builder.push(" END WHERE session_id = ");
    builder.push_bind(session_id);
    builder.push(" AND id IN (");
    let mut separated = builder.separated(", ");
    for id in message_ids {
        separated.push_bind(id);
    }
    separated.push_unseparated(")");

    let query = builder.build();
    query
        .execute(pool)
        .await
        .context("Failed to update message delivery status in batch")?;

    Ok(())
}

pub async fn mark_as_deleted(session_id: &str, target_id: &str) -> Result<()> {
    let pool = crate::database::get();
    sqlx::query!(
        r#"
        UPDATE messages
        SET is_deleted = 1
        WHERE id = $1 AND session_id = $2
        "#,
        target_id,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to mark message as deleted")?;

    Ok(())
}

pub async fn update_edited_message(
    session_id: &str,
    target_id: &str,
    new_body: Option<&str>,
) -> Result<()> {
    let pool = crate::database::get();
    sqlx::query!(
        r#"
        UPDATE messages
        SET body = $1,
            is_edited = 1
        WHERE id = $2 AND session_id = $3
        "#,
        new_body,
        target_id,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to update edited message in DB")?;

    Ok(())
}

pub async fn update_media_sha256(
    id: &str,
    session_id: &str,
    media_sha256: &str,
) -> Result<SqliteQueryResult> {
    let pool = crate::database::get();
    sqlx::query!(
        r#"
        UPDATE messages
        SET media_sha256 = $1
        WHERE id = $2 AND session_id = $3
        "#,
        media_sha256,
        id,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to update media_sha256 for message in DB")
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
