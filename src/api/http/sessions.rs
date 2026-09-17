use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use std::sync::Arc;
use crate::api::http::{
    error::{AppError, AppResult},
    state::AppState,
};
use crate::models::sessions::Session;
use crate::repository::sessions as repo;

#[derive(Deserialize)]
pub struct CreateSessionRequest {
    pub id: String,
}

#[derive(Deserialize)]
pub struct RefreshQrRequest {
    pub qr_payload: String,
}

pub async fn list_sessions(
    State(_state): State<Arc<AppState>>,
) -> AppResult<Json<Vec<Session>>> {
    let sessions = repo::get_all().await?;
    Ok(Json(sessions))
}

pub async fn get_session(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<Session>> {
    let session = repo::get_one(&id).await?;
    match session {
        Some(s) => Ok(Json(s)),
        None => Err(AppError::NotFound(format!("Session '{}' not found", id))),
    }
}

pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSessionRequest>,
) -> AppResult<impl IntoResponse> {
    let id = req.id.trim();
    if id.is_empty() {
        return Err(AppError::BadRequest("Session id cannot be empty".to_string()));
    }

    if repo::get_one(id).await?.is_some() {
        return Err(AppError::Conflict(format!("Session '{}' already exists", id)));
    }

    repo::create(id).await?;

    // Start background WhatsApp bot session
    let sid = id.to_string();
    if let Err(e) = state.sessions.start_session(sid).await {
        tracing::error!(session_id = %id, error = %e, "Failed to start WhatsApp bot");
        let _ = repo::delete(id).await;
        return Err(AppError::Internal(e));
    }

    let created = repo::get_one(id)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Created session not found")))?;

    Ok((StatusCode::CREATED, Json(created)))
}

pub async fn refresh_qr(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<RefreshQrRequest>,
) -> AppResult<Json<Session>> {
    let _ = repo::get_one(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", id)))?;

    repo::refresh_qr(&id, &req.qr_payload).await?;

    let updated = repo::get_one(&id)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Updated session not found")))?;

    Ok(Json(updated))
}

pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    let _ = repo::get_one(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", id)))?;

    // Stop active bot if running
    state.sessions.stop_session(&id).await;

    repo::delete(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}
