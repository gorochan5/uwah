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
use crate::models::messages::Message;
use crate::repository::messages as repo;
use crate::repository::sessions as session_repo;

#[derive(Deserialize)]
pub struct SendMessageRequest {
    pub body: Option<String>,
    pub message_type: Option<String>,
    pub quoted_message_id: Option<String>,
}

pub async fn list_messages(
    State(_state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
) -> AppResult<Json<Vec<Message>>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let messages = repo::get_by_chat(&session_id, &jid).await?;
    Ok(Json(messages))
}

pub async fn send_message(
    State(_state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Json(req): Json<SendMessageRequest>,
) -> AppResult<impl IntoResponse> {
    let session = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let body_str = req.body.unwrap_or_default();
    let msg_type = req.message_type.unwrap_or_else(|| "text".to_string());

    if body_str.trim().is_empty() && msg_type == "text" {
        return Err(AppError::BadRequest("Message body cannot be empty for text messages".to_string()));
    }

    let sender_jid = session
        .phone_number
        .map(|p| format!("{}@s.whatsapp.net", p))
        .unwrap_or_else(|| "me@s.whatsapp.net".to_string());

    let message_id = format!("msg_{}", chrono::Utc::now().timestamp_millis());

    let message = Message {
        id: message_id.clone(),
        session_id: session_id.clone(),
        chat_jid: jid,
        sender_jid,
        quoted_message_id: req.quoted_message_id,
        is_from_me: true,
        message_type: msg_type,
        body: Some(body_str),
        cdn_url: None,
        media_key: None,
        file_enc_sha256: None,
        media_sha256: None,
        delivery_status: "pending".to_string(),
        is_edited: false,
        is_deleted: false,
        timestamp: chrono::Utc::now(),
    };

    repo::create(&message).await?;

    let saved = repo::get_one(&message_id, &session_id)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Failed to retrieve created message")))?;

    Ok((StatusCode::CREATED, Json(saved)))
}

pub async fn delete_message(
    State(_state): State<Arc<AppState>>,
    Path((session_id, _jid, msg_id)): Path<(String, String, String)>,
) -> AppResult<StatusCode> {
    let _ = repo::get_one(&msg_id, &session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Message '{}' not found", msg_id)))?;

    repo::delete(&msg_id, &session_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

