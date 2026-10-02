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

    let send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if let Some(ref filter) = session_filter
                && &event.session_id != filter
            {
                continue;
            }

            if let Ok(json_str) = serde_json::to_string(&*event)
                && sender.send(Message::Text(json_str.into())).await.is_err()
            {
                break;
            }
        }
    });

    let recv_task = tokio::spawn(async move {
        while let Some(msg) = receiver.next().await {
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

    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }
}
