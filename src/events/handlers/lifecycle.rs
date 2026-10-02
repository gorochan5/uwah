use crate::api::session_manager::SessionManager;
use crate::models::sessions::Session;
use crate::repository::sessions as repo;
use anyhow::Result;
use tracing::{error, info, warn};
use whatsapp_rust::prelude::*;

pub async fn handle_connected(session_id: &str, client: &Client) -> Result<()> {
    let phone_number = client.pn().map(|jid| jid.user.to_string());
    let push_name = {
        let name = client.push_name();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    };

    info!(
        session_id = %session_id,
        phone = ?phone_number,
        push_name = ?push_name,
        "WhatsApp session successfully authenticated"
    );

    let session_update = Session {
        id: session_id.to_string(),
        phone_number,
        push_name,
        status: "CONNECTED".to_string(),
        qr_payload: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    if let Err(err) = repo::auth(session_update.clone()).await {
        error!(
            session_id = %session_id,
            error = %err,
            "Failed to persist CONNECTED status in database"
        );
    } else {
        crate::events::webhook::dispatch_event("session.connected", session_id, session_update);
    }

    Ok(())
}

pub async fn handle_logged_out(
    session_id: &str,
    session_mgr: &SessionManager,
) -> Result<()> {
    error!(session_id = %session_id, "WhatsApp session was unlinked/logged out (Code 401)");
    session_mgr.unregister_session(session_id).await;

    let session_update = Session {
        id: session_id.to_string(),
        phone_number: None,
        push_name: None,
        status: "LOGGED_OUT".to_string(),
        qr_payload: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    if let Err(err) = repo::auth(session_update.clone()).await {
        error!(
            session_id = %session_id,
            error = %err,
            "Failed to persist LOGGED_OUT status in database"
        );
    } else {
        crate::events::webhook::dispatch_event("session.logged_out", session_id, session_update);
    }

    Ok(())
}

pub async fn handle_qr(session_id: &str, code: &str) -> Result<()> {
    info!(
        session_id = %session_id,
        "Persisting refreshed QR payload from WhatsApp"
    );
    if let Err(err) = repo::refresh_qr(session_id, code).await {
        error!(
            session_id = %session_id,
            error = %err,
            "Failed to update QR payload in database"
        );
    } else {
        crate::events::webhook::dispatch_event(
            "session.qr",
            session_id,
            serde_json::json!({ "qr": code }),
        );
    }
    Ok(())
}

pub async fn handle_qr_exhausted(session_id: &str) -> Result<()> {
    warn!(session_id = %session_id, "QR pairing attempts exhausted");
    let _ = repo::update_status(session_id, "QR_EXHAUSTED").await;
    let _ = repo::refresh_qr(session_id, "").await;

    crate::events::webhook::dispatch_event(
        "session.qr_exhausted",
        session_id,
        serde_json::json!({ "status": "QR_EXHAUSTED" }),
    );
    Ok(())
}

pub async fn handle_stream_replaced(
    session_id: &str,
    session_mgr: &SessionManager,
) -> Result<()> {
    error!(
        session_id = %session_id,
        "WhatsApp stream replaced / conflict (Code 409): another instance connected"
    );
    session_mgr.unregister_session(session_id).await;
    let _ = repo::update_status(session_id, "CONFLICT").await;

    crate::events::webhook::dispatch_event(
        "session.stream_replaced",
        session_id,
        serde_json::json!({ "status": "CONFLICT", "reason": "Stream replaced by another connection" }),
    );
    Ok(())
}

pub async fn handle_stream_error(
    session_id: &str,
    code: Option<u16>,
    session_mgr: &SessionManager,
) -> Result<()> {
    warn!(
        session_id = %session_id,
        code = ?code,
        "Received WhatsApp stream error"
    );

    match code {
        Some(401) => {
            handle_logged_out(session_id, session_mgr).await?;
        }
        Some(408) => {
            let _ = repo::update_status(session_id, "RECONNECTING").await;
            crate::events::webhook::dispatch_event(
                "session.reconnecting",
                session_id,
                serde_json::json!({ "status": "RECONNECTING", "code": 408 }),
            );
        }
        Some(409) => {
            handle_stream_replaced(session_id, session_mgr).await?;
        }
        Some(429) => {
            warn!(session_id = %session_id, "Session rate-limited by WhatsApp (Code 429)");
            crate::events::webhook::dispatch_event(
                "session.rate_limited",
                session_id,
                serde_json::json!({ "status": "RATE_LIMITED", "code": 429 }),
            );
        }
        _ => {
            crate::events::webhook::dispatch_event(
                "session.stream_error",
                session_id,
                serde_json::json!({ "code": code }),
            );
        }
    }

    Ok(())
}

pub async fn handle_disconnected(session_id: &str) -> Result<()> {
    info!(session_id = %session_id, "WhatsApp socket disconnected");
    crate::events::webhook::dispatch_event(
        "session.disconnected",
        session_id,
        serde_json::json!({ "status": "DISCONNECTED" }),
    );
    Ok(())
}
