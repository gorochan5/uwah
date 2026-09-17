use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use std::sync::Arc;
use crate::api::http::{
    error::{AppError, AppResult},
    state::AppState,
};
use crate::models::group_participants::GroupParticipant;
use crate::repository::group_participants as repo;
use crate::repository::sessions as session_repo;

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

