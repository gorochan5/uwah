use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::str::FromStr;
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};
use whatsapp_rust::Jid;
use whatsapp_rust::download::MediaType;
use whatsapp_rust::prelude::*;
use whatsapp_rust::proto_helpers;
use whatsapp_rust::upload::UploadOptions;
use whatsapp_rust::waproto::buffa::MessageField;
use whatsapp_rust::waproto::whatsapp as wa;

use crate::api::http::{
    error::{AppError, AppResult, ErrorResponse},
    state::AppState,
};
use crate::models::messages::Message;
use crate::repository::messages as repo;
use crate::repository::sessions as session_repo;

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListMessagesQuery {
    #[param(example = 50)]
    pub limit: Option<i64>,
    pub before: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SendMessageRequest {
    pub body: Option<String>,
    pub message_type: Option<String>,
    pub quoted_message_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SendMediaRequest {
    /// Base64 encoded binary file data
    pub data_base64: String,
    /// Media category: "image", "audio", "video", "document", or "sticker"
    pub media_type: String,
    /// MIME type string (e.g. "image/jpeg", "audio/ogg", "video/mp4", "application/pdf")
    pub mimetype: String,
    /// Optional caption (for images, videos, documents)
    pub caption: Option<String>,
    /// Optional document filename (e.g. "invoice.pdf")
    pub filename: Option<String>,
    /// Optional quoted message ID for replies
    pub quoted_message_id: Option<String>,
    /// Optional flag for voice note / push-to-talk audio (audio/ogg)
    pub is_voice_note: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SendStickerRequest {
    /// Base64 encoded image data (PNG, JPEG, GIF, or WebP)
    pub data_base64: String,
    /// Optional quoted message ID for replies
    pub quoted_message_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ReactMessageRequest {
    /// Emoji string to react with (e.g. "👍", "❤️"), or empty string "" to remove
    pub emoji: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EditMessageRequest {
    /// Replacement text body
    pub body: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MessageActionResponse {
    pub message_id: String,
    pub status: String,
}

fn parse_recipient_jid(jid: &str) -> Result<Jid, AppError> {
    let normalized = if !jid.contains('@') {
        format!("{}@s.whatsapp.net", jid)
    } else {
        jid.to_string()
    };
    Jid::from_str(&normalized)
        .map_err(|e| AppError::BadRequest(format!("Invalid WhatsApp JID '{}': {}", normalized, e)))
}

async fn build_quote_context(
    session_id: &str,
    client: &Arc<Client>,
    session: &crate::models::sessions::Session,
    parsed_jid: &Jid,
    quoted_id: &str,
) -> wa::ContextInfo {
    let quoted_db_msg = repo::get_one(quoted_id, session_id).await.unwrap_or(None);

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
        (
            parsed_jid.clone(),
            parsed_jid.clone(),
            wa::Message::default(),
        )
    };

    proto_helpers::build_quote_context_with_info(
        quoted_id,
        &sender_jid,
        &quoted_chat_jid,
        parsed_jid,
        &quoted_msg,
    )
}

async fn auto_register_contact_if_needed(session_id: &str, parsed_jid: &Jid, chat_jid_str: &str) {
    if matches!(
        parsed_jid.server,
        whatsapp_rust::Server::Pn | whatsapp_rust::Server::Lid
    ) {
        let phone = if parsed_jid.server == whatsapp_rust::Server::Pn {
            Some(parsed_jid.user.as_str())
        } else {
            None
        };
        let lid = if parsed_jid.server == whatsapp_rust::Server::Lid {
            Some(chat_jid_str)
        } else {
            None
        };

        if let Err(err) =
            crate::repository::contacts::ensure_exists(session_id, chat_jid_str, phone, None, lid)
                .await
        {
            tracing::warn!(
                session_id = %session_id,
                jid = %chat_jid_str,
                error = %err,
                "Failed to auto-register outbound contact (best-effort)"
            );
        }
    }
}

#[utoipa::path(
    get,
    path = "/sessions/{id}/chats/{jid}/messages",
    tag = "Messages",
    summary = "List messages",
    description = "Retrieves stored messages for a specific chat with limit and keyset pagination.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "WhatsApp chat JID"),
        ListMessagesQuery
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
    Query(query): Query<ListMessagesQuery>,
) -> AppResult<Json<Vec<Message>>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let messages = repo::get_by_chat_paginated(&session_id, &jid, limit, query.before).await?;
    Ok(Json(messages))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/messages",
    tag = "Messages",
    summary = "Send text message",
    description = "Transmits an outgoing text or quote reply message via active WhatsApp bot session.",
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
) -> AppResult<(StatusCode, Json<Message>)> {
    let session = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let body_str = req.body.unwrap_or_default();
    let msg_type = req.message_type.unwrap_or_else(|| "text".to_string());

    if body_str.trim().is_empty() && msg_type == "text" {
        return Err(AppError::BadRequest(
            "Message body cannot be empty for text messages".to_string(),
        ));
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

    let parsed_jid = parse_recipient_jid(&jid)?;
    let clean_quoted_id = req
        .quoted_message_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let wa_message = if let Some(quoted_id) = clean_quoted_id {
        let context_info =
            build_quote_context(&session_id, &client, &session, &parsed_jid, quoted_id).await;
        wa::Message::text_with_context(&body_str, context_info)
    } else {
        wa::Message::text(&body_str)
    };

    state
        .traffic
        .apply_typing_jitter(&session_id, &parsed_jid, &client, body_str.len(), false)
        .await;

    let send_res = client
        .send_message(parsed_jid.clone(), wa_message)
        .await
        .map_err(|err| {
            AppError::Internal(anyhow::anyhow!("Failed to send WhatsApp message: {}", err))
        })?;

    let sender_jid = session
        .phone_number
        .map(|p| format!("{}@s.whatsapp.net", p))
        .unwrap_or_else(|| "me@s.whatsapp.net".to_string());

    let chat_jid_str = parsed_jid.to_string();
    let _ = crate::repository::chats::ensure_exists(&chat_jid_str, &session_id).await;
    auto_register_contact_if_needed(&session_id, &parsed_jid, &chat_jid_str).await;

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
    crate::api::http::metrics::inc_messages_outbound();

    let saved = repo::get_one(&send_res.message_id, &session_id)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Failed to retrieve created message")))?;

    Ok((StatusCode::CREATED, Json(saved)))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/messages/media",
    tag = "Messages",
    summary = "Send media message",
    description = "Encrypts, uploads to WhatsApp CDN, and transmits media (image, audio, video, document, sticker).",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Recipient WhatsApp JID or phone number")
    ),
    request_body = SendMediaRequest,
    responses(
        (status = 201, description = "Media message sent and stored", body = Message),
        (status = 400, description = "Invalid payload or unsupported media type", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Failed to upload or transmit media", body = ErrorResponse)
    )
)]
pub async fn send_media(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Json(req): Json<SendMediaRequest>,
) -> AppResult<(StatusCode, Json<Message>)> {
    let session = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

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

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(req.data_base64.trim())
        .map_err(|e| AppError::BadRequest(format!("Invalid base64 payload: {}", e)))?;

    if bytes.is_empty() {
        return Err(AppError::BadRequest(
            "Media binary payload cannot be empty".to_string(),
        ));
    }

    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let sha256_bytes = hasher.finalize();
    let sha256_hex = hex::encode(sha256_bytes);

    let media_dir = std::path::Path::new(&state.config.media.dir);
    tokio::fs::create_dir_all(media_dir)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to create media dir: {}", e)))?;

    let ext = match req.media_type.as_str() {
        "image" => {
            if req.mimetype.contains("png") {
                "png"
            } else {
                "jpg"
            }
        }
        "video" => "mp4",
        "audio" => {
            if req.is_voice_note.unwrap_or(false) {
                "ogg"
            } else {
                "mp3"
            }
        }
        "sticker" => "webp",
        "document" => req
            .filename
            .as_deref()
            .and_then(|f| f.split('.').next_back())
            .unwrap_or("bin"),
        _ => "bin",
    };
    let is_group = jid.ends_with("@g.us");
    let chat_name = if is_group {
        crate::repository::chats::get_one(&jid, &session_id)
            .await
            .ok()
            .flatten()
            .and_then(|c| c.name)
            .unwrap_or_else(|| jid.clone())
    } else {
        crate::repository::contacts::get_one(&jid, &session_id)
            .await
            .ok()
            .flatten()
            .and_then(|c| c.push_name.or(c.phone_number))
            .unwrap_or_else(|| jid.clone())
    };

    let disk_path = crate::media::storage::get_chat_media_path(
        &state.config.media.dir,
        is_group,
        &chat_name,
        &req.media_type,
        &sha256_hex,
        ext,
    );
    let disk_path_str = disk_path.to_string_lossy().to_string();

    if let Ok(Some(existing)) = crate::repository::media_cache::get_one(&sha256_hex).await
        && tokio::fs::try_exists(&existing.disk_path).await.unwrap_or(false)
    {
        let _ = crate::repository::media_cache::touch_last_accessed(&sha256_hex).await;
        tracing::info!(
            media_sha256 = %sha256_hex,
            disk_path = %existing.disk_path,
            "Media cache hit, skipping disk write and DB upsert"
        );
    } else {
        if let Some(parent) = disk_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        tokio::fs::write(&disk_path, &bytes).await.map_err(|e| {
            AppError::Internal(anyhow::anyhow!("Failed to persist media to disk: {}", e))
        })?;

        let _ = crate::repository::media_cache::upsert(
            &sha256_hex,
            &disk_path_str,
            &req.mimetype,
            bytes.len() as i64,
        )
        .await;
    }

    let media_kind = match req.media_type.as_str() {
        "image" => MediaType::Image,
        "video" => MediaType::Video,
        "audio" => MediaType::Audio,
        "document" => MediaType::Document,
        "sticker" => MediaType::Sticker,
        other => {
            return Err(AppError::BadRequest(format!(
                "Unsupported media_type '{}'. Allowed: image, video, audio, document, sticker",
                other
            )));
        }
    };

    let is_anim = if req.media_type == "sticker" {
        whatsapp_rust::webp::is_animated(&bytes)
    } else {
        false
    };

    let upload = client
        .upload(bytes, media_kind, UploadOptions::default())
        .await
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!("WhatsApp CDN media upload failed: {}", e))
        })?;

    let parsed_jid = parse_recipient_jid(&jid)?;
    let clean_quoted_id = req
        .quoted_message_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let context_info: MessageField<wa::ContextInfo> = if let Some(quoted_id) = clean_quoted_id {
        MessageField::some(
            build_quote_context(&session_id, &client, &session, &parsed_jid, quoted_id).await,
        )
    } else {
        MessageField::none()
    };

    let wa_message = match req.media_type.as_str() {
        "image" => wa::Message {
            image_message: MessageField::some(wa::message::ImageMessage {
                url: Some(upload.url.clone()),
                direct_path: Some(upload.direct_path.clone()),
                media_key: Some(upload.media_key.to_vec()),
                file_enc_sha256: Some(upload.file_enc_sha256.to_vec()),
                file_sha256: Some(upload.file_sha256.to_vec()),
                file_length: Some(upload.file_length),
                media_key_timestamp: Some(upload.media_key_timestamp),
                caption: req.caption.clone(),
                mimetype: Some(req.mimetype.clone()),
                context_info,
                ..Default::default()
            }),
            ..Default::default()
        },
        "video" => wa::Message {
            video_message: MessageField::some(wa::message::VideoMessage {
                url: Some(upload.url.clone()),
                direct_path: Some(upload.direct_path.clone()),
                media_key: Some(upload.media_key.to_vec()),
                file_enc_sha256: Some(upload.file_enc_sha256.to_vec()),
                file_sha256: Some(upload.file_sha256.to_vec()),
                file_length: Some(upload.file_length),
                media_key_timestamp: Some(upload.media_key_timestamp),
                streaming_sidecar: upload.streaming_sidecar.clone(),
                caption: req.caption.clone(),
                mimetype: Some(req.mimetype.clone()),
                context_info,
                ..Default::default()
            }),
            ..Default::default()
        },
        "audio" => wa::Message {
            audio_message: MessageField::some(wa::message::AudioMessage {
                url: Some(upload.url.clone()),
                direct_path: Some(upload.direct_path.clone()),
                media_key: Some(upload.media_key.to_vec()),
                file_enc_sha256: Some(upload.file_enc_sha256.to_vec()),
                file_sha256: Some(upload.file_sha256.to_vec()),
                file_length: Some(upload.file_length),
                media_key_timestamp: Some(upload.media_key_timestamp),
                streaming_sidecar: upload.streaming_sidecar.clone(),
                mimetype: Some(req.mimetype.clone()),
                ptt: req.is_voice_note,
                context_info,
                ..Default::default()
            }),
            ..Default::default()
        },
        "document" => wa::Message {
            document_message: MessageField::some(wa::message::DocumentMessage {
                url: Some(upload.url.clone()),
                direct_path: Some(upload.direct_path.clone()),
                media_key: Some(upload.media_key.to_vec()),
                file_enc_sha256: Some(upload.file_enc_sha256.to_vec()),
                file_sha256: Some(upload.file_sha256.to_vec()),
                file_length: Some(upload.file_length),
                media_key_timestamp: Some(upload.media_key_timestamp),
                title: req.filename.clone().or_else(|| req.caption.clone()),
                file_name: req.filename.clone(),
                caption: req.caption.clone(),
                mimetype: Some(req.mimetype.clone()),
                context_info,
                ..Default::default()
            }),
            ..Default::default()
        },
        "sticker" => wa::Message {
            sticker_message: MessageField::some(wa::message::StickerMessage {
                    url: Some(upload.url.clone()),
                    direct_path: Some(upload.direct_path.clone()),
                    media_key: Some(upload.media_key.to_vec()),
                    file_enc_sha256: Some(upload.file_enc_sha256.to_vec()),
                    file_sha256: Some(upload.file_sha256.to_vec()),
                    file_length: Some(upload.file_length),
                    media_key_timestamp: Some(upload.media_key_timestamp),
                    mimetype: Some("image/webp".to_string()),
                    is_animated: Some(is_anim),
                    context_info,
                    ..Default::default()
                }),
                ..Default::default()
            },
        _ => unreachable!(),
    };

    state
        .traffic
        .apply_typing_jitter(
            &session_id,
            &parsed_jid,
            &client,
            req.caption.as_deref().unwrap_or("").len(),
            req.media_type == "audio",
        )
        .await;

    let send_res = client
        .send_message(parsed_jid.clone(), wa_message)
        .await
        .map_err(|err| {
            AppError::Internal(anyhow::anyhow!(
                "Failed to send media message over WhatsApp: {}",
                err
            ))
        })?;

    let sender_jid = session
        .phone_number
        .map(|p| format!("{}@s.whatsapp.net", p))
        .unwrap_or_else(|| "me@s.whatsapp.net".to_string());

    let chat_jid_str = parsed_jid.to_string();
    let _ = crate::repository::chats::ensure_exists(&chat_jid_str, &session_id).await;
    auto_register_contact_if_needed(&session_id, &parsed_jid, &chat_jid_str).await;

    let message = Message {
        id: send_res.message_id.clone(),
        session_id: session_id.clone(),
        chat_jid: chat_jid_str,
        sender_jid,
        quoted_message_id: clean_quoted_id.map(ToString::to_string),
        is_from_me: true,
        message_type: req.media_type,
        body: req.caption,
        cdn_url: Some(upload.direct_path),
        media_key: Some(upload.media_key.to_vec()),
        file_enc_sha256: Some(hex::encode(upload.file_enc_sha256)),
        media_sha256: Some(sha256_hex),
        delivery_status: "sent".to_string(),
        is_edited: false,
        is_deleted: false,
        timestamp: chrono::Utc::now(),
    };

    repo::create(&message).await?;
    crate::api::http::metrics::inc_messages_outbound();

    let saved = repo::get_one(&send_res.message_id, &session_id)
        .await?
        .ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!("Failed to retrieve created media message"))
        })?;

    Ok((StatusCode::CREATED, Json(saved)))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/messages/sticker",
    tag = "Messages",
    summary = "Send sticker message",
    description = "Converts input image (PNG, JPEG, GIF, WebP) to a 512x512 WebP sticker, uploads to WhatsApp CDN, and transmits.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Recipient WhatsApp JID or phone number")
    ),
    request_body = SendStickerRequest,
    responses(
        (status = 201, description = "Sticker message sent and stored", body = Message),
        (status = 400, description = "Invalid payload or image processing failure", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Failed to upload or transmit sticker", body = ErrorResponse)
    )
)]
pub async fn send_sticker(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Json(req): Json<SendStickerRequest>,
) -> AppResult<(StatusCode, Json<Message>)> {
    let session = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

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

    let raw_bytes = base64::engine::general_purpose::STANDARD
        .decode(req.data_base64.trim())
        .map_err(|e| AppError::BadRequest(format!("Invalid base64 payload: {}", e)))?;

    if raw_bytes.is_empty() {
        return Err(AppError::BadRequest(
            "Sticker binary payload cannot be empty".to_string(),
        ));
    }

    // Process image into a 512x512 WebP sticker format
    let webp_bytes = crate::media::sticker::process_sticker_image(&raw_bytes)
        .map_err(|e| AppError::BadRequest(format!("Sticker image processing failed: {}", e)))?;

    let mut hasher = Sha256::new();
    hasher.update(&webp_bytes);
    let sha256_bytes = hasher.finalize();
    let sha256_hex = hex::encode(sha256_bytes);

    let media_dir = std::path::Path::new(&state.config.media.dir);
    tokio::fs::create_dir_all(media_dir)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to create media dir: {}", e)))?;

    let is_group = jid.ends_with("@g.us");
    let chat_name = if is_group {
        crate::repository::chats::get_one(&jid, &session_id)
            .await
            .ok()
            .flatten()
            .and_then(|c| c.name)
            .unwrap_or_else(|| jid.clone())
    } else {
        crate::repository::contacts::get_one(&jid, &session_id)
            .await
            .ok()
            .flatten()
            .and_then(|c| c.push_name.or(c.phone_number))
            .unwrap_or_else(|| jid.clone())
    };

    let disk_path = crate::media::storage::get_chat_media_path(
        &state.config.media.dir,
        is_group,
        &chat_name,
        "sticker",
        &sha256_hex,
        "webp",
    );
    let disk_path_str = disk_path.to_string_lossy().to_string();

    if let Ok(Some(existing)) = crate::repository::media_cache::get_one(&sha256_hex).await
        && tokio::fs::try_exists(&existing.disk_path).await.unwrap_or(false)
    {
        let _ = crate::repository::media_cache::touch_last_accessed(&sha256_hex).await;
        tracing::info!(
            media_sha256 = %sha256_hex,
            disk_path = %existing.disk_path,
            "Media cache hit, skipping disk write and DB upsert"
        );
    } else {
        if let Some(parent) = disk_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        tokio::fs::write(&disk_path, &webp_bytes).await.map_err(|e| {
            AppError::Internal(anyhow::anyhow!("Failed to persist sticker to disk: {}", e))
        })?;

        let _ = crate::repository::media_cache::upsert(
            &sha256_hex,
            &disk_path_str,
            "image/webp",
            webp_bytes.len() as i64,
        )
        .await;
    }

    let is_anim = whatsapp_rust::webp::is_animated(&webp_bytes);

    let upload = client
        .upload(webp_bytes, MediaType::Sticker, UploadOptions::default())
        .await
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!("WhatsApp CDN sticker upload failed: {}", e))
        })?;

    let parsed_jid = parse_recipient_jid(&jid)?;
    let clean_quoted_id = req
        .quoted_message_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let context_info: MessageField<wa::ContextInfo> = if let Some(quoted_id) = clean_quoted_id {
        MessageField::some(
            build_quote_context(&session_id, &client, &session, &parsed_jid, quoted_id).await,
        )
    } else {
        MessageField::none()
    };

    let wa_message = wa::Message {
        sticker_message: MessageField::some(wa::message::StickerMessage {
            url: Some(upload.url.clone()),
            direct_path: Some(upload.direct_path.clone()),
            media_key: Some(upload.media_key.to_vec()),
            file_enc_sha256: Some(upload.file_enc_sha256.to_vec()),
            file_sha256: Some(upload.file_sha256.to_vec()),
            file_length: Some(upload.file_length),
            media_key_timestamp: Some(upload.media_key_timestamp),
            mimetype: Some("image/webp".to_string()),
            is_animated: Some(is_anim),
            context_info,
            ..Default::default()
        }),
        ..Default::default()
    };

    state
        .traffic
        .apply_typing_jitter(
            &session_id,
            &parsed_jid,
            &client,
            0,
            false,
        )
        .await;

    let send_res = client
        .send_message(parsed_jid.clone(), wa_message)
        .await
        .map_err(|err| {
            AppError::Internal(anyhow::anyhow!(
                "Failed to send sticker message over WhatsApp: {}",
                err
            ))
        })?;

    let sender_jid = session
        .phone_number
        .map(|p| format!("{}@s.whatsapp.net", p))
        .unwrap_or_else(|| "me@s.whatsapp.net".to_string());

    let chat_jid_str = parsed_jid.to_string();
    let _ = crate::repository::chats::ensure_exists(&chat_jid_str, &session_id).await;
    auto_register_contact_if_needed(&session_id, &parsed_jid, &chat_jid_str).await;

    let message = Message {
        id: send_res.message_id.clone(),
        session_id: session_id.clone(),
        chat_jid: chat_jid_str,
        sender_jid,
        quoted_message_id: clean_quoted_id.map(ToString::to_string),
        is_from_me: true,
        message_type: "sticker".to_string(),
        body: None,
        cdn_url: Some(upload.direct_path),
        media_key: Some(upload.media_key.to_vec()),
        file_enc_sha256: Some(hex::encode(upload.file_enc_sha256)),
        media_sha256: Some(sha256_hex),
        delivery_status: "sent".to_string(),
        is_edited: false,
        is_deleted: false,
        timestamp: chrono::Utc::now(),
    };

    repo::create(&message).await?;
    crate::api::http::metrics::inc_messages_outbound();

    let saved = repo::get_one(&send_res.message_id, &session_id)
        .await?
        .ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!("Failed to retrieve created sticker message"))
        })?;

    Ok((StatusCode::CREATED, Json(saved)))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/messages/{msg_id}/reaction",
    tag = "Messages",
    summary = "React to message",
    description = "Sends an emoji reaction to a message over WhatsApp protocol. Pass empty string \"\" to remove reaction.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "WhatsApp chat JID"),
        ("msg_id" = String, Path, description = "Target message identifier")
    ),
    request_body = ReactMessageRequest,
    responses(
        (status = 200, description = "Reaction transmitted", body = MessageActionResponse),
        (status = 400, description = "Session not connected or bot worker inactive", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Network or send failure", body = ErrorResponse)
    )
)]
pub async fn react_message(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid, msg_id)): Path<(String, String, String)>,
    Json(req): Json<ReactMessageRequest>,
) -> AppResult<Json<MessageActionResponse>> {
    let session = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    if session.status != "CONNECTED" {
        return Err(AppError::BadRequest(format!(
            "Session '{}' is not connected (current: {})",
            session_id, session.status
        )));
    }

    let client = state
        .sessions
        .get_client(&session_id)
        .await
        .ok_or_else(|| {
            AppError::BadRequest(format!(
                "Bot worker not running for session '{}'",
                session_id
            ))
        })?;

    let chat_jid = parse_recipient_jid(&jid)?;
    let target = repo::get_one(&msg_id, &session_id).await?;

    let is_from_me = target.as_ref().map(|m| m.is_from_me).unwrap_or(false);
    let participant = if jid.ends_with("@g.us") {
        target
            .as_ref()
            .and_then(|m| Jid::from_str(&m.sender_jid).ok())
    } else {
        None
    };

    let target_key = wa::MessageKey {
        remote_jid: Some(chat_jid.to_string()),
        from_me: Some(is_from_me),
        id: Some(msg_id.clone()),
        participant: participant.map(|p| p.to_string()),
    };

    let send_res = client
        .send_reaction(&chat_jid, target_key, &req.emoji)
        .await
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!(
                "Failed to send reaction over WhatsApp: {}",
                e
            ))
        })?;

    Ok(Json(MessageActionResponse {
        message_id: send_res.message_id,
        status: "sent".to_string(),
    }))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/messages/{msg_id}/revoke",
    tag = "Messages",
    summary = "Revoke message (delete for everyone)",
    description = "Transmits a WhatsApp protocol revoke stanza to delete a message for everyone in the chat and marks it deleted locally.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "WhatsApp chat JID"),
        ("msg_id" = String, Path, description = "Message identifier to revoke")
    ),
    responses(
        (status = 200, description = "Message revoked successfully", body = MessageActionResponse),
        (status = 400, description = "Session not connected or bot worker inactive", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session or message not found", body = ErrorResponse),
        (status = 500, description = "Protocol or database failure", body = ErrorResponse)
    )
)]
pub async fn revoke_message(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid, msg_id)): Path<(String, String, String)>,
) -> AppResult<Json<MessageActionResponse>> {
    let session = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    if session.status != "CONNECTED" {
        return Err(AppError::BadRequest(format!(
            "Session '{}' is not connected (current: {})",
            session_id, session.status
        )));
    }

    let client = state
        .sessions
        .get_client(&session_id)
        .await
        .ok_or_else(|| {
            AppError::BadRequest(format!(
                "Bot worker not running for session '{}'",
                session_id
            ))
        })?;

    let chat_jid = parse_recipient_jid(&jid)?;
    client
        .revoke_message(&chat_jid, &msg_id, whatsapp_rust::send::RevokeType::Sender)
        .await
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!(
                "Failed to revoke message over WhatsApp: {}",
                e
            ))
        })?;

    repo::mark_as_deleted(&session_id, &msg_id).await?;

    Ok(Json(MessageActionResponse {
        message_id: msg_id,
        status: "revoked".to_string(),
    }))
}

