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
use crate::models::group_participants::GroupParticipant;
use crate::repository::group_participants as repo;
use crate::repository::sessions as session_repo;

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
    State(_state): State<Arc<AppState>>,
    Path((session_id, group_jid)): Path<(String, String)>,
) -> AppResult<Json<Vec<GroupParticipant>>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let participants = repo::get_by_group(&session_id, &group_jid).await?;
    Ok(Json(participants))
}

#[utoipa::path(
    delete,
    path = "/sessions/{id}/chats/{jid}/participants/{user_jid}",
    tag = "Groups",
    summary = "Remove group participant",
    description = "Removes a participant from local group records.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Group WhatsApp JID"),
        ("user_jid" = String, Path, description = "Participant WhatsApp JID")
    ),
    responses(
        (status = 204, description = "Participant removed successfully"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Participant not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn remove_participant(
    State(_state): State<Arc<AppState>>,
    Path((session_id, group_jid, user_jid)): Path<(String, String, String)>,
) -> AppResult<StatusCode> {
    let _ = repo::get_one(&session_id, &group_jid, &user_jid)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Participant '{}' not found in group '{}'", user_jid, group_jid)))?;

    repo::delete(&session_id, &group_jid, &user_jid).await?;
    Ok(StatusCode::NO_CONTENT)
}

