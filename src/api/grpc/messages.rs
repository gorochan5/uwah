use std::str::FromStr;
use std::sync::Arc;
use base64::Engine;
use sha2::{Digest, Sha256};
use tonic::{Request, Response, Status};
use whatsapp_rust::download::MediaType;
use whatsapp_rust::prelude::*;
use whatsapp_rust::proto_helpers;
use whatsapp_rust::upload::UploadOptions;
use whatsapp_rust::waproto::buffa::MessageField;
use whatsapp_rust::waproto::whatsapp as wa;
use whatsapp_rust::Jid;

use crate::api::grpc::chats::parse_target_jid;
use crate::api::grpc::proto::message_service_server::MessageService;
use crate::api::grpc::proto::{
    DeleteMessageRequest, DeleteMessageResponse, EditMessageRequest, ListMessagesRequest,
    ListMessagesResponse, MessageActionResponse, MessageItem, ReactMessageRequest,
    RevokeMessageRequest, SendContactRequest, SendLocationRequest, SendMediaRequest,
    SendMessageRequest,
};
use crate::api::http::state::AppState;
use crate::models::messages::Message;
use crate::repository::messages as repo;
use crate::repository::sessions as session_repo;

pub struct MessageServiceImpl {
    state: Arc<AppState>,
}

