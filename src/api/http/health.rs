use axum::{extract::State, Json};
use std::sync::Arc;
use crate::api::http::{error::AppResult, state::AppState};
use crate::models::health::HealthStatus;

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

