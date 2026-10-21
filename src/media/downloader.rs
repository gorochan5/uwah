use std::sync::Arc;
use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use tracing::{info, warn};
use whatsapp_rust::prelude::Client;

static GLOBAL_DOWNLOAD_SEMAPHORE: std::sync::OnceLock<tokio::sync::Semaphore> = std::sync::OnceLock::new();

fn get_download_semaphore() -> &'static tokio::sync::Semaphore {
    GLOBAL_DOWNLOAD_SEMAPHORE.get_or_init(|| {
        let permits = crate::config::try_get()
            .and_then(|c| c.media.max_concurrent_downloads)
            .unwrap_or(3);
        tokio::sync::Semaphore::new(permits)
    })
}

/// Performs download, decryption, disk persistence and cache registration of a WhatsApp media.
/// Returns the resulting sha256 hex string and disk path if successful.
pub async fn download_and_cache_media(
    client: &Arc<Client>,
    session_id: &str,
    chat_jid: &str,
    msg_id: &str,
    direct_path: &str,
    media_key: &[u8],
    raw_file_sha256: Option<&[u8]>,
    raw_file_enc_sha256: Option<&[u8]>,
    file_length: u64,
    msg_type: &str,
) -> Result<(String, String)> {
    let media_type_enum = match msg_type {
        "image" => whatsapp_rust::download::MediaType::Image,
        "video" => whatsapp_rust::download::MediaType::Video,
        "audio" => whatsapp_rust::download::MediaType::Audio,
        "document" => whatsapp_rust::download::MediaType::Document,
        "sticker" => whatsapp_rust::download::MediaType::Sticker,
        "sticker_pack" => whatsapp_rust::download::MediaType::StickerPack,
        _ => whatsapp_rust::download::MediaType::Image,
    };

    let file_sha256 = raw_file_sha256.unwrap_or_default();
    let file_enc_sha256 = raw_file_enc_sha256.unwrap_or_default();

    let params = whatsapp_rust::download::DownloadParams::encrypted(
        direct_path,
        media_key,
        file_sha256,
        file_enc_sha256,
        file_length,
        media_type_enum,
    );

    let _permit = get_download_semaphore().acquire().await;

    let download_res = client.download(&params).await;
    let bytes = match download_res {
        Ok(b) => b,
        Err(e) => {
            let err_str = e.to_string();
            if err_str.contains("429") || err_str.contains("rate-overlimit") {
                warn!(
                    msg_id = %msg_id,
                    "WhatsApp CDN rate-limit (429) hit, retrying after backoff..."
                );
                tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                client
                    .download(&params)
                    .await
                    .map_err(|e2| {
                        warn!(msg_id = %msg_id, error = %e2, "Retry after 429 also failed");
                        anyhow::anyhow!("WhatsApp CDN download failed: {}", e2)
                    })?
            } else {
                warn!(
                    msg_id = %msg_id,
                    error = %e,
                    "WhatsApp CDN download failed"
                );
                return Err(anyhow::anyhow!("WhatsApp CDN download failed: {}", e));
            }
        }
    };

    let sha256_hex = hex::encode(Sha256::digest(&bytes));

    let is_group = chat_jid.ends_with("@g.us");
    let chat_name = if is_group {
        crate::repository::chats::get_one(chat_jid, session_id)
            .await
            .ok()
            .flatten()
            .and_then(|c| c.name)
            .unwrap_or_else(|| chat_jid.to_string())
    } else {
        crate::repository::contacts::get_one(chat_jid, session_id)
            .await
            .ok()
            .flatten()
            .and_then(|c| c.push_name.or(c.phone_number))
            .unwrap_or_else(|| chat_jid.to_string())
    };

    let (mut mime_type, mut ext) = match msg_type {
        "image" => ("image/jpeg", "jpg"),
        "video" => ("video/mp4", "mp4"),
        "audio" => ("audio/ogg", "ogg"),
        "sticker" => ("image/webp", "webp"),
        "sticker_pack" => ("application/zip", "zip"),
        "document" => ("application/octet-stream", "bin"),
        _ => ("application/octet-stream", "bin"),
    };

    if msg_type == "document" || mime_type == "application/octet-stream" {
        if bytes.starts_with(b"%PDF") {
            mime_type = "application/pdf";
            ext = "pdf";
        } else if bytes.starts_with(b"\xFF\xD8\xFF") {
            mime_type = "image/jpeg";
            ext = "jpg";
        } else if bytes.starts_with(b"\x89PNG") {
            mime_type = "image/png";
            ext = "png";
        } else if bytes.starts_with(b"PK\x03\x04") {
            mime_type = "application/zip";
            ext = "zip";
        }
    }

    let media_dir = crate::config::get().media.dir.clone();
    let file_path = crate::media::storage::get_chat_media_path(
        &media_dir,
        is_group,
        &chat_name,
        msg_type,
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
        tokio::fs::write(&file_path, &bytes)
            .await
            .context("Failed to write downloaded media file to disk")?;

        let _ = crate::repository::media_cache::upsert(
            &sha256_hex,
            &disk_path_str,
            mime_type,
            bytes.len() as i64,
        )
        .await;
    }

    // Update message row with computed media_sha256
    let _ = crate::repository::messages::update_media_sha256(
        msg_id,
        session_id,
        &sha256_hex,
    )
    .await;

    info!(
        message_id = %msg_id,
        media_sha256 = %sha256_hex,
        disk_path = %disk_path_str,
        "WhatsApp media downloaded, decrypted and cached successfully"
    );

    Ok((sha256_hex, disk_path_str))
}
