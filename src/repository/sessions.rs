use anyhow::{Context, Result};
use sqlx::sqlite::SqliteQueryResult;
use tracing::info;

type Session = crate::models::sessions::Session;

pub async fn get_all() -> Result<Vec<Session>> {
    info!("Fetching all sessions from DB");

    let pool = crate::database::get();
    sqlx::query_as!(
        Session,
        r#"
        SELECT
            id,
            phone_number,
            push_name,
            status,
            qr_payload,
            created_at AS "created_at!: chrono::DateTime<chrono::Utc>",
            updated_at AS "updated_at!: chrono::DateTime<chrono::Utc>"
        FROM sessions
        ORDER BY created_at DESC
        "#
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch sessions from DB")
}

pub async fn get_one(id: &str) -> Result<Option<Session>> {
    info!(session_id = id, "Fetching session from DB");

    let pool = crate::database::get();
    sqlx::query_as!(
        Session,
        r#"
        SELECT
            id,
            phone_number,
            push_name,
            status,
            qr_payload,
            created_at AS "created_at!: chrono::DateTime<chrono::Utc>",
            updated_at AS "updated_at!: chrono::DateTime<chrono::Utc>"
        FROM sessions
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to fetch session from DB")
}

pub async fn create(id: &str) -> Result<SqliteQueryResult> {
    info!(session_id = id, "Creating session in DB");

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        INSERT INTO sessions (
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
    .context("Failed to create new session on DB")
}

pub async fn delete(id: &str) -> Result<SqliteQueryResult> {
    info!(session_id = id, "Deleting session in DB");

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        DELETE FROM sessions
        WHERE id = $1
        "#,
        id,
    )
    .execute(pool)
    .await
    .context("Failed to delete session from DB")
}

pub async fn auth(session: Session) -> Result<SqliteQueryResult> {
    info!(
        id = session.id,
        phone_number = session.phone_number,
        push_name = session.push_name,
        status = session.status,
        "Updating session in DB as authenticated"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        UPDATE sessions
        SET
            phone_number = $1,
            push_name = $2,
            status = $3,
            qr_payload = $4,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = $5
        "#,
        session.phone_number,
        session.push_name,
        session.status,
        session.qr_payload,
        session.id
    )
    .execute(pool)
    .await
    .context("Failed to update session on DB")
}

pub async fn refresh_qr(id: &str, qr_payload: &str) -> Result<SqliteQueryResult> {
    info!(
        id = id,
        qr_payload = qr_payload,
        "Refreshing QR code for session"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        UPDATE sessions
        SET qr_payload = $1,
            status = 'QRCODE',
            updated_at = CURRENT_TIMESTAMP
        WHERE id = $2
        "#,
        qr_payload,
        id
    )
    .execute(pool)
    .await
    .context("Failed to refresh QR code for session")
}

pub async fn update_status(id: &str, status: &str) -> Result<SqliteQueryResult> {
    let pool = crate::database::get();
    sqlx::query!(
        r#"
        UPDATE sessions
        SET status = $1,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = $2
        "#,
        status,
        id
    )
    .execute(pool)
    .await
    .context("Failed to update session status on DB")
}

