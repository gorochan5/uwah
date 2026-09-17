pub mod calls;
pub mod chats;
pub mod contacts;
pub mod error;
pub mod group_participants;
pub mod health;
pub mod media;
pub mod messages;
pub mod scan;
pub mod sessions;
pub mod state;

use axum::{
    middleware,
    routing::{delete, get, post},
    Router,
};
use std::sync::Arc;
use crate::api::http::state::AppState;

pub fn create_router(state: Arc<AppState>) -> Router {
    let public_routes = Router::new()
        .route("/health", get(health::health_check));

    let view_routes = Router::new()
        .route("/sessions/{id}/scan", get(scan::scan_page))
        .route("/sessions/{id}/qr-frame", get(scan::qr_frame));

    let protected_routes = Router::new()
        // Sessions
        .route("/sessions", get(sessions::list_sessions).post(sessions::create_session))
        .route("/sessions/{id}", get(sessions::get_session).delete(sessions::delete_session))
        .route("/sessions/{id}/qr", post(sessions::refresh_qr))
        // Chats
        .route("/sessions/{id}/chats", get(chats::list_chats))
        .route("/sessions/{id}/chats/{jid}", get(chats::get_chat).delete(chats::delete_chat))
        // Messages
        .route("/sessions/{id}/chats/{jid}/messages", get(messages::list_messages).post(messages::send_message))
        .route("/sessions/{id}/chats/{jid}/messages/{msg_id}", delete(messages::delete_message))
        // Group Participants
        .route("/sessions/{id}/chats/{jid}/participants", get(group_participants::list_participants))
        .route("/sessions/{id}/chats/{jid}/participants/{user_jid}", delete(group_participants::remove_participant))
        // Contacts (wacore contact sync & queries)
        .route("/sessions/{id}/contacts", get(contacts::list_contacts).post(contacts::upsert_contact))
        .route("/sessions/{id}/contacts/sync", post(contacts::sync_contacts))
        .route("/sessions/{id}/contacts/{jid}", get(contacts::get_contact).delete(contacts::delete_contact))
        // Calls
        .route("/sessions/{id}/calls", get(calls::list_calls))
        .route("/sessions/{id}/calls/{call_id}", get(calls::get_call).delete(calls::delete_call))
        // Media
        .route("/sessions/{id}/media/{sha256}", get(media::get_media))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::security::auth_middleware,
        ));

    Router::new()
        .merge(public_routes)
        .merge(view_routes)
        .merge(protected_routes)
        .with_state(state)
}
