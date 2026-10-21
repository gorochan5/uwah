use crate::api::http::{
    error::{AppError, AppResult, ErrorResponse},
    state::AppState,
};
use crate::models::chats::Chat;
use crate::models::group_participants::GroupParticipant;
use crate::repository::chats as repo;
use crate::repository::sessions as session_repo;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};
use whatsapp_rust::{GroupCreateOptions, GroupDescription, GroupParticipantOptions, GroupSubject, Jid, PreviousDescription, Server};

pub(crate) fn parse_target_jid(jid: &str) -> Result<Jid, AppError> {
    let normalized = if !jid.contains('@') {
        format!("{}@s.whatsapp.net", jid)
    } else {
        jid.to_string()
    };
    Jid::from_str(&normalized).map_err(|e| AppError::BadRequest(format!("Invalid WhatsApp JID '{}': {}", normalized, e)))
}

pub(crate) fn parse_group_jid(jid: &str) -> Result<Jid, AppError> {
    let target = parse_target_jid(jid)?;
    if target.server != Server::Group {
        return Err(AppError::BadRequest(format!(
            "JID '{}' is not a valid WhatsApp group JID (must end with @g.us)",
            jid
        )));
    }
    Ok(target)
}

pub(crate) fn map_wa_error<E: std::fmt::Display>(e: E) -> AppError {
    let msg = e.to_string();
    let lower = msg.to_lowercase();
    if lower.contains("403") || lower.contains("not-authorized") || lower.contains("forbidden") || lower.contains("admin") {
        AppError::Forbidden(msg)
    } else if lower.contains("409") || lower.contains("conflict") {
        AppError::Conflict(msg)
    } else if lower.contains("400") || lower.contains("invalid") || lower.contains("bad request") {
        AppError::BadRequest(msg)
    } else if lower.contains("404") || lower.contains("not found") {
        AppError::NotFound(msg)
    } else {
        AppError::Internal(anyhow::anyhow!(msg))
    }
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListChatsQuery {
    #[param(example = 50)]
    pub limit: Option<i64>,
    #[param(example = 0)]
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MarkChatAsReadRequest {
    /// Mark as read (true) or unread (false). Defaults to true.
    pub read: Option<bool>,
    /// Optional specific message IDs to acknowledge via read receipt
    pub message_ids: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct MuteChatRequest {
    /// Whether to mute (true) or unmute (false)
    pub muted: bool,
    /// Optional mute duration in seconds. If omitted or null when muted=true, muted indefinitely.
    pub duration_seconds: Option<i64>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PinChatRequest {
    /// Whether to pin (true) or unpin (false) the chat
    pub pinned: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ArchiveChatRequest {
    /// Whether to archive (true) or unarchive (false) the chat
    pub archived: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChatActionResponse {
    pub jid: String,
    pub status: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateGroupRequest {
    /// Group title / subject (1..=100 characters)
    #[schema(example = "Dev Team Ops")]
    pub subject: String,
    /// List of participant phone numbers or JIDs
    #[schema(example = json!(["5511999999999@s.whatsapp.net"]))]
    pub participants: Vec<String>,
    /// Optional group description text
    #[schema(example = "Operations and alerts channel")]
    pub description: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GroupParticipantItem {
    pub jid: String,
    pub role: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CreateGroupResponse {
    pub group_jid: String,
    pub subject: String,
    pub description: Option<String>,
    pub participants: Vec<GroupParticipantItem>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetSubjectRequest {
    /// New group subject/name (1..=100 characters)
    #[schema(example = "New Project Title")]
    pub subject: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetDescriptionRequest {
    /// New group description. Passing null or empty string clears description.
    #[schema(example = "Project description and repository links")]
    pub description: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct GroupInviteQuery {
    /// Whether to revoke previous invite link and generate a fresh one
    pub reset: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GroupInviteLinkResponse {
    pub invite_url: String,
    pub code: String,
}

#[utoipa::path(
    get,
    path = "/sessions/{id}/chats",
    tag = "Chats",
    summary = "List chats",
    description = "Retrieves conversation threads for the specified session with optional pagination. When limit is omitted, all chats are returned.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ListChatsQuery
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
    Query(query): Query<ListChatsQuery>,
) -> AppResult<Json<Vec<Chat>>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let limit = query.limit.map(|l| l.clamp(1, 100));
    let offset = query.offset.map(|o| o.max(0));
    let chats = repo::get_by_session_paginated(&session_id, limit, offset).await?;
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
pub async fn get_chat(State(_state): State<Arc<AppState>>, Path((session_id, jid)): Path<(String, String)>) -> AppResult<Json<Chat>> {
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
pub async fn delete_chat(State(_state): State<Arc<AppState>>, Path((session_id, jid)): Path<(String, String)>) -> AppResult<StatusCode> {
    let _ = repo::get_one(&jid, &session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Chat '{}' not found", jid)))?;

    repo::delete(&jid, &session_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/read",
    tag = "Chats",
    summary = "Mark chat as read/unread",
    description = "Synchronizes chat read status across WhatsApp multi-device app state and resets local unread counter.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "WhatsApp JID")
    ),
    request_body = MarkChatAsReadRequest,
    responses(
        (status = 200, description = "Chat read status updated", body = ChatActionResponse),
        (status = 400, description = "Invalid JID or inactive session", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn mark_as_read(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Json(body): Json<MarkChatAsReadRequest>,
) -> AppResult<Json<ChatActionResponse>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let client = state.sessions.get_client(&session_id).await.ok_or_else(|| {
        AppError::BadRequest(format!(
            "Session '{}' is not connected or active in memory. Scan QR first.",
            session_id
        ))
    })?;

    let target_jid = parse_target_jid(&jid)?;
    let is_read = body.read.unwrap_or(true);

    if is_read {
        repo::mark_as_read(&session_id, &jid).await?;
    }

    if is_read && let Some(msg_ids) = &body.message_ids {
        let client_clone = client.clone();
        let target_jid_clone = target_jid.clone();
        let id_refs: Vec<String> = msg_ids.clone();
        tokio::spawn(async move {
            let refs: Vec<&str> = id_refs.iter().map(String::as_str).collect();
            let _ = client_clone.mark_as_read(&target_jid_clone, None, &refs).await;
        });
    }

    Ok(Json(ChatActionResponse {
        jid,
        status: if is_read { "read".to_string() } else { "unread".to_string() },
    }))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/mute",
    tag = "Chats",
    summary = "Mute or unmute chat",
    description = "Mutes or unmutes a chat conversation on WhatsApp network and updates local store.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "WhatsApp JID")
    ),
    request_body = MuteChatRequest,
    responses(
        (status = 200, description = "Chat mute status updated", body = ChatActionResponse),
        (status = 400, description = "Invalid request or inactive session", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn mute_chat(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Json(body): Json<MuteChatRequest>,
) -> AppResult<Json<ChatActionResponse>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let client = state.sessions.get_client(&session_id).await.ok_or_else(|| {
        AppError::BadRequest(format!(
            "Session '{}' is not connected or active in memory. Scan QR first.",
            session_id
        ))
    })?;

    let target_jid = parse_target_jid(&jid)?;
    if body.muted {
        match body.duration_seconds {
            Some(secs) if secs > 0 => {
                let end_ms = chrono::Utc::now().timestamp_millis() + (secs * 1000);
                client
                    .chat_actions()
                    .mute_chat_until(&target_jid, end_ms)
                    .await
                    .map_err(map_wa_error)?;
                repo::update_mute(&session_id, &jid, Some(end_ms / 1000)).await?;
            }
            _ => {
                client.chat_actions().mute_chat(&target_jid).await.map_err(map_wa_error)?;
                repo::update_mute(&session_id, &jid, Some(-1)).await?;
            }
        }
    } else {
        client.chat_actions().unmute_chat(&target_jid).await.map_err(map_wa_error)?;
        repo::update_mute(&session_id, &jid, None).await?;
    }

    Ok(Json(ChatActionResponse {
        jid,
        status: if body.muted { "muted".to_string() } else { "unmuted".to_string() },
    }))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/pin",
    tag = "Chats",
    summary = "Pin or unpin chat",
    description = "Pins or unpins a chat conversation on WhatsApp network and updates local store.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "WhatsApp JID")
    ),
    request_body = PinChatRequest,
    responses(
        (status = 200, description = "Chat pin status updated", body = ChatActionResponse),
        (status = 400, description = "Invalid request or inactive session", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn pin_chat(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Json(body): Json<PinChatRequest>,
) -> AppResult<Json<ChatActionResponse>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let client = state.sessions.get_client(&session_id).await.ok_or_else(|| {
        AppError::BadRequest(format!(
            "Session '{}' is not connected or active in memory. Scan QR first.",
            session_id
        ))
    })?;

    let target_jid = parse_target_jid(&jid)?;
    if body.pinned {
        client.chat_actions().pin_chat(&target_jid).await.map_err(map_wa_error)?;
    } else {
        client.chat_actions().unpin_chat(&target_jid).await.map_err(map_wa_error)?;
    }

    repo::update_pinned(&session_id, &jid, body.pinned).await?;

    Ok(Json(ChatActionResponse {
        jid,
        status: if body.pinned {
            "pinned".to_string()
        } else {
            "unpinned".to_string()
        },
    }))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/archive",
    tag = "Chats",
    summary = "Archive or unarchive chat",
    description = "Archives or unarchives a chat conversation on WhatsApp network and updates local store.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "WhatsApp JID")
    ),
    request_body = ArchiveChatRequest,
    responses(
        (status = 200, description = "Chat archive status updated", body = ChatActionResponse),
        (status = 400, description = "Invalid request or inactive session", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn archive_chat(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Json(body): Json<ArchiveChatRequest>,
) -> AppResult<Json<ChatActionResponse>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let client = state.sessions.get_client(&session_id).await.ok_or_else(|| {
        AppError::BadRequest(format!(
            "Session '{}' is not connected or active in memory. Scan QR first.",
            session_id
        ))
    })?;

    let target_jid = parse_target_jid(&jid)?;
    if body.archived {
        client.chat_actions().archive_chat(&target_jid, None).await.map_err(map_wa_error)?;
    } else {
        client
            .chat_actions()
            .unarchive_chat(&target_jid, None)
            .await
            .map_err(map_wa_error)?;
    }

    repo::update_archived(&session_id, &jid, body.archived).await?;

    Ok(Json(ChatActionResponse {
        jid,
        status: if body.archived {
            "archived".to_string()
        } else {
            "unarchived".to_string()
        },
    }))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/groups",
    tag = "Groups",
    summary = "Create WhatsApp group",
    description = "Creates a new WhatsApp group on the network and persists it to local storage.",
    params(
        ("id" = String, Path, description = "Session identifier")
    ),
    request_body = CreateGroupRequest,
    responses(
        (status = 201, description = "Group created successfully", body = CreateGroupResponse),
        (status = 400, description = "Invalid request parameters", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden by WhatsApp policy", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn create_group(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<CreateGroupRequest>,
) -> AppResult<(StatusCode, Json<CreateGroupResponse>)> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    if body.subject.trim().is_empty() {
        return Err(AppError::BadRequest("Group subject cannot be empty or whitespace only".to_string()));
    }

    if body.subject.chars().count() > 100 {
        return Err(AppError::BadRequest(
            "Group subject exceeds maximum length of 100 characters".to_string(),
        ));
    }

    if body.participants.is_empty() {
        return Err(AppError::BadRequest("Group must have at least one initial participant".to_string()));
    }

    let client = state.sessions.get_client(&session_id).await.ok_or_else(|| {
        AppError::BadRequest(format!(
            "Session '{}' is not connected or active in memory. Scan QR first.",
            session_id
        ))
    })?;

    let _subject_validated = GroupSubject::new(&body.subject).map_err(|e| AppError::BadRequest(format!("Invalid group subject: {}", e)))?;

    let description = match body.description.as_deref() {
        Some(desc) if !desc.trim().is_empty() => {
            Some(GroupDescription::new(desc).map_err(|e| AppError::BadRequest(format!("Invalid group description: {}", e)))?)
        }
        _ => None,
    };

    let mut participant_opts = Vec::new();
    for p_str in &body.participants {
        let p_jid = parse_target_jid(p_str)?;
        if p_jid.server == Server::Group {
            return Err(AppError::BadRequest(format!(
                "Participant '{}' is a group, not an individual user",
                p_str
            )));
        }
        participant_opts.push(GroupParticipantOptions::new(p_jid));
    }

    let builder = GroupCreateOptions::builder()
        .subject(body.subject.clone())
        .participants(participant_opts)
        .maybe_description(description);

    let create_result = client.groups().create_group(builder.build()).await.map_err(map_wa_error)?;

    let meta = create_result.metadata;
    let group_jid_str = meta.id.to_string();

    repo::ensure_exists(&group_jid_str, &session_id).await?;
    let creator_str = meta.creator.as_ref().map(ToString::to_string);
    let creation_time = meta.creation_time.map(|t| t as i64);

    let _ = repo::update_subject(
        &session_id,
        &group_jid_str,
        Some(&meta.subject),
        creator_str.as_deref(),
        creation_time,
    )
    .await;

    let mut participant_items = Vec::new();
    let now = chrono::Utc::now();

    for p in &meta.participants {
        let p_jid_str = p.jid.to_string();
        let role = if p.is_super_admin() {
            "superadmin"
        } else if p.is_admin() {
            "admin"
        } else {
            "member"
        };

        let gp = GroupParticipant {
            session_id: session_id.clone(),
            group_jid: group_jid_str.clone(),
            user_jid: p_jid_str.clone(),
            role: role.to_string(),
            joined_at: now,
        };

        let _ = crate::repository::group_participants::upsert(&gp).await;

        participant_items.push(GroupParticipantItem {
            jid: p_jid_str,
            role: role.to_string(),
        });
    }

    Ok((
        StatusCode::CREATED,
        Json(CreateGroupResponse {
            group_jid: group_jid_str,
            subject: meta.subject,
            description: body.description,
            participants: participant_items,
        }),
    ))
}

#[utoipa::path(
    patch,
    path = "/sessions/{id}/chats/{jid}/subject",
    tag = "Groups",
    summary = "Set group subject",
    description = "Updates the subject/title of a WhatsApp group.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Group WhatsApp JID")
    ),
    request_body = SetSubjectRequest,
    responses(
        (status = 200, description = "Subject updated", body = ChatActionResponse),
        (status = 400, description = "Invalid subject or not a group", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden (admin permissions required)", body = ErrorResponse),
        (status = 404, description = "Session or chat not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn set_subject(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Json(body): Json<SetSubjectRequest>,
) -> AppResult<Json<ChatActionResponse>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    if body.subject.trim().is_empty() {
        return Err(AppError::BadRequest("Group subject cannot be empty".to_string()));
    }

    let client = state.sessions.get_client(&session_id).await.ok_or_else(|| {
        AppError::BadRequest(format!(
            "Session '{}' is not connected or active in memory. Scan QR first.",
            session_id
        ))
    })?;

    let group_jid = parse_group_jid(&jid)?;
    let subject = GroupSubject::new(&body.subject).map_err(|e| AppError::BadRequest(format!("Invalid group subject: {}", e)))?;

    client.groups().set_subject(&group_jid, subject).await.map_err(map_wa_error)?;

    let _ = repo::update_subject(&session_id, &jid, Some(&body.subject), None, None).await;

    Ok(Json(ChatActionResponse {
        jid,
        status: "ok".to_string(),
    }))
}

#[utoipa::path(
    patch,
    path = "/sessions/{id}/chats/{jid}/description",
    tag = "Groups",
    summary = "Set group description",
    description = "Updates or clears the description of a WhatsApp group.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Group WhatsApp JID")
    ),
    request_body = SetDescriptionRequest,
    responses(
        (status = 200, description = "Description updated", body = ChatActionResponse),
        (status = 400, description = "Invalid request or not a group", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden (admin permissions required)", body = ErrorResponse),
        (status = 404, description = "Session or chat not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn set_description(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Json(body): Json<SetDescriptionRequest>,
) -> AppResult<Json<ChatActionResponse>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let client = state.sessions.get_client(&session_id).await.ok_or_else(|| {
        AppError::BadRequest(format!(
            "Session '{}' is not connected or active in memory. Scan QR first.",
            session_id
        ))
    })?;

    let group_jid = parse_group_jid(&jid)?;

    let desc_opt = match body.description.as_deref() {
        Some(d) if !d.trim().is_empty() => {
            Some(GroupDescription::new(d).map_err(|e| AppError::BadRequest(format!("Invalid description: {}", e)))?)
        }
        _ => None,
    };

    client
        .groups()
        .set_description(&group_jid, desc_opt, PreviousDescription::Resolve)
        .await
        .map_err(map_wa_error)?;

    Ok(Json(ChatActionResponse {
        jid,
        status: "ok".to_string(),
    }))
}

#[utoipa::path(
    get,
    path = "/sessions/{id}/chats/{jid}/invite-code",
    tag = "Groups",
    summary = "Get or revoke group invite link",
    description = "Retrieves current group invite link or revokes it to generate a new invite code.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Group WhatsApp JID"),
        GroupInviteQuery
    ),
    responses(
        (status = 200, description = "Group invite link", body = GroupInviteLinkResponse),
        (status = 400, description = "Not a group or inactive session", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden (admin permissions required)", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn get_invite_link(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Query(query): Query<GroupInviteQuery>,
) -> AppResult<Json<GroupInviteLinkResponse>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let client = state.sessions.get_client(&session_id).await.ok_or_else(|| {
        AppError::BadRequest(format!(
            "Session '{}' is not connected or active in memory. Scan QR first.",
            session_id
        ))
    })?;

    let group_jid = parse_group_jid(&jid)?;
    let reset = query.reset.unwrap_or(false);

    let code = client.groups().get_invite_link(&group_jid, reset).await.map_err(map_wa_error)?;

    let invite_url = format!("https://chat.whatsapp.com/{}", code);

    Ok(Json(GroupInviteLinkResponse { invite_url, code }))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/leave",
    tag = "Groups",
    summary = "Leave WhatsApp group",
    description = "Causes the bot to leave the specified WhatsApp group.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Group WhatsApp JID")
    ),
    responses(
        (status = 204, description = "Left group successfully"),
        (status = 400, description = "Not a group or inactive session", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn leave_group(State(state): State<Arc<AppState>>, Path((session_id, jid)): Path<(String, String)>) -> AppResult<StatusCode> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let client = state.sessions.get_client(&session_id).await.ok_or_else(|| {
        AppError::BadRequest(format!(
            "Session '{}' is not connected or active in memory. Scan QR first.",
            session_id
        ))
    })?;

    let group_jid = parse_group_jid(&jid)?;

    client.groups().leave(&group_jid).await.map_err(map_wa_error)?;

    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct JoinGroupRequest {
    /// Group invite code or full invite URL (e.g. "AbCdEf12345" or "https://chat.whatsapp.com/AbCdEf12345")
    #[schema(example = "AbCdEf12345")]
    pub invite_code: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct JoinGroupResponse {
    pub group_jid: String,
    pub status: String,
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/groups/join",
    tag = "Groups",
    summary = "Join group via invite link",
    description = "Accepts a WhatsApp group invitation link or code and joins the bot to the group.",
    params(
        ("id" = String, Path, description = "Session identifier")
    ),
    request_body = JoinGroupRequest,
    responses(
        (status = 200, description = "Joined group successfully", body = JoinGroupResponse),
        (status = 400, description = "Invalid invite code or inactive session", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn join_group(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<JoinGroupRequest>,
) -> AppResult<Json<JoinGroupResponse>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let client = state.sessions.get_client(&session_id).await.ok_or_else(|| {
        AppError::BadRequest(format!(
            "Session '{}' is not connected or active in memory. Scan QR first.",
            session_id
        ))
    })?;

    let raw_code = body.invite_code.trim();
    let code = if let Some(stripped) = raw_code.strip_prefix("https://chat.whatsapp.com/") {
        stripped.trim()
    } else if let Some(stripped) = raw_code.strip_prefix("chat.whatsapp.com/") {
        stripped.trim()
    } else {
        raw_code
    };

    if code.is_empty() {
        return Err(AppError::BadRequest("Invite code cannot be empty".to_string()));
    }

    let join_res = client.groups().join_with_invite_code(code).await.map_err(map_wa_error)?;

    let group_jid_str = join_res.group_jid().to_string();
    let status_str = match join_res {
        whatsapp_rust::JoinGroupResult::Joined(_) => "joined",
        whatsapp_rust::JoinGroupResult::PendingApproval(_) => "pending_approval",
    };

    let _ = crate::repository::chats::ensure_exists(&group_jid_str, &session_id).await;

    Ok(Json(JoinGroupResponse {
        group_jid: group_jid_str,
        status: status_str.to_string(),
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetAnnouncementRequest {
    /// True if only administrators can send messages; false if all participants can send messages
    pub announcement: bool,
}

#[utoipa::path(
    put,
    path = "/sessions/{id}/chats/{jid}/settings/announcement",
    tag = "Groups",
    summary = "Set group announcement mode",
    description = "Configures whether only admins or all members can send messages to the group.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Group WhatsApp JID")
    ),
    request_body = SetAnnouncementRequest,
    responses(
        (status = 200, description = "Announcement setting updated", body = ChatActionResponse),
        (status = 400, description = "Not a group or inactive session", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden (admin permissions required)", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn set_group_announcement(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Json(body): Json<SetAnnouncementRequest>,
) -> AppResult<Json<ChatActionResponse>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let client = state.sessions.get_client(&session_id).await.ok_or_else(|| {
        AppError::BadRequest(format!(
            "Session '{}' is not connected or active in memory. Scan QR first.",
            session_id
        ))
    })?;

    let group_jid = parse_group_jid(&jid)?;

    client
        .groups()
        .set_announce(&group_jid, body.announcement)
        .await
        .map_err(map_wa_error)?;

    Ok(Json(ChatActionResponse {
        jid,
        status: if body.announcement {
            "announcement_enabled".to_string()
        } else {
            "announcement_disabled".to_string()
        },
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetLockRequest {
    /// True if only administrators can modify group subject, description and icon; false for all members
    pub locked: bool,
}

#[utoipa::path(
    put,
    path = "/sessions/{id}/chats/{jid}/settings/lock",
    tag = "Groups",
    summary = "Set group lock / restrict mode",
    description = "Configures whether only admins or all members can modify group subject, description, and avatar.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Group WhatsApp JID")
    ),
    request_body = SetLockRequest,
    responses(
        (status = 200, description = "Lock setting updated", body = ChatActionResponse),
        (status = 400, description = "Not a group or inactive session", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden (admin permissions required)", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn set_group_lock(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Json(body): Json<SetLockRequest>,
) -> AppResult<Json<ChatActionResponse>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let client = state.sessions.get_client(&session_id).await.ok_or_else(|| {
        AppError::BadRequest(format!(
            "Session '{}' is not connected or active in memory. Scan QR first.",
            session_id
        ))
    })?;

    let group_jid = parse_group_jid(&jid)?;

    client.groups().set_locked(&group_jid, body.locked).await.map_err(map_wa_error)?;

    Ok(Json(ChatActionResponse {
        jid,
        status: if body.locked {
            "locked".to_string()
        } else {
            "unlocked".to_string()
        },
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetEphemeralRequest {
    /// Disappearing message timer in seconds (0 = off, 86400 = 24h, 604800 = 7d, 7776000 = 90d)
    #[schema(example = 604800)]
    pub ephemeral_duration_seconds: u32,
}

#[utoipa::path(
    put,
    path = "/sessions/{id}/chats/{jid}/settings/ephemeral",
    tag = "Chats",
    summary = "Set disappearing messages timer",
    description = "Configures or disables ephemeral disappearing messages timer for the chat.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "WhatsApp chat or group JID")
    ),
    request_body = SetEphemeralRequest,
    responses(
        (status = 200, description = "Ephemeral timer updated", body = ChatActionResponse),
        (status = 400, description = "Invalid duration or inactive session", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn set_ephemeral_timer(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Json(body): Json<SetEphemeralRequest>,
) -> AppResult<Json<ChatActionResponse>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let client = state.sessions.get_client(&session_id).await.ok_or_else(|| {
        AppError::BadRequest(format!(
            "Session '{}' is not connected or active in memory. Scan QR first.",
            session_id
        ))
    })?;

    let target_jid = parse_target_jid(&jid)?;

    client
        .groups()
        .set_ephemeral(&target_jid, body.ephemeral_duration_seconds)
        .await
        .map_err(map_wa_error)?;

    Ok(Json(ChatActionResponse {
        jid,
        status: format!("ephemeral_{}s", body.ephemeral_duration_seconds),
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetChatStateRequest {
    /// Chat ephemeral state: "composing" (typing), "recording" (audio), or "paused" (idle)
    #[schema(example = "composing")]
    pub state: String,
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/presence",
    tag = "Chats",
    summary = "Set chat typing indicator",
    description = "Dispatches ephemeral typing or recording indicator to a chat conversation.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "WhatsApp conversation JID")
    ),
    request_body = SetChatStateRequest,
    responses(
        (status = 200, description = "Chat state transmitted", body = ChatActionResponse),
        (status = 400, description = "Invalid state or inactive session", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn set_chat_state(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Json(body): Json<SetChatStateRequest>,
) -> AppResult<Json<ChatActionResponse>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let client = state.sessions.get_client(&session_id).await.ok_or_else(|| {
        AppError::BadRequest(format!(
            "Session '{}' is not connected or active in memory. Scan QR first.",
            session_id
        ))
    })?;

    let target_jid = parse_target_jid(&jid)?;

    let chat_state = match body.state.trim().to_lowercase().as_str() {
        "composing" | "typing" => whatsapp_rust::features::ChatStateType::Composing,
        "recording" => whatsapp_rust::features::ChatStateType::Recording,
        "paused" | "idle" => whatsapp_rust::features::ChatStateType::Paused,
        other => {
            return Err(AppError::BadRequest(format!(
                "Invalid chat state '{}'. Supported values: 'composing', 'recording', 'paused'",
                other
            )));
        }
    };

    client.chatstate().send(&target_jid, chat_state).await.map_err(map_wa_error)?;

    Ok(Json(ChatActionResponse {
        jid,
        status: body.state.to_lowercase(),
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SetGlobalPresenceRequest {
    /// True for online/available; false for offline/unavailable
    pub available: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GlobalPresenceResponse {
    pub session_id: String,
    pub available: bool,
    pub status: String,
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/presence",
    tag = "Chats",
    summary = "Set global presence status",
    description = "Updates the global online/available or offline/unavailable presence for the session.",
    params(
        ("id" = String, Path, description = "Session identifier")
    ),
    request_body = SetGlobalPresenceRequest,
    responses(
        (status = 200, description = "Presence updated", body = GlobalPresenceResponse),
        (status = 400, description = "Pushname empty or inactive session", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn set_global_presence(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(body): Json<SetGlobalPresenceRequest>,
) -> AppResult<Json<GlobalPresenceResponse>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let client = state.sessions.get_client(&session_id).await.ok_or_else(|| {
        AppError::BadRequest(format!(
            "Session '{}' is not connected or active in memory. Scan QR first.",
            session_id
        ))
    })?;

    let status = if body.available {
        whatsapp_rust::features::PresenceStatus::Available
    } else {
        whatsapp_rust::features::PresenceStatus::Unavailable
    };

    client.presence().set(status).await.map_err(|err| match err {
        whatsapp_rust::features::PresenceError::PushNameEmpty => {
            AppError::BadRequest("Cannot set presence: session push_name is empty or session is not authenticated yet.".to_string())
        }
        other => map_wa_error(other),
    })?;

    Ok(Json(GlobalPresenceResponse {
        session_id,
        available: body.available,
        status: if body.available {
            "available".to_string()
        } else {
            "unavailable".to_string()
        },
    }))
}
