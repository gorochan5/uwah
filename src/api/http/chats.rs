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
use crate::models::chats::Chat;
use crate::repository::chats as repo;
use crate::repository::sessions as session_repo;

pub async fn list_chats(
    State(_state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> AppResult<Json<Vec<Chat>>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let chats = repo::get_by_session(&session_id).await?;
    Ok(Json(chats))
}

pub async fn get_chat(
    State(_state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
) -> AppResult<Json<Chat>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let chat = repo::get_one(&jid, &session_id).await?;
    match chat {
        Some(c) => Ok(Json(c)),
        None => Err(AppError::NotFound(format!("Chat '{}' not found in session '{}'", jid, session_id))),
    }
}

pub async fn delete_chat(
    State(_state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
) -> AppResult<StatusCode> {
    let _ = repo::get_one(&jid, &session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Chat '{}' not found", jid)))?;

    repo::delete(&jid, &session_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

