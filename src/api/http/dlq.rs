use crate::api::http::{
    error::{AppError, AppResult, ErrorResponse},
    state::AppState,
};
use crate::models::outbox::WebhookDlqEntry;
use crate::repository::outbox as repo;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Serialize;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct DlqReplayResponse {
    pub message: String,
    pub count: usize,
}

#[utoipa::path(
    get,
    path = "/webhooks/dlq",
    tag = "Webhooks",
    summary = "List Dead-Letter Queue (DLQ) entries",
    description = "Retrieves all exhausted or permanent webhook delivery failures quarantined in DLQ.",
    responses(
        (status = 200, description = "List of DLQ entries", body = Vec<WebhookDlqEntry>),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn list_dlq(
    State(_state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<WebhookDlqEntry>>> {
    let entries = repo::get_dlq_all().await?;
    Ok(Json(entries))
}

#[utoipa::path(
    get,
    path = "/webhooks/dlq/{id}",
    tag = "Webhooks",
    summary = "Get Dead-Letter Queue (DLQ) entry",
    description = "Retrieves specific DLQ entry by ID.",
    params(
        ("id" = String, Path, description = "DLQ entry identifier")
    ),
    responses(
        (status = 200, description = "DLQ entry details", body = WebhookDlqEntry),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "DLQ entry not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn get_dlq(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<WebhookDlqEntry>> {
    let entry = repo::get_dlq_by_id(&id).await?;
    match entry {
        Some(e) => Ok(Json(e)),
        None => Err(AppError::NotFound(format!("DLQ entry '{}' not found", id))),
    }
}

#[utoipa::path(
    post,
    path = "/webhooks/dlq/{id}/replay",
    tag = "Webhooks",
    summary = "Replay DLQ entry",
    description = "Moves a DLQ entry back into active outbox for immediate retry.",
    params(
        ("id" = String, Path, description = "DLQ entry identifier")
    ),
    responses(
        (status = 200, description = "DLQ entry replayed successfully", body = DlqReplayResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "DLQ entry not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn replay_dlq(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<DlqReplayResponse>> {
    let success = repo::replay_dlq(&id).await?;
    if success {
        Ok(Json(DlqReplayResponse {
            message: format!("DLQ entry '{}' re-enqueued for delivery", id),
            count: 1,
        }))
    } else {
        Err(AppError::NotFound(format!("DLQ entry '{}' not found", id)))
    }
}

#[utoipa::path(
    post,
    path = "/webhooks/dlq/replay-all",
    tag = "Webhooks",
    summary = "Replay all DLQ entries",
    description = "Moves all quarantined DLQ entries back into active outbox for retry.",
    responses(
        (status = 200, description = "All DLQ entries replayed", body = DlqReplayResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn replay_all_dlq(
    State(_state): State<Arc<AppState>>,
) -> AppResult<Json<DlqReplayResponse>> {
    let count = repo::replay_dlq_all().await?;
    Ok(Json(DlqReplayResponse {
        message: format!("Re-enqueued {} DLQ entries for delivery", count),
        count,
    }))
}

#[utoipa::path(
    delete,
    path = "/webhooks/dlq/{id}",
    tag = "Webhooks",
    summary = "Delete DLQ entry",
    description = "Permanently removes a DLQ entry without replaying.",
    params(
        ("id" = String, Path, description = "DLQ entry identifier")
    ),
    responses(
        (status = 204, description = "DLQ entry deleted successfully"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "DLQ entry not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn delete_dlq(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let deleted = repo::delete_dlq(&id).await?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound(format!("DLQ entry '{}' not found", id)))
    }
}
