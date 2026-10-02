use std::sync::Arc;
use tonic::{Request, Response, Status};
use whatsapp_rust::{
    GroupCreateOptions, GroupParticipantOptions, GroupSubject, Server,
};

use crate::api::grpc::chats::{map_wa_error, parse_group_jid, parse_target_jid};
use crate::api::grpc::proto::group_participant_service_server::GroupParticipantService;
use crate::api::grpc::proto::group_service_server::GroupService;
use crate::api::grpc::proto::{
    AddParticipantsRequest, CreateGroupRequest, CreateGroupResponse, GroupParticipantItem,
    JoinGroupRequest, JoinGroupResponse, ListParticipantsRequest, ListParticipantsResponse,
    ModifyParticipantRolesRequest, ParticipantActionResult, ParticipantActionResponse,
    RemoveParticipantRequest,
};
use crate::api::http::state::AppState;
use crate::models::group_participants::GroupParticipant;
use crate::repository::chats as chat_repo;
use crate::repository::group_participants as gp_repo;
use crate::repository::sessions as session_repo;

pub struct GroupServiceImpl {
    state: Arc<AppState>,
}

impl GroupServiceImpl {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

pub struct GroupParticipantServiceImpl {
    state: Arc<AppState>,
}

impl GroupParticipantServiceImpl {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl GroupService for GroupServiceImpl {
    async fn create_group(
        &self,
        request: Request<CreateGroupRequest>,
    ) -> Result<Response<CreateGroupResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;

        let _ = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        if req.subject.trim().is_empty() {
            return Err(Status::invalid_argument(
                "Group subject cannot be empty or whitespace only",
            ));
        }

        if req.subject.chars().count() > 100 {
            return Err(Status::invalid_argument(
                "Group subject exceeds maximum length of 100 characters",
            ));
        }

        if req.participants.is_empty() {
            return Err(Status::invalid_argument(
                "Group must have at least one initial participant",
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

        let _subject_validated = GroupSubject::new(&req.subject)
            .map_err(|e| Status::invalid_argument(format!("Invalid group subject: {}", e)))?;

        let mut participant_opts = Vec::new();
        for p_str in &req.participants {
            let p_jid = parse_target_jid(p_str)?;
            if p_jid.server == Server::Group {
                return Err(Status::invalid_argument(format!(
                    "Participant '{}' is a group, not an individual user",
                    p_str
                )));
            }
            participant_opts.push(GroupParticipantOptions::new(p_jid));
        }

        let builder = GroupCreateOptions::builder()
            .subject(req.subject.clone())
            .participants(participant_opts);

        let create_result = client
            .groups()
            .create_group(builder.build())
            .await
            .map_err(map_wa_error)?;

        let meta = create_result.metadata;
        let group_jid_str = meta.id.to_string();

        chat_repo::ensure_exists(&group_jid_str, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Failed to register group in chats: {}", e)))?;

        let creator_str = meta.creator.as_ref().map(ToString::to_string);
        let creation_time = meta.creation_time.map(|t| t as i64);

        let _ = chat_repo::update_subject(
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

            let _ = gp_repo::upsert(&gp).await;

            participant_items.push(GroupParticipantItem {
                user_jid: p_jid_str,
                role: role.to_string(),
            });
        }

        Ok(Response::new(CreateGroupResponse {
            session_id,
            group_jid: group_jid_str,
            subject: meta.subject,
            participants: participant_items,
        }))
    }

    async fn join_group(
        &self,
        request: Request<JoinGroupRequest>,
    ) -> Result<Response<JoinGroupResponse>, Status> {
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

        let raw_code = req.invite_code.trim();
        let code = if let Some(stripped) = raw_code.strip_prefix("https://chat.whatsapp.com/") {
            stripped.trim()
        } else if let Some(stripped) = raw_code.strip_prefix("chat.whatsapp.com/") {
            stripped.trim()
        } else {
            raw_code
        };

        if code.is_empty() {
            return Err(Status::invalid_argument("Invite code cannot be empty"));
        }

        let join_res = client
            .groups()
            .join_with_invite_code(code)
            .await
            .map_err(map_wa_error)?;

        let group_jid_str = join_res.group_jid().to_string();
        let status_str = match join_res {
            whatsapp_rust::JoinGroupResult::Joined(_) => "joined",
            whatsapp_rust::JoinGroupResult::PendingApproval(_) => "pending_approval",
        };

        let _ = chat_repo::ensure_exists(&group_jid_str, &session_id).await;

        Ok(Response::new(JoinGroupResponse {
            session_id,
            group_jid: group_jid_str,
            status: status_str.to_string(),
        }))
    }
}

#[tonic::async_trait]
impl GroupParticipantService for GroupParticipantServiceImpl {
    async fn list_participants(
        &self,
        request: Request<ListParticipantsRequest>,
    ) -> Result<Response<ListParticipantsResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let group_jid = req.jid;

        let _ = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        let mut participants = gp_repo::get_by_group(&session_id, &group_jid)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        if participants.is_empty()
            && let Some(client) = self.state.sessions.get_client(&session_id).await
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
                let gp = crate::models::group_participants::GroupParticipant {
                    session_id: session_id.clone(),
                    group_jid: group_jid.clone(),
                    user_jid: p.jid.to_string(),
                    role: role.to_string(),
                    joined_at: now,
                };
                let _ = gp_repo::upsert(&gp).await;
                fresh_list.push(gp);
            }
            if !fresh_list.is_empty() {
                participants = fresh_list;
            }
        }

        let items = participants
            .into_iter()
            .map(|p| GroupParticipantItem {
                user_jid: p.user_jid,
                role: p.role,
            })
            .collect();

        Ok(Response::new(ListParticipantsResponse {
            participants: items,
        }))
    }

