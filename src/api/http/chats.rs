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
use crate::models::chats::Chat;
use crate::repository::chats as repo;
use crate::repository::sessions as session_repo;

#[utoipa::path(
    get,
    path = "/sessions/{id}/chats",
    tag = "Chats",
    summary = "List chats",
    description = "Retrieves all conversation threads for the specified session.",
    params(
        ("id" = String, Path, description = "Session identifier")
    ),
    responses(
        (status = 200, description = "List of chats", body = Vec<Chat>),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
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

#[utoipa::path(
    get,
    path = "/sessions/{id}/chats/{jid}",
    tag = "Chats",
    summary = "Get chat details",
    description = "Retrieves a specific chat by JID for the given session.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "WhatsApp JID (e.g. 123456789@s.whatsapp.net or group@g.us)")
    ),
    responses(
        (status = 200, description = "Chat details", body = Chat),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Chat or session not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
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

#[utoipa::path(
    delete,
    path = "/sessions/{id}/chats/{jid}",
    tag = "Chats",
    summary = "Delete chat",
    description = "Deletes a chat record from local storage.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "WhatsApp JID")
    ),
    responses(
        (status = 204, description = "Chat deleted successfully"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Chat not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
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

