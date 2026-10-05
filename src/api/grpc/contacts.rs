use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::api::grpc::chats::{map_wa_error, parse_target_jid};
use crate::api::grpc::proto::contact_service_server::ContactService;
use crate::api::grpc::proto::{
    BlockContactRequest, CheckContactsRequest, CheckContactsResponse, ContactActionResponse,
    ContactItem, ContactStatusItem, DeleteContactRequest, DeleteContactResponse,
    GetContactRequest, GetProfilePictureRequest, GetProfilePictureResponse, ListContactsRequest,
    ListContactsResponse, SyncContactsRequest, SyncContactsResponse, UnblockContactRequest,
    UpsertContactRequest,
};
use crate::api::http::state::AppState;
use crate::models::contacts::Contact;
use crate::repository::contacts as repo;
use crate::repository::sessions as session_repo;

pub struct ContactServiceImpl {
    state: Arc<AppState>,
}

impl ContactServiceImpl {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

fn model_to_proto(c: Contact) -> ContactItem {
    ContactItem {
        session_id: c.session_id,
        jid: c.jid,
        lid_jid: c.lid_jid,
        phone_number: c.phone_number,
        push_name: c.push_name,
        status_text: c.status_text,
        profile_pic_url: c.profile_pic_url,
        is_business: c.is_business,
        username: c.username,
        devices: c.devices,
        verified_name: c.verified_name,
        updated_at: c.updated_at.timestamp(),
    }
}

#[tonic::async_trait]
impl ContactService for ContactServiceImpl {
    async fn list_contacts(
        &self,
        request: Request<ListContactsRequest>,
    ) -> Result<Response<ListContactsResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;

        let _ = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        let limit = req.limit.filter(|&l| l > 0).map(|l| l.clamp(1, 100));
        let offset = req.offset.filter(|&o| o > 0);
        let contacts = repo::get_by_session_paginated(&session_id, limit, offset, None)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        let items = contacts.into_iter().map(model_to_proto).collect();
        Ok(Response::new(ListContactsResponse { contacts: items }))
    }

    async fn upsert_contact(
        &self,
        request: Request<UpsertContactRequest>,
    ) -> Result<Response<ContactItem>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;

        let _ = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

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

        repo::upsert(&contact)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        let saved = repo::get_one(&req.jid, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::internal("Failed to retrieve saved contact"))?;

        Ok(Response::new(model_to_proto(saved)))
    }

    async fn sync_contacts(
        &self,
        request: Request<SyncContactsRequest>,
    ) -> Result<Response<SyncContactsResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;

        let _ = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

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

        let count = repo::upsert_batch(&session_id, &contacts)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        Ok(Response::new(SyncContactsResponse {
            synced_count: count as i64,
        }))
    }

    async fn check_contacts(
        &self,
        request: Request<CheckContactsRequest>,
    ) -> Result<Response<CheckContactsResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;

        let _ = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        if req.contacts.is_empty() {
            return Err(Status::invalid_argument("Contacts array cannot be empty"));
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

        let mut jids = Vec::with_capacity(req.contacts.len());
        let mut input_map = std::collections::HashMap::new();

        for raw in &req.contacts {
            let parsed = parse_target_jid(raw)?;
            if !parsed.is_pn() && !parsed.is_lid() {
                return Err(Status::invalid_argument(format!(
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
            .map_err(map_wa_error)?;

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

        Ok(Response::new(CheckContactsResponse {
            results: status_items,
        }))
    }

    async fn get_contact(
        &self,
        request: Request<GetContactRequest>,
    ) -> Result<Response<ContactItem>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.jid;

        let _ = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        let contact = repo::get_one(&jid, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| {
                Status::not_found(format!(
                    "Contact '{}' not found in session '{}'",
                    jid, session_id
                ))
            })?;

        Ok(Response::new(model_to_proto(contact)))
    }

    async fn delete_contact(
        &self,
        request: Request<DeleteContactRequest>,
    ) -> Result<Response<DeleteContactResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.jid;

        let _ = repo::get_one(&jid, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Contact '{}' not found", jid)))?;

        repo::delete(&jid, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        Ok(Response::new(DeleteContactResponse { success: true }))
    }

    async fn get_profile_picture(
        &self,
        request: Request<GetProfilePictureRequest>,
    ) -> Result<Response<GetProfilePictureResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let jid = req.jid;
        let preview = req.preview.unwrap_or(false);

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
        let pic_opt = client
            .contacts()
            .get_profile_picture(&target_jid, preview)
            .await
            .map_err(map_wa_error)?;

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
                    let media_dir = self.state.config.media.dir.clone();
                    tokio::spawn(async move {
                        if let Ok(resp) = reqwest::get(&download_url).await
                            && resp.status().is_success()
                            && let Ok(bytes) = resp.bytes().await
                        {
                            use sha2::Digest;
                            let sha256_hex = hex::encode(sha2::Sha256::digest(&bytes));
                            let file_path =
                                std::path::Path::new(&media_dir).join(format!("{}.jpg", sha256_hex));
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
                                let _ = std::fs::create_dir_all(&media_dir);
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

                Ok(Response::new(GetProfilePictureResponse {
                    id: pic.id,
                    url: pic.url,
                    direct_path: pic.direct_path,
                    hash: pic.hash,
                }))
            }
            None => Err(Status::not_found(format!(
                "Profile picture not available for '{}'",
                jid
            ))),
        }
    }

    async fn block_contact(
        &self,
        request: Request<BlockContactRequest>,
    ) -> Result<Response<ContactActionResponse>, Status> {
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
            .blocking()
            .block(&target_jid)
            .await
            .map_err(map_wa_error)?;

        Ok(Response::new(ContactActionResponse {
            session_id,
            jid,
            status: "blocked".to_string(),
        }))
    }

    async fn unblock_contact(
        &self,
        request: Request<UnblockContactRequest>,
    ) -> Result<Response<ContactActionResponse>, Status> {
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
            .blocking()
            .unblock(&target_jid)
            .await
            .map_err(map_wa_error)?;

        Ok(Response::new(ContactActionResponse {
            session_id,
            jid,
            status: "unblocked".to_string(),
        }))
    }
}
