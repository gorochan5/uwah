use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::api::grpc::proto::call_service_server::CallService;
use crate::api::grpc::proto::{
    CallItem, DeleteCallRequest, DeleteCallResponse, GetCallRequest, ListCallsRequest,
    ListCallsResponse, RejectCallRequest,
};
use crate::api::http::state::AppState;
use crate::models::calls::Call;
use crate::repository::calls as repo;
use crate::repository::sessions as session_repo;

pub struct CallServiceImpl {
    state: Arc<AppState>,
}

impl CallServiceImpl {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

fn model_to_proto(c: Call) -> CallItem {
    CallItem {
        session_id: c.session_id,
        id: c.id,
        chat_jid: c.chat_jid,
        caller_jid: c.caller_jid,
        is_video: c.is_video,
        is_group_call: c.is_group_call,
        status: c.status,
        duration_seconds: c.duration_seconds,
        created_at: c.created_at.timestamp(),
        ended_at: c.ended_at.map(|t| t.timestamp()),
    }
}

#[tonic::async_trait]
impl CallService for CallServiceImpl {
    async fn list_calls(
        &self,
        request: Request<ListCallsRequest>,
    ) -> Result<Response<ListCallsResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;

        let _ = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        let calls = repo::get_by_session(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        let items = calls.into_iter().map(model_to_proto).collect();
        Ok(Response::new(ListCallsResponse { calls: items }))
    }

    async fn get_call(
        &self,
        request: Request<GetCallRequest>,
    ) -> Result<Response<CallItem>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let call_id = req.call_id;

        let _ = session_repo::get_one(&session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Session '{}' not found", session_id)))?;

        let call = repo::get_one(&call_id, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| {
                Status::not_found(format!(
                    "Call '{}' not found in session '{}'",
                    call_id, session_id
                ))
            })?;

        Ok(Response::new(model_to_proto(call)))
    }

    async fn delete_call(
        &self,
        request: Request<DeleteCallRequest>,
    ) -> Result<Response<DeleteCallResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let call_id = req.call_id;

        crate::security::validate_session_id(&session_id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let _ = repo::get_one(&call_id, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Call '{}' not found", call_id)))?;

        repo::delete(&call_id, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        Ok(Response::new(DeleteCallResponse { success: true }))
    }

    async fn reject_call(
        &self,
        request: Request<RejectCallRequest>,
    ) -> Result<Response<CallItem>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let call_id = req.call_id;

        crate::security::validate_session_id(&session_id)
            .map_err(|e| Status::invalid_argument(e.to_string()))?;

        let call = repo::get_one(&call_id, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Call '{}' not found", call_id)))?;

        let now = chrono::Utc::now();
        repo::update_status(&session_id, &call_id, "rejected", None, Some(now))
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        let updated = repo::get_one(&call_id, &session_id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .unwrap_or(call);

        crate::events::webhook::dispatch_event("call.rejected", &session_id, updated.clone());

        Ok(Response::new(model_to_proto(updated)))
    }
}
