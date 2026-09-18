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
    info!(session_id = session_id, "Fetching chats for session from DB");

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
        ORDER BY last_message_at DESC NULLS LAST
        "#,
        session_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch chats for session from DB")
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
    sqlx::query!(
        r#"
        INSERT INTO chats (jid, session_id)
        VALUES ($1, $2)
        ON CONFLICT(session_id, jid) DO NOTHING
        "#,
        jid,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to ensure chat exists in DB")?;

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
