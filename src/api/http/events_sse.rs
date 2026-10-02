use crate::api::http::{
    error::{AppError, AppResult, ErrorResponse},
    state::AppState,
};
use crate::events::bus::GatewayEvent;
use crate::security::validate_session_id;
use axum::{
    extract::{Path, State},
    response::sse::{Event, KeepAlive, Sse},
};
use futures_util::stream::{Stream, StreamExt};
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;

#[utoipa::path(
    get,
    path = "/events",
    tag = "Events",
    summary = "Global real-time SSE stream",
    description = "Subscribes to a real-time Server-Sent Events (SSE) stream of all gateway events across all active sessions.",
    responses(
        (status = 200, description = "SSE stream opened successfully (text/event-stream)", body = GatewayEvent),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn sse_events_global(
    State(state): State<Arc<AppState>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.event_bus.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|res| async move {
        match res {
            Ok(event) => {
                let data = serde_json::to_string(&*event).unwrap_or_default();
                Some(Ok(Event::default().event(&event.event_type).data(data)))
            }
            Err(_) => None,
        }
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    )
}

#[utoipa::path(
    get,
    path = "/sessions/{id}/events",
    tag = "Events",
    summary = "Session-scoped real-time SSE stream",
    description = "Subscribes to a real-time Server-Sent Events (SSE) stream scoped exclusively to the specified session ID.",
    params(
        ("id" = String, Path, description = "Session identifier")
    ),
    responses(
        (status = 200, description = "SSE stream opened successfully (text/event-stream)", body = GatewayEvent),
        (status = 400, description = "Invalid session identifier", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn sse_events_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> AppResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    validate_session_id(&session_id)?;

    let _ = crate::repository::sessions::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    let rx = state.event_bus.subscribe();
    let sid = session_id.clone();
    let stream = BroadcastStream::new(rx).filter_map(move |res| {
        let sid = sid.clone();
        async move {
            match res {
                Ok(event) if event.session_id == sid => {
                    let data = serde_json::to_string(&*event).unwrap_or_default();
                    Some(Ok(Event::default().event(&event.event_type).data(data)))
                }
                _ => None,
            }
        }
    });

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keep-alive"),
    ))
}
