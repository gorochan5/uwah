use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};
use whatsapp_rust::prelude::*;

use crate::models::sessions::Session;
use crate::repository::sessions as repo;

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
        let store = SqliteStore::new(&db_path)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to initialize SqliteStore for '{}': {}", session_id, e))?;

        // 3. Assemble BotBuilder with event callbacks
        let sid_for_qr = session_id.clone();
        let sid_for_conn = session_id.clone();
        let sid_for_logout = session_id.clone();
        let sid_for_msg = session_id.clone();
        let clients_ref = Arc::clone(&self.clients);

        let builder = Bot::builder()
            .with_backend(store)
            // Message Callback: persists incoming live messages into uwah.db
            .on_message(move |ctx: MessageContext| {
                let sid = sid_for_msg.clone();
                async move {
                    let msg_id = ctx.info.id.clone();
                    let chat_jid = ctx.info.source.chat.to_string();
                    let sender_jid = ctx.info.source.sender.to_string();
                    let is_from_me = ctx.info.source.is_from_me;
                    let timestamp = ctx.info.timestamp;

                    let body = ctx.message.text_content().map(|s| s.to_string());
                    let message_type = if body.is_some() {
                        "text".to_string()
                    } else {
                        "other".to_string()
                    };

                    info!(
                        session_id = %sid,
                        message_id = %msg_id,
                        chat = %chat_jid,
                        sender = %sender_jid,
                        "Received incoming WhatsApp message"
                    );

                    // Ensure chat stub exists in DB so foreign key does not fail
                    let _ = crate::repository::chats::ensure_exists(&chat_jid, &sid).await;

                    let db_message = crate::models::messages::Message {
                        id: msg_id,
                        session_id: sid.clone(),
                        chat_jid,
                        sender_jid,
                        quoted_message_id: None,
                        is_from_me,
                        message_type,
                        body,
                        cdn_url: None,
                        media_key: None,
                        file_enc_sha256: None,
                        media_sha256: None,
                        delivery_status: if is_from_me { "sent".to_string() } else { "received".to_string() },
                        is_edited: false,
                        is_deleted: false,
                        timestamp,
                    };

                    if let Err(err) = crate::repository::messages::create(&db_message).await {
                        error!(
                            session_id = %sid,
                            error = %err,
                            "Failed to store inbound message in uwah.db"
                        );
                    }
                }
            })
            // QR Code Callback: persists refreshed QR string into the DB
            .on_qr_code(move |code: String, timeout: std::time::Duration| {
                let sid = sid_for_qr.clone();
                async move {
                    info!(
                        session_id = %sid,
                        validity_secs = timeout.as_secs(),
                        "Received QR pairing payload from WhatsApp"
                    );
                    if let Err(err) = repo::refresh_qr(&sid, &code).await {
                        error!(
                            session_id = %sid,
                            error = %err,
                            "Failed to update QR payload in database"
                        );
                    }
                }
            })
            // Connected Callback: updates DB status to CONNECTED and stores phone number
            .on_connected(move |client: Arc<Client>| {
                let sid = sid_for_conn.clone();
                async move {
                    let phone_number = client.pn().map(|jid| jid.user.to_string());
                    let push_name = {
                        let name = client.push_name();
                        if name.is_empty() {
                            None
                        } else {
                            Some(name)
                        }
                    };

                    info!(
                        session_id = %sid,
                        phone = ?phone_number,
                        push_name = ?push_name,
                        "WhatsApp session successfully authenticated"
                    );

                    let session_update = Session {
                        id: sid.clone(),
                        phone_number,
                        push_name,
                        status: "CONNECTED".to_string(),
                        qr_payload: None,
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                    };

                    if let Err(err) = repo::auth(session_update).await {
                        error!(
                            session_id = %sid,
                            error = %err,
                            "Failed to persist CONNECTED status in database"
                        );
                    }
                }
            })
            // Logout Callback: handles device unlinking from WhatsApp phone app
            .on_logged_out(move |_info| {
                let sid = sid_for_logout.clone();
                let clients = Arc::clone(&clients_ref);
                async move {
                    error!(session_id = %sid, "WhatsApp session was unlinked/logged out");
                    clients.write().await.remove(&sid);
                    let session_update = Session {
                        id: sid.clone(),
                        phone_number: None,
                        push_name: None,
                        status: "LOGGED_OUT".to_string(),
                        qr_payload: None,
                        created_at: chrono::Utc::now(),
                        updated_at: chrono::Utc::now(),
                    };
                    let _ = repo::auth(session_update).await;
                }
            });

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

