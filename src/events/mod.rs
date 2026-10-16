pub mod bus;
pub mod extractor;
pub mod handlers;
pub mod outbox_worker;
pub mod webhook;

use crate::api::session_manager::SessionManager;
use std::sync::Arc;
use whatsapp_rust::prelude::*;
use whatsapp_rust::types::events::Event;

/// Main event router that dispatches incoming WhatsApp events to specialized handlers
pub async fn dispatch(
    session_id: String,
    event: Arc<Event>,
    client: Arc<Client>,
    session_mgr: SessionManager,
) {
    let result = match &*event {
        Event::Messages(batch) => handlers::messages::handle(&session_id, batch, &client).await,
        Event::Receipt(receipt) => handlers::receipts::handle(&session_id, receipt).await,
        Event::GroupUpdate(update) => handlers::groups::handle(&session_id, update).await,
        Event::IncomingCall(call) => handlers::calls::handle(&session_id, call).await,
        Event::HistorySync(lazy_sync) => {
            handlers::history_sync::handle(&session_id, lazy_sync, &client).await
        }
        Event::ContactUpdated(update) => {
            handlers::contacts::handle_updated(&session_id, update).await
        }
        Event::ContactNumberChanged(update) => {
            handlers::contacts::handle_changed(&session_id, update).await
        }
        Event::Connected(_) => handlers::lifecycle::handle_connected(&session_id, &client).await,
        Event::LoggedOut(_) => handlers::lifecycle::handle_logged_out(&session_id, &session_mgr).await,
        Event::PairingQrCode(qr) => handlers::lifecycle::handle_qr(&session_id, &qr.code).await,
        Event::PairingQrCodesExhausted(_) => {
            handlers::lifecycle::handle_qr_exhausted(&session_id).await
        }
        Event::Disconnected(_) => handlers::lifecycle::handle_disconnected(&session_id).await,
        Event::StreamReplaced(_) => {
            handlers::lifecycle::handle_stream_replaced(&session_id, &session_mgr).await
        }
        Event::StreamError(err) => {
            let code = err.code.parse::<u16>().ok();
            handlers::lifecycle::handle_stream_error(&session_id, code, &session_mgr).await
        }
        _ => Ok(()),
    };

    if let Err(err) = result {
        tracing::error!(
            session_id = %session_id,
            error = %err,
            "Error processing WhatsApp event"
        );
    }
}
