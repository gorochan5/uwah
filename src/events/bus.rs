use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast;
use utoipa::ToSchema;

/// Standardized event envelope broadcast to SSE, WebSockets, and gRPC streams
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct GatewayEvent {
    pub seq: u64,
    pub session_id: String,
    pub event_type: String,
    pub timestamp: i64,
    pub payload: serde_json::Value,
}

#[derive(Clone)]
pub struct GatewayEventBus {
    sender: broadcast::Sender<Arc<GatewayEvent>>,
    seq_counter: Arc<AtomicU64>,
}

impl Default for GatewayEventBus {
    fn default() -> Self {
        Self::new(4096)
    }
}

impl GatewayEventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            seq_counter: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Publishes an event to the bounded broadcast ring buffer.
    pub fn publish(
        &self,
        session_id: impl Into<String>,
        event_type: impl Into<String>,
        payload: serde_json::Value,
    ) {
        let seq = self.seq_counter.fetch_add(1, Ordering::SeqCst);
        let timestamp = chrono::Utc::now().timestamp_millis();
        let event = Arc::new(GatewayEvent {
            seq,
            session_id: session_id.into(),
            event_type: event_type.into(),
            timestamp,
            payload,
        });

        let _ = self.sender.send(event);
    }

    /// Subscribes to the live event stream.
    pub fn subscribe(&self) -> broadcast::Receiver<Arc<GatewayEvent>> {
        self.sender.subscribe()
    }
}

static GLOBAL_BUS: std::sync::LazyLock<GatewayEventBus> =
    std::sync::LazyLock::new(GatewayEventBus::default);

/// Publishes an event to the global event bus.
pub fn publish(
    session_id: impl Into<String>,
    event_type: impl Into<String>,
    payload: serde_json::Value,
) {
    GLOBAL_BUS.publish(session_id, event_type, payload);
}

/// Retrieves the shared global event bus instance.
pub fn global() -> &'static GatewayEventBus {
    &GLOBAL_BUS
}

#[cfg(test)]
mod tests {
    use super::GatewayEventBus;

    #[tokio::test]
    async fn publish_delivers_event_fields_and_increasing_sequence() {
        let bus = GatewayEventBus::new(8);
        let mut receiver = bus.subscribe();

        bus.publish("session-a", "message", serde_json::json!({"text": "hello"}));
        let first = receiver.recv().await.unwrap();
        bus.publish("session-b", "receipt", serde_json::json!({"id": 3}));
        let second = receiver.recv().await.unwrap();

        assert_eq!(first.seq, 1);
        assert_eq!(first.session_id, "session-a");
        assert_eq!(first.event_type, "message");
        assert_eq!(first.payload, serde_json::json!({"text": "hello"}));
        assert!(first.timestamp > 0);
        assert_eq!(second.seq, 2);
        assert_eq!(second.session_id, "session-b");
    }

    #[tokio::test]
    async fn cloned_bus_shares_sequence_and_events() {
        let bus = GatewayEventBus::new(8);
        let clone = bus.clone();
        let mut receiver = bus.subscribe();

        clone.publish("s", "event", serde_json::Value::Null);
        assert_eq!(receiver.recv().await.unwrap().seq, 1);
    }
}
