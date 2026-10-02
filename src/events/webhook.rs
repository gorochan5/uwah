use crate::security::{SIGNATURE_HEADER, sign_payload};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::time::Duration;
use tracing::{debug, error, warn};
use utoipa::ToSchema;

/// Reusable HTTP client configured with a 5-second socket & connect timeout.
static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(3))
        .build()
        .expect("Failed to initialize static webhook reqwest client")
});

/// Standardized webhook event envelope dispatched to subscribed consumers.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct WebhookPayload<T> {
    pub event: String,
    pub session_id: String,
    pub timestamp: i64,
    pub data: T,
}

/// Dispatches an event asynchronously to the configured webhook endpoint.
///
/// Execution is non-blocking: serialized and spawned onto a detached Tokio task.
/// If webhooks are disabled or no URL is configured, returns immediately without heap allocation.
pub fn dispatch_event<T>(event_type: &str, session_id: &str, payload: T)
where
    T: Serialize + Send + 'static,
{
    let event = event_type.to_string();
    let sid = session_id.to_string();
    let timestamp = chrono::Utc::now().timestamp_millis();

    let payload_val = serde_json::to_value(&payload).unwrap_or(serde_json::Value::Null);
    crate::events::bus::publish(&sid, &event, payload_val);

    let config = crate::config::get();
    let webhook_cfg = match &config.webhook {
        Some(w) if w.enabled => w,
        _ => return,
    };

    let url = match &webhook_cfg.url {
        Some(u) if !u.trim().is_empty() => u.clone(),
        _ => return,
    };

    let secret = webhook_cfg.secret.clone();

    let envelope = WebhookPayload {
        event: event.clone(),
        session_id: sid.clone(),
        timestamp,
        data: payload,
    };

    // Serialize payload to JSON bytes
    let body_bytes = match serde_json::to_vec(&envelope) {
        Ok(bytes) => bytes,
        Err(err) => {
            error!(
                event = %event,
                session_id = %sid,
                error = %err,
                "Failed to serialize webhook payload to JSON"
            );
            return;
        }
    };

    // Generate HMAC-SHA256 signature if secret is provided (FAIL-CLOSED)
    let signature_opt = if let Some(sec) = secret.as_deref() {
        match sign_payload(sec, timestamp, &body_bytes) {
            Ok(sig) => Some(sig),
            Err(err) => {
                error!(
                    event = %event,
                    session_id = %sid,
                    error = %err,
                    "CRITICAL: Failed to sign webhook payload in fail-closed mode; aborting dispatch"
                );
                return;
            }
        }
    } else {
        None
    };

    // Detached asynchronous HTTP POST dispatch
    tokio::spawn(async move {
        let mut request = HTTP_CLIENT
            .post(&url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::USER_AGENT, "Uwah-Webhook-Dispatcher/0.1.0")
            .body(body_bytes);

        if let Some(sig) = signature_opt {
            request = request.header(SIGNATURE_HEADER, sig);
        }

        match request.send().await {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    debug!(
                        event = %event,
                        session_id = %sid,
                        status = %status,
                        "Webhook event successfully acknowledged"
                    );
                } else {
                    warn!(
                        event = %event,
                        session_id = %sid,
                        status = %status,
                        "Webhook endpoint returned non-2xx status"
                    );
                }
            }
            Err(err) => {
                error!(
                    event = %event,
                    session_id = %sid,
                    error = %err,
                    "Failed to deliver webhook HTTP request (timeout or connection error)"
                );
            }
        }
    });
}