impl MessageServiceImpl {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

fn model_to_proto(m: Message) -> MessageItem {
    MessageItem {
        session_id: m.session_id,
        id: m.id,
        chat_jid: m.chat_jid,
        sender_jid: m.sender_jid,
        quoted_message_id: m.quoted_message_id,
        is_from_me: m.is_from_me,
        message_type: m.message_type,
        body: m.body,
        cdn_url: m.cdn_url,
        media_key: m.media_key,
        file_enc_sha256: m.file_enc_sha256,
        media_sha256: m.media_sha256,
        delivery_status: m.delivery_status,
        is_edited: m.is_edited,
        is_deleted: m.is_deleted,
        timestamp: m.timestamp.timestamp(),
    }
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

#[tonic::async_trait]
impl MessageService for MessageServiceImpl {
    async fn list_messages(
        &self,
        request: Request<ListMessagesRequest>,
    ) -> Result<Response<ListMessagesResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let chat_jid = req.chat_jid;

        let _ = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        let limit = req.limit.unwrap_or(50).clamp(1, 100);
        let before = req.before.and_then(|ts| chrono::DateTime::from_timestamp(ts, 0));

        let messages = repo::get_by_chat_paginated(&session_id, &chat_jid, limit, before)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        let items = messages.into_iter().map(model_to_proto).collect();
        Ok(Response::new(ListMessagesResponse { messages: items }))
    }

    async fn send_message(
        &self,
        request: Request<SendMessageRequest>,
    ) -> Result<Response<MessageItem>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.chat_jid;

        let session = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        let body_str = req.body.unwrap_or_default();
        let msg_type = req.message_type.unwrap_or_else(|| "text".to_string());

        if body_str.trim().is_empty() && msg_type == "text" {
            return Err(Status::invalid_argument(
                "Message body cannot be empty for text messages",
            ));
        }

        let client = self
            .state
            .sessions
            .get_client(&session_id)
            .await
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "Session '{}' is not connected or active in memory. Scan QR first.",
                    session_id
                ))
            })?;

        let parsed_jid = parse_target_jid(&jid)?;
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

        self.state
            .traffic
            .apply_typing_jitter(&session_id, &parsed_jid, &client, body_str.len(), false)
            .await;

        let send_res = client
            .send_message(parsed_jid.clone(), wa_message)
            .await
            .map_err(|err| Status::internal(format!("Failed to send WhatsApp message: {}", err)))?;

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

        repo::create(&message)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;
        crate::api::http::metrics::inc_messages_outbound();

        let saved = repo::get_one(&send_res.message_id, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::internal("Failed to retrieve created message"))?;

        Ok(Response::new(model_to_proto(saved)))
    }

    async fn send_location(
        &self,
        request: Request<SendLocationRequest>,
    ) -> Result<Response<MessageItem>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.chat_jid;

        let session = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        let client = self
            .state
            .sessions
            .get_client(&session_id)
            .await
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "Session '{}' is not connected or active in memory. Scan QR first.",
                    session_id
                ))
            })?;

        let parsed_jid = parse_target_jid(&jid)?;
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

        self.state
            .traffic
            .apply_typing_jitter(&session_id, &parsed_jid, &client, 20, false)
            .await;

        let send_res = client
            .send_message(parsed_jid.clone(), wa_message)
            .await
            .map_err(|err| Status::internal(format!("Failed to send WhatsApp location message: {}", err)))?;

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

        repo::create(&message)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;
        crate::api::http::metrics::inc_messages_outbound();

        let saved = repo::get_one(&send_res.message_id, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::internal("Failed to retrieve created message"))?;

        Ok(Response::new(model_to_proto(saved)))
    }

    async fn send_contact(
        &self,
        request: Request<SendContactRequest>,
    ) -> Result<Response<MessageItem>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.chat_jid;

        let session = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        if req.display_name.trim().is_empty() || req.vcard.trim().is_empty() {
            return Err(Status::invalid_argument(
                "Both display_name and vcard are required and cannot be empty",
            ));
        }

        let client = self
            .state
            .sessions
            .get_client(&session_id)
            .await
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "Session '{}' is not connected or active in memory. Scan QR first.",
                    session_id
                ))
            })?;

        let parsed_jid = parse_target_jid(&jid)?;
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

        self.state
            .traffic
            .apply_typing_jitter(&session_id, &parsed_jid, &client, req.display_name.len(), false)
            .await;

        let send_res = client
            .send_message(parsed_jid.clone(), wa_message)
            .await
            .map_err(|err| Status::internal(format!("Failed to send WhatsApp contact message: {}", err)))?;

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

        repo::create(&message)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;
        crate::api::http::metrics::inc_messages_outbound();

        let saved = repo::get_one(&send_res.message_id, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::internal("Failed to retrieve created message"))?;

        Ok(Response::new(model_to_proto(saved)))
    }

    async fn send_media(
        &self,
        request: Request<SendMediaRequest>,
    ) -> Result<Response<MessageItem>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.chat_jid;

        let session = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        let client = self
            .state
            .sessions
            .get_client(&session_id)
            .await
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "Session '{}' is not connected or active in memory. Scan QR first.",
                    session_id
                ))
            })?;

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(req.data_base64.trim())
            .map_err(|e| Status::invalid_argument(format!("Invalid base64 payload: {}", e)))?;

        if bytes.is_empty() {
            return Err(Status::invalid_argument(
                "Media binary payload cannot be empty",
            ));
        }

        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let sha256_bytes = hasher.finalize();
        let sha256_hex = hex::encode(sha256_bytes);

        let media_dir = std::path::Path::new(&self.state.config.media.dir);
        tokio::fs::create_dir_all(media_dir)
            .await
            .map_err(|e| Status::internal(format!("Failed to create media dir: {}", e)))?;

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
        let disk_filename = format!("{}.{}", sha256_hex, ext);
        let disk_path = media_dir.join(&disk_filename);
        tokio::fs::write(&disk_path, &bytes)
            .await
            .map_err(|e| Status::internal(format!("Failed to persist media to disk: {}", e)))?;

        let _ = crate::repository::media_cache::upsert(
            &sha256_hex,
            disk_path.to_str().unwrap_or(&disk_filename),
            &req.mimetype,
            bytes.len() as i64,
        )
        .await;

        let media_kind = match req.media_type.as_str() {
            "image" => MediaType::Image,
            "video" => MediaType::Video,
            "audio" => MediaType::Audio,
            "document" => MediaType::Document,
            "sticker" => MediaType::Sticker,
            other => {
                return Err(Status::invalid_argument(format!(
                    "Unsupported media_type '{}'. Allowed: image, video, audio, document, sticker",
                    other
                )));
            }
        };

        let upload = client
            .upload(bytes, media_kind, UploadOptions::default())
            .await
            .map_err(|e| Status::internal(format!("WhatsApp CDN media upload failed: {}", e)))?;

        let parsed_jid = parse_target_jid(&jid)?;
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
                    context_info,
                    ..Default::default()
                }),
                ..Default::default()
            },
            _ => unreachable!(),
        };

        self.state
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
            .map_err(|err| Status::internal(format!("Failed to send media message over WhatsApp: {}", err)))?;

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

        repo::create(&message)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;
        crate::api::http::metrics::inc_messages_outbound();

        let saved = repo::get_one(&send_res.message_id, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::internal("Failed to retrieve created media message"))?;

        Ok(Response::new(model_to_proto(saved)))
    }

    async fn edit_message(
        &self,
        request: Request<EditMessageRequest>,
    ) -> Result<Response<MessageActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.chat_jid;
        let msg_id = req.message_id;

        let session = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        if session.status != "CONNECTED" {
            return Err(Status::failed_precondition(format!(
                "Session '{}' is not connected (current: {})",
                session_id, session.status
            )));
        }

        if req.body.trim().is_empty() {
            return Err(Status::invalid_argument(
                "Replacement message body cannot be empty",
            ));
        }

        let client = self
            .state
            .sessions
            .get_client(&session_id)
            .await
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "Bot worker not running for session '{}'",
                    session_id
                ))
            })?;

        let chat_jid = parse_target_jid(&jid)?;
        let new_content = wa::Message {
            conversation: Some(req.body.clone()),
            ..Default::default()
        };

        let send_res = client
            .edit_message(&chat_jid, &msg_id, new_content)
            .await
            .map_err(|e| Status::internal(format!("Failed to edit message over WhatsApp: {}", e)))?;

        repo::update_edited_message(&session_id, &msg_id, Some(&req.body))
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        Ok(Response::new(MessageActionResponse {
            message_id: send_res,
            status: "edited".to_string(),
        }))
    }

    async fn delete_message(
        &self,
        request: Request<DeleteMessageRequest>,
    ) -> Result<Response<DeleteMessageResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let msg_id = req.message_id;

        let _ = repo::get_one(&msg_id, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Message '{}' not found", msg_id)))?;

        repo::delete(&msg_id, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to delete message: {}", e)))?;

        Ok(Response::new(DeleteMessageResponse { success: true }))
    }

    async fn react_message(
        &self,
        request: Request<ReactMessageRequest>,
    ) -> Result<Response<MessageActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.chat_jid;
        let msg_id = req.message_id;

        let session = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        if session.status != "CONNECTED" {
            return Err(Status::failed_precondition(format!(
                "Session '{}' is not connected (current: {})",
                session_id, session.status
            )));
        }

        let client = self
            .state
            .sessions
            .get_client(&session_id)
            .await
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "Bot worker not running for session '{}'",
                    session_id
                ))
            })?;

        let chat_jid = parse_target_jid(&jid)?;
        let target = repo::get_one(&msg_id, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

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
            id: Some(msg_id),
            participant: participant.map(|p| p.to_string()),
        };

        let send_res = client
            .send_reaction(&chat_jid, target_key, &req.emoji)
            .await
            .map_err(|e| Status::internal(format!("Failed to send reaction over WhatsApp: {}", e)))?;

        Ok(Response::new(MessageActionResponse {
            message_id: send_res.message_id,
            status: "sent".to_string(),
        }))
    }

    async fn revoke_message(
        &self,
        request: Request<RevokeMessageRequest>,
    ) -> Result<Response<MessageActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.chat_jid;
        let msg_id = req.message_id;

        let session = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        if session.status != "CONNECTED" {
            return Err(Status::failed_precondition(format!(
                "Session '{}' is not connected (current: {})",
                session_id, session.status
            )));
        }

        let client = self
            .state
            .sessions
            .get_client(&session_id)
            .await
            .ok_or_else(|| {
                Status::failed_precondition(format!(
                    "Bot worker not running for session '{}'",
                    session_id
                ))
            })?;

        let chat_jid = parse_target_jid(&jid)?;
        client
            .revoke_message(&chat_jid, &msg_id, whatsapp_rust::send::RevokeType::Sender)
            .await
            .map_err(|e| Status::internal(format!("Failed to revoke message over WhatsApp: {}", e)))?;

        repo::mark_as_deleted(&session_id, &msg_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        Ok(Response::new(MessageActionResponse {
            message_id: msg_id,
            status: "revoked".to_string(),
        }))
    }
}
