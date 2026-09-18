use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;
use whatsapp_rust::prelude::*;

#[derive(Clone)]
pub struct SessionManager {
    clients: Arc<RwLock<HashMap<String, Arc<Client>>>>,
    sessions_dir: String,
}

impl SessionManager {
    pub fn new(sessions_dir: impl Into<String>) -> Self {
        Self {
            clients: Arc::new(RwLock::new(HashMap::new())),
            sessions_dir: sessions_dir.into(),
        }
    }

    /// Retrieves an active WhatsApp client by session ID.
    pub async fn get_client(&self, session_id: &str) -> Option<Arc<Client>> {
        let guard = self.clients.read().await;
        guard.get(session_id).cloned()
    }

    /// Checks if a session is currently running in memory.
    pub async fn is_running(&self, session_id: &str) -> bool {
        let guard = self.clients.read().await;
        guard.contains_key(session_id)
    }

    /// Stops and unregisters an active WhatsApp session.
    pub async fn stop_session(&self, session_id: &str) {
        let mut guard = self.clients.write().await;
        if let Some(client) = guard.remove(session_id) {
            info!(session_id = %session_id, "Disconnecting WhatsApp client");
            client.disconnect().await;
        }
    }

    /// Starts a WhatsApp session bot instance.
    pub async fn start_session(&self, session_id: String) -> anyhow::Result<Arc<Client>> {
        // Prevent duplicate background tasks if already active
        if let Some(existing) = self.get_client(&session_id).await {
            info!(session_id = %session_id, "Session already active in memory");
            return Ok(existing);
        }

        // 1. Ensure isolated storage directory exists
        let sessions_dir = Path::new(&self.sessions_dir);
        if !sessions_dir.exists() {
            tokio::fs::create_dir_all(sessions_dir).await?;
        }
        let db_path = format!("{}/{}.db", self.sessions_dir, session_id);

        // 2. Initialize isolated SqliteStore backend
        let store = SqliteStore::new(&db_path).await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to initialize SqliteStore for '{}': {}",
                session_id,
                e
            )
        })?;

        // 3. Assemble BotBuilder with decoupled event pipeline
        let sid_events = session_id.clone();
        let clients_ref = Arc::clone(&self.clients);

        let builder = Bot::builder()
            .with_backend(store)
            .with_event_delivery(EventDelivery::Ordered { capacity: 1024 })
            .on_event_for(
                &[
                    EventKind::Messages,
                    EventKind::Receipt,
                    EventKind::GroupUpdate,
                    EventKind::IncomingCall,
                    EventKind::HistorySync,
                    EventKind::ContactUpdated,
                    EventKind::ContactNumberChanged,
                    EventKind::Connected,
                    EventKind::LoggedOut,
                    EventKind::PairingQrCode,
                ],
                move |event: Arc<Event>, client: Arc<Client>| {
                    let sid = sid_events.clone();
                    let clients = Arc::clone(&clients_ref);
                    async move {
                        crate::events::dispatch(sid, event, client, clients).await;
                    }
                },
            );

        // 4. Build Bot instance
        let bot = builder
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to build Bot for '{}': {}", session_id, e))?;

        let bot_handle = bot.spawn();
        let client = bot_handle.client();

        // 5. Register client in memory and drive BotHandle on background Tokio task
        self.clients
            .write()
            .await
            .insert(session_id.clone(), Arc::clone(&client));

        let sid_task = session_id.clone();
        tokio::spawn(async move {
            bot_handle.await;
            info!(session_id = %sid_task, "WhatsApp bot worker loop exited");
        });

        Ok(client)
    }
}
