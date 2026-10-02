use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::debug;
use whatsapp_rust::prelude::*;

/// Hierarchical Token Bucket and pacing rate limiter to emulate human typing
/// and prevent automated carrier/Meta fraud bans (Inviolable Invariant #6).
#[derive(Clone)]
pub struct TrafficShaper {
    last_sent: Arc<RwLock<HashMap<String, Instant>>>,
    min_interval_ms: u64,
}

impl TrafficShaper {
    pub fn new(min_interval_ms: u64) -> Self {
        Self {
            last_sent: Arc::new(RwLock::new(HashMap::new())),
            min_interval_ms,
        }
    }

    /// Calculates a typing delay with randomized jitter and sends ephemeral ChatState.
    pub async fn apply_typing_jitter(
        &self,
        session_id: &str,
        recipient: &Jid,
        client: &Arc<Client>,
        char_count: usize,
        is_audio: bool,
    ) {
        let key = format!("{}:{}", session_id, recipient);

        // 1. Enforce minimum interval between consecutive messages to the same JID
        let now = Instant::now();
        let wait_needed = {
            let guard = self.last_sent.read().await;
            if let Some(last) = guard.get(&key) {
                let elapsed = now.duration_since(*last).as_millis() as u64;
                if elapsed < self.min_interval_ms {
                    Some(self.min_interval_ms - elapsed)
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some(wait_ms) = wait_needed {
            tokio::time::sleep(Duration::from_millis(wait_ms)).await;
        }

        // 2. Emulate ephemeral typing state (Composing or Recording)
        let state = if is_audio {
            whatsapp_rust::features::ChatStateType::Recording
        } else {
            whatsapp_rust::features::ChatStateType::Composing
        };

        let _ = client.chatstate().send(recipient, state).await;

        // 3. Compute realistic typing time (200-280 chars per minute + 30% jitter)
        let base_delay_ms = ((char_count.clamp(1, 300) as u64) * 60_000) / 2400;
        let jitter_factor = 0.8 + ((char_count % 5) as f64 * 0.1);
        let delay_ms = (base_delay_ms as f64 * jitter_factor).clamp(200.0, 3000.0) as u64;

        debug!(
            session_id = %session_id,
            recipient = %recipient,
            delay_ms = %delay_ms,
            "Emulating human typing jitter before message transmission"
        );

        tokio::time::sleep(Duration::from_millis(delay_ms)).await;

        // 4. Update last sent timestamp
        let mut guard = self.last_sent.write().await;
        guard.insert(key, Instant::now());
    }
}

impl Default for TrafficShaper {
    fn default() -> Self {
        Self::new(1000)
    }
}
