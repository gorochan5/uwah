use crate::api::http::{
    error::{AppError, AppResult},
    state::AppState,
};
use crate::models::sessions::Session;
use crate::repository::sessions as repo;
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct CreateSessionRequest {
    pub id: String,
}

#[derive(Deserialize, ToSchema)]
pub struct RefreshQrRequest {
    pub qr_payload: String,
}

use crate::api::http::error::ErrorResponse;

#[utoipa::path(
    get,
    path = "/sessions",
    tag = "Sessions",
    summary = "List sessions",
    description = "Retrieves all configured WhatsApp sessions and their current status.",
    responses(
        (status = 200, description = "List of WhatsApp sessions", body = Vec<Session>),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn list_sessions(State(_state): State<Arc<AppState>>) -> AppResult<Json<Vec<Session>>> {
    let sessions = repo::get_all().await?;
    Ok(Json(sessions))
}

#[utoipa::path(
    get,
    path = "/sessions/{id}",
    tag = "Sessions",
    summary = "Get session",
    description = "Retrieves session details by session ID.",
    params(
        ("id" = String, Path, description = "Session identifier")
    ),
    responses(
        (status = 200, description = "Session details", body = Session),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn get_session(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<Session>> {
    crate::security::validate_session_id(&id)?;
    let session = repo::get_one(&id).await?;
    match session {
        Some(s) => Ok(Json(s)),
        None => Err(AppError::NotFound(format!("Session '{}' not found", id))),
    }
}

#[utoipa::path(
    post,
    path = "/sessions",
    tag = "Sessions",
    summary = "Create session",
    description = "Initializes a new WhatsApp session and starts the background worker bot.",
    request_body = CreateSessionRequest,
    responses(
        (status = 201, description = "Session created successfully", body = Session),
        (status = 400, description = "Session id is empty or invalid", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 409, description = "Session already exists", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateSessionRequest>,
) -> AppResult<(StatusCode, Json<Session>)> {
    let id = req.id.trim();
    crate::security::validate_session_id(id)?;

    if repo::get_one(id).await?.is_some() {
        return Err(AppError::Conflict(format!(
            "Session '{}' already exists",
            id
        )));
    }

    repo::create(id).await?;

    // Start background WhatsApp bot session
    let sid = id.to_string();
    if let Err(e) = state.sessions.start_session(sid).await {
        tracing::error!(session_id = %id, error = %e, "Failed to start WhatsApp bot");
        let _ = repo::delete(id).await;
        return Err(AppError::Internal(e));
    }

    let created = repo::get_one(id)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Created session not found")))?;

    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/qr",
    tag = "Sessions",
    summary = "Refresh QR payload",
    description = "Updates the raw QR code pairing payload for a session.",
    params(
        ("id" = String, Path, description = "Session identifier")
    ),
    request_body = RefreshQrRequest,
    responses(
        (status = 200, description = "Session QR payload updated", body = Session),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn refresh_qr(
    State(_state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<RefreshQrRequest>,
) -> AppResult<Json<Session>> {
    crate::security::validate_session_id(&id)?;
    let _ = repo::get_one(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", id)))?;

    repo::refresh_qr(&id, &req.qr_payload).await?;

    let updated = repo::get_one(&id)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Updated session not found")))?;

    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/sessions/{id}",
    tag = "Sessions",
    summary = "Delete session",
    description = "Stops the active session bot worker and deletes the session.",
    params(
        ("id" = String, Path, description = "Session identifier")
    ),
    responses(
        (status = 204, description = "Session deleted successfully"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn delete_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<StatusCode> {
    crate::security::validate_session_id(&id)?;
    let _ = repo::get_one(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", id)))?;

    // Stop active bot and clean up filesystem keystore files
    state.sessions.delete_session_files(&id).await;

    repo::delete(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionActionResponse {
    pub id: String,
    pub status: String,
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/restart",
    tag = "Sessions",
    summary = "Restart session bot worker",
    description = "Stops the in-memory WhatsApp client and spawns a fresh bot worker instance.",
    params(
        ("id" = String, Path, description = "Session identifier")
    ),
    responses(
        (status = 200, description = "Session restarted successfully", body = SessionActionResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn restart_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<SessionActionResponse>> {
    crate::security::validate_session_id(&id)?;
    let _ = repo::get_one(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", id)))?;

    state.sessions.stop_session(&id).await;

    state
        .sessions
        .start_session(id.clone())
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to restart session: {}", e)))?;

    Ok(Json(SessionActionResponse {
        id,
        status: "restarted".to_string(),
    }))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/logout",
    tag = "Sessions",
    summary = "Logout session from WhatsApp",
    description = "Unlinks the companion device from WhatsApp server and disconnects local bot.",
    params(
        ("id" = String, Path, description = "Session identifier")
    ),
    responses(
        (status = 200, description = "Session logged out", body = SessionActionResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn logout_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> AppResult<Json<SessionActionResponse>> {
    crate::security::validate_session_id(&id)?;
    let session = repo::get_one(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", id)))?;

    if let Some(client) = state.sessions.get_client(&id).await {
        client.logout().await;
    }
    state.sessions.stop_session(&id).await;

    let updated = Session {
        id: id.clone(),
        phone_number: None,
        push_name: None,
        status: "LOGGED_OUT".to_string(),
        qr_payload: None,
        created_at: session.created_at,
        updated_at: chrono::Utc::now(),
    };
    repo::auth(updated).await?;

    Ok(Json(SessionActionResponse {
        id,
        status: "logged_out".to_string(),
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PairingCodeRequest {
    /// Phone number in international format without '+' (e.g. "5511999999999")
    #[schema(example = "5511999999999")]
    pub phone_number: String,
    /// Optional custom 8-character Crockford Base32 pairing code
    pub custom_code: Option<String>,
    /// Whether to prompt notification on primary device (defaults to true)
    pub show_push_notification: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PairingCodeResponse {
    pub session_id: String,
    pub phone_number: String,
    pub pairing_code: String,
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/pairing-code",
    tag = "Sessions",
    summary = "Request phone pairing code",
    description = "Requests an 8-character pairing code for linking via phone number instead of QR code.",
    params(
        ("id" = String, Path, description = "Session identifier")
    ),
    request_body = PairingCodeRequest,
    responses(
        (status = 200, description = "Pairing code generated", body = PairingCodeResponse),
        (status = 400, description = "Invalid phone number or pairing error", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn request_pairing_code(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<PairingCodeRequest>,
) -> AppResult<Json<PairingCodeResponse>> {
    crate::security::validate_session_id(&id)?;
    let _ = repo::get_one(&id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", id)))?;

    let cleaned_phone: String = req
        .phone_number
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    if cleaned_phone.is_empty() || cleaned_phone.len() < 7 || cleaned_phone.len() > 15 {
        return Err(AppError::BadRequest(
            "Invalid phone_number: must contain 7 to 15 digits in E.164 without '+'".to_string(),
        ));
    }

    if req
        .custom_code
        .as_ref()
        .is_some_and(|code| code.trim().len() != 8)
    {
        return Err(AppError::BadRequest(
            "custom_code must be exactly 8 alphanumeric Crockford Base32 characters".to_string(),
        ));
    }

    let client = match state.sessions.get_client(&id).await {
        Some(c) => c,
        None => state
            .sessions
            .start_session(id.clone())
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to initialize bot: {}", e)))?,
    };

    use whatsapp_rust::pair_code::PairCodeOptions;

    let options = PairCodeOptions {
        phone_number: cleaned_phone.clone(),
        custom_code: req.custom_code,
        show_push_notification: req.show_push_notification.unwrap_or(true),
        ..Default::default()
    };

    let pairing_code = client
        .pair_with_code(options)
        .await
        .map_err(|err| AppError::BadRequest(format!("Failed to acquire pairing code: {}", err)))?;

    Ok(Json(PairingCodeResponse {
        session_id: id,
        phone_number: cleaned_phone,
        pairing_code,
    }))
}
