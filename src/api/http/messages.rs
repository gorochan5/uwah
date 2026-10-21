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
pub struct StickerPackItemRequest {
    /// Base64 encoded sticker image data (PNG, JPEG, GIF, or WebP)
    pub data_base64: String,
    /// Optional list of emojis associated with this sticker (e.g. ["😀", "🎉"])
    pub emojis: Option<Vec<String>>,
    /// Optional accessibility description for the sticker
    pub accessibility_label: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SendStickerPackRequest {
    /// Optional custom sticker pack identifier (auto-generated UUID v4 if omitted)
    pub pack_id: Option<String>,
    /// Display name of the sticker pack
    #[schema(example = "Meus Stickers")]
    pub name: String,
    /// Publisher / author name of the sticker pack
    #[schema(example = "Uwah")]
    pub publisher: String,
    /// Optional description of the sticker pack
    pub description: Option<String>,
    /// Optional caption displayed with the sticker pack message
    pub caption: Option<String>,
    /// List of stickers in the pack (1 to 60 items)
    pub stickers: Vec<StickerPackItemRequest>,
    /// Optional base64 cover/tray icon image (defaults to the first sticker)
    pub cover_base64: Option<String>,
    /// Optional base64 thumbnail image (defaults to auto-generated JPEG from cover/first sticker)
    pub thumbnail_base64: Option<String>,
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
        && tokio::fs::try_exists(&existing.disk_path)
            .await
            .unwrap_or(false)
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
        && tokio::fs::try_exists(&existing.disk_path)
            .await
            .unwrap_or(false)
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
        tokio::fs::write(&disk_path, &webp_bytes)
            .await
            .map_err(|e| {
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
        .apply_typing_jitter(&session_id, &parsed_jid, &client, 0, false)
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
            AppError::Internal(anyhow::anyhow!(
                "Failed to retrieve created sticker message"
            ))
        })?;

    Ok((StatusCode::CREATED, Json(saved)))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/messages/sticker-pack",
    tag = "Messages",
    summary = "Send sticker pack message",
    description = "Bundles 1 to 60 stickers into a WhatsApp sticker pack ZIP with cover and JPEG thumbnail, uploads to WhatsApp CDN, and transmits as an installable sticker pack.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Recipient WhatsApp JID or phone number")
    ),
    request_body = SendStickerPackRequest,
    responses(
        (status = 201, description = "Sticker pack sent and stored", body = Message),
        (status = 400, description = "Invalid payload or image processing failure", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Failed to upload or transmit sticker pack", body = ErrorResponse)
    )
)]
pub async fn send_sticker_pack(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Json(req): Json<SendStickerPackRequest>,
) -> AppResult<(StatusCode, Json<Message>)> {
    use whatsapp_rust::wacore::sticker_pack::{
        StickerInput, StickerPackMetadata, build_sticker_pack_message, create_sticker_pack_zip,
    };

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

    let pack_name = req.name.trim().to_string();
    let publisher = req.publisher.trim().to_string();
    if pack_name.is_empty() || publisher.is_empty() {
        return Err(AppError::BadRequest(
            "Both 'name' and 'publisher' are required and cannot be empty".to_string(),
        ));
    }

    if req.stickers.is_empty() || req.stickers.len() > 60 {
        return Err(AppError::BadRequest(format!(
            "Sticker pack must contain between 1 and 60 stickers (got {})",
            req.stickers.len()
        )));
    }

    let pack_id = req
        .pack_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            s.chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
        })
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let mut processed_webp_buffers: Vec<Vec<u8>> = Vec::with_capacity(req.stickers.len());
    let mut first_raw_bytes: Option<Vec<u8>> = None;

    for (idx, item) in req.stickers.iter().enumerate() {
        let raw_bytes = base64::engine::general_purpose::STANDARD
            .decode(item.data_base64.trim())
            .map_err(|e| {
                AppError::BadRequest(format!("Invalid base64 at sticker index {}: {}", idx, e))
            })?;

        if raw_bytes.is_empty() {
            return Err(AppError::BadRequest(format!(
                "Sticker binary payload at index {} cannot be empty",
                idx
            )));
        }

        if first_raw_bytes.is_none() {
            first_raw_bytes = Some(raw_bytes.clone());
        }

        let webp_bytes = crate::media::sticker::process_sticker_image(&raw_bytes).map_err(|e| {
            AppError::BadRequest(format!(
                "Sticker image processing failed at index {}: {}",
                idx, e
            ))
        })?;

        processed_webp_buffers.push(webp_bytes);
    }

    let mut sticker_inputs: Vec<StickerInput<'_>> = Vec::with_capacity(req.stickers.len());
    for (idx, item) in req.stickers.iter().enumerate() {
        let mut input = StickerInput::new(&processed_webp_buffers[idx]);
        if let Some(emojis) = &item.emojis
            && !emojis.is_empty()
        {
            input = input.with_emojis(emojis.clone());
        }
        if let Some(label) = item
            .accessibility_label
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            input = input.with_accessibility_label(label.to_string());
        }
        sticker_inputs.push(input);
    }

    // Resolve tray icon WebP (96x96 static WebP < 50 KB required by WhatsApp Android)
    let (cover_webp, cover_raw_for_thumb) = if let Some(cover_b64) = req
        .cover_base64
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let raw_cover = base64::engine::general_purpose::STANDARD
            .decode(cover_b64)
            .map_err(|e| AppError::BadRequest(format!("Invalid cover_base64 payload: {}", e)))?;
        let webp_cover = crate::media::sticker::process_sticker_pack_tray_icon_webp(&raw_cover)
            .map_err(|e| AppError::BadRequest(format!("Cover image processing failed: {}", e)))?;
        (webp_cover, Some(raw_cover))
    } else {
        let webp_cover = crate::media::sticker::process_sticker_pack_tray_icon_webp(
            first_raw_bytes.as_deref().unwrap_or(&processed_webp_buffers[0]),
        )
        .unwrap_or_else(|_| processed_webp_buffers[0].clone());
        (webp_cover, None)
    };

    // Resolve JPEG thumbnail (explicit thumbnail_base64 -> cover raw -> first sticker raw)
    let thumbnail_jpeg = if let Some(thumb_b64) = req
        .thumbnail_base64
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let raw_thumb = base64::engine::general_purpose::STANDARD
            .decode(thumb_b64)
            .map_err(|e| {
                AppError::BadRequest(format!("Invalid thumbnail_base64 payload: {}", e))
            })?;
        crate::media::sticker::process_sticker_pack_thumbnail_jpeg(&raw_thumb).map_err(|e| {
            AppError::BadRequest(format!("Thumbnail image processing failed: {}", e))
        })?
    } else {
        let source_bytes = cover_raw_for_thumb
            .as_deref()
            .or(first_raw_bytes.as_deref())
            .unwrap_or(&cover_webp);
        crate::media::sticker::process_sticker_pack_thumbnail_jpeg(source_bytes)
            .map_err(|e| AppError::BadRequest(format!("Auto-thumbnail generation failed: {}", e)))?
    };

    let zip_result = create_sticker_pack_zip(&pack_id, &sticker_inputs, &cover_webp)
        .map_err(|e| AppError::BadRequest(format!("Failed to create sticker pack ZIP: {}", e)))?;

    let sha256_hex = hex::encode(Sha256::digest(&zip_result.zip_bytes));

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
        "sticker_pack",
        &sha256_hex,
        "zip",
    );
    let disk_path_str = disk_path.to_string_lossy().to_string();

    if let Ok(Some(existing)) = crate::repository::media_cache::get_one(&sha256_hex).await
        && tokio::fs::try_exists(&existing.disk_path)
            .await
            .unwrap_or(false)
    {
        let _ = crate::repository::media_cache::touch_last_accessed(&sha256_hex).await;
    } else {
        if let Some(parent) = disk_path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        tokio::fs::write(&disk_path, &zip_result.zip_bytes)
            .await
            .map_err(|e| {
                AppError::Internal(anyhow::anyhow!(
                    "Failed to persist sticker pack ZIP to disk: {}",
                    e
                ))
            })?;

        let _ = crate::repository::media_cache::upsert(
            &sha256_hex,
            &disk_path_str,
            "application/zip",
            zip_result.zip_bytes.len() as i64,
        )
        .await;
    }

    // Upload the sticker pack ZIP
    let zip_upload = client
        .upload(
            zip_result.zip_bytes.clone(),
            MediaType::StickerPack,
            UploadOptions::default(),
        )
        .await
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!(
                "WhatsApp CDN sticker pack ZIP upload failed: {}",
                e
            ))
        })?;

    // Upload the thumbnail JPEG sharing the same media_key as the ZIP upload
    let thumb_upload = client
        .upload(
            thumbnail_jpeg,
            MediaType::StickerPackThumbnail,
            UploadOptions::new().with_media_key(zip_upload.media_key),
        )
        .await
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!(
                "WhatsApp CDN sticker pack thumbnail upload failed: {}",
                e
            ))
        })?;

    let parsed_jid = parse_recipient_jid(&jid)?;
    let clean_quoted_id = req
        .quoted_message_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let mut metadata = StickerPackMetadata::new(pack_id, pack_name.clone(), publisher);
    if let Some(desc) = req
        .description
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        metadata = metadata.with_description(desc.to_string());
    }
    if let Some(cap) = req
        .caption
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        metadata = metadata.with_caption(cap.to_string());
    }

    let direct_path = zip_upload.direct_path.clone();
    let media_key_vec = zip_upload.media_key.to_vec();
    let file_enc_sha256_hex = hex::encode(zip_upload.file_enc_sha256);
    let zip_media_key_ts = zip_upload.media_key_timestamp;
    let thumb_sha256_b64 =
        base64::engine::general_purpose::STANDARD.encode(thumb_upload.file_sha256);

    let mut wa_message = build_sticker_pack_message(
        &zip_result,
        &zip_upload.into(),
        &thumb_upload.into(),
        metadata,
    )
    .map_err(|e| AppError::BadRequest(format!("Failed to build sticker pack message: {}", e)))?;

    if let Some(mut pack_msg) = wa_message.sticker_pack_message.as_option().cloned() {
        pack_msg.sticker_pack_origin =
            Some(wa::message::sticker_pack_message::StickerPackOrigin::UserCreated);
        pack_msg.media_key_timestamp = Some(zip_media_key_ts);
        pack_msg.thumbnail_height = Some(crate::media::sticker::STICKER_THUMBNAIL_SIZE);
        pack_msg.thumbnail_width = Some(crate::media::sticker::STICKER_THUMBNAIL_SIZE);
        pack_msg.image_data_hash = Some(thumb_sha256_b64);

        if let Some(quoted_id) = clean_quoted_id {
            pack_msg.context_info = MessageField::some(
                build_quote_context(&session_id, &client, &session, &parsed_jid, quoted_id).await,
            );
        }
        wa_message.sticker_pack_message = MessageField::some(pack_msg);
    }

    state
        .traffic
        .apply_typing_jitter(&session_id, &parsed_jid, &client, 0, false)
        .await;

    let send_res = client
        .send_message(parsed_jid.clone(), wa_message)
        .await
        .map_err(|err| {
            AppError::Internal(anyhow::anyhow!(
                "Failed to send sticker pack message over WhatsApp: {}",
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
        message_type: "sticker_pack".to_string(),
        body: Some(pack_name),
        cdn_url: Some(direct_path),
        media_key: Some(media_key_vec),
        file_enc_sha256: Some(file_enc_sha256_hex),
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
            AppError::Internal(anyhow::anyhow!(
                "Failed to retrieve created sticker pack message"
            ))
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
            AppError::Internal(anyhow::anyhow!(
                "Failed to send WhatsApp location message: {}",
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

    let body_repr = req
        .address
        .or(req.name)
        .unwrap_or_else(|| format!("Location: ({:.6}, {:.6})", req.latitude, req.longitude));

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
    #[schema(
        example = "BEGIN:VCARD\nVERSION:3.0\nN:Doe;Jane;;;\nFN:Jane Doe\nTEL;TYPE=CELL:+1234567890\nEND:VCARD"
    )]
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
        .apply_typing_jitter(
            &session_id,
            &parsed_jid,
            &client,
            req.display_name.len(),
            false,
        )
        .await;

    let send_res = client
        .send_message(parsed_jid.clone(), wa_message)
        .await
        .map_err(|err| {
            AppError::Internal(anyhow::anyhow!(
                "Failed to send WhatsApp contact message: {}",
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
