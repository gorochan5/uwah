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

use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct SendMessageRequest {
    pub body: Option<String>,
    pub message_type: Option<String>,
    pub quoted_message_id: Option<String>,
}

use crate::api::http::error::ErrorResponse;

#[utoipa::path(
    get,
    path = "/sessions/{id}/chats/{jid}/messages",
    tag = "Messages",
    summary = "List messages",
    description = "Retrieves stored messages for a specific chat.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "WhatsApp chat JID")
    ),
    responses(
        (status = 200, description = "List of messages", body = Vec<Message>),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
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

use std::str::FromStr;
use whatsapp_rust::prelude::*;
use whatsapp_rust::proto_helpers;
use whatsapp_rust::Jid;

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/messages",
    tag = "Messages",
    summary = "Send message",
    description = "Transmits an outgoing message (text or quote reply) via active WhatsApp bot session.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Recipient WhatsApp JID or phone number")
    ),
    request_body = SendMessageRequest,
    responses(
        (status = 201, description = "Message sent and stored", body = Message),
        (status = 400, description = "Invalid payload or inactive WhatsApp bot", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Failed to send message over socket", body = ErrorResponse)
    )
)]
pub async fn send_message(
    State(state): State<Arc<AppState>>,
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

    let client = state
        .sessions
        .get_client(&session_id)
        .await
        .ok_or_else(|| {
            AppError::BadRequest(format!(
                "Session '{}' is not connected or active in memory. Scan QR first.",
                session_id
            ))
        })?;

    let normalized_jid = if !jid.contains('@') {
        format!("{}@s.whatsapp.net", jid)
    } else {
        jid
    };

    let parsed_jid = Jid::from_str(&normalized_jid)
        .map_err(|e| AppError::BadRequest(format!("Invalid WhatsApp JID '{}': {}", normalized_jid, e)))?;

    let clean_quoted_id = req
        .quoted_message_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    // Build the WhatsApp message (with quote context if quoted_message_id is provided)
    let wa_message = if let Some(quoted_id) = clean_quoted_id {
        let quoted_db_msg = repo::get_one(quoted_id, &session_id).await.unwrap_or(None);

        let (sender_jid, quoted_chat_jid, quoted_msg) = if let Some(orig) = quoted_db_msg {
            let s_jid = if orig.is_from_me {
                client.pn().unwrap_or_else(|| {
                    session
                        .phone_number
                        .as_deref()
                        .and_then(|p| Jid::from_str(&format!("{}@s.whatsapp.net", p)).ok())
                        .unwrap_or_else(|| parsed_jid.clone())
                })
            } else {
                Jid::from_str(&orig.sender_jid).unwrap_or_else(|_| parsed_jid.clone())
            };

            let c_jid = Jid::from_str(&orig.chat_jid).unwrap_or_else(|_| parsed_jid.clone());

            let q_msg = if let Some(body) = orig.body {
                wa::Message {
                    conversation: Some(body),
                    ..Default::default()
                }
            } else {
                wa::Message::default()
            };

            (s_jid, c_jid, q_msg)
        } else {
            // Fallback when message is not found in local SQLite DB:
            // Quote with stanza_id and target JID
            (parsed_jid.clone(), parsed_jid.clone(), wa::Message::default())
        };

        let context_info = proto_helpers::build_quote_context_with_info(
            quoted_id,
            &sender_jid,
            &quoted_chat_jid,
            &parsed_jid,
            &quoted_msg,
        );

        wa::Message::text_with_context(&body_str, context_info)
    } else {
        wa::Message::text(&body_str)
    };

    let send_res = client
        .send_message(parsed_jid.clone(), wa_message)
        .await
        .map_err(|err| AppError::Internal(anyhow::anyhow!("Failed to send WhatsApp message: {}", err)))?;

    let sender_jid = session
        .phone_number
        .map(|p| format!("{}@s.whatsapp.net", p))
        .unwrap_or_else(|| "me@s.whatsapp.net".to_string());

    // Ensure chat stub exists so message insertion doesn't fail foreign key constraint
    let chat_jid_str = parsed_jid.to_string();
    let _ = crate::repository::chats::ensure_exists(&chat_jid_str, &session_id).await;

    let message = Message {
        id: send_res.message_id.clone(),
        session_id: session_id.clone(),
        chat_jid: chat_jid_str,
        sender_jid,
        quoted_message_id: clean_quoted_id.map(ToString::to_string),
        is_from_me: true,
        message_type: msg_type,
        body: Some(body_str),
        cdn_url: None,
        media_key: None,
        file_enc_sha256: None,
        media_sha256: None,
        delivery_status: "sent".to_string(),
        is_edited: false,
        is_deleted: false,
        timestamp: chrono::Utc::now(),
    };

    repo::create(&message).await?;

    let saved = repo::get_one(&send_res.message_id, &session_id)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Failed to retrieve created message")))?;

    Ok((StatusCode::CREATED, Json(saved)))
}

#[utoipa::path(
    delete,
    path = "/sessions/{id}/chats/{jid}/messages/{msg_id}",
    tag = "Messages",
    summary = "Delete message",
    description = "Deletes a message record from local storage by message ID.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "WhatsApp chat JID"),
        ("msg_id" = String, Path, description = "Message identifier")
    ),
    responses(
        (status = 204, description = "Message deleted successfully"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Message not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
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

