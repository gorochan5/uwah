use std::pin::Pin;
use std::sync::Arc;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status};

use crate::api::grpc::proto::event_stream_service_server::EventStreamService;
use crate::api::grpc::proto::{GatewayEventEnvelope, StreamEventsRequest};
use crate::api::http::state::AppState;

pub struct EventStreamServiceImpl {
    state: Arc<AppState>,
}

impl EventStreamServiceImpl {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl EventStreamService for EventStreamServiceImpl {
    type StreamEventsStream = Pin<
        Box<dyn Stream<Item = Result<GatewayEventEnvelope, Status>> + Send + 'static>,
    >;

    async fn stream_events(
        &self,
        request: Request<StreamEventsRequest>,
    ) -> Result<Response<Self::StreamEventsStream>, Status> {
        let req = request.into_inner();
        let filter_session_id = req.session_id;

        let rx = self.state.event_bus.subscribe();
        let stream = BroadcastStream::new(rx).filter_map(move |item| {
            let filter_session = filter_session_id.clone();
            match item {
                Ok(event) => {
                    if let Some(ref filter_sid) = filter_session
                        && &event.session_id != filter_sid
                    {
                        return None;
                    }

                    let envelope = GatewayEventEnvelope {
                        event_id: event.seq.to_string(),
                        session_id: event.session_id.clone(),
                        event_type: event.event_type.clone(),
                        timestamp: event.timestamp,
                        payload_json: event.payload.to_string(),
                    };
                    Some(Ok(envelope))
                }
                Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(skipped)) => {
                    tracing::warn!("gRPC event stream client lagged; skipped {} events", skipped);
                    None
                }
            }
        });

        Ok(Response::new(Box::pin(stream)))
    }
}
