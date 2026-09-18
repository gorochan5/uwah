use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::api::http::{
    error::{AppError, AppResult},
    state::AppState,
};
use crate::models::contacts::Contact;
use crate::repository::contacts as repo;
use crate::repository::sessions as session_repo;

use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct UpsertContactRequest {
    pub jid: String,
    pub lid_jid: Option<String>,
    pub phone_number: Option<String>,
    pub push_name: Option<String>,
    pub status_text: Option<String>,
    pub profile_pic_url: Option<String>,
    pub is_business: Option<bool>,
    pub username: Option<String>,
    pub devices: Option<String>,
    pub verified_name: Option<String>,
}

use crate::api::http::error::ErrorResponse;

#[derive(Deserialize, ToSchema)]
pub struct SyncContactsRequest {
    pub contacts: Vec<UpsertContactRequest>,
}

#[derive(Serialize, ToSchema)]
pub struct SyncContactsResponse {
    pub synced_count: usize,
}

#[utoipa::path(
    get,
    path = "/sessions/{id}/contacts",
    tag = "Contacts",
    summary = "List contacts",
    description = "Retrieves all synced contacts for the specified session.",
    params(
        ("id" = String, Path, description = "Session identifier")
    ),
    responses(
        (status = 200, description = "List of contacts", body = Vec<Contact>),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn list_contacts(
    State(_state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> AppResult<Json<Vec<Contact>>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let contacts = repo::get_by_session(&session_id).await?;
    Ok(Json(contacts))
}

#[utoipa::path(
    get,
    path = "/sessions/{id}/contacts/{jid}",
    tag = "Contacts",
    summary = "Get contact",
    description = "Retrieves a contact by JID for the specified session.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Contact WhatsApp JID")
    ),
    responses(
        (status = 200, description = "Contact details", body = Contact),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Contact or session not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn get_contact(
    State(_state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
) -> AppResult<Json<Contact>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let contact = repo::get_one(&jid, &session_id).await?;
    match contact {
        Some(c) => Ok(Json(c)),
        None => Err(AppError::NotFound(format!("Contact '{}' not found in session '{}'", jid, session_id))),
    }
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/contacts",
    tag = "Contacts",
    summary = "Upsert contact",
    description = "Creates or updates a single contact record in local storage.",
    params(
        ("id" = String, Path, description = "Session identifier")
    ),
    request_body = UpsertContactRequest,
    responses(
        (status = 200, description = "Contact upserted successfully", body = Contact),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn upsert_contact(
    State(_state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(req): Json<UpsertContactRequest>,
) -> AppResult<impl IntoResponse> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let contact = Contact {
        session_id: session_id.clone(),
        jid: req.jid.clone(),
        lid_jid: req.lid_jid,
        phone_number: req.phone_number,
        push_name: req.push_name,
        status_text: req.status_text,
        profile_pic_url: req.profile_pic_url,
        is_business: req.is_business.unwrap_or(false),
        username: req.username,
        devices: req.devices,
        verified_name: req.verified_name,
        updated_at: chrono::Utc::now(),
    };

    repo::upsert(&contact).await?;

    let saved = repo::get_one(&req.jid, &session_id)
        .await?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Failed to retrieve saved contact")))?;

    Ok((StatusCode::OK, Json(saved)))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/contacts/sync",
    tag = "Contacts",
    summary = "Sync contacts in bulk",
    description = "Batch synchronizes contacts into local storage for the specified session.",
    params(
        ("id" = String, Path, description = "Session identifier")
    ),
    request_body = SyncContactsRequest,
    responses(
        (status = 200, description = "Batch sync summary", body = SyncContactsResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn sync_contacts(
    State(_state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(req): Json<SyncContactsRequest>,
) -> AppResult<impl IntoResponse> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let count = req.contacts.len();
    for item in req.contacts {
        let contact = Contact {
            session_id: session_id.clone(),
            jid: item.jid,
            lid_jid: item.lid_jid,
            phone_number: item.phone_number,
            push_name: item.push_name,
            status_text: item.status_text,
            profile_pic_url: item.profile_pic_url,
            is_business: item.is_business.unwrap_or(false),
            username: item.username,
            devices: item.devices,
            verified_name: item.verified_name,
            updated_at: chrono::Utc::now(),
        };
        repo::upsert(&contact).await?;
    }

    Ok((StatusCode::OK, Json(SyncContactsResponse { synced_count: count })))
}

#[utoipa::path(
    delete,
    path = "/sessions/{id}/contacts/{jid}",
    tag = "Contacts",
    summary = "Delete contact",
    description = "Deletes a contact record from local storage.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "Contact WhatsApp JID")
    ),
    responses(
        (status = 204, description = "Contact deleted successfully"),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Contact not found", body = ErrorResponse),
        (status = 500, description = "Database error", body = ErrorResponse)
    )
)]
pub async fn delete_contact(
    State(_state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
) -> AppResult<StatusCode> {
    let _ = repo::get_one(&jid, &session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Contact '{}' not found", jid)))?;

    repo::delete(&jid, &session_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

