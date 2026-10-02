use std::str::FromStr;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use whatsapp_rust::{
    GroupDescription, GroupSubject, Jid, PreviousDescription, Server,
};

use crate::api::grpc::proto::chat_service_server::ChatService;
use crate::api::grpc::proto::{
    ArchiveChatRequest, ChatActionResponse, ChatItem, DeleteChatRequest, GetChatRequest,
    GetInviteLinkRequest, GetInviteLinkResponse, GlobalPresenceResponse, LeaveGroupRequest,
    ListChatsRequest, ListChatsResponse, MarkAsReadRequest, MuteChatRequest, PinChatRequest,
    SetChatPresenceRequest, SetDescriptionRequest, SetEphemeralTimerRequest,
    SetGlobalPresenceRequest, SetGroupAnnouncementRequest, SetGroupLockRequest, SetSubjectRequest,
};
use crate::api::http::state::AppState;
use crate::models::chats::Chat;
use crate::repository::chats as repo;
use crate::repository::sessions as session_repo;

pub struct ChatServiceImpl {
    state: Arc<AppState>,
}

impl ChatServiceImpl {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

pub(crate) fn parse_target_jid(jid: &str) -> Result<Jid, Status> {
    let normalized = if !jid.contains('@') {
        format!("{}@s.whatsapp.net", jid)
    } else {
        jid.to_string()
    };
    Jid::from_str(&normalized)
        .map_err(|e| Status::invalid_argument(format!("Invalid WhatsApp JID '{}': {}", normalized, e)))
}

pub(crate) fn parse_group_jid(jid: &str) -> Result<Jid, Status> {
    let target = parse_target_jid(jid)?;
    if target.server != Server::Group {
        return Err(Status::invalid_argument(format!(
            "JID '{}' is not a valid WhatsApp group JID (must end with @g.us)",
            jid
        )));
    }
    Ok(target)
}

pub(crate) fn map_wa_error<E: std::fmt::Display>(e: E) -> Status {
    let msg = e.to_string();
    let lower = msg.to_lowercase();
    if lower.contains("403")
        || lower.contains("not-authorized")
        || lower.contains("forbidden")
        || lower.contains("admin")
    {
        Status::permission_denied(msg)
    } else if lower.contains("409") || lower.contains("conflict") {
        Status::already_exists(msg)
    } else if lower.contains("400") || lower.contains("invalid") || lower.contains("bad request") {
        Status::invalid_argument(msg)
    } else if lower.contains("404") || lower.contains("not found") {
        Status::not_found(msg)
    } else {
        Status::internal(msg)
    }
}

fn chat_to_proto(c: Chat) -> ChatItem {
    ChatItem {
        session_id: c.session_id,
        jid: c.jid,
        name: c.name,
        unread_count: c.unread_count as i32,
        last_message_timestamp: c.last_message_at.unwrap_or(0),
        is_archived: c.is_archived,
        is_pinned: c.is_pinned,
        muted_until: c.muted_until.unwrap_or(0),
        ephemeral_expiration: 0,
        created_at: c.created_at.timestamp(),
        updated_at: c.updated_at.timestamp(),
    }
}

#[tonic::async_trait]
impl ChatService for ChatServiceImpl {
    async fn set_global_presence(
        &self,
        request: Request<SetGlobalPresenceRequest>,
    ) -> Result<Response<GlobalPresenceResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;

        let _ = session_repo::get_one(&session_id)
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

        let is_available = req.presence.trim().eq_ignore_ascii_case("available")
            || req.presence.trim().eq_ignore_ascii_case("online")
            || req.presence.trim().eq_ignore_ascii_case("true");

        let status = if is_available {
            whatsapp_rust::features::PresenceStatus::Available
        } else {
            whatsapp_rust::features::PresenceStatus::Unavailable
        };

        client.presence().set(status).await.map_err(|err| match err {
            whatsapp_rust::features::PresenceError::PushNameEmpty => Status::invalid_argument(
                "Cannot set presence: session push_name is empty or session is not authenticated yet.",
            ),
            other => map_wa_error(other),
        })?;

        Ok(Response::new(GlobalPresenceResponse {
            session_id,
            presence: if is_available { "available" } else { "unavailable" }.to_string(),
            status: "ok".to_string(),
        }))
    }

    async fn set_chat_presence(
        &self,
        request: Request<SetChatPresenceRequest>,
    ) -> Result<Response<ChatActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.jid;

        let _ = session_repo::get_one(&session_id)
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

        let target_jid = parse_target_jid(&jid)?;

        let chat_state = match req.state.trim().to_lowercase().as_str() {
            "composing" | "typing" => whatsapp_rust::features::ChatStateType::Composing,
            "recording" => whatsapp_rust::features::ChatStateType::Recording,
            "paused" | "idle" => whatsapp_rust::features::ChatStateType::Paused,
            other => {
                return Err(Status::invalid_argument(format!(
                    "Invalid chat state '{}'. Supported values: 'composing', 'recording', 'paused'",
                    other
                )));
            }
        };