#[utoipa::path(
    patch,
    path = "/sessions/{id}/chats/{jid}/messages/{msg_id}",
    tag = "Messages",
    summary = "Edit message",
    description = "Edits a previously sent text message within WhatsApp's 15-minute allowed window.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "WhatsApp chat JID"),
        ("msg_id" = String, Path, description = "Original message identifier")
    ),
    request_body = EditMessageRequest,
    responses(
        (status = 200, description = "Message edited successfully", body = MessageActionResponse),
        (status = 400, description = "Session not connected or invalid request", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session or message not found", body = ErrorResponse),
        (status = 500, description = "Protocol or database failure", body = ErrorResponse)
    )
)]
pub async fn edit_message(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid, msg_id)): Path<(String, String, String)>,
    Json(req): Json<EditMessageRequest>,
) -> AppResult<Json<MessageActionResponse>> {
    let session = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    if session.status != "CONNECTED" {
        return Err(AppError::BadRequest(format!(
            "Session '{}' is not connected (current: {})",
            session_id, session.status
        )));
    }

    if req.body.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Replacement message body cannot be empty".to_string(),
        ));
    }

    let client = state
        .sessions
        .get_client(&session_id)
        .await
        .ok_or_else(|| {
            AppError::BadRequest(format!(
                "Bot worker not running for session '{}'",
                session_id
            ))
        })?;

    let chat_jid = parse_recipient_jid(&jid)?;
    let new_content = wa::Message {
        conversation: Some(req.body.clone()),
        ..Default::default()
    };

    let send_res = client
        .edit_message(&chat_jid, &msg_id, new_content)
        .await
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!(
                "Failed to edit message over WhatsApp: {}",
                e
            ))
        })?;

    repo::update_edited_message(&session_id, &msg_id, Some(&req.body)).await?;

    Ok(Json(MessageActionResponse {
        message_id: send_res,
        status: "edited".to_string(),
    }))
}

