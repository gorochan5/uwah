use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, SqlitePool, sqlite::SqliteQueryResult};
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
            created_at AS "created_at?: chrono::DateTime<chrono::Utc>",
            updated_at AS "updated_at?: chrono::DateTime<chrono::Utc>"
        FROM chats
        "#
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch chats from DB")
}

pub async fn get_one(id: &str, session_id: &str) -> Result<Option<Chat>> {
    info!(
        chat_id = id,
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
            created_at AS "created_at?: chrono::DateTime<chrono::Utc>",
            updated_at AS "updated_at?: chrono::DateTime<chrono::Utc>"
        FROM chats
        WHERE jid = $1
        AND session_id = $2
        "#,
        id,
        session_id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to fetch chat from DB")
}

pub async fn create(id: &str, session_id: &str) -> Result<SqliteQueryResult> {
    info!(
        chat_id = id,
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
        id,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to create new chat on DB")
}

pub async fn delete(id: &str, session_id: &str) -> Result<SqliteQueryResult>  {
    info!(
        chat_id = id,
        "Deleting chat in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        DELETE FROM chats   
        WHERE jid = $1
        AND session_id = $2
        "#,
        id,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to delete chat from DB")
}
