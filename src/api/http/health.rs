use crate::api::http::{
    error::{AppError, AppResult, ErrorResponse},
    state::AppState,
};
use crate::models::health::HealthStatus;
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct LivenessResponse {
    pub status: String,
    pub uptime_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ReadinessResponse {
    pub status: String,
    pub database: String,
    pub active_sessions: usize,
    pub storage: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct StartupResponse {
    pub status: String,
    pub initialized: bool,
    pub version: String,
}

#[utoipa::path(
    get,
    path = "/health",
    tag = "Health",
    summary = "General health check",
    description = "Checks gateway health and database connectivity. Returns 503 if database is unreachable.",
    security(()),
    responses(
        (status = 200, description = "Gateway operational status", body = HealthStatus),
        (status = 503, description = "Service degraded / database unreachable", body = ErrorResponse)
    )
)]
pub async fn health_check(State(state): State<Arc<AppState>>) -> AppResult<Json<HealthStatus>> {
    match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => Ok(Json(HealthStatus {
            status: "ok".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            database: "connected".to_string(),
        })),
        Err(err) => Err(AppError::Internal(anyhow::anyhow!(
            "Database connectivity failure: {}",
            err
        ))),
    }
}

#[utoipa::path(
    get,
    path = "/health/live",
    tag = "Health",
    summary = "Liveness probe",
    description = "Zero-overhead event-loop liveness check for container orchestrators (Kubernetes / Docker).",
    security(()),
    responses(
        (status = 200, description = "Process event loop is responsive", body = LivenessResponse)
    )
)]
pub async fn liveness_check() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(LivenessResponse {
            status: "live".to_string(),
            uptime_seconds: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }),
    )
}

#[utoipa::path(
    get,
    path = "/health/ready",
    tag = "Health",
    summary = "Readiness probe",
    description = "Deep dependency validation checking database query execution, active session memory, and storage availability. Returns 503 on degraded dependencies.",
    security(()),
    responses(
        (status = 200, description = "Gateway ready to accept traffic", body = ReadinessResponse),
        (status = 503, description = "Gateway dependencies degraded", body = ReadinessResponse)
    )
)]
pub async fn readiness_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let db_ok = sqlx::query("SELECT 1").execute(&state.pool).await.is_ok();
    let storage_ok = tokio::fs::create_dir_all(&state.config.media.dir).await.is_ok();
    let active_sessions = state.sessions.active_count().await;

    let is_ready = db_ok && storage_ok;
    let status_code = if is_ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(ReadinessResponse {
            status: if is_ready { "ready".to_string() } else { "degraded".to_string() },
            database: if db_ok { "connected".to_string() } else { "unreachable".to_string() },
            active_sessions,
            storage: if storage_ok { "accessible".to_string() } else { "inaccessible".to_string() },
        }),
    )
}

#[utoipa::path(
    get,
    path = "/health/startup",
    tag = "Health",
    summary = "Startup probe",
    description = "Validates schema initialization and service readiness prior to admitting initial traffic.",
    security(()),
    responses(
        (status = 200, description = "Service successfully initialized", body = StartupResponse),
        (status = 503, description = "Service startup pending or failed", body = StartupResponse)
    )
)]
pub async fn startup_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let db_migrated = sqlx::query("SELECT count(*) FROM sessions")
        .execute(&state.pool)
        .await
        .is_ok();

    let status_code = if db_migrated {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status_code,
        Json(StartupResponse {
            status: if db_migrated { "initialized".to_string() } else { "uninitialized".to_string() },
            initialized: db_migrated,
            version: env!("CARGO_PKG_VERSION").to_string(),
        }),
    )
}