#[utoipa::path(
    delete,
    path = "/sessions/{id}/chats/{jid}/messages/{msg_id}",
    tag = "Messages",
    summary = "Delete message (local)",
    description = "Deletes a message record from local storage by message ID (does not revoke from other users).",
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct SendLocationRequest {
    /// Latitude in degrees (-90.0 to 90.0)
    #[schema(example = -23.55052)]
    pub latitude: f64,
    /// Longitude in degrees (-180.0 to 180.0)
    #[schema(example = -46.633308)]
    pub longitude: f64,
    /// Optional name of the location / point of interest
    #[schema(example = "São Paulo Cathedral")]
    pub name: Option<String>,
    /// Optional physical street address
    #[schema(example = "Praça da Sé - Sé, São Paulo - SP, Brazil")]
    pub address: Option<String>,
    /// Optional URL link for the location
    pub url: Option<String>,
    /// Optional quoted message ID for replies
    pub quoted_message_id: Option<String>,
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/messages/location",
    tag = "Messages",
    summary = "Send location message",
    description = "Transmits a geographic coordinate location message to the specified WhatsApp recipient.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Recipient WhatsApp JID")
    ),
    request_body = SendLocationRequest,
    responses(
        (status = 201, description = "Location message sent and stored", body = Message),
        (status = 400, description = "Invalid payload or inactive WhatsApp bot", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Protocol or storage failure", body = ErrorResponse)
    )
)]
pub async fn send_location(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Json(req): Json<SendLocationRequest>,
) -> AppResult<(StatusCode, Json<Message>)> {
    let session = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

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

    let parsed_jid = parse_recipient_jid(&jid)?;
    let clean_quoted_id = req
        .quoted_message_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let context_info = if let Some(quoted_id) = clean_quoted_id {
        Some(build_quote_context(&session_id, &client, &session, &parsed_jid, quoted_id).await)
    } else {
        None
    };

    let loc_msg = wa::message::LocationMessage {
        degrees_latitude: Some(req.latitude),
        degrees_longitude: Some(req.longitude),
        name: req.name.clone(),
        address: req.address.clone(),
        url: req.url.clone(),
        context_info: context_info.map(MessageField::some).unwrap_or_default(),
        ..Default::default()
    };

    let wa_message = wa::Message {
        location_message: MessageField::some(loc_msg),
        ..Default::default()
    };

    state
        .traffic
        .apply_typing_jitter(&session_id, &parsed_jid, &client, 20, false)
        .await;

    let send_res = client
        .send_message(parsed_jid.clone(), wa_message)
        .await
        .map_err(|err| {
            AppError::Internal(anyhow::anyhow!("Failed to send WhatsApp location message: {}", err))
        })?;

    let sender_jid = session
        .phone_number
        .map(|p| format!("{}@s.whatsapp.net", p))
        .unwrap_or_else(|| "me@s.whatsapp.net".to_string());

    let chat_jid_str = parsed_jid.to_string();
    let _ = crate::repository::chats::ensure_exists(&chat_jid_str, &session_id).await;
    auto_register_contact_if_needed(&session_id, &parsed_jid, &chat_jid_str).await;

    let body_repr = req.address.or(req.name).unwrap_or_else(|| {
        format!("Location: ({:.6}, {:.6})", req.latitude, req.longitude)
    });

    let message = Message {
        id: send_res.message_id.clone(),
        session_id: session_id.clone(),
        chat_jid: chat_jid_str,
        sender_jid,
        quoted_message_id: clean_quoted_id.map(ToString::to_string),
        is_from_me: true,
        message_type: "location".to_string(),
        body: Some(body_repr),
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
    crate::api::http::metrics::inc_messages_outbound();

    let saved = repo::get_one(&send_res.message_id, &session_id)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Failed to retrieve created message")))?;

    Ok((StatusCode::CREATED, Json(saved)))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SendContactRequest {
    /// Full contact name (e.g. "Jane Doe")
    #[schema(example = "Jane Doe")]
    pub display_name: String,
    /// Standard vCard formatted contact string (vCard 3.0)
    #[schema(example = "BEGIN:VCARD\nVERSION:3.0\nN:Doe;Jane;;;\nFN:Jane Doe\nTEL;TYPE=CELL:+1234567890\nEND:VCARD")]
    pub vcard: String,
    /// Optional quoted message ID for replies
    pub quoted_message_id: Option<String>,
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/messages/contact",
    tag = "Messages",
    summary = "Send contact card",
    description = "Transmits a vCard contact business card to the specified WhatsApp recipient.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Recipient WhatsApp JID")
    ),
    request_body = SendContactRequest,
    responses(
        (status = 201, description = "Contact card sent and stored", body = Message),
        (status = 400, description = "Invalid payload or inactive WhatsApp bot", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Protocol or storage failure", body = ErrorResponse)
    )
)]
pub async fn send_contact(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Json(req): Json<SendContactRequest>,
) -> AppResult<(StatusCode, Json<Message>)> {
    let session = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    if req.display_name.trim().is_empty() || req.vcard.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Both display_name and vcard are required and cannot be empty".to_string(),
        ));
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

    let parsed_jid = parse_recipient_jid(&jid)?;
    let clean_quoted_id = req
        .quoted_message_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let context_info = if let Some(quoted_id) = clean_quoted_id {
        Some(build_quote_context(&session_id, &client, &session, &parsed_jid, quoted_id).await)
    } else {
        None
    };

    let contact_msg = wa::message::ContactMessage {
        display_name: Some(req.display_name.clone()),
        vcard: Some(req.vcard.clone()),
        context_info: context_info.map(MessageField::some).unwrap_or_default(),
        ..Default::default()
    };

    let wa_message = wa::Message {
        contact_message: MessageField::some(contact_msg),
        ..Default::default()
    };

    state
        .traffic
        .apply_typing_jitter(&session_id, &parsed_jid, &client, req.display_name.len(), false)
        .await;

    let send_res = client
        .send_message(parsed_jid.clone(), wa_message)
        .await
        .map_err(|err| {
            AppError::Internal(anyhow::anyhow!("Failed to send WhatsApp contact message: {}", err))
        })?;

    let sender_jid = session
        .phone_number
        .map(|p| format!("{}@s.whatsapp.net", p))
        .unwrap_or_else(|| "me@s.whatsapp.net".to_string());

    let chat_jid_str = parsed_jid.to_string();
    let _ = crate::repository::chats::ensure_exists(&chat_jid_str, &session_id).await;
    auto_register_contact_if_needed(&session_id, &parsed_jid, &chat_jid_str).await;

    let message = Message {
        id: send_res.message_id.clone(),
        session_id: session_id.clone(),
        chat_jid: chat_jid_str,
        sender_jid,
        quoted_message_id: clean_quoted_id.map(ToString::to_string),
        is_from_me: true,
        message_type: "contact".to_string(),
        body: Some(req.display_name),
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
    crate::api::http::metrics::inc_messages_outbound();

    let saved = repo::get_one(&send_res.message_id, &session_id)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Failed to retrieve created message")))?;

    Ok((StatusCode::CREATED, Json(saved)))
}

