use crate::models::contacts::Contact;
use crate::models::messages::Message;
use anyhow::Result;
use tracing::{error, info, warn};
use whatsapp_rust::types::events::LazyHistorySync;

pub async fn handle(session_id: &str, lazy_sync: &LazyHistorySync) -> Result<()> {
    let sync_type = lazy_sync.sync_type();
    let chunk_order = lazy_sync.chunk_order();
    let progress = lazy_sync.progress();

    info!(
        session_id = %session_id,
        sync_type = sync_type,
        chunk_order = ?chunk_order,
        progress = ?progress,
        "Starting streaming ingestion of HistorySync payload"
    );

    let mut stream = lazy_sync.stream();
    let mut total_conversations = 0usize;
    let mut total_messages = 0usize;

    while let Some(conv) = stream.next_conversation()? {
        let chat_jid = &conv.id;
        if chat_jid.is_empty() {
            continue;
        }

        total_conversations += 1;

        // 1. Ensure chat exists in DB so FK constraint never fails
        if let Err(err) = crate::repository::chats::ensure_exists(chat_jid, session_id).await {
            error!(
                session_id = %session_id,
                chat_jid = %chat_jid,
                error = %err,
                "Failed to ensure chat exists during history sync"
            );
            continue;
        }

        // 2. Auto-register contact for 1:1 direct conversations (exclude groups, broadcasts, newsletters)
        let is_direct_contact = !chat_jid.ends_with("@g.us")
            && !chat_jid.ends_with("@broadcast")
            && !chat_jid.ends_with("@newsletter");

        if is_direct_contact {
            let phone = if chat_jid.ends_with("@s.whatsapp.net") {
                chat_jid.split('@').next()
            } else {
                None
            };
            let lid = if chat_jid.ends_with("@lid") {
                Some(chat_jid.as_str())
            } else {
                None
            };
            let contact_name = conv.name.as_deref().filter(|s| !s.trim().is_empty());

            if let Err(err) = crate::repository::contacts::ensure_exists(
                session_id,
                chat_jid,
                phone,
                contact_name,
                lid,
            )
            .await
            {
                warn!(
                    session_id = %session_id,
                    chat_jid = %chat_jid,
                    error = %err,
                    "Failed to auto-register contact from history sync conversation (best-effort)"
                );
            }
        }

        // 3. Update chat subject if name is present
        if let Some(name) = &conv.name {
            let update_res = crate::repository::chats::update_subject(
                session_id,
                chat_jid,
                Some(name.as_str()),
                None,
                None,
            )
            .await;

            if let Err(err) = update_res {
                warn!(
                    session_id = %session_id,
                    chat_jid = %chat_jid,
                    error = %err,
                    "Failed to update chat subject during history sync"
                );
            }
        }

        // 3. Update last message timestamp if available
        if let Some(last_ts) = conv.last_msg_timestamp {
            let _ = crate::repository::chats::update_last_message(
                chat_jid,
                session_id,
                last_ts as i64,
                false,
            )
            .await;
        }

        // 4. Ingest conversation messages in batch
        let mut conv_messages: Vec<Message> = Vec::new();
        for hist_msg in &conv.messages {
            let Some(web_msg) = hist_msg.message.as_option() else {
                continue;
            };
            let Some(key) = web_msg.key.as_option() else {
                continue;
            };
            let Some(msg_id) = key.id.as_deref() else {
                continue;
            };

            let is_from_me = key.from_me.unwrap_or(false);
            let sender_jid = if is_from_me {
                "me"
            } else {
                key.participant.as_deref().unwrap_or(chat_jid)
            };

            let ts_secs = web_msg.message_timestamp.unwrap_or(0);
            let dt = chrono::DateTime::from_timestamp(ts_secs as i64, 0)
                .unwrap_or_else(chrono::Utc::now);

            let extracted = if let Some(msg) = web_msg.message.as_option() {
                crate::events::extractor::extract_message_content(msg)
            } else {
                crate::events::extractor::ExtractedMessage::default()
            };

            let (cdn_url, media_key, file_enc_sha256, media_sha256) = match extracted.media {
                Some(m) => (m.cdn_url, m.media_key, m.file_enc_sha256, m.media_sha256),
                None => (None, None, None, None),
            };

            let delivery_status = if is_from_me {
                "sent".to_string()
            } else {
                "received".to_string()
            };

            conv_messages.push(Message {
                id: msg_id.to_string(),
                session_id: session_id.to_string(),
                chat_jid: chat_jid.to_string(),
                sender_jid: sender_jid.to_string(),
                quoted_message_id: extracted.quoted_message_id,
                is_from_me,
                message_type: extracted.message_type,
                body: extracted.body,
                cdn_url,
                media_key,
                file_enc_sha256,
                media_sha256,
                delivery_status,
                is_edited: extracted.is_edited,
                is_deleted: extracted.is_revoked,
                timestamp: dt,
            });
        }

        if !conv_messages.is_empty() {
            let msg_count = conv_messages.len();
            if let Err(err) = crate::repository::messages::create_batch(&conv_messages).await {
                error!(
                    session_id = %session_id,
                    chat_jid = %chat_jid,
                    count = msg_count,
                    error = %err,
                    "Failed to insert batch of historical messages in DB"
                );
            } else {
                total_messages += msg_count;
            }
        }

        // Yield to Tokio runtime after each conversation to prevent monopolizing the executor
        // and allow concurrent DB operations in SQLite WAL pool.
        tokio::task::yield_now().await;
    }

    let skipped = stream.skipped_conversations();
    if skipped > 0 {
        warn!(
            session_id = %session_id,
            skipped_count = skipped,
            "Encountered corrupt conversations skipped during streaming"
        );
    }

    // 5. Decode non-conversation remainder payload (pushnames, contact mappings)
    match stream.remainder() {
        Ok(remainder) => {
            let mut total_contacts = 0usize;
            for pushname in &remainder.pushnames {
                let Some(jid) = pushname.id.as_deref() else {
                    continue;
                };
                if jid.is_empty() {
                    continue;
                }

                let phone = jid.split('@').next().map(str::to_string);
                let contact = Contact {
                    session_id: session_id.to_string(),
                    jid: jid.to_string(),
                    lid_jid: None,
                    phone_number: phone,
                    push_name: pushname.pushname.clone(),
                    status_text: None,
                    profile_pic_url: None,
                    is_business: false,
                    username: None,
                    devices: None,
                    verified_name: None,
                    updated_at: chrono::Utc::now(),
                };

                if let Err(err) = crate::repository::contacts::upsert(&contact).await {
                    error!(
                        session_id = %session_id,
                        jid = %jid,
                        error = %err,
                        "Failed to upsert contact pushname from HistorySync"
                    );
                } else {
                    total_contacts += 1;
                }
            }

            info!(
                session_id = %session_id,
                total_conversations = total_conversations,
                total_messages = total_messages,
                total_contacts = total_contacts,
                "HistorySync ingestion completed successfully"
            );
        }
        Err(err) => {
            warn!(
                session_id = %session_id,
                error = %err,
                total_conversations = total_conversations,
                total_messages = total_messages,
                "Failed to decode HistorySync stream remainder"
            );
        }
    }

    Ok(())
}
