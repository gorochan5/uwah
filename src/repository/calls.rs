use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, SqlitePool, sqlite::SqliteQueryResult};
use tracing::info;

type Call = crate::models::calls::Call;

pub async fn get_all() -> Result<Vec<Call>> {
    info!("Fetching all calls from DB");

    let pool = crate::database::get();
    sqlx::query_as!(
        Call,
        r#"
        SELECT
            session_id,
            id,
            chat_jid,
            caller_jid,
            is_video,
            is_group_call,
            status,
            duration_seconds,
            created_at AS "created_at: chrono::DateTime<chrono::Utc>",
            ended_at AS "ended_at?: chrono::DateTime<chrono::Utc>"
        FROM calls
        "#
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch calls from DB")
}

pub async fn get_one(id: &str, session_id: &str) -> Result<Option<Call>> {
    info!(
        call_id = id,
        "Fetching call from DB"
    );
    
    let pool = crate::database::get();
    sqlx::query_as!(
        Call,
        r#"
        SELECT
            session_id,
            id,
            chat_jid,
            caller_jid,
            is_video,
            is_group_call,
            status,
            duration_seconds,
            created_at AS "created_at: chrono::DateTime<chrono::Utc>",
            ended_at AS "ended_at?: chrono::DateTime<chrono::Utc>"
        FROM calls
        WHERE id = $1
        AND session_id = $2
        "#,
        id,
        session_id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to fetch call from DB")
}

pub async fn create(id: &str, session_id: &str) -> Result<SqliteQueryResult> {
    info!(
        call_id = id,
        "Creating call in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        INSERT INTO calls ( 
                session_id,
                id
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
    .context("Failed to create new call on DB")
}

pub async fn delete(id: &str, session_id: &str) -> Result<SqliteQueryResult>  {
    info!(
        call_id = id,
        "Deleting call in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        DELETE FROM calls   
        WHERE id = $1
        AND session_id = $2
        "#,
        id,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to delete call from DB")
}
