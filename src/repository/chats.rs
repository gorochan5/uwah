use anyhow::{Context, Result};
use sqlx::sqlite::SqliteQueryResult;
use tracing::info;

type Chat = crate::models::chats::Chat;

pub async fn get_all() -> Result<Vec<Chat>> {
    info!("Fetching all chats from DB");

    let pool = crate::database::get();
    sqlx::query_as!(
        Chat,
        r#"
        SELECT
            session_id,
            jid,
            name,
            kind,
            unread_count,
            is_archived,
            is_pinned,
            muted_until,
            last_message_at,
            subject_owner,
            subject_time,
            created_at AS "created_at!: chrono::DateTime<chrono::Utc>",
            updated_at AS "updated_at!: chrono::DateTime<chrono::Utc>"
        FROM chats
        "#
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch chats from DB")
}

pub async fn get_by_session(session_id: &str) -> Result<Vec<Chat>> {
    info!(
        session_id = session_id,
        "Fetching chats for session from DB"
    );

    let pool = crate::database::get();
    sqlx::query_as!(
        Chat,
        r#"
        SELECT
            session_id,
            jid,
            name,
            kind,
            unread_count,
            is_archived,
            is_pinned,
            muted_until,
            last_message_at,
            subject_owner,
            subject_time,
            created_at AS "created_at!: chrono::DateTime<chrono::Utc>",
            updated_at AS "updated_at!: chrono::DateTime<chrono::Utc>"
        FROM chats
        WHERE session_id = $1
        ORDER BY last_message_at DESC
        "#,
        session_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch chats for session from DB")
}

pub async fn get_by_session_paginated(
    session_id: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<Chat>> {
    info!(
        session_id = session_id,
        limit = limit,
        offset = offset,
        "Fetching paginated chats for session from DB"
    );

    let pool = crate::database::get();
    let limit = limit.clamp(1, 100);
    let offset = offset.max(0);

    sqlx::query_as!(
        Chat,
        r#"
        SELECT
            session_id,
            jid,
            name,
            kind,
            unread_count,
            is_archived,
            is_pinned,
            muted_until,
            last_message_at,
            subject_owner,
            subject_time,
            created_at AS "created_at!: chrono::DateTime<chrono::Utc>",
            updated_at AS "updated_at!: chrono::DateTime<chrono::Utc>"
        FROM chats
        WHERE session_id = $1
        ORDER BY last_message_at DESC
        LIMIT $2 OFFSET $3
        "#,
        session_id,
        limit,
        offset
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch paginated chats for session from DB")
}

pub async fn get_one(jid: &str, session_id: &str) -> Result<Option<Chat>> {
    info!(
        chat_jid = jid,
        session_id = session_id,
        "Fetching chat from DB"
    );

    let pool = crate::database::get();
    sqlx::query_as!(
        Chat,
        r#"
        SELECT
            session_id,
            jid,
            name,
            kind,
            unread_count,
            is_archived,
            is_pinned,
            muted_until,
            last_message_at,
            subject_owner,
            subject_time,
            created_at AS "created_at!: chrono::DateTime<chrono::Utc>",
            updated_at AS "updated_at!: chrono::DateTime<chrono::Utc>"
        FROM chats
        WHERE jid = $1
        AND session_id = $2
        "#,
        jid,
        session_id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to fetch chat from DB")
}

pub async fn create(jid: &str, session_id: &str) -> Result<SqliteQueryResult> {
    info!(
        chat_jid = jid,
        session_id = session_id,
        "Creating chat in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        INSERT INTO chats ( 
                jid,
                session_id
            )
        VALUES (
                $1,
                $2
            )
        "#,
        jid,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to create new chat on DB")
}

pub async fn ensure_exists(jid: &str, session_id: &str) -> Result<()> {
    let pool = crate::database::get();

    // 1. Defensively guarantee root session row exists so foreign key never fails
    sqlx::query!(
        r#"
        INSERT INTO sessions (id, status, created_at, updated_at)
        VALUES ($1, 'QRCODE', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO NOTHING
        "#,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to assert root session exists in DB")?;

    // 2. Classify kind based on standard WhatsApp JID suffixes
    let kind = if jid.ends_with("@g.us") {
        "group"
    } else if jid.ends_with("@newsletter") {
        "newsletter"
    } else if jid.ends_with("@broadcast") {
        "broadcast"
    } else {
        "direct"
    };

    sqlx::query!(
        r#"
        INSERT INTO chats (jid, session_id, kind)
        VALUES ($1, $2, $3)
        ON CONFLICT(session_id, jid) DO UPDATE SET
            kind = CASE WHEN chats.kind = 'direct' AND excluded.kind != 'direct' THEN excluded.kind ELSE chats.kind END
        "#,
        jid,
        session_id,
        kind
    )
    .execute(pool)
    .await
    .context("Failed to ensure chat exists in DB")?;

    Ok(())
}

pub async fn update_last_message(
    jid: &str,
    session_id: &str,
    timestamp: i64,
    increment_unread: bool,
) -> Result<()> {
    let pool = crate::database::get();
    let unread_delta: i64 = if increment_unread { 1 } else { 0 };

    sqlx::query!(
        r#"
        UPDATE chats
        SET last_message_at = MAX(COALESCE(last_message_at, 0), $1),
            unread_count = unread_count + $2,
            updated_at = CURRENT_TIMESTAMP
        WHERE jid = $3 AND session_id = $4
        "#,
        timestamp,
        unread_delta,
        jid,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to update chat last_message_at in DB")?;

    Ok(())
}

pub async fn update_subject(
    session_id: &str,
    jid: &str,
    name: Option<&str>,
    subject_owner: Option<&str>,
    subject_time: Option<i64>,
) -> Result<()> {
    let pool = crate::database::get();
    sqlx::query!(
        r#"
        UPDATE chats
        SET name = COALESCE($1, name),
            subject_owner = COALESCE($2, subject_owner),
            subject_time = COALESCE($3, subject_time),
            updated_at = CURRENT_TIMESTAMP
        WHERE jid = $4 AND session_id = $5
        "#,
        name,
        subject_owner,
        subject_time,
        jid,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to update chat subject in DB")?;

    Ok(())
}

pub async fn delete(jid: &str, session_id: &str) -> Result<SqliteQueryResult> {
    info!(
        chat_jid = jid,
        session_id = session_id,
        "Deleting chat in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        DELETE FROM chats   
        WHERE jid = $1
        AND session_id = $2
        "#,
        jid,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to delete chat from DB")
}

pub async fn mark_as_read(session_id: &str, jid: &str) -> Result<()> {
    let pool = crate::database::get();
    sqlx::query!(
        r#"
        UPDATE chats
        SET unread_count = 0,
            updated_at = CURRENT_TIMESTAMP
        WHERE session_id = $1 AND jid = $2
        "#,
        session_id,
        jid
    )
    .execute(pool)
    .await
    .context("Failed to mark chat as read in DB")?;

    Ok(())
}

pub async fn update_mute(session_id: &str, jid: &str, muted_until: Option<i64>) -> Result<()> {
    let pool = crate::database::get();
    sqlx::query!(
        r#"
        UPDATE chats
        SET muted_until = $1,
            updated_at = CURRENT_TIMESTAMP
        WHERE session_id = $2 AND jid = $3
        "#,
        muted_until,
        session_id,
        jid
    )
    .execute(pool)
    .await
    .context("Failed to update chat mute status in DB")?;

    Ok(())
}

pub async fn update_pinned(session_id: &str, jid: &str, is_pinned: bool) -> Result<()> {
    let pool = crate::database::get();
    sqlx::query!(
        r#"
        UPDATE chats
        SET is_pinned = $1,
            updated_at = CURRENT_TIMESTAMP
        WHERE session_id = $2 AND jid = $3
        "#,
        is_pinned,
        session_id,
        jid
    )
    .execute(pool)
    .await
    .context("Failed to update chat pinned status in DB")?;

    Ok(())
}

pub async fn update_archived(session_id: &str, jid: &str, is_archived: bool) -> Result<()> {
    let pool = crate::database::get();
    sqlx::query!(
        r#"
        UPDATE chats
        SET is_archived = $1,
            updated_at = CURRENT_TIMESTAMP
        WHERE session_id = $2 AND jid = $3
        "#,
        is_archived,
        session_id,
        jid
    )
    .execute(pool)
    .await
    .context("Failed to update chat archived status in DB")?;

    Ok(())
}
