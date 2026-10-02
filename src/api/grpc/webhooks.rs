use std::sync::Arc;
use tonic::{Request, Response, Status};

use crate::api::grpc::proto::webhook_dlq_service_server::WebhookDlqService;
use crate::api::grpc::proto::{
    DeleteDlqRequest, DeleteDlqResponse, DlqReplayResponse, GetDlqRequest, ListDlqRequest,
    ListDlqResponse, ReplayAllDlqRequest, ReplayDlqRequest, WebhookDlqEntryItem,
};
use crate::api::http::state::AppState;
use crate::models::outbox::WebhookDlqEntry;
use crate::repository::outbox as repo;

pub struct WebhookDlqServiceImpl {
    _state: Arc<AppState>,
}

impl WebhookDlqServiceImpl {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { _state: state }
    }
}

fn model_to_proto(e: WebhookDlqEntry) -> WebhookDlqEntryItem {
    WebhookDlqEntryItem {
        id: e.id,
        session_id: e.session_id,
        event_type: e.event_type,
        aggregate_id: e.aggregate_id,
        payload: e.payload,
        retry_count: e.retry_count,
        error_reason: e.error_reason,
        failed_at: e.failed_at.timestamp(),
    }
}

#[tonic::async_trait]
impl WebhookDlqService for WebhookDlqServiceImpl {
    async fn list_dlq(
        &self,
        request: Request<ListDlqRequest>,
    ) -> Result<Response<ListDlqResponse>, Status> {
        let req = request.into_inner();
        let filter_session = req.session_id;

        let entries = repo::get_dlq_all()
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        let filtered: Vec<WebhookDlqEntryItem> = entries
            .into_iter()
            .filter(|e| {
                if let Some(ref sid) = filter_session {
                    &e.session_id == sid
                } else {
                    true
                }
            })
            .map(model_to_proto)
            .collect();

        Ok(Response::new(ListDlqResponse { entries: filtered }))
    }

    async fn get_dlq(
        &self,
        request: Request<GetDlqRequest>,
    ) -> Result<Response<WebhookDlqEntryItem>, Status> {
        let id = request.into_inner().id;

        let entry = repo::get_dlq_by_id(&id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("DLQ entry '{}' not found", id)))?;

        Ok(Response::new(model_to_proto(entry)))
    }

    async fn replay_dlq(
        &self,
        request: Request<ReplayDlqRequest>,
    ) -> Result<Response<DlqReplayResponse>, Status> {
        let id = request.into_inner().id;

        let success = repo::replay_dlq(&id)
            .await
            .map_err(|e| Status::internal(format!("Failed to replay DLQ: {}", e)))?;

        if success {
            Ok(Response::new(DlqReplayResponse {
                message: format!("DLQ entry '{}' re-enqueued for delivery", id),
                count: 1,
            }))
        } else {
            Err(Status::not_found(format!("DLQ entry '{}' not found", id)))
        }
    }

    async fn replay_all_dlq(
        &self,
        _request: Request<ReplayAllDlqRequest>,
    ) -> Result<Response<DlqReplayResponse>, Status> {
        let count = repo::replay_dlq_all()
            .await
            .map_err(|e| Status::internal(format!("Failed to replay all DLQ: {}", e)))?;

        Ok(Response::new(DlqReplayResponse {
            message: format!("Re-enqueued {} DLQ entries for delivery", count),
            count: count as i64,
        }))
    }

    async fn delete_dlq(
        &self,
        request: Request<DeleteDlqRequest>,
    ) -> Result<Response<DeleteDlqResponse>, Status> {
        let id = request.into_inner().id;

        let success = repo::delete_dlq(&id)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        if success {
            Ok(Response::new(DeleteDlqResponse { success: true }))
        } else {
            Err(Status::not_found(format!("DLQ entry '{}' not found", id)))
        }
    }
}