        client
            .chatstate()
            .send(&target_jid, chat_state)
            .await
            .map_err(map_wa_error)?;

        Ok(Response::new(ChatActionResponse {
            session_id,
            jid,
            action: "presence".to_string(),
            status: req.state.to_lowercase(),
        }))
    }

    async fn list_chats(
        &self,
        request: Request<ListChatsRequest>,
    ) -> Result<Response<ListChatsResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;

        let _ = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        let limit = req.limit.unwrap_or(50).clamp(1, 100);
        let offset = req.offset.unwrap_or(0).max(0);
        let chats = repo::get_by_session_paginated(&session_id, limit, offset)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        let items = chats.into_iter().map(chat_to_proto).collect();
        Ok(Response::new(ListChatsResponse { chats: items }))
    }

    async fn get_chat(
        &self,
        request: Request<GetChatRequest>,
    ) -> Result<Response<ChatItem>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.jid;

        let _ = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        let chat = repo::get_one(&jid, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| {
                Status::not_found(format!(
                    "Chat '{}' not found in session '{}'",
                    jid, session_id
                ))
            })?;

        Ok(Response::new(chat_to_proto(chat)))
    }

    async fn delete_chat(
        &self,
        request: Request<DeleteChatRequest>,
    ) -> Result<Response<ChatActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.jid;

        let _ = repo::get_one(&jid, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Chat '{}' not found", jid)))?;

        repo::delete(&jid, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to delete chat: {}", e)))?;

        Ok(Response::new(ChatActionResponse {
            session_id,
            jid,
            action: "delete".to_string(),
            status: "deleted".to_string(),
        }))
    }

    async fn mark_as_read(
        &self,
        request: Request<MarkAsReadRequest>,
    ) -> Result<Response<ChatActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.jid;

        let _ = session_repo::get_one(&session_id)
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

        let target_jid = parse_target_jid(&jid)?;

        client
            .chat_actions()
            .mark_chat_as_read(&target_jid, true, None)
            .await
            .map_err(map_wa_error)?;

        repo::mark_as_read(&session_id, &jid)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        Ok(Response::new(ChatActionResponse {
            session_id,
            jid,
            action: "read".to_string(),
            status: "read".to_string(),
        }))
    }

    async fn mute_chat(
        &self,
        request: Request<MuteChatRequest>,
    ) -> Result<Response<ChatActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.jid;
        let duration = req.mute_duration_seconds;

        let _ = session_repo::get_one(&session_id)
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

        let target_jid = parse_target_jid(&jid)?;

        if duration == 0 {
            client
                .chat_actions()
                .unmute_chat(&target_jid)
                .await
                .map_err(map_wa_error)?;
            repo::update_mute(&session_id, &jid, None)
                .await
                .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

            Ok(Response::new(ChatActionResponse {
                session_id,
                jid,
                action: "mute".to_string(),
                status: "unmuted".to_string(),
            }))
        } else if duration > 0 {
            let end_ms = chrono::Utc::now().timestamp_millis() + (duration * 1000);
            client
                .chat_actions()
                .mute_chat_until(&target_jid, end_ms)
                .await
                .map_err(map_wa_error)?;
            repo::update_mute(&session_id, &jid, Some(end_ms / 1000))
                .await
                .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

            Ok(Response::new(ChatActionResponse {
                session_id,
                jid,
                action: "mute".to_string(),
                status: "muted".to_string(),
            }))
        } else {
            client
                .chat_actions()
                .mute_chat(&target_jid)
                .await
                .map_err(map_wa_error)?;
            repo::update_mute(&session_id, &jid, Some(-1))
                .await
                .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

            Ok(Response::new(ChatActionResponse {
                session_id,
                jid,
                action: "mute".to_string(),
                status: "muted".to_string(),
            }))
        }
    }

    async fn pin_chat(
        &self,
        request: Request<PinChatRequest>,
    ) -> Result<Response<ChatActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.jid;

        let _ = session_repo::get_one(&session_id)
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

        let target_jid = parse_target_jid(&jid)?;
        if req.pinned {
            client
                .chat_actions()
                .pin_chat(&target_jid)
                .await
                .map_err(map_wa_error)?;
        } else {
            client
                .chat_actions()
                .unpin_chat(&target_jid)
                .await
                .map_err(map_wa_error)?;
        }

        repo::update_pinned(&session_id, &jid, req.pinned)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        Ok(Response::new(ChatActionResponse {
            session_id,
            jid,
            action: "pin".to_string(),
            status: if req.pinned { "pinned" } else { "unpinned" }.to_string(),
        }))
    }

    async fn archive_chat(
        &self,
        request: Request<ArchiveChatRequest>,
    ) -> Result<Response<ChatActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.jid;

        let _ = session_repo::get_one(&session_id)
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

        let target_jid = parse_target_jid(&jid)?;
        if req.archived {
            client
                .chat_actions()
                .archive_chat(&target_jid, None)
                .await
                .map_err(map_wa_error)?;
        } else {
            client
                .chat_actions()
                .unarchive_chat(&target_jid, None)
                .await
                .map_err(map_wa_error)?;
        }

        repo::update_archived(&session_id, &jid, req.archived)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        Ok(Response::new(ChatActionResponse {
            session_id,
            jid,
            action: "archive".to_string(),
            status: if req.archived { "archived" } else { "unarchived" }.to_string(),
        }))
    }

    async fn set_subject(
        &self,
        request: Request<SetSubjectRequest>,
    ) -> Result<Response<ChatActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.jid;

        let _ = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        if req.subject.trim().is_empty() {
            return Err(Status::invalid_argument("Group subject cannot be empty"));
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

        let group_jid = parse_group_jid(&jid)?;
        let subject = GroupSubject::new(&req.subject)
            .map_err(|e| Status::invalid_argument(format!("Invalid group subject: {}", e)))?;

        client
            .groups()
            .set_subject(&group_jid, subject)
            .await
            .map_err(map_wa_error)?;

        let _ = repo::update_subject(&session_id, &jid, Some(&req.subject), None, None).await;

        Ok(Response::new(ChatActionResponse {
            session_id,
            jid,
            action: "set_subject".to_string(),
            status: "ok".to_string(),
        }))
    }

    async fn set_description(
        &self,
        request: Request<SetDescriptionRequest>,
    ) -> Result<Response<ChatActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.jid;

        let _ = session_repo::get_one(&session_id)
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

        let group_jid = parse_group_jid(&jid)?;
        let desc_opt = if !req.description.trim().is_empty() {
            Some(
                GroupDescription::new(&req.description)
                    .map_err(|e| Status::invalid_argument(format!("Invalid description: {}", e)))?,
            )
        } else {
            None
        };

        client
            .groups()
            .set_description(&group_jid, desc_opt, PreviousDescription::Resolve)
            .await
            .map_err(map_wa_error)?;

        Ok(Response::new(ChatActionResponse {
            session_id,
            jid,
            action: "set_description".to_string(),
            status: "ok".to_string(),
        }))
    }

    async fn set_ephemeral_timer(
        &self,
        request: Request<SetEphemeralTimerRequest>,
    ) -> Result<Response<ChatActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.jid;
        let duration = req.ephemeral_timer_seconds as u32;

        let _ = session_repo::get_one(&session_id)
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

        let target_jid = parse_target_jid(&jid)?;
        client
            .groups()
            .set_ephemeral(&target_jid, duration)
            .await
            .map_err(map_wa_error)?;

        Ok(Response::new(ChatActionResponse {
            session_id,
            jid,
            action: "set_ephemeral".to_string(),
            status: format!("ephemeral_{}s", duration),
        }))
    }

    async fn set_group_announcement(
        &self,
        request: Request<SetGroupAnnouncementRequest>,
    ) -> Result<Response<ChatActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.jid;

        let _ = session_repo::get_one(&session_id)
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

        let group_jid = parse_group_jid(&jid)?;
        client
            .groups()
            .set_announce(&group_jid, req.announcement_only)
            .await
            .map_err(map_wa_error)?;

        Ok(Response::new(ChatActionResponse {
            session_id,
            jid,
            action: "set_announcement".to_string(),
            status: if req.announcement_only {
                "announcement_enabled".to_string()
            } else {
                "announcement_disabled".to_string()
            },
        }))
    }

    async fn set_group_lock(
        &self,
        request: Request<SetGroupLockRequest>,
    ) -> Result<Response<ChatActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.jid;

        let _ = session_repo::get_one(&session_id)
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

        let group_jid = parse_group_jid(&jid)?;
        client
            .groups()
            .set_locked(&group_jid, req.locked)
            .await
            .map_err(map_wa_error)?;

        Ok(Response::new(ChatActionResponse {
            session_id,
            jid,
            action: "set_lock".to_string(),
            status: if req.locked { "locked" } else { "unlocked" }.to_string(),
        }))
    }

    async fn get_invite_link(
        &self,
        request: Request<GetInviteLinkRequest>,
    ) -> Result<Response<GetInviteLinkResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.jid;

        let _ = session_repo::get_one(&session_id)
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

        let group_jid = parse_group_jid(&jid)?;
        let code = client
            .groups()
            .get_invite_link(&group_jid, req.reset)
            .await
            .map_err(map_wa_error)?;

        let invite_link = format!("https://chat.whatsapp.com/{}", code);

        Ok(Response::new(GetInviteLinkResponse {
            session_id,
            group_jid: jid,
            invite_code: code,
            invite_link,
        }))
    }

    async fn leave_group(
        &self,
        request: Request<LeaveGroupRequest>,
    ) -> Result<Response<ChatActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.jid;

        let _ = session_repo::get_one(&session_id)
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

        let group_jid = parse_group_jid(&jid)?;
        client
            .groups()
            .leave(&group_jid)
            .await
            .map_err(map_wa_error)?;

        Ok(Response::new(ChatActionResponse {
            session_id,
            jid,
            action: "leave".to_string(),
            status: "left".to_string(),
        }))
    }
}
