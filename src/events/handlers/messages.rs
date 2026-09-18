use crate::events::extractor;
use crate::models::messages::Message;
use anyhow::Result;
use tracing::{error, info};
use whatsapp_rust::types::events::MessageBatch;
use whatsapp_rust::types::message::EditAttribute;

pub async fn handle(session_id: &str, batch: &MessageBatch) -> Result<()> {
    for inbound in batch.iter() {
        let info = &inbound.info;
        let msg_id = info.id.to_string();
        let chat_jid = info.source.chat.to_string();
        let sender_jid = info.source.sender.to_string();
        let is_from_me = info.source.is_from_me;
        let timestamp = info.timestamp;

        info!(
            session_id = %session_id,
            message_id = %msg_id,
            chat = %chat_jid,
            sender = %sender_jid,
            "Processing inbound WhatsApp message"
        );

        // 1. Ensure chat exists in DB so foreign key constraint never fails
        if let Err(err) = crate::repository::chats::ensure_exists(&chat_jid, session_id).await {
            error!(
                session_id = %session_id,
                chat = %chat_jid,
                error = %err,
                "Failed to ensure chat exists in DB"
            );
            continue;
        }

        // 2. Extract message body, media metadata, quoted context, and protocol actions
        let extracted = extractor::extract_message_content(&inbound.message);

        // 3. Handle edit and revoke protocol actions
        if let (true, Some(target_id)) = (extracted.is_revoked, &extracted.edit_target_id) {
            let _ = crate::repository::messages::mark_as_deleted(session_id, target_id).await;
            crate::events::webhook::dispatch_event(
                "message.revoked",
                session_id,
                serde_json::json!({
                    "message_id": target_id,
                    "revoked_by": sender_jid,
                    "chat_jid": chat_jid,
                }),
            );
        }
        if let (true, Some(target_id)) = (extracted.is_edited, &extracted.edit_target_id) {
            let _ = crate::repository::messages::update_edited_message(
                session_id,
                target_id,
                extracted.body.as_deref(),
            )
            .await;
            crate::events::webhook::dispatch_event(
                "message.edited",
                session_id,
                serde_json::json!({
                    "message_id": target_id,
                    "edited_body": extracted.body,
                    "chat_jid": chat_jid,
                }),
            );
        }

        // Check stanza edit attributes
        let (is_edited, is_deleted) = match info.edit {
            EditAttribute::MessageEdit | EditAttribute::AdminEdit => (true, false),
            EditAttribute::SenderRevoke | EditAttribute::AdminRevoke => (false, true),
            _ => (extracted.is_edited, extracted.is_revoked),
        };

        // 4. Map media fields
        let (cdn_url, media_key, file_enc_sha256, media_sha256) = match extracted.media {
            Some(m) => (m.cdn_url, m.media_key, m.file_enc_sha256, m.media_sha256),
            None => (None, None, None, None),
        };

        let delivery_status = if is_from_me {
            "sent".to_string()
        } else {
            "received".to_string()
        };

        let db_message = Message {
            id: msg_id.clone(),
            session_id: session_id.to_string(),
            chat_jid: chat_jid.clone(),
            sender_jid: sender_jid.clone(),
            quoted_message_id: extracted.quoted_message_id,
            is_from_me,
            message_type: extracted.message_type,
            body: extracted.body,
            cdn_url,
            media_key,
            file_enc_sha256,
            media_sha256,
            delivery_status,
            is_edited,
            is_deleted,
            timestamp,
        };

        if let Err(err) = crate::repository::messages::create(&db_message).await {
            error!(
                session_id = %session_id,
                message_id = %msg_id,
                error = %err,
                "Failed to store message in uwah.db"
            );
        } else {
            crate::events::webhook::dispatch_event(
                "message.received",
                session_id,
                db_message.clone(),
            );
        }

        // 5. Advance chat's last_message_at and unread_count
        let _ = crate::repository::chats::update_last_message(
            &chat_jid,
            session_id,
            timestamp.timestamp_millis(),
            !is_from_me,
        )
        .await;

        // 6. Auto-register contact on message per specification:
        // - In group chats: individual participant if not from me
        // - In 1:1 chats: peer (chat if sent from me, sender if received from peer)
        let contact_jid = if info.source.is_group {
            if is_from_me {
                None
            } else {
                Some(&info.source.sender)
            }
        } else if is_from_me {
            Some(&info.source.chat)
        } else {
            Some(&info.source.sender)
        };

        let eligible_target = contact_jid.filter(|t| {
            matches!(
                t.server,
                whatsapp_rust::Server::Pn | whatsapp_rust::Server::Lid
            )
        });

        if let Some(target) = eligible_target {
            let target_str = target.to_string();
            let push_name = if info.push_name.trim().is_empty() {
                None
            } else {
                Some(info.push_name.as_str())
            };

            let alt_jid_str = info.source.sender_alt.as_ref().map(ToString::to_string);
            let (phone, lid) = if target.server == whatsapp_rust::Server::Lid {
                (
                    info.source.sender_alt.as_ref().map(|j| j.user.as_str()),
                    Some(target_str.as_str()),
                )
            } else {
                (Some(target.user.as_str()), alt_jid_str.as_deref())
            };

            if let Err(err) = crate::repository::contacts::ensure_exists(
                session_id,
                &target_str,
                phone,
                push_name,
                lid,
            )
            .await
            {
                tracing::warn!(
                    session_id = %session_id,
                    jid = %target_str,
                    error = %err,
                    "Failed to auto-register contact on message (best-effort)"
                );
            }
        }
    }

    Ok(())
}
