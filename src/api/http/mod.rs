pub mod calls;
pub mod chats;
pub mod contacts;
pub mod error;
pub mod group_participants;
pub mod health;
pub mod media;
pub mod messages;
pub mod openapi;
pub mod scan;
pub mod sessions;
pub mod state;

use crate::api::http::{openapi::ApiDoc, state::AppState};
use axum::{
    Router, middleware,
    routing::{delete, get, patch, post},
};
use std::sync::Arc;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub fn create_router(state: Arc<AppState>) -> Router {
    let public_routes = Router::new().route("/health", get(health::health_check));

    let view_routes = Router::new()
        .route("/sessions/{id}/scan", get(scan::scan_page))
        .route("/sessions/{id}/qr-frame", get(scan::qr_frame));

    let swagger_router: Router<Arc<AppState>> = SwaggerUi::new("/swagger")
        .url("/api-docs/openapi.json", ApiDoc::openapi())
        .into();

    let protected_swagger = swagger_router.layer(middleware::from_fn_with_state(
        state.clone(),
        crate::security::swagger_auth_middleware,
    ));

    let protected_routes = Router::new()
        // Sessions
        .route(
            "/sessions",
            get(sessions::list_sessions).post(sessions::create_session),
        )
        .route(
            "/sessions/{id}",
            get(sessions::get_session).delete(sessions::delete_session),
        )
        .route("/sessions/{id}/qr", post(sessions::refresh_qr))
        .route("/sessions/{id}/restart", post(sessions::restart_session))
        .route("/sessions/{id}/logout", post(sessions::logout_session))
        .route(
            "/sessions/{id}/pairing-code",
            post(sessions::request_pairing_code),
        )
        // Presence
        .route("/sessions/{id}/presence", post(chats::set_global_presence))
        // Chats
        .route("/sessions/{id}/chats", get(chats::list_chats))
        .route(
            "/sessions/{id}/chats/{jid}",
            get(chats::get_chat).delete(chats::delete_chat),
        )
        .route("/sessions/{id}/chats/{jid}/read", post(chats::mark_as_read))
        .route("/sessions/{id}/chats/{jid}/mute", post(chats::mute_chat))
        .route("/sessions/{id}/chats/{jid}/pin", post(chats::pin_chat))
        .route(
            "/sessions/{id}/chats/{jid}/archive",
            post(chats::archive_chat),
        )
        .route(
            "/sessions/{id}/chats/{jid}/subject",
            patch(chats::set_subject),
        )
        .route(
            "/sessions/{id}/chats/{jid}/description",
            patch(chats::set_description),
        )
        .route(
            "/sessions/{id}/chats/{jid}/invite-code",
            get(chats::get_invite_link),
        )
        .route("/sessions/{id}/chats/{jid}/leave", post(chats::leave_group))
        .route(
            "/sessions/{id}/chats/{jid}/presence",
            post(chats::set_chat_state),
        )
        // Groups
        .route("/sessions/{id}/groups", post(chats::create_group))
        // Messages
        .route(
            "/sessions/{id}/chats/{jid}/messages",
            get(messages::list_messages).post(messages::send_message),
        )
        .route(
            "/sessions/{id}/chats/{jid}/messages/media",
            post(messages::send_media),
        )
        .route(
            "/sessions/{id}/chats/{jid}/messages/{msg_id}",
            patch(messages::edit_message).delete(messages::delete_message),
        )
        .route(
            "/sessions/{id}/chats/{jid}/messages/{msg_id}/reaction",
            post(messages::react_message),
        )
        .route(
            "/sessions/{id}/chats/{jid}/messages/{msg_id}/revoke",
            post(messages::revoke_message),
        )
        // Group Participants
        .route(
            "/sessions/{id}/chats/{jid}/participants",
            get(group_participants::list_participants).post(group_participants::add_participants),
        )
        .route(
            "/sessions/{id}/chats/{jid}/participants/promote",
            post(group_participants::promote_participants),
        )
        .route(
            "/sessions/{id}/chats/{jid}/participants/demote",
            post(group_participants::demote_participants),
        )
        .route(
            "/sessions/{id}/chats/{jid}/participants/{user_jid}",
            delete(group_participants::remove_participant),
        )
        // Contacts (wacore contact sync, verification & actions)
        .route(
            "/sessions/{id}/contacts",
            get(contacts::list_contacts).post(contacts::upsert_contact),
        )
        .route(
            "/sessions/{id}/contacts/sync",
            post(contacts::sync_contacts),
        )
        .route(
            "/sessions/{id}/contacts/check",
            post(contacts::check_contacts),
        )
        .route(
            "/sessions/{id}/contacts/{jid}",
            get(contacts::get_contact).delete(contacts::delete_contact),
        )
        .route(
            "/sessions/{id}/contacts/{jid}/profile-picture",
            get(contacts::get_profile_picture),
        )
        .route(
            "/sessions/{id}/contacts/{jid}/block",
            post(contacts::block_contact),
        )
        .route(
            "/sessions/{id}/contacts/{jid}/unblock",
            post(contacts::unblock_contact),
        )
        // Calls
        .route("/sessions/{id}/calls", get(calls::list_calls))
        .route(
            "/sessions/{id}/calls/{call_id}",
            get(calls::get_call).delete(calls::delete_call),
        )
        // Media
        .route("/sessions/{id}/media/{sha256}", get(media::get_media))
        .route("/sessions/{id}/media/upload", post(media::upload_media))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            crate::security::auth_middleware,
        ));

    Router::new()
        .merge(public_routes)
        .merge(view_routes)
        .merge(protected_routes)
        .merge(protected_swagger)
        .with_state(state)
}
