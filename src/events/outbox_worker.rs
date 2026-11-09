use crate::models::outbox::OutboxEvent;
use crate::repository::outbox as repo;
use crate::security::{SIGNATURE_HEADER, sign_payload};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

static HTTP_CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .connect_timeout(Duration::from_secs(3))
        .build()
        .expect("Failed to initialize outbox reqwest client")
});

pub fn start_outbox_daemon(cancel_token: CancellationToken) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("Starting Supervised Webhook Outbox Dispatcher Daemon");
        let semaphore = Arc::new(Semaphore::new(20));
        let mut dispatches = tokio::task::JoinSet::new();

        while !cancel_token.is_cancelled() {
            while dispatches.try_join_next().is_some() {}
            if dispatches.len() >= 20 {
                tokio::select! {
                    _ = cancel_token.cancelled() => break,
                    result = dispatches.join_next() => {
                        if let Some(Err(err)) = result {
                            warn!(error = %err, "Outbox dispatch task failed");
                        }
                    }
                }
                continue;
            }
            let config = crate::config::get();
            let webhook_cfg = match &config.webhook {
                Some(w) if w.enabled => w,
                _ => {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };

            let url = match &webhook_cfg.url {
                Some(u) if !u.trim().is_empty() => u.clone(),
                _ => {
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    continue;
                }
            };

            let secret = webhook_cfg.secret.clone();

            match repo::poll_pending(20).await {
                Ok(events) if !events.is_empty() => {
                    for event in events {
                        let sem = Arc::clone(&semaphore);
                        let target_url = url.clone();
                        let sec = secret.clone();

                        dispatches.spawn(async move {
                            let _permit = match sem.acquire().await {
                                Ok(p) => p,
                                Err(_) => return,
                            };
                            dispatch_outbox_event(event, &target_url, sec.as_deref()).await;
                        });
                    }
                }
                Ok(_) => {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
                Err(err) => {
                    error!(error = %err, "Failed to poll outbox events from DB");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }

        while let Some(result) = dispatches.join_next().await {
            if let Err(err) = result {
                warn!(error = %err, "Outbox dispatch task failed");
            }
        }

        info!("Webhook Outbox Dispatcher Daemon stopped.");
    })
}

async fn dispatch_outbox_event(event: OutboxEvent, url: &str, secret: Option<&str>) {
    let payload_bytes = event.payload.as_bytes();
    let timestamp = chrono::Utc::now().timestamp_millis();

    let signature_opt = if let Some(sec) = secret {
        match sign_payload(sec, timestamp, payload_bytes) {
            Ok(sig) => Some(sig),
            Err(err) => {
                error!(
                    event_id = %event.id,
                    error = %err,
                    "Failed to sign outbox payload in fail-closed mode; aborting dispatch"
                );
                let _ = repo::move_to_dlq(&event.id, &format!("Signature generation failed: {}", err)).await;
                return;
            }
        }
    } else {
        None
    };

    let mut request = HTTP_CLIENT
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(reqwest::header::USER_AGENT, "Uwah-Outbox-Dispatcher/0.2.0")
        .body(payload_bytes.to_vec());

    if let Some(sig) = signature_opt {
        request = request.header(SIGNATURE_HEADER, sig);
    }

    match request.send().await {
        Ok(resp) if resp.status().is_success() => {
            debug!(event_id = %event.id, "Outbox event dispatched successfully");
            crate::api::http::metrics::inc_webhooks_dispatched();
            let _ = repo::mark_dispatched(&event.id).await;
        }
        Ok(resp) => {
            crate::api::http::metrics::inc_webhooks_failed();
            let status_code = resp.status().as_u16();
            let err_msg = format!("HTTP {}", status_code);
            handle_failure(event, &err_msg, (400..500).contains(&status_code) && status_code != 429).await;
        }
        Err(err) => {
            crate::api::http::metrics::inc_webhooks_failed();
            let err_msg = err.to_string();
            handle_failure(event, &err_msg, false).await;
        }
    }
}

async fn handle_failure(event: OutboxEvent, error_msg: &str, permanent: bool) {
    let next_retry = event.retry_count + 1;
    if permanent || next_retry >= 10 {
        warn!(
            event_id = %event.id,
            retry_count = next_retry,
            reason = %error_msg,
            "Moving outbox event to Dead-Letter Queue (DLQ)"
        );
        let _ = repo::move_to_dlq(&event.id, error_msg).await;
    } else {
        let max_backoff_ms = (1000u64 * 2u64.pow(next_retry as u32)).min(900_000);
        let jitter = (next_retry as u64 * 313) % max_backoff_ms;
        let delay_ms = (max_backoff_ms / 2) + jitter;
        let next_retry_at = chrono::Utc::now().timestamp_millis() + delay_ms as i64;

        warn!(
            event_id = %event.id,
            retry_count = next_retry,
            delay_ms = delay_ms,
            error = %error_msg,
            "Outbox dispatch failed; scheduling retry"
        );
        let _ = repo::mark_failed(&event.id, next_retry, next_retry_at, error_msg).await;
    }
}
