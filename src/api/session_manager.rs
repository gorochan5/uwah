use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use whatsapp_rust::prelude::*;
use whatsapp_rust::types::events::Event;

#[derive(Clone)]
pub enum SessionState {
    Starting(broadcast::Sender<Result<Arc<Client>, String>>),
    Running(Arc<Client>, CancellationToken),
}

#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, SessionState>>>,
    sessions_dir: String,
}

impl SessionManager {
    pub fn new(sessions_dir: impl Into<String>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            sessions_dir: sessions_dir.into(),
        }
    }

    /// Retrieves an active WhatsApp client by session ID if running.
    pub async fn get_client(&self, session_id: &str) -> Option<Arc<Client>> {
        let guard = self.sessions.read().await;
        match guard.get(session_id) {
            Some(SessionState::Running(client, _)) => Some(Arc::clone(client)),
            _ => None,
        }
    }

    /// Checks if a session is currently running in memory.
    pub async fn is_running(&self, session_id: &str) -> bool {
        let guard = self.sessions.read().await;
        matches!(guard.get(session_id), Some(SessionState::Running(_, _)))
    }

    /// Returns the number of currently running sessions.
    pub async fn active_count(&self) -> usize {
        let guard = self.sessions.read().await;
        guard
            .values()
            .filter(|s| matches!(s, SessionState::Running(_, _)))
            .count()
    }

    /// Returns a list of all active session IDs currently running in memory.
    pub async fn list_active_ids(&self) -> Vec<String> {
        let guard = self.sessions.read().await;
        guard
            .iter()
            .filter_map(|(id, s)| match s {
                SessionState::Running(_, _) => Some(id.clone()),
                _ => None,
            })
            .collect()
    }

    /// Removes a session from the in-memory map without blocking network I/O in the write lock.
    pub async fn unregister_session(&self, session_id: &str) {
        let state_opt = {
            let mut guard = self.sessions.write().await;
            guard.remove(session_id)
        };

        if let Some(SessionState::Running(_, cancel_token)) = state_opt {
            info!(session_id = %session_id, "Unregistering session from memory");
            cancel_token.cancel();
        }
    }

    /// Stops and unregisters an active WhatsApp session with structured task cancellation.
    /// Never holds the write lock across network `.await` boundaries (Inviolable Invariant #2).
    pub async fn stop_session(&self, session_id: &str) {
        let state_opt = {
            let mut guard = self.sessions.write().await;
            guard.remove(session_id)
        };

        match state_opt {
            Some(SessionState::Running(client, cancel_token)) => {
                info!(session_id = %session_id, "Disconnecting and terminating WhatsApp client");
                cancel_token.cancel();
                client.disconnect().await;
            }
            Some(SessionState::Starting(tx)) => {
                let _ = tx.send(Err("Session stopped while starting".to_string()));
            }
            None => {}
        }
    }

    /// Deletes keystore database files (`.db`, `.db-wal`, `.db-shm`) from the filesystem on session deletion.
    pub async fn delete_session_files(&self, session_id: &str) {
        self.stop_session(session_id).await;

        let db_file = format!("{}/{}.db", self.sessions_dir, session_id);
        let wal_file = format!("{}/{}.db-wal", self.sessions_dir, session_id);
        let shm_file = format!("{}/{}.db-shm", self.sessions_dir, session_id);

        for path in &[db_file, wal_file, shm_file] {
            if let Err(err) = tokio::fs::remove_file(path).await
                && err.kind() != std::io::ErrorKind::NotFound {
                warn!(
                    session_id = %session_id,
                    path = %path,
                    error = %err,
                    "Failed to remove session database file on cleanup"
                );
            }
        }
    }

    /// Starts a WhatsApp session bot instance with thundering-herd deduplication and structured supervision.
    pub async fn start_session(&self, session_id: String) -> anyhow::Result<Arc<Client>> {
        crate::security::validate_session_id(&session_id)
            .map_err(|e| anyhow::anyhow!("Invalid session_id: {}", e))?;

        // 1. Check if already active or currently initializing
        let (broadcast_tx, wait_rx) = {
            let mut guard = self.sessions.write().await;
            match guard.get(&session_id) {
                Some(SessionState::Running(client, _)) => {
                    info!(session_id = %session_id, "Session already active in memory");
                    return Ok(Arc::clone(client));
                }
                Some(SessionState::Starting(tx)) => {
                    info!(session_id = %session_id, "Subscribing to concurrent session startup");
                    (None, Some(tx.subscribe()))
                }
                None => {
                    let (tx, _rx) = broadcast::channel(1);
                    guard.insert(session_id.clone(), SessionState::Starting(tx.clone()));
                    (Some(tx), None)
                }
            }
        };

        // 2. If another task is already initializing this session, wait for its result
        if let Some(mut rx) = wait_rx {
            return match rx.recv().await {
                Ok(Ok(client)) => Ok(client),
                Ok(Err(err)) => Err(anyhow::anyhow!("Concurrent start failed: {}", err)),
                Err(e) => Err(anyhow::anyhow!("Broadcast receive error: {}", e)),
            };
        }

        let notify_tx = broadcast_tx.expect("Startup channel must be set for initializer");

        // 3. Perform initialization outside of lock guards
        let init_res = self.initialize_bot(&session_id).await;

        match init_res {
            Ok((client, cancel_token)) => {
                let client_ret = Arc::clone(&client);
                let mut guard = self.sessions.write().await;
                guard.insert(
                    session_id.clone(),
                    SessionState::Running(Arc::clone(&client), cancel_token),
                );
                let _ = notify_tx.send(Ok(Arc::clone(&client_ret)));
                Ok(client_ret)
            }
            Err(err) => {
                let err_msg = err.to_string();
                let mut guard = self.sessions.write().await;
                guard.remove(&session_id);
                let _ = notify_tx.send(Err(err_msg));
                Err(err)
            }
        }
    }

    async fn initialize_bot(&self, session_id: &str) -> anyhow::Result<(Arc<Client>, CancellationToken)> {
        // Ensure storage directory exists
        let sessions_dir = Path::new(&self.sessions_dir);
        if !sessions_dir.exists() {
            tokio::fs::create_dir_all(sessions_dir).await?;
        }
        let db_path = format!("{}/{}.db", self.sessions_dir, session_id);

        // Initialize isolated SqliteStore backend
        let store = SqliteStore::new(&db_path).await.map_err(|e| {
            anyhow::anyhow!(
                "Failed to initialize SqliteStore for '{}': {}",
                session_id,
                e
            )
        })?;

        // Assemble BotBuilder with unified event router
        let sid_events = session_id.to_string();
        let mgr_ref = self.clone();

        let builder = Bot::builder()
            .with_backend(store)
            .with_event_delivery(EventDelivery::Ordered { capacity: 1024 })
            .on_event(move |event: Arc<Event>, client: Arc<Client>| {
                let sid = sid_events.clone();
                let mgr = mgr_ref.clone();
                async move {
                    crate::events::dispatch(sid, event, client, mgr).await;
                }
            });

        // Build Bot instance
        let bot = builder
            .build()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to build Bot for '{}': {}", session_id, e))?;

        let bot_handle = bot.spawn();
        let client = bot_handle.client();
        let cancel_token = CancellationToken::new();
        let cancel_task = cancel_token.clone();

        let sid_task = session_id.to_string();
        tokio::spawn(async move {
            tokio::select! {
                _ = cancel_task.cancelled() => {
                    info!(session_id = %sid_task, "WhatsApp bot worker cancellation acknowledged");
                }
                _ = bot_handle => {
                    info!(session_id = %sid_task, "WhatsApp bot worker loop completed");
                }
            }
        });

        Ok((client, cancel_token))
    }
}
