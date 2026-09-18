use axum::{extract::State, Json};
use std::sync::Arc;
use crate::api::http::{
    error::{AppResult, ErrorResponse},
    state::AppState,
};
use crate::models::health::HealthStatus;

#[utoipa::path(
    get,
    path = "/health",
    tag = "Health",
    summary = "Health check",
    description = "Checks gateway health and database connectivity.",
    security(()),
    responses(
        (status = 200, description = "Gateway operational status", body = HealthStatus),
        (status = 500, description = "Internal or database error", body = ErrorResponse)
    )
)]
pub async fn health_check(
    State(state): State<Arc<AppState>>,
) -> AppResult<Json<HealthStatus>> {
    let db_status = match sqlx::query("SELECT 1").execute(&state.pool).await {
        Ok(_) => "connected".to_string(),
        Err(err) => format!("error: {}", err),
    };

    Ok(Json(HealthStatus {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        database: db_status,
    }))
}