    async fn add_participants(
        &self,
        request: Request<AddParticipantsRequest>,
    ) -> Result<Response<ParticipantActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let group_jid = req.jid;

        let _ = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        if req.participants.is_empty() {
            return Err(Status::invalid_argument("Participants list cannot be empty"));
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

        let parsed_group_jid = parse_group_jid(&group_jid)?;

        let mut user_jids = Vec::new();
        for p_str in &req.participants {
            let u_jid = parse_target_jid(p_str)?;
            if u_jid.server == Server::Group {
                return Err(Status::invalid_argument(format!(
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
                let _ = gp_repo::upsert(&gp).await;
            }

            results.push(ParticipantActionResult {
                user_jid: u_jid_str,
                status: status_code.to_string(),
                error: if is_success {
                    None
                } else {
                    Some(format!("WhatsApp error status: {}", status_code))
                },
            });
        }

        Ok(Response::new(ParticipantActionResponse {
            session_id,
            group_jid,
            results,
        }))
    }

    async fn remove_participant(
        &self,
        request: Request<RemoveParticipantRequest>,
    ) -> Result<Response<ParticipantActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let group_jid = req.jid;
        let user_jid = req.user_jid;

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

        let parsed_group_jid = parse_group_jid(&group_jid)?;
        let parsed_user_jid = parse_target_jid(&user_jid)?;

        client
            .groups()
            .remove_participants(&parsed_group_jid, &[parsed_user_jid])
            .await
            .map_err(map_wa_error)?;

        let _ = gp_repo::delete(&session_id, &group_jid, &user_jid).await;

        Ok(Response::new(ParticipantActionResponse {
            session_id,
            group_jid,
            results: vec![ParticipantActionResult {
                user_jid,
                status: "200".to_string(),
                error: None,
            }],
        }))
    }

    async fn promote_participants(
        &self,
        request: Request<ModifyParticipantRolesRequest>,
    ) -> Result<Response<ParticipantActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let group_jid = req.jid;

        let _ = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        if req.participants.is_empty() {
            return Err(Status::invalid_argument("Participants list cannot be empty"));
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

        let parsed_group_jid = parse_group_jid(&group_jid)?;

        let mut user_jids = Vec::new();
        for p_str in &req.participants {
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
                let _ = gp_repo::update_role(&session_id, &group_jid, &u_jid_str, "admin").await;
            }

            results.push(ParticipantActionResult {
                user_jid: u_jid_str,
                status: status_code.to_string(),
                error: if is_success {
                    None
                } else {
                    Some(format!("WhatsApp error status: {}", status_code))
                },
            });
        }

        Ok(Response::new(ParticipantActionResponse {
            session_id,
            group_jid,
            results,
        }))
    }

    async fn demote_participants(
        &self,
        request: Request<ModifyParticipantRolesRequest>,
    ) -> Result<Response<ParticipantActionResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let group_jid = req.jid;

        let _ = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        if req.participants.is_empty() {
            return Err(Status::invalid_argument("Participants list cannot be empty"));
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

        let parsed_group_jid = parse_group_jid(&group_jid)?;

        let mut user_jids = Vec::new();
        for p_str in &req.participants {
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
                let _ = gp_repo::update_role(&session_id, &group_jid, &u_jid_str, "member").await;
            }

            results.push(ParticipantActionResult {
                user_jid: u_jid_str,
                status: status_code.to_string(),
                error: if is_success {
                    None
                } else {
                    Some(format!("WhatsApp error status: {}", status_code))
                },
            });
        }

        Ok(Response::new(ParticipantActionResponse {
            session_id,
            group_jid,
            results,
        }))
    }
}
