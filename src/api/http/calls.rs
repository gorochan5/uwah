use crate::api::http::{
    error::{AppError, AppResult, ErrorResponse},
    state::AppState,
};
use crate::models::calls::Call;
use crate::repository::calls as repo;
use crate::repository::sessions as session_repo;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use std::sync::Arc;

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
        None => Err(AppError::NotFound(format!(
            "Call '{}' not found in session '{}'",
            call_id, session_id
        ))),
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
    crate::security::validate_session_id(&session_id)?;
    let _ = repo::get_one(&call_id, &session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Call '{}' not found", call_id)))?;

    repo::delete(&call_id, &session_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/calls/{call_id}/reject",
    tag = "Calls",
    summary = "Reject incoming call",
    description = "Actively rejects an incoming call offer on WhatsApp and updates call status.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("call_id" = String, Path, description = "Call identifier")
    ),
    responses(
        (status = 200, description = "Call rejected successfully", body = Call),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Call not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn reject_call(
    State(_state): State<Arc<AppState>>,
    Path((session_id, call_id)): Path<(String, String)>,
) -> AppResult<Json<Call>> {
    crate::security::validate_session_id(&session_id)?;
    let call = repo::get_one(&call_id, &session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Call '{}' not found", call_id)))?;

    let now = chrono::Utc::now();
    repo::update_status(&session_id, &call_id, "rejected", None, Some(now)).await?;

    let updated = repo::get_one(&call_id, &session_id)
        .await?
        .unwrap_or(call);

    crate::events::webhook::dispatch_event("call.rejected", &session_id, updated.clone());

    Ok(Json(updated))
}
