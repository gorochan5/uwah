use crate::models::calls::Call;
use anyhow::Result;
use tracing::{debug, error, info};
use whatsapp_rust::wacore::types::call::{CallAction, IncomingCall};

pub async fn handle(session_id: &str, call: &IncomingCall) -> Result<()> {
    match &call.action {
        CallAction::Offer {
            call_id,
            call_creator,
            is_video,
            group_jid,
            ..
        } => {
            let chat_jid = group_jid
                .as_ref()
                .map(|j| j.to_string())
                .unwrap_or_else(|| call.from.to_string());
            let caller_jid = call_creator.to_string();

            info!(
                session_id = %session_id,
                call_id = %call_id,
                chat = %chat_jid,
                caller = %caller_jid,
                is_video = %is_video,
                "Incoming WhatsApp call offered"
            );

            // Ensure chat exists
            let _ = crate::repository::chats::ensure_exists(&chat_jid, session_id).await;

            let db_call = Call {
                session_id: session_id.to_string(),
                id: call_id.clone(),
                chat_jid,
                caller_jid,
                is_video: *is_video,
                is_group_call: group_jid.is_some(),
                status: "offered".to_string(),
                duration_seconds: None,
                created_at: call.timestamp,
                ended_at: None,
            };

            if let Err(err) = crate::repository::calls::upsert(&db_call).await {
                error!(
                    session_id = %session_id,
                    call_id = %call_id,
                    error = %err,
                    "Failed to record offered call in DB"
                );
            } else {
                crate::events::webhook::dispatch_event(
                    "call.received",
                    session_id,
                    db_call.clone(),
                );
            }
        }
        CallAction::Accept { call_id, .. } => {
            debug!(
                session_id = %session_id,
                call_id = %call_id,
                "WhatsApp call accepted"
            );
            let _ = crate::repository::calls::update_status(
                session_id, call_id, "accepted", None, None,
            )
            .await;
        }
        CallAction::Reject { call_id, .. } => {
            debug!(
                session_id = %session_id,
                call_id = %call_id,
                "WhatsApp call rejected"
            );
            let _ = crate::repository::calls::update_status(
                session_id,
                call_id,
                "rejected",
                None,
                Some(call.timestamp),
            )
            .await;
        }
        CallAction::Terminate {
            call_id, duration, ..
        } => {
            debug!(
                session_id = %session_id,
                call_id = %call_id,
                duration = ?duration,
                "WhatsApp call terminated"
            );
            let _ = crate::repository::calls::update_status(
                session_id,
                call_id,
                "ended",
                duration.map(|d| d as i64),
                Some(call.timestamp),
            )
            .await;
        }
        _ => {}
    }

    Ok(())
}
