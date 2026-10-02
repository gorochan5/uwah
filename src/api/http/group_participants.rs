use crate::api::http::{
    chats::{map_wa_error, parse_group_jid, parse_target_jid},
    error::{AppError, AppResult, ErrorResponse},
    state::AppState,
};
use crate::models::group_participants::GroupParticipant;
use crate::repository::group_participants as repo;
use crate::repository::sessions as session_repo;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;
use whatsapp_rust::Server;

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddParticipantsRequest {
    /// List of participant phone numbers or JIDs to add
    #[schema(example = json!(["5511999999999@s.whatsapp.net"]))]
    pub participants: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ModifyParticipantRolesRequest {
    /// List of participant phone numbers or JIDs to promote or demote
    #[schema(example = json!(["5511999999999@s.whatsapp.net"]))]
    pub participants: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ParticipantActionResult {
    pub jid: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ParticipantActionResponse {
    pub results: Vec<ParticipantActionResult>,
}

#[utoipa::path(
    get,
    path = "/sessions/{id}/chats/{jid}/participants",
    tag = "Groups",
    summary = "List group participants",
    description = "Retrieves all participants and their admin status for a group chat.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Group WhatsApp JID (e.g. 123456@g.us)")
    ),
    responses(
        (status = 200, description = "List of group participants", body = Vec<GroupParticipant>),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn list_participants(
    State(state): State<Arc<AppState>>,
    Path((session_id, group_jid)): Path<(String, String)>,
) -> AppResult<Json<Vec<GroupParticipant>>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let mut participants = repo::get_by_group(&session_id, &group_jid).await?;

    if participants.is_empty()
        && let Some(client) = state.sessions.get_client(&session_id).await
        && let Ok(parsed_group_jid) = parse_group_jid(&group_jid)
        && let Ok(metadata) = client.groups().get_metadata(&parsed_group_jid).await
    {
        let now = chrono::Utc::now();
        let mut fresh_list = Vec::new();
        for p in metadata.participants {
            let role = if p.is_super_admin() {
                "superadmin"
            } else if p.is_admin() {
                "admin"
            } else {
                "member"
            };
            let gp = GroupParticipant {
                session_id: session_id.clone(),
                group_jid: group_jid.clone(),
                user_jid: p.jid.to_string(),
                role: role.to_string(),
                joined_at: now,
            };
            let _ = repo::upsert(&gp).await;
            fresh_list.push(gp);
        }
        if !fresh_list.is_empty() {
            participants = fresh_list;
        }
    }

    Ok(Json(participants))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/participants",
    tag = "Groups",
    summary = "Add participants to group",
    description = "Adds new participant members to an existing WhatsApp group.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Group WhatsApp JID")
    ),
    request_body = AddParticipantsRequest,
    responses(
        (status = 200, description = "Participants added result", body = ParticipantActionResponse),
        (status = 400, description = "Invalid request or not a group", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden (bot not admin or participant privacy)", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn add_participants(
    State(state): State<Arc<AppState>>,
    Path((session_id, group_jid)): Path<(String, String)>,
    Json(body): Json<AddParticipantsRequest>,
) -> AppResult<Json<ParticipantActionResponse>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    if body.participants.is_empty() {
        return Err(AppError::BadRequest(
            "Participants list cannot be empty".to_string(),
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

    let parsed_group_jid = parse_group_jid(&group_jid)?;

    let mut user_jids = Vec::new();
    for p_str in &body.participants {
        let u_jid = parse_target_jid(p_str)?;
        if u_jid.server == Server::Group {
            return Err(AppError::BadRequest(format!(
                "Cannot add group '{}' as participant",
                p_str
            )));
        }
        user_jids.push(u_jid);
    }

    let change_results = client
        .groups()
        .add_participants(&parsed_group_jid, &user_jids)
        .await
        .map_err(map_wa_error)?;

    let mut results = Vec::new();
    let now = chrono::Utc::now();

    for item in change_results {
        let u_jid_str = item.jid.to_string();
        let status_code = item.status.as_deref().unwrap_or("200");
        let is_success = status_code == "200";

        if is_success {
            let gp = GroupParticipant {
                session_id: session_id.clone(),
                group_jid: group_jid.clone(),
                user_jid: u_jid_str.clone(),
                role: "member".to_string(),
                joined_at: now,
            };
            let _ = repo::upsert(&gp).await;
        }

        results.push(ParticipantActionResult {
            jid: u_jid_str,
            status: status_code.to_string(),
            error: if is_success {
                None
            } else {
                Some(format!("WhatsApp error status: {}", status_code))
            },
        });
    }

    Ok(Json(ParticipantActionResponse { results }))
}

#[utoipa::path(
    delete,
    path = "/sessions/{id}/chats/{jid}/participants/{user_jid}",
    tag = "Groups",
    summary = "Remove group participant",
    description = "Removes a participant member from a WhatsApp group on the network and locally.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Group WhatsApp JID"),
        ("user_jid" = String, Path, description = "Participant WhatsApp JID")
    ),
    responses(
        (status = 204, description = "Participant removed successfully"),
        (status = 400, description = "Invalid request or not a group", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden (bot not admin)", body = ErrorResponse),
        (status = 404, description = "Participant or session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn remove_participant(
    State(state): State<Arc<AppState>>,
    Path((session_id, group_jid, user_jid)): Path<(String, String, String)>,
) -> AppResult<StatusCode> {
    let _ = session_repo::get_one(&session_id)
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

    let parsed_group_jid = parse_group_jid(&group_jid)?;
    let parsed_user_jid = parse_target_jid(&user_jid)?;

    client
        .groups()
        .remove_participants(&parsed_group_jid, &[parsed_user_jid])
        .await
        .map_err(map_wa_error)?;

    let _ = repo::delete(&session_id, &group_jid, &user_jid).await;

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/participants/promote",
    tag = "Groups",
    summary = "Promote participants to admin",
    description = "Promotes group participant members to group administrators on the WhatsApp network.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Group WhatsApp JID")
    ),
    request_body = ModifyParticipantRolesRequest,
    responses(
        (status = 200, description = "Participants promoted result", body = ParticipantActionResponse),
        (status = 400, description = "Invalid request or not a group", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden (bot not admin)", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn promote_participants(
    State(state): State<Arc<AppState>>,
    Path((session_id, group_jid)): Path<(String, String)>,
    Json(body): Json<ModifyParticipantRolesRequest>,
) -> AppResult<Json<ParticipantActionResponse>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    if body.participants.is_empty() {
        return Err(AppError::BadRequest(
            "Participants list cannot be empty".to_string(),
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

    let parsed_group_jid = parse_group_jid(&group_jid)?;

    let mut user_jids = Vec::new();
    for p_str in &body.participants {
        let u_jid = parse_target_jid(p_str)?;
        user_jids.push(u_jid);
    }

    let change_results = client
        .groups()
        .promote_participants(&parsed_group_jid, &user_jids)
        .await
        .map_err(map_wa_error)?;

    let mut results = Vec::new();
    for item in change_results {
        let u_jid_str = item.jid.to_string();
        let status_code = item.status.as_deref().unwrap_or("200");
        let is_success = status_code == "200";

        if is_success {
            let _ = repo::update_role(&session_id, &group_jid, &u_jid_str, "admin").await;
        }

        results.push(ParticipantActionResult {
            jid: u_jid_str,
            status: status_code.to_string(),
            error: if is_success {
                None
            } else {
                Some(format!("WhatsApp error status: {}", status_code))
            },
        });
    }

    Ok(Json(ParticipantActionResponse { results }))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/chats/{jid}/participants/demote",
    tag = "Groups",
    summary = "Demote participants to member",
    description = "Demotes group administrators to regular members on the WhatsApp network.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Group WhatsApp JID")
    ),
    request_body = ModifyParticipantRolesRequest,
    responses(
        (status = 200, description = "Participants demoted result", body = ParticipantActionResponse),
        (status = 400, description = "Invalid request or not a group", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden (bot not admin)", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn demote_participants(
    State(state): State<Arc<AppState>>,
    Path((session_id, group_jid)): Path<(String, String)>,
    Json(body): Json<ModifyParticipantRolesRequest>,
) -> AppResult<Json<ParticipantActionResponse>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    if body.participants.is_empty() {
        return Err(AppError::BadRequest(
            "Participants list cannot be empty".to_string(),
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

    let parsed_group_jid = parse_group_jid(&group_jid)?;

    let mut user_jids = Vec::new();
    for p_str in &body.participants {
        let u_jid = parse_target_jid(p_str)?;
        user_jids.push(u_jid);
    }

    let change_results = client
        .groups()
        .demote_participants(&parsed_group_jid, &user_jids)
        .await
        .map_err(map_wa_error)?;

    let mut results = Vec::new();
    for item in change_results {
        let u_jid_str = item.jid.to_string();
        let status_code = item.status.as_deref().unwrap_or("200");
        let is_success = status_code == "200";

        if is_success {
            let _ = repo::update_role(&session_id, &group_jid, &u_jid_str, "member").await;
        }

        results.push(ParticipantActionResult {
            jid: u_jid_str,
            status: status_code.to_string(),
            error: if is_success {
                None
            } else {
                Some(format!("WhatsApp error status: {}", status_code))
            },
        });
    }

    Ok(Json(ParticipantActionResponse { results }))
}
