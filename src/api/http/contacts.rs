use crate::api::http::{
    chats::ChatActionResponse,
    error::{AppError, AppResult},
    state::AppState,
};
use crate::models::contacts::Contact;
use crate::repository::contacts as repo;
use crate::repository::sessions as session_repo;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

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
use utoipa::IntoParams;

#[derive(Debug, Deserialize, IntoParams)]
pub struct ListContactsQuery {
    #[param(example = 50)]
    pub limit: Option<i64>,
    #[param(example = 0)]
    pub offset: Option<i64>,
    #[param(example = "John")]
    pub search: Option<String>,
    #[param(example = "John")]
    pub q: Option<String>,
}

#[utoipa::path(
    get,
    path = "/sessions/{id}/contacts",
    tag = "Contacts",
    summary = "List contacts",
    description = "Retrieves contacts for the specified session with optional search filtering and pagination. When limit is omitted, all contacts are returned.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ListContactsQuery
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
    Query(query): Query<ListContactsQuery>,
) -> AppResult<Json<Vec<Contact>>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let limit = query.limit.map(|l| l.clamp(1, 100));
    let offset = query.offset.map(|o| o.max(0));
    let search_term = query.search.as_deref().or(query.q.as_deref());

    let contacts = repo::get_by_session_paginated(&session_id, limit, offset, search_term).await?;
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
        None => Err(AppError::NotFound(format!(
            "Contact '{}' not found in session '{}'",
            jid, session_id
        ))),
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
) -> AppResult<(StatusCode, Json<Contact>)> {
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
) -> AppResult<(StatusCode, Json<SyncContactsResponse>)> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let contacts: Vec<Contact> = req
        .contacts
        .into_iter()
        .map(|item| Contact {
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
        })
        .collect();

    let count = repo::upsert_batch(&session_id, &contacts).await?;

    Ok((
        StatusCode::OK,
        Json(SyncContactsResponse {
            synced_count: count,
        }),
    ))
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct CheckContactsRequest {
    /// Array of phone numbers (e.g. "5511999999999") or JIDs ("123456789@s.whatsapp.net")
    #[schema(example = json!(["5511999999999", "1234567890@s.whatsapp.net"]))]
    pub contacts: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ContactStatusItem {
    pub input: String,
    pub jid: String,
    pub is_registered: bool,
    pub is_business: bool,
    pub lid: Option<String>,
    pub pn_jid: Option<String>,
    pub verified_name: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CheckContactsResponse {
    pub results: Vec<ContactStatusItem>,
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/contacts/check",
    tag = "Contacts",
    summary = "Check WhatsApp registration",
    description = "Checks whether one or more phone numbers or JIDs are registered on WhatsApp.",
    params(
        ("id" = String, Path, description = "Session identifier")
    ),
    request_body = CheckContactsRequest,
    responses(
        (status = 200, description = "Registration check results", body = CheckContactsResponse),
        (status = 400, description = "Empty contacts list or inactive session", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn check_contacts(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Json(req): Json<CheckContactsRequest>,
) -> AppResult<Json<CheckContactsResponse>> {
    let _ = session_repo::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    if req.contacts.is_empty() {
        return Err(AppError::BadRequest(
            "Contacts array cannot be empty".to_string(),
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

    let mut jids = Vec::with_capacity(req.contacts.len());
    let mut input_map = std::collections::HashMap::new();

    for raw in &req.contacts {
        let parsed = crate::api::http::chats::parse_target_jid(raw)?;
        if !parsed.is_pn() && !parsed.is_lid() {
            return Err(AppError::BadRequest(format!(
                "Contact '{}' resolved to unsupported JID type. Only PN and LID contacts are supported.",
                raw
            )));
        }
        input_map.insert(parsed.to_non_ad().to_string(), raw.clone());
        jids.push(parsed);
    }

    let results = client
        .contacts()
        .is_on_whatsapp(&jids)
        .await
        .map_err(crate::api::http::chats::map_wa_error)?;

    let status_items: Vec<ContactStatusItem> = results
        .into_iter()
        .map(|r| {
            let jid_str = r.jid.to_string();
            let input = input_map
                .get(&r.jid.to_non_ad().to_string())
                .cloned()
                .unwrap_or_else(|| jid_str.clone());

            let error = r.contact_error.map(|e| format!("{:?}", e));
            let verified_name = r.verified_name.and_then(|v| v.name);

            ContactStatusItem {
                input,
                jid: jid_str,
                is_registered: r.is_registered,
                is_business: r.is_business,
                lid: r.lid.map(|j| j.to_string()),
                pn_jid: r.pn_jid.map(|j| j.to_string()),
                verified_name,
                error,
            }
        })
        .collect();

    Ok(Json(CheckContactsResponse {
        results: status_items,
    }))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct ProfilePictureQuery {
    /// If true, requests preview thumbnail; otherwise full-resolution picture. Default: false.
    pub preview: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProfilePictureResponse {
    pub id: String,
    pub url: String,
    pub direct_path: Option<String>,
    pub hash: Option<String>,
}

#[utoipa::path(
    get,
    path = "/sessions/{id}/contacts/{jid}/profile-picture",
    tag = "Contacts",
    summary = "Get contact profile picture",
    description = "Fetches the public profile picture URL and metadata for a WhatsApp contact or group.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "WhatsApp JID"),
        ProfilePictureQuery
    ),
    responses(
        (status = 200, description = "Profile picture details", body = ProfilePictureResponse),
        (status = 400, description = "Invalid JID or inactive session", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Profile picture or session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn get_profile_picture(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
    Query(query): Query<ProfilePictureQuery>,
) -> AppResult<Json<ProfilePictureResponse>> {
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

    let target_jid = crate::api::http::chats::parse_target_jid(&jid)?;
    let preview = query.preview.unwrap_or(false);

    let pic_opt = client
        .contacts()
        .get_profile_picture(&target_jid, preview)
        .await
        .map_err(crate::api::http::chats::map_wa_error)?;

    match pic_opt {
        Some(pic) => {
            let pic_url_unchanged = repo::get_one(&jid, &session_id)
                .await
                .ok()
                .flatten()
                .and_then(|c| c.profile_pic_url)
                .map(|url| url == pic.url)
                .unwrap_or(false);

            let _ = repo::update_profile_pic_url(&session_id, &jid, &pic.url).await;

            if pic_url_unchanged {
                tracing::info!(
                    contact_jid = %jid,
                    "Profile picture URL unchanged, skipping background download"
                );
            } else {
                let download_url = pic.url.clone();
                let media_dir = state.config.media.dir.clone();
                let contact_jid_for_pic = jid.clone();
                let session_id_for_pic = session_id.clone();
                tokio::spawn(async move {
                    if let Ok(resp) = reqwest::get(&download_url).await
                        && resp.status().is_success()
                        && let Ok(bytes) = resp.bytes().await
                    {
                        use sha2::Digest;
                        let sha256_hex = hex::encode(sha2::Sha256::digest(&bytes));
                        let is_group = contact_jid_for_pic.ends_with("@g.us");
                        let name_or_jid = if is_group {
                            crate::repository::chats::get_one(&contact_jid_for_pic, &session_id_for_pic)
                                .await
                                .ok()
                                .flatten()
                                .and_then(|c| c.name)
                                .unwrap_or_else(|| contact_jid_for_pic.clone())
                        } else {
                            crate::repository::contacts::get_one(&contact_jid_for_pic, &session_id_for_pic)
                                .await
                                .ok()
                                .flatten()
                                .and_then(|c| c.push_name.or(c.phone_number))
                                .unwrap_or_else(|| contact_jid_for_pic.clone())
                        };
                        let file_path = crate::media::storage::get_profile_picture_path(
                            &media_dir,
                            is_group,
                            &name_or_jid,
                            &sha256_hex,
                        );
                        let disk_path_str = file_path.to_string_lossy().to_string();

                        if let Ok(Some(existing)) =
                            crate::repository::media_cache::get_one(&sha256_hex).await
                            && tokio::fs::try_exists(&existing.disk_path)
                                .await
                                .unwrap_or(false)
                        {
                            let _ = crate::repository::media_cache::touch_last_accessed(&sha256_hex)
                                .await;
                            tracing::info!(
                                media_sha256 = %sha256_hex,
                                disk_path = %existing.disk_path,
                                "Profile picture cache hit, skipping disk write and DB upsert"
                            );
                        } else {
                            if let Some(parent) = file_path.parent() {
                                let _ = tokio::fs::create_dir_all(parent).await;
                            }
                            let _ = tokio::fs::write(&file_path, &bytes).await;
                            let _ = crate::repository::media_cache::upsert(
                                &sha256_hex,
                                &disk_path_str,
                                "image/jpeg",
                                bytes.len() as i64,
                            )
                            .await;
                        }
                    }
                });
            }

            Ok(Json(ProfilePictureResponse {
                id: pic.id,
                url: pic.url,
                direct_path: pic.direct_path,
                hash: pic.hash,
            }))
        }
        None => Err(AppError::NotFound(format!(
            "Profile picture not available for '{}'",
            jid
        ))),
    }
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/contacts/{jid}/block",
    tag = "Contacts",
    summary = "Block contact",
    description = "Blocks a contact on WhatsApp network.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "WhatsApp contact JID")
    ),
    responses(
        (status = 200, description = "Contact blocked successfully", body = ChatActionResponse),
        (status = 400, description = "Invalid JID or inactive session", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn block_contact(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
) -> AppResult<Json<crate::api::http::chats::ChatActionResponse>> {
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

    let target_jid = crate::api::http::chats::parse_target_jid(&jid)?;
    client
        .blocking()
        .block(&target_jid)
        .await
        .map_err(crate::api::http::chats::map_wa_error)?;

    Ok(Json(crate::api::http::chats::ChatActionResponse {
        jid,
        status: "blocked".to_string(),
    }))
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/contacts/{jid}/unblock",
    tag = "Contacts",
    summary = "Unblock contact",
    description = "Unblocks a previously blocked contact on WhatsApp network.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("jid" = String, Path, description = "WhatsApp contact JID")
    ),
    responses(
        (status = 200, description = "Contact unblocked successfully", body = ChatActionResponse),
        (status = 400, description = "Invalid JID or inactive session", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal error", body = ErrorResponse)
    )
)]
pub async fn unblock_contact(
    State(state): State<Arc<AppState>>,
    Path((session_id, jid)): Path<(String, String)>,
) -> AppResult<Json<crate::api::http::chats::ChatActionResponse>> {
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

    let target_jid = crate::api::http::chats::parse_target_jid(&jid)?;
    client
        .blocking()
        .unblock(&target_jid)
        .await
        .map_err(crate::api::http::chats::map_wa_error)?;

    Ok(Json(crate::api::http::chats::ChatActionResponse {
        jid,
        status: "unblocked".to_string(),
    }))
}
