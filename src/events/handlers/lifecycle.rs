use crate::models::sessions::Session;
use crate::repository::sessions as repo;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use whatsapp_rust::prelude::*;

pub async fn handle_connected(session_id: &str, client: &Client) -> Result<()> {
    let phone_number = client.pn().map(|jid| jid.user.to_string());
    let push_name = {
        let name = client.push_name();
        if name.is_empty() { None } else { Some(name) }
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
    clients: Arc<RwLock<HashMap<String, Arc<Client>>>>,
) -> Result<()> {
    error!(session_id = %session_id, "WhatsApp session was unlinked/logged out");
    clients.write().await.remove(session_id);

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
