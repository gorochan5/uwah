pub mod proto {
    tonic::include_proto!("uwah.v1");

    pub const FILE_DESCRIPTOR_SET: &[u8] = tonic::include_file_descriptor_set!("uwah_descriptor");
}

pub mod calls;
pub mod chats;
pub mod contacts;
pub mod events;
pub mod groups;
pub mod health;
pub mod media;
pub mod messages;
pub mod sessions;
pub mod webhooks;

use std::net::SocketAddr;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
use crate::api::http::state::AppState;
use self::proto::call_service_server::CallServiceServer;
use self::proto::chat_service_server::ChatServiceServer;
use self::proto::contact_service_server::ContactServiceServer;
use self::proto::event_stream_service_server::EventStreamServiceServer;
use self::proto::group_participant_service_server::GroupParticipantServiceServer;
use self::proto::group_service_server::GroupServiceServer;
use self::proto::health_service_server::HealthServiceServer;
use self::proto::media_service_server::MediaServiceServer;
use self::proto::message_service_server::MessageServiceServer;
use self::proto::session_service_server::SessionServiceServer;
use self::proto::webhook_dlq_service_server::WebhookDlqServiceServer;

pub async fn start_grpc_server(
    state: Arc<AppState>,
    addr: SocketAddr,
    cancel_token: CancellationToken,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let events_svc = events::EventStreamServiceImpl::new(state.clone());
    let health_svc = health::HealthServiceImpl::new(state.clone());
    let sessions_svc = sessions::SessionServiceImpl::new(state.clone());
    let chats_svc = chats::ChatServiceImpl::new(state.clone());
    let groups_svc = groups::GroupServiceImpl::new(state.clone());
    let group_participants_svc = groups::GroupParticipantServiceImpl::new(state.clone());
    let messages_svc = messages::MessageServiceImpl::new(state.clone());
    let contacts_svc = contacts::ContactServiceImpl::new(state.clone());
    let calls_svc = calls::CallServiceImpl::new(state.clone());
    let media_svc = media::MediaServiceImpl::new(state.clone());
    let webhooks_svc = webhooks::WebhookDlqServiceImpl::new(state.clone());
    let auth_secret = state.config.service.auth.clone();

    tracing::info!("Starting gRPC server on {}", addr);

    let auth_interceptor = grpc_auth_interceptor(auth_secret);

    let reflection_svc = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(proto::FILE_DESCRIPTOR_SET)
        .build_v1()?;

    Server::builder()
        .add_service(reflection_svc)
        .add_service(EventStreamServiceServer::with_interceptor(
            events_svc,
            auth_interceptor.clone(),
        ))
        .add_service(HealthServiceServer::with_interceptor(
            health_svc,
            auth_interceptor.clone(),
        ))
        .add_service(SessionServiceServer::with_interceptor(
            sessions_svc,
            auth_interceptor.clone(),
        ))
        .add_service(ChatServiceServer::with_interceptor(
            chats_svc,
            auth_interceptor.clone(),
        ))
        .add_service(GroupServiceServer::with_interceptor(
            groups_svc,
            auth_interceptor.clone(),
        ))
        .add_service(GroupParticipantServiceServer::with_interceptor(
            group_participants_svc,
            auth_interceptor.clone(),
        ))
        .add_service(MessageServiceServer::with_interceptor(
            messages_svc,
            auth_interceptor.clone(),
        ))
        .add_service(ContactServiceServer::with_interceptor(
            contacts_svc,
            auth_interceptor.clone(),
        ))
        .add_service(CallServiceServer::with_interceptor(
            calls_svc,
            auth_interceptor.clone(),
        ))
        .add_service(MediaServiceServer::with_interceptor(
            media_svc,
            auth_interceptor.clone(),
        ))
        .add_service(WebhookDlqServiceServer::with_interceptor(
            webhooks_svc,
            auth_interceptor,
        ))
        .serve_with_shutdown(addr, async move {
            cancel_token.cancelled().await;
            tracing::info!("gRPC server shutdown received");
        })
        .await?;

    Ok(())
}

/// Creates a Tonic interceptor enforcing `authorization: Bearer <TOKEN>` authentication
/// via constant-time token verification.
pub fn grpc_auth_interceptor(
    expected_auth: String,
) -> impl Fn(tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> + Clone {
    move |req: tonic::Request<()>| {
        let auth_header = req
            .metadata()
            .get("authorization")
            .and_then(|val| val.to_str().ok());

        let token = match auth_header {
            Some(header) if header.starts_with("Bearer ") => header[7..].trim(),
            _ => return Err(tonic::Status::unauthenticated("unauthenticated")),
        };

        if token.is_empty() {
            return Err(tonic::Status::unauthenticated("unauthenticated"));
        }

        if crate::security::verify_token(token, &expected_auth) {
            Ok(req)
        } else {
            Err(tonic::Status::unauthenticated("unauthenticated"))
        }
    }
}

#[cfg(test)]
mod tests;
