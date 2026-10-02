use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use crate::api::http::state::AppState;

static MESSAGES_INBOUND: AtomicU64 = AtomicU64::new(0);
static MESSAGES_OUTBOUND: AtomicU64 = AtomicU64::new(0);
static WEBHOOKS_DISPATCHED: AtomicU64 = AtomicU64::new(0);
static WEBHOOKS_FAILED: AtomicU64 = AtomicU64::new(0);
static START_TIME: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);

pub fn inc_messages_inbound() {
    MESSAGES_INBOUND.fetch_add(1, Ordering::Relaxed);
}

pub fn inc_messages_outbound() {
    MESSAGES_OUTBOUND.fetch_add(1, Ordering::Relaxed);
}

pub fn inc_webhooks_dispatched() {
    WEBHOOKS_DISPATCHED.fetch_add(1, Ordering::Relaxed);
}

pub fn inc_webhooks_failed() {
    WEBHOOKS_FAILED.fetch_add(1, Ordering::Relaxed);
}

pub async fn generate_metrics_text(state: &AppState) -> String {
    let _ = *START_TIME; // ensure initialized
    let uptime_secs = START_TIME.elapsed().as_secs();
    let inbound = MESSAGES_INBOUND.load(Ordering::Relaxed);
    let outbound = MESSAGES_OUTBOUND.load(Ordering::Relaxed);
    let webhooks_ok = WEBHOOKS_DISPATCHED.load(Ordering::Relaxed);
    let webhooks_err = WEBHOOKS_FAILED.load(Ordering::Relaxed);

    // Active session count
    let active_sessions = match crate::repository::sessions::get_all().await {
        Ok(sessions) => {
            let mut active = 0;
            for s in sessions {
                if state.sessions.is_running(&s.id).await {
                    active += 1;
                }
            }
            active
        }
        Err(_) => 0,
    };

    let mut body = String::new();
    body.push_str("# HELP uwah_uptime_seconds Process uptime in seconds\n");
    body.push_str("# TYPE uwah_uptime_seconds counter\n");
    body.push_str(&format!("uwah_uptime_seconds {}\n", uptime_secs));

    body.push_str("# HELP uwah_sessions_active Number of active connected sessions\n");
    body.push_str("# TYPE uwah_sessions_active gauge\n");
    body.push_str(&format!("uwah_sessions_active {}\n", active_sessions));

    body.push_str("# HELP uwah_messages_inbound_total Total inbound messages received\n");
    body.push_str("# TYPE uwah_messages_inbound_total counter\n");
    body.push_str(&format!("uwah_messages_inbound_total {}\n", inbound));

    body.push_str("# HELP uwah_messages_outbound_total Total outbound messages sent\n");
    body.push_str("# TYPE uwah_messages_outbound_total counter\n");
    body.push_str(&format!("uwah_messages_outbound_total {}\n", outbound));

    body.push_str("# HELP uwah_webhooks_dispatched_total Total webhook deliveries succeeded\n");
    body.push_str("# TYPE uwah_webhooks_dispatched_total counter\n");
    body.push_str(&format!("uwah_webhooks_dispatched_total {}\n", webhooks_ok));

    body.push_str("# HELP uwah_webhooks_failed_total Total webhook deliveries failed\n");
    body.push_str("# TYPE uwah_webhooks_failed_total counter\n");
    body.push_str(&format!("uwah_webhooks_failed_total {}\n", webhooks_err));

    body
}

#[utoipa::path(
    get,
    path = "/metrics",
    tag = "Health",
    responses(
        (status = 200, description = "Prometheus-formatted operational metrics", body = String)
    )
)]
pub async fn export_metrics(State(state): State<Arc<AppState>>) -> Response {
    let body = generate_metrics_text(&state).await;

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        body,
    )
        .into_response()
}
