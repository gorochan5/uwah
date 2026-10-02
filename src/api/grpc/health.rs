use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::wrappers::IntervalStream;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status};

use crate::api::grpc::proto::health_service_server::HealthService;
use crate::api::grpc::proto::{
    ExportMetricsRequest, ExportMetricsResponse, HealthCheckRequest, HealthCheckResponse,
    LivenessRequest, LivenessResponse, ReadinessRequest, ReadinessResponse, StartupRequest,
    StartupResponse,
};
use crate::api::http::state::AppState;

pub struct HealthServiceImpl {
    state: Arc<AppState>,
}

impl HealthServiceImpl {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl HealthService for HealthServiceImpl {
    async fn check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let is_ok = sqlx::query("SELECT 1")
            .execute(&self.state.pool)
            .await
            .is_ok();

        let (status, msg) = if is_ok {
            (1, "Gateway is SERVING and healthy".to_string())
        } else {
            (2, "Database connectivity failure".to_string())
        };

        Ok(Response::new(HealthCheckResponse {
            status,
            message: msg,
        }))
    }

    type WatchStream = Pin<
        Box<dyn Stream<Item = Result<HealthCheckResponse, Status>> + Send + 'static>,
    >;

    async fn watch(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        let pool = self.state.pool.clone();
        let interval = tokio::time::interval(std::time::Duration::from_secs(5));
        let stream = IntervalStream::new(interval).then(move |_| {
            let pool = pool.clone();
            async move {
                let is_ok = sqlx::query("SELECT 1").execute(&pool).await.is_ok();
                let (status, msg) = if is_ok {
                    (1, "SERVING".to_string())
                } else {
                    (2, "NOT_SERVING".to_string())
                };
                Ok(HealthCheckResponse {
                    status,
                    message: msg,
                })
            }
        });

        Ok(Response::new(Box::pin(stream)))
    }

    async fn check_liveness(
        &self,
        _request: Request<LivenessRequest>,
    ) -> Result<Response<LivenessResponse>, Status> {
        Ok(Response::new(LivenessResponse {
            status: "UP".to_string(),
            timestamp: chrono::Utc::now().timestamp(),
        }))
    }

    async fn check_readiness(
        &self,
        _request: Request<ReadinessRequest>,
    ) -> Result<Response<ReadinessResponse>, Status> {
        let db_ok = sqlx::query("SELECT 1")
            .execute(&self.state.pool)
            .await
            .is_ok();
        let active_sessions = self.state.sessions.active_count().await as i32;

        let status = if db_ok { "UP" } else { "DOWN" }.to_string();

        Ok(Response::new(ReadinessResponse {
            status,
            database: db_ok,
            active_sessions,
            timestamp: chrono::Utc::now().timestamp(),
        }))
    }

    async fn check_startup(
        &self,
        _request: Request<StartupRequest>,
    ) -> Result<Response<StartupResponse>, Status> {
        let db_ok = sqlx::query("SELECT 1")
            .execute(&self.state.pool)
            .await
            .is_ok();

        let status = if db_ok { "UP" } else { "DOWN" }.to_string();

        Ok(Response::new(StartupResponse {
            status,
            database_initialized: db_ok,
            timestamp: chrono::Utc::now().timestamp(),
        }))
    }

    async fn export_metrics(
        &self,
        _request: Request<ExportMetricsRequest>,
    ) -> Result<Response<ExportMetricsResponse>, Status> {
        let metrics_text = crate::api::http::metrics::generate_metrics_text(&self.state).await;
        Ok(Response::new(ExportMetricsResponse {
            metrics: metrics_text,
        }))
    }
}
