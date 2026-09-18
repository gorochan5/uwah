use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use crate::api::http::{
    error::{AppError, AppResult, ErrorResponse},
    state::AppState,
};
use crate::models::calls::Call;
use crate::repository::calls as repo;
use crate::repository::sessions as session_repo;

#[utoipa::path(
    get,
    path = "/sessions/{id}/calls",
    tag = "Calls",
    summary = "List calls",
    description = "Retrieves voice and video call logs for the specified session.",
    params(
        ("id" = String, Path, description = "Session identifier")
    ),
    responses(
        (status = 200, description = "List of calls", body = Vec<Call>),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn list_calls(
    State(_state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> AppResult<Json<Vec<Call>>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let calls = repo::get_by_session(&session_id).await?;
    Ok(Json(calls))
}

#[utoipa::path(
    get,
    path = "/sessions/{id}/calls/{call_id}",
    tag = "Calls",
    summary = "Get call details",
    description = "Retrieves details of a specific voice or video call.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("call_id" = String, Path, description = "Call identifier")
    ),
    responses(
        (status = 200, description = "Call details", body = Call),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Call or session not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn get_call(
    State(_state): State<Arc<AppState>>,
    Path((session_id, call_id)): Path<(String, String)>,
) -> AppResult<Json<Call>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let call = repo::get_one(&call_id, &session_id).await?;
    match call {
        Some(c) => Ok(Json(c)),
        None => Err(AppError::NotFound(format!("Call '{}' not found in session '{}'", call_id, session_id))),
    }
}

#[utoipa::path(
    delete,
    path = "/sessions/{id}/calls/{call_id}",
    tag = "Calls",
    summary = "Delete call",
    description = "Deletes a call log entry from local storage.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("call_id" = String, Path, description = "Call identifier")
    ),
    responses(
        (status = 204, description = "Call record deleted successfully"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Call not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn delete_call(
    State(_state): State<Arc<AppState>>,
    Path((session_id, call_id)): Path<(String, String)>,
) -> AppResult<StatusCode> {
    let _ = repo::get_one(&call_id, &session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Call '{}' not found", call_id)))?;

    repo::delete(&call_id, &session_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

