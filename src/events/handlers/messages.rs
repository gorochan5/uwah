use crate::events::extractor;
use crate::models::messages::Message;
use anyhow::Result;
use tracing::info;
use whatsapp_rust::types::events::MessageBatch;
use whatsapp_rust::types::message::EditAttribute;
use whatsapp_rust::prelude::Client;
use std::sync::Arc;

pub async fn handle(session_id: &str, batch: &MessageBatch, client: &Arc<Client>) -> Result<()> {
    let pool = crate::database::get();
    let mut tx = pool.begin().await?;

    // 1. Defensively guarantee root session row exists
    sqlx::query!(
        r#"
        INSERT INTO sessions (id, status, created_at, updated_at)
        VALUES ($1, 'QRCODE', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO NOTHING
        "#,
        session_id
    )
    .execute(&mut *tx)
    .await?;

    let mut pending_webhooks: Vec<(String, String, serde_json::Value)> = Vec::new();
    let mut pending_downloads: Vec<(String, String, (String, Vec<u8>, Option<Vec<u8>>, Option<Vec<u8>>, u64, String))> = Vec::new();

    for inbound in batch.iter() {
        let info = &inbound.info;
        let msg_id = info.id.to_string();
        let chat_jid = info.source.chat.to_string();
        let sender_jid = info.source.sender.to_string();
        let is_from_me = info.source.is_from_me;
        let timestamp = info.timestamp;

        if !crate::config::get().filter.is_allowed_jid(&chat_jid) {
            tracing::debug!(
                session_id = %session_id,
                message_id = %msg_id,
                chat = %chat_jid,
                "Skipping inbound message due to [filter] configuration"
            );
            continue;
        }

        info!(
            session_id = %session_id,
            message_id = %msg_id,
            chat = %chat_jid,
            sender = %sender_jid,
            "Processing inbound WhatsApp message (atomic transaction)"
        );

        crate::api::http::metrics::inc_messages_inbound();

        // 2. Ensure chat exists in DB within transaction
        let kind = if chat_jid.ends_with("@g.us") {
            "group"
        } else if chat_jid.ends_with("@newsletter") {
            "newsletter"
        } else if chat_jid.ends_with("@broadcast") {
            "broadcast"
        } else {
            "direct"
        };

        sqlx::query!(
            r#"
            INSERT INTO chats (jid, session_id, kind)
            VALUES ($1, $2, $3)
            ON CONFLICT(session_id, jid) DO UPDATE SET
                kind = CASE WHEN chats.kind = 'direct' AND excluded.kind != 'direct' THEN excluded.kind ELSE chats.kind END
            "#,
            chat_jid,
            session_id,
            kind
        )
        .execute(&mut *tx)
        .await?;

        // 3. Extract message content with recursive wrapper unwrapping
        let extracted = extractor::extract_message_content(&inbound.message);

        // Check stanza edit attributes
        let (is_edited, is_revoked) = match info.edit {
            EditAttribute::MessageEdit | EditAttribute::AdminEdit => (true, false),
            EditAttribute::SenderRevoke | EditAttribute::AdminRevoke => (false, true),
            _ => (extracted.is_edited, extracted.is_revoked),
        };

        // 4. Handle edit and revoke protocol actions
        if is_revoked
            && let Some(target_id) = &extracted.edit_target_id {
            sqlx::query!(
                "UPDATE messages SET is_deleted = 1 WHERE id = $1 AND session_id = $2",
                target_id,
                session_id
            )
            .execute(&mut *tx)
            .await?;

            pending_webhooks.push((
                "message.revoked".to_string(),
                session_id.to_string(),
                serde_json::json!({
                    "message_id": target_id,
                    "revoked_by": sender_jid,
                    "chat_jid": chat_jid,
                }),
            ));
        }

        if is_edited
            && let Some(target_id) = &extracted.edit_target_id {
            let edit_body = extracted.body.as_deref();
            sqlx::query!(
                "UPDATE messages SET body = $1, is_edited = 1 WHERE id = $2 AND session_id = $3",
                edit_body,
                target_id,
                session_id
            )
            .execute(&mut *tx)
            .await?;

            pending_webhooks.push((
                "message.edited".to_string(),
                session_id.to_string(),
                serde_json::json!({
                    "message_id": target_id,
                    "edited_body": extracted.body,
                    "chat_jid": chat_jid,
                }),
            ));
        }

        // 5. Map media fields
        let media_download_info = extracted.media.as_ref().and_then(|m| {
            if let (Some(direct_path), Some(media_key)) = (&m.cdn_url, &m.media_key) {
                Some((
                    direct_path.clone(),
                    media_key.clone(),
                    m.raw_file_sha256.clone(),
                    m.raw_file_enc_sha256.clone(),
                    m.file_length.unwrap_or(0),
                    extracted.message_type.clone(),
                ))
            } else {
                None
            }
        });

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
            is_deleted: is_revoked,
            timestamp,
        };

        if let Some(info) = media_download_info {
            pending_downloads.push((msg_id.clone(), chat_jid.clone(), info));
        }

        // 6. Insert/Upsert message
        sqlx::query!(
            r#"
            INSERT INTO messages (
                id, session_id, chat_jid, sender_jid, quoted_message_id,
                is_from_me, message_type, body, cdn_url, media_key,
                file_enc_sha256, media_sha256, delivery_status, is_edited,
                is_deleted, timestamp
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            ON CONFLICT(id, session_id) DO UPDATE SET
                delivery_status = CASE
                    WHEN messages.delivery_status = 'read' THEN 'read'
                    WHEN messages.delivery_status = 'delivered' AND excluded.delivery_status IN ('pending', 'sent', 'received') THEN messages.delivery_status
                    ELSE excluded.delivery_status
                END,
                body = CASE
                    WHEN excluded.is_edited = 1 THEN excluded.body
                    WHEN messages.is_edited = 1 THEN messages.body
                    ELSE COALESCE(excluded.body, messages.body)
                END,
                is_edited = messages.is_edited OR excluded.is_edited,
                is_deleted = messages.is_deleted OR excluded.is_deleted,
                cdn_url = COALESCE(excluded.cdn_url, messages.cdn_url),
                media_key = COALESCE(excluded.media_key, messages.media_key),
                file_enc_sha256 = COALESCE(excluded.file_enc_sha256, messages.file_enc_sha256),
                media_sha256 = COALESCE(excluded.media_sha256, messages.media_sha256)
            "#,
            db_message.id,
            db_message.session_id,
            db_message.chat_jid,
            db_message.sender_jid,
            db_message.quoted_message_id,
            db_message.is_from_me,
            db_message.message_type,
            db_message.body,
            db_message.cdn_url,
            db_message.media_key,
            db_message.file_enc_sha256,
            db_message.media_sha256,
            db_message.delivery_status,
            db_message.is_edited,
            db_message.is_deleted,
            db_message.timestamp
        )
        .execute(&mut *tx)
        .await?;

        // 7. Update chat last_message_at and unread_count
        let unread_delta: i64 = if !is_from_me { 1 } else { 0 };
        let ts_millis = timestamp.timestamp_millis();
        sqlx::query!(
            r#"
            UPDATE chats
            SET last_message_at = MAX(COALESCE(last_message_at, 0), $1),
                unread_count = unread_count + $2,
                updated_at = CURRENT_TIMESTAMP
            WHERE jid = $3 AND session_id = $4
            "#,
            ts_millis,
            unread_delta,
            chat_jid,
            session_id
        )
        .execute(&mut *tx)
        .await?;

        // 8. Auto-register contact on message
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

            sqlx::query!(
                r#"
                INSERT INTO contacts (
                    session_id,
                    jid,
                    lid_jid,
                    phone_number,
                    push_name,
                    updated_at
                )
                VALUES ($1, $2, $3, $4, $5, CURRENT_TIMESTAMP)
                ON CONFLICT(session_id, jid) DO UPDATE SET
                    lid_jid = COALESCE(excluded.lid_jid, contacts.lid_jid),
                    phone_number = COALESCE(excluded.phone_number, contacts.phone_number),
                    push_name = COALESCE(excluded.push_name, contacts.push_name),
                    updated_at = CURRENT_TIMESTAMP
                "#,
                session_id,
                target_str,
                lid,
                phone,
                push_name
            )
            .execute(&mut *tx)
            .await?;
        }

        if let Ok(val) = serde_json::to_value(&db_message) {
            pending_webhooks.push(("message.received".to_string(), session_id.to_string(), val));
        }
    }

    // 9. Enqueue outbox events atomically within the transaction (Track 3.1 & 4.1)
    for (event, sid, payload) in &pending_webhooks {
        let _ = crate::repository::outbox::enqueue_tx(&mut tx, sid, event, None, payload).await;
    }

    // Commit atomic transaction
    tx.commit().await?;

    // Dispatch in-process event bus and direct webhook notifications post-commit
    for (event, sid, payload) in pending_webhooks {
        crate::events::webhook::dispatch_event(&event, &sid, payload);
    }

    // Spawn background media download and decryption tasks for inbound media
    if !pending_downloads.is_empty() {
        let client_clone = Arc::clone(client);
        let session_id_owned = session_id.to_string();
        let media_dir = crate::config::get().media.dir.clone();

        tokio::spawn(async move {
            for (msg_id, chat_jid, (direct_path, media_key, raw_file_sha256, raw_file_enc_sha256, file_length, msg_type)) in pending_downloads {
                let media_type_enum = match msg_type.as_str() {
                    "image" => whatsapp_rust::download::MediaType::Image,
                    "video" => whatsapp_rust::download::MediaType::Video,
                    "audio" => whatsapp_rust::download::MediaType::Audio,
                    "document" => whatsapp_rust::download::MediaType::Document,
                    "sticker" => whatsapp_rust::download::MediaType::Sticker,
                    _ => whatsapp_rust::download::MediaType::Image,
                };

                let file_sha256 = raw_file_sha256.unwrap_or_default();
                let file_enc_sha256 = raw_file_enc_sha256.unwrap_or_default();

                let params = whatsapp_rust::download::DownloadParams::encrypted(
                    direct_path,
                    &media_key,
                    &file_sha256,
                    &file_enc_sha256,
                    file_length,
                    media_type_enum,
                );

                match client_clone.download(&params).await {
                    Ok(bytes) => {
                        use sha2::Digest;
                        let sha256_hex = hex::encode(sha2::Sha256::digest(&bytes));

                        let is_group = chat_jid.ends_with("@g.us");
                        let chat_name = if is_group {
                            crate::repository::chats::get_one(&chat_jid, &session_id_owned)
                                .await
                                .ok()
                                .flatten()
                                .and_then(|c| c.name)
                                .unwrap_or_else(|| chat_jid.clone())
                        } else {
                            crate::repository::contacts::get_one(&chat_jid, &session_id_owned)
                                .await
                                .ok()
                                .flatten()
                                .and_then(|c| c.push_name.or(c.phone_number))
                                .unwrap_or_else(|| chat_jid.clone())
                        };

                        let (mime_type, ext) = match msg_type.as_str() {
                            "image" => ("image/jpeg", "jpg"),
                            "video" => ("video/mp4", "mp4"),
                            "audio" => ("audio/ogg", "ogg"),
                            "sticker" => ("image/webp", "webp"),
                            "document" => ("application/octet-stream", "bin"),
                            _ => ("application/octet-stream", "bin"),
                        };

                        let file_path = crate::media::storage::get_chat_media_path(
                            &media_dir,
                            is_group,
                            &chat_name,
                            &msg_type,
                            &sha256_hex,
                            ext,
                        );
                        let disk_path_str = file_path.to_string_lossy().to_string();

                        if let Ok(Some(existing)) = crate::repository::media_cache::get_one(&sha256_hex).await
                            && tokio::fs::try_exists(&existing.disk_path).await.unwrap_or(false)
                        {
                            let _ = crate::repository::media_cache::touch_last_accessed(&sha256_hex).await;
                        } else {
                            if let Some(parent) = file_path.parent() {
                                let _ = tokio::fs::create_dir_all(parent).await;
                            }
                            if let Err(e) = tokio::fs::write(&file_path, &bytes).await {
                                tracing::error!(error = %e, path = %disk_path_str, "Failed to write downloaded media file");
                                continue;
                            }
                            let _ = crate::repository::media_cache::upsert(
                                &sha256_hex,
                                &disk_path_str,
                                mime_type,
                                bytes.len() as i64,
                            )
                            .await;
                        }

                        // Update message row with computed media_sha256
                        if let Err(e) = crate::repository::messages::update_media_sha256(
                            &msg_id,
                            &session_id_owned,
                            &sha256_hex,
                        )
                        .await
                        {
                            tracing::error!(
                                message_id = %msg_id,
                                session_id = %session_id_owned,
                                error = %e,
                                "Failed to update message media_sha256 after inbound download"
                            );
                        } else {
                            tracing::info!(
                                message_id = %msg_id,
                                media_sha256 = %sha256_hex,
                                disk_path = %disk_path_str,
                                "Inbound WhatsApp media downloaded, decrypted and cached successfully"
                            );
                        }
                    }
                    Err(err) => {
                        tracing::warn!(
                            message_id = %msg_id,
                            session_id = %session_id_owned,
                            error = %err,
                            "Failed to download/decrypt inbound media from WhatsApp CDN"
                        );
                    }
                }
            }
        });
    }

    Ok(())
}
