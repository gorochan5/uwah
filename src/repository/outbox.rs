use crate::models::outbox::{OutboxEvent, WebhookDlqEntry};
use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::SqliteConnection;
use uuid::Uuid;

pub async fn enqueue_tx(
    tx: &mut SqliteConnection,
    session_id: &str,
    event_type: &str,
    aggregate_id: Option<&str>,
    payload: &serde_json::Value,
) -> Result<String> {
    let id = Uuid::now_v7().to_string();
    let payload_str = serde_json::to_string(payload)
        .context("Failed to serialize outbox event payload to JSON string")?;
    let now_ts = Utc::now().timestamp_millis();

    sqlx::query(
        r#"
        INSERT INTO outbox_events (
            id, session_id, event_type, aggregate_id, payload,
            status, retry_count, next_retry_at, lease_until, created_at
        )
        VALUES ($1, $2, $3, $4, $5, 'PENDING', 0, $6, 0, CURRENT_TIMESTAMP)
        "#,
    )
    .bind(&id)
    .bind(session_id)
    .bind(event_type)
    .bind(aggregate_id)
    .bind(&payload_str)
    .bind(now_ts)
    .execute(tx)
    .await
    .context("Failed to insert outbox event in transaction")?;

    Ok(id)
}

pub async fn enqueue(
    session_id: &str,
    event_type: &str,
    aggregate_id: Option<&str>,
    payload: &serde_json::Value,
) -> Result<String> {
    let pool = crate::database::get();
    let mut conn = pool.acquire().await?;
    enqueue_tx(&mut conn, session_id, event_type, aggregate_id, payload).await
}

pub async fn poll_pending(limit: i64) -> Result<Vec<OutboxEvent>> {
    let pool = crate::database::get();
    let now_ts = Utc::now().timestamp_millis();

    sqlx::query_as::<_, OutboxEvent>(
        r#"
        SELECT
            id,
            session_id,
            event_type,
            aggregate_id,
            payload,
            status,
            retry_count,
            next_retry_at,
            lease_until,
            last_error,
            created_at
        FROM outbox_events
        WHERE status = 'PENDING' AND next_retry_at <= $1
        ORDER BY created_at ASC
        LIMIT $2
        "#,
    )
    .bind(now_ts)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("Failed to poll pending outbox events")
}

pub async fn mark_dispatched(id: &str) -> Result<()> {
    let pool = crate::database::get();
    sqlx::query(
        r#"
        DELETE FROM outbox_events
        WHERE id = $1
        "#,
    )
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to remove dispatched outbox event")?;

    Ok(())
}

pub async fn mark_failed(
    id: &str,
    retry_count: i64,
    next_retry_at: i64,
    error: &str,
) -> Result<()> {
    let pool = crate::database::get();
    sqlx::query(
        r#"
        UPDATE outbox_events
        SET retry_count = $1,
            next_retry_at = $2,
            last_error = $3,
            status = 'PENDING'
        WHERE id = $4
        "#,
    )
    .bind(retry_count)
    .bind(next_retry_at)
    .bind(error)
    .bind(id)
    .execute(pool)
    .await
    .context("Failed to update failed outbox event")?;

    Ok(())
}

pub async fn move_to_dlq(id: &str, error_reason: &str) -> Result<()> {
    let pool = crate::database::get();
    let mut tx = pool.begin().await?;

    let event_opt = sqlx::query_as::<_, OutboxEvent>(
        r#"
        SELECT
            id,
            session_id,
            event_type,
            aggregate_id,
            payload,
            status,
            retry_count,
            next_retry_at,
            lease_until,
            last_error,
            created_at
        FROM outbox_events
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(event) = event_opt {
        sqlx::query(
            r#"
            INSERT INTO webhook_dlq (
                id, session_id, event_type, aggregate_id, payload,
                retry_count, error_reason, failed_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                error_reason = excluded.error_reason,
                retry_count = excluded.retry_count,
                failed_at = CURRENT_TIMESTAMP
            "#,
        )
        .bind(&event.id)
        .bind(&event.session_id)
        .bind(&event.event_type)
        .bind(&event.aggregate_id)
        .bind(&event.payload)
        .bind(event.retry_count)
        .bind(error_reason)
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM outbox_events WHERE id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }

    tx.commit().await?;
    Ok(())
}

pub async fn get_dlq_all() -> Result<Vec<WebhookDlqEntry>> {
    let pool = crate::database::get();
    sqlx::query_as::<_, WebhookDlqEntry>(
        r#"
        SELECT
            id,
            session_id,
            event_type,
            aggregate_id,
            payload,
            retry_count,
            error_reason,
            failed_at
        FROM webhook_dlq
        ORDER BY failed_at DESC
        "#,
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch DLQ entries")
}

pub async fn get_dlq_by_id(id: &str) -> Result<Option<WebhookDlqEntry>> {
    let pool = crate::database::get();
    sqlx::query_as::<_, WebhookDlqEntry>(
        r#"
        SELECT
            id,
            session_id,
            event_type,
            aggregate_id,
            payload,
            retry_count,
            error_reason,
            failed_at
        FROM webhook_dlq
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .context("Failed to fetch DLQ entry by ID")
}

pub async fn replay_dlq(id: &str) -> Result<bool> {
    let pool = crate::database::get();
    let mut tx = pool.begin().await?;

    let dlq_opt = sqlx::query_as::<_, WebhookDlqEntry>(
        r#"
        SELECT
            id,
            session_id,
            event_type,
            aggregate_id,
            payload,
            retry_count,
            error_reason,
            failed_at
        FROM webhook_dlq
        WHERE id = $1
        "#,
    )
    .bind(id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(entry) = dlq_opt else {
        return Ok(false);
    };

    let now_ts = Utc::now().timestamp_millis();
    sqlx::query(
        r#"
        INSERT INTO outbox_events (
            id, session_id, event_type, aggregate_id, payload,
            status, retry_count, next_retry_at, lease_until, created_at
        )
        VALUES ($1, $2, $3, $4, $5, 'PENDING', 0, $6, 0, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO UPDATE SET
            status = 'PENDING',
            retry_count = 0,
            next_retry_at = excluded.next_retry_at,
            last_error = NULL
        "#,
    )
    .bind(&entry.id)
    .bind(&entry.session_id)
    .bind(&entry.event_type)
    .bind(&entry.aggregate_id)
    .bind(&entry.payload)
    .bind(now_ts)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM webhook_dlq WHERE id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;
    Ok(true)
}

pub async fn replay_dlq_all() -> Result<usize> {
    let all = get_dlq_all().await?;
    let mut count = 0;
    for entry in all {
        if replay_dlq(&entry.id).await? {
            count += 1;
        }
    }
    Ok(count)
}

pub async fn delete_dlq(id: &str) -> Result<bool> {
    let pool = crate::database::get();
    let res = sqlx::query("DELETE FROM webhook_dlq WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .context("Failed to delete DLQ entry")?;

    Ok(res.rows_affected() > 0)
}
