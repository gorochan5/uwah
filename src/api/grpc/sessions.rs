use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::api::grpc::proto::session_service_server::SessionService;
use crate::api::grpc::proto::{
    CreateSessionRequest, DeleteSessionRequest, DeleteSessionResponse, GetSessionRequest,
    ListSessionsRequest, ListSessionsResponse, LogoutSessionRequest, PairingCodeRequest,
    PairingCodeResponse, RefreshQrRequest, RestartSessionRequest, SessionActionResponse,
    SessionItem,
};
use crate::api::http::state::AppState;
use crate::models::sessions::Session;
use crate::repository::sessions as repo;

pub struct SessionServiceImpl {
    state: Arc<AppState>,
}

impl SessionServiceImpl {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

fn model_to_proto(s: Session) -> SessionItem {
    SessionItem {
        id: s.id,
        status: s.status,
        phone_number: s.phone_number,
        push_name: s.push_name,
        qr_payload: s.qr_payload,
        created_at: s.created_at.timestamp(),
        updated_at: s.updated_at.timestamp(),
    }
}

#[tonic::async_trait]
impl SessionService for SessionServiceImpl {
    async fn list_sessions(
        &self,
        _request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        let sessions = repo::get_all()
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        let items = sessions.into_iter().map(model_to_proto).collect();
        Ok(Response::new(ListSessionsResponse { sessions: items }))
    }

    async fn create_session(
        &self,
        request: Request<CreateSessionRequest>,
    ) -> Result<Response<SessionItem>, Status> {
        let req = request.into_inner();
        let id = req.id.trim();

        crate::security::validate_session_id(id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        if repo::get_one(id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .is_some()
        {
            return Err(Status::already_exists(format!(
                "Session '{}' already exists",
                id
            )));
        }

        repo::create(id)
            .await
            .map_err(|e| Status::internal(format!("Failed to create session: {}", e)))?;

        let sid = id.to_string();
        if let Err(e) = self.state.sessions.start_session(sid).await {
            tracing::error!(session_id = %id, error = %e, "Failed to start WhatsApp bot");
            let _ = repo::delete(id).await;
            return Err(Status::internal(format!("Failed to start bot worker: {}", e)));
        }

        let created = repo::get_one(id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::internal("Created session not found"))?;

        Ok(Response::new(model_to_proto(created)))
    }

    async fn get_session(
        &self,
        request: Request<GetSessionRequest>,
    ) -> Result<Response<SessionItem>, Status> {
        let id = request.into_inner().id;
        crate::security::validate_session_id(&id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let session = repo::get_one(&id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", id)))?;

        Ok(Response::new(model_to_proto(session)))
    }

    async fn delete_session(
        &self,
        request: Request<DeleteSessionRequest>,
    ) -> Result<Response<DeleteSessionResponse>, Status> {
        let id = request.into_inner().id;
        crate::security::validate_session_id(&id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let _ = repo::get_one(&id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", id)))?;

        self.state.sessions.delete_session_files(&id).await;

        repo::delete(&id)
            .await
            .map_err(|e| Status::internal(format!("Failed to delete session: {}", e)))?;

        Ok(Response::new(DeleteSessionResponse { success: true }))
    }

    async fn refresh_qr(
        &self,
        request: Request<RefreshQrRequest>,
    ) -> Result<Response<SessionActionResponse>, Status> {
        let req = request.into_inner();
        let id = req.id;
        crate::security::validate_session_id(&id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let _ = repo::get_one(&id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", id)))?;

        repo::refresh_qr(&id, &req.qr_payload)
            .await
            .map_err(|e| Status::internal(format!("Failed to refresh QR: {}", e)))?;

        Ok(Response::new(SessionActionResponse {
            session_id: id,
            status: "qr_refreshed".to_string(),
            message: "QR payload refreshed successfully".to_string(),
        }))
    }

    async fn restart_session(
        &self,
        request: Request<RestartSessionRequest>,
    ) -> Result<Response<SessionActionResponse>, Status> {
        let id = request.into_inner().id;
        crate::security::validate_session_id(&id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let _ = repo::get_one(&id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", id)))?;

        self.state.sessions.stop_session(&id).await;

        self.state
            .sessions
            .start_session(id.clone())
            .await
            .map_err(|e| Status::internal(format!("Failed to restart session: {}", e)))?;

        Ok(Response::new(SessionActionResponse {
            session_id: id,
            status: "restarted".to_string(),
            message: "Session bot worker restarted successfully".to_string(),
        }))
    }

    async fn logout_session(
        &self,
        request: Request<LogoutSessionRequest>,
    ) -> Result<Response<SessionActionResponse>, Status> {
        let id = request.into_inner().id;
        crate::security::validate_session_id(&id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let session = repo::get_one(&id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", id)))?;

        if let Some(client) = self.state.sessions.get_client(&id).await {
            client.logout().await;
        }
        self.state.sessions.stop_session(&id).await;

        let updated = Session {
            id: id.clone(),
            phone_number: None,
            push_name: None,
            status: "LOGGED_OUT".to_string(),
            qr_payload: None,
            created_at: session.created_at,
            updated_at: chrono::Utc::now(),
        };
        repo::auth(updated)
            .await
            .map_err(|e| Status::internal(format!("Failed to update session status: {}", e)))?;

        Ok(Response::new(SessionActionResponse {
            session_id: id,
            status: "logged_out".to_string(),
            message: "Session logged out successfully".to_string(),
        }))
    }

    async fn request_pairing_code(
        &self,
        request: Request<PairingCodeRequest>,
    ) -> Result<Response<PairingCodeResponse>, Status> {
        let req = request.into_inner();
        let id = req.id;
        crate::security::validate_session_id(&id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let _ = repo::get_one(&id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", id)))?;

        let cleaned_phone: String = req
            .phone_number
            .chars()
            .filter(|c| c.is_ascii_digit())
            .collect();
        if cleaned_phone.is_empty() || cleaned_phone.len() < 7 || cleaned_phone.len() > 15 {
            return Err(Status::invalid_argument(
                "Invalid phone_number: must contain 7 to 15 digits in E.164 without '+'",
            ));
        }

        let client = match self.state.sessions.get_client(&id).await {
            Some(c) => c,
            None => self
                .state
                .sessions
                .start_session(id.clone())
                .await
                .map_err(|e| Status::internal(format!("Failed to initialize bot: {}", e)))?,
        };

        use whatsapp_rust::pair_code::PairCodeOptions;
        let options = PairCodeOptions {
            phone_number: cleaned_phone.clone(),
            custom_code: None,
            show_push_notification: true,
            ..Default::default()
        };

        let pairing_code = client
            .pair_with_code(options)
            .await
            .map_err(|err| Status::invalid_argument(format!("Failed to acquire pairing code: {}", err)))?;

        Ok(Response::new(PairingCodeResponse {
            session_id: id,
            pairing_code,
            status: "pairing_code_generated".to_string(),
        }))
    }
}
