use utoipa::{
    Modify, OpenApi,
    openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme},
};

use crate::api::http::{
    calls, chats,
    chats::{
        ArchiveChatRequest, ChatActionResponse, CreateGroupRequest, CreateGroupResponse,
        GlobalPresenceResponse, GroupInviteLinkResponse, GroupParticipantItem,
        JoinGroupRequest, JoinGroupResponse, MarkChatAsReadRequest, MuteChatRequest,
        PinChatRequest, SetAnnouncementRequest, SetChatStateRequest, SetDescriptionRequest,
        SetEphemeralRequest, SetGlobalPresenceRequest, SetLockRequest, SetSubjectRequest,
    },
    contacts,
    contacts::{
        CheckContactsRequest, CheckContactsResponse, ContactStatusItem, ProfilePictureResponse,
        SyncContactsRequest, SyncContactsResponse, UpsertContactRequest,
    },
    error::{ErrorDetail, ErrorResponse},
    group_participants,
    group_participants::{
        AddParticipantsRequest, ModifyParticipantRolesRequest, ParticipantActionResponse,
        ParticipantActionResult,
    },
    health, media,
    media::UploadMediaResponse,
    messages, metrics,
    messages::{
        EditMessageRequest, MessageActionResponse, ReactMessageRequest, SendMediaRequest,
        SendMessageRequest,
    },
    scan, sessions,
    sessions::{
        CreateSessionRequest, PairingCodeRequest, PairingCodeResponse, RefreshQrRequest,
        SessionActionResponse,
    },
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
                        .description(Some(
                            "SHA-256 or plaintext auth token configured in uwah.toml",
                        ))
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
        health::liveness_check,
        health::readiness_check,
        health::startup_check,
        metrics::export_metrics,
        sessions::list_sessions,
        sessions::create_session,
        sessions::get_session,
        sessions::delete_session,
        sessions::refresh_qr,
        sessions::restart_session,
        sessions::logout_session,
        sessions::request_pairing_code,
        scan::scan_page,
        scan::qr_frame,
        chats::list_chats,
        chats::get_chat,
        chats::delete_chat,
        chats::mark_as_read,
        chats::mute_chat,
        chats::pin_chat,
        chats::archive_chat,
        chats::create_group,
        chats::join_group,
        chats::set_subject,
        chats::set_description,
        chats::set_ephemeral_timer,
        chats::set_group_announcement,
        chats::set_group_lock,
        chats::get_invite_link,
        chats::leave_group,
        chats::set_chat_state,
        chats::set_global_presence,
        messages::list_messages,
        messages::send_message,
        messages::send_location,
        messages::send_contact,
        messages::send_media,
        messages::react_message,
        messages::revoke_message,
        messages::edit_message,
        messages::delete_message,
        contacts::list_contacts,
        contacts::upsert_contact,
        contacts::sync_contacts,
        contacts::get_contact,
        contacts::delete_contact,
        contacts::check_contacts,
        contacts::get_profile_picture,
        contacts::block_contact,
        contacts::unblock_contact,
        group_participants::list_participants,
        group_participants::add_participants,
        group_participants::remove_participant,
        group_participants::promote_participants,
        group_participants::demote_participants,
        calls::list_calls,
        calls::get_call,
        calls::delete_call,
        calls::reject_call,
        media::get_media,
        media::upload_media,
        crate::api::http::events_sse::sse_events_global,
        crate::api::http::events_sse::sse_events_session,
        crate::api::http::dlq::list_dlq,
        crate::api::http::dlq::get_dlq,
        crate::api::http::dlq::replay_dlq,
        crate::api::http::dlq::replay_all_dlq,
        crate::api::http::dlq::delete_dlq,
    ),
    components(
        schemas(
            HealthStatus,
            health::LivenessResponse,
            health::ReadinessResponse,
            health::StartupResponse,
            Session,
            CreateSessionRequest,
            RefreshQrRequest,
            SessionActionResponse,
            PairingCodeRequest,
            PairingCodeResponse,
            Chat,
            Message,
            SendMessageRequest,
            messages::SendLocationRequest,
            messages::SendContactRequest,
            SendMediaRequest,
            ReactMessageRequest,
            EditMessageRequest,
            MessageActionResponse,
            Contact,
            UpsertContactRequest,
            SyncContactsRequest,
            SyncContactsResponse,
            CheckContactsRequest,
            CheckContactsResponse,
            ContactStatusItem,
            ProfilePictureResponse,
            GroupParticipant,
            Call,
            MediaCache,
            ErrorResponse,
            ErrorDetail,
            MarkChatAsReadRequest,
            MuteChatRequest,
            PinChatRequest,
            ArchiveChatRequest,
            ChatActionResponse,
            SetChatStateRequest,
            SetGlobalPresenceRequest,
            GlobalPresenceResponse,
            CreateGroupRequest,
            CreateGroupResponse,
            JoinGroupRequest,
            JoinGroupResponse,
            GroupParticipantItem,
            SetSubjectRequest,
            SetDescriptionRequest,
            SetAnnouncementRequest,
            SetLockRequest,
            SetEphemeralRequest,
            GroupInviteLinkResponse,
            AddParticipantsRequest,
            ModifyParticipantRolesRequest,
            ParticipantActionResult,
            ParticipantActionResponse,
            UploadMediaResponse,
            crate::models::outbox::OutboxEvent,
            crate::models::outbox::WebhookDlqEntry,
            crate::api::http::dlq::DlqReplayResponse,
            crate::events::bus::GatewayEvent,
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
        (name = "Media", description = "Cached media access and deduplication"),
        (name = "Real-Time Streams", description = "Server-Sent Events and WebSockets event streaming"),
        (name = "Webhooks", description = "Webhook Dead-Letter Queue management and replay"),
    )
)]
pub struct ApiDoc;
