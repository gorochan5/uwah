use utoipa::{
    openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme},
    Modify, OpenApi,
};

use crate::api::http::{
    calls, chats, contacts,
    contacts::{SyncContactsRequest, SyncContactsResponse, UpsertContactRequest},
    error::{ErrorDetail, ErrorResponse},
    group_participants, health, media, messages,
    messages::SendMessageRequest,
    scan, sessions,
    sessions::{CreateSessionRequest, RefreshQrRequest},
};
use crate::models::{
    calls::Call, chats::Chat, contacts::Contact, group_participants::GroupParticipant,
    health::HealthStatus, media_cache::MediaCache, messages::Message, sessions::Session,
};

pub struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "bearer_auth",
                SecurityScheme::Http(
                    HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("Token")
                        .description(Some("SHA-256 or plaintext auth token configured in uwah.toml"))
                        .build(),
                ),
            );
            components.add_security_scheme(
                "api_key_query",
                SecurityScheme::ApiKey(ApiKey::Query(ApiKeyValue::new("key"))),
            );
        }
    }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Uwah WhatsApp Gateway API",
        version = "0.1.0",
        description = "High-performance self-hosted multi-session WhatsApp Gateway REST API powered by whatsapp-rust and Axum.",
        contact(name = "Uwah Gateway Contributors")
    ),
    paths(
        health::health_check,
        sessions::list_sessions,
        sessions::create_session,
        sessions::get_session,
        sessions::delete_session,
        sessions::refresh_qr,
        scan::scan_page,
        chats::list_chats,
        chats::get_chat,
        chats::delete_chat,
        messages::list_messages,
        messages::send_message,
        messages::delete_message,
        contacts::list_contacts,
        contacts::upsert_contact,
        contacts::sync_contacts,
        contacts::get_contact,
        contacts::delete_contact,
        group_participants::list_participants,
        group_participants::remove_participant,
        calls::list_calls,
        calls::get_call,
        calls::delete_call,
        media::get_media,
    ),
    components(
        schemas(
            HealthStatus,
            Session,
            CreateSessionRequest,
            RefreshQrRequest,
            Chat,
            Message,
            SendMessageRequest,
            Contact,
            UpsertContactRequest,
            SyncContactsRequest,
            SyncContactsResponse,
            GroupParticipant,
            Call,
            MediaCache,
            ErrorResponse,
            ErrorDetail,
        )
    ),
    modifiers(&SecurityAddon),
    security(
        ("bearer_auth" = []),
        ("api_key_query" = [])
    ),
    tags(
        (name = "Health", description = "Gateway liveness and health diagnostics"),
        (name = "Sessions", description = "WhatsApp multi-session lifecycle management & pairing"),
        (name = "Chats", description = "Conversation threads, groups, and newsletters"),
        (name = "Messages", description = "Real-time message transmission, history, and replies"),
        (name = "Contacts", description = "Contact address book & synchronization"),
        (name = "Groups", description = "Group chat participants & administration"),
        (name = "Calls", description = "Voice and video call history"),
        (name = "Media", description = "Cached media access and deduplication")
    )
)]
pub struct ApiDoc;


