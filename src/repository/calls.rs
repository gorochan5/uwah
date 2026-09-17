use anyhow::{Context, Result};
use sqlx::sqlite::SqliteQueryResult;
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

pub async fn get_by_session(session_id: &str) -> Result<Vec<Call>> {
    info!(session_id = session_id, "Fetching calls for session from DB");

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
        WHERE session_id = $1
        ORDER BY created_at DESC
        "#,
        session_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch calls for session from DB")
}

pub async fn get_one(id: &str, session_id: &str) -> Result<Option<Call>> {
    info!(
        call_id = id,
        session_id = session_id,
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

pub async fn create(call: &Call) -> Result<SqliteQueryResult> {
    info!(
        call_id = call.id,
        session_id = call.session_id,
        "Creating call in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        INSERT INTO calls ( 
            session_id,
            id,
            chat_jid,
            caller_jid,
            is_video,
            is_group_call,
            status,
            duration_seconds,
            created_at,
            ended_at
        )
        VALUES (
            $1, $2, $3, $4, $5, $6, $7, $8, $9, $10
        )
        "#,
        call.session_id,
        call.id,
        call.chat_jid,
        call.caller_jid,
        call.is_video,
        call.is_group_call,
        call.status,
        call.duration_seconds,
        call.created_at,
        call.ended_at
    )
    .execute(pool)
    .await
    .context("Failed to create new call on DB")
}

pub async fn delete(id: &str, session_id: &str) -> Result<SqliteQueryResult> {
    info!(
        call_id = id,
        session_id = session_id,
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
