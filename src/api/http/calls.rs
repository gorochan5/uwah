use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use crate::api::http::{
    error::{AppError, AppResult},
    state::AppState,
};
use crate::models::calls::Call;
use crate::repository::calls as repo;
use crate::repository::sessions as session_repo;

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

