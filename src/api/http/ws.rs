use crate::api::http::{
    error::{AppError, AppResult},
    state::AppState,
};
use crate::security::validate_session_id;
use axum::{
    extract::{
        Path, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tracing::debug;
use tokio_util::sync::CancellationToken;

pub async fn ws_global(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state, None))
}

pub async fn ws_session(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> AppResult<impl IntoResponse> {
    validate_session_id(&session_id)?;

    let _ = crate::repository::sessions::get_one(&session_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Session '{}' not found", session_id)))?;

    Ok(ws.on_upgrade(move |socket| handle_ws(socket, state, Some(session_id))))
}

async fn handle_ws(socket: WebSocket, state: Arc<AppState>, session_filter: Option<String>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.event_bus.subscribe();
    let cancel = CancellationToken::new();
    let send_cancel = cancel.clone();
    let recv_cancel = cancel.clone();

    let send_task = tokio::spawn(async move {
        loop {
            let event = tokio::select! {
                _ = send_cancel.cancelled() => break,
                event = rx.recv() => match event {
                    Ok(event) => event,
                    Err(_) => break,
                }
            };
            if let Some(ref filter) = session_filter
                && &event.session_id != filter
            {
                continue;
            }

            if let Ok(json_str) = serde_json::to_string(&*event)
            {
                let send_result = tokio::select! {
                    _ = send_cancel.cancelled() => break,
                    result = sender.send(Message::Text(json_str.into())) => result,
                };
                if send_result.is_err() {
                    break;
                }
            }
        }
    });

    let recv_task = tokio::spawn(async move {
        loop {
            let msg = tokio::select! {
                _ = recv_cancel.cancelled() => break,
                msg = receiver.next() => match msg {
                    Some(msg) => msg,
                    None => break,
                }
            };
            match msg {
                Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {}
                Ok(Message::Close(_)) => break,
                Err(e) => {
                    debug!("WebSocket connection closed with error: {}", e);
                    break;
                }
                _ => {}
            }
        }
    });

    let mut send_task = send_task;
    let mut recv_task = recv_task;
    tokio::select! {
        _ = &mut send_task => {},
        _ = &mut recv_task => {},
    }
    cancel.cancel();
    let _ = tokio::join!(send_task, recv_task);
}
