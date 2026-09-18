use crate::api::http::{
    error::{AppError, AppResult, ErrorResponse},
    state::AppState,
};
use crate::repository::media_cache as repo;
use axum::{
    Json,
    body::Body,
    extract::{Path, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::SeekFrom;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tokio_util::io::ReaderStream;
use utoipa::ToSchema;

/// Represents a parsed HTTP RFC 7233 byte range specification.
#[derive(Debug, PartialEq, Eq)]
pub enum HttpRange {
    FromTo(u64, u64),
    From(u64),
    Suffix(u64),
}

impl HttpRange {
    pub fn parse(header_value: &str) -> Option<Self> {
        let trimmed = header_value.trim();
        let range_spec = trimmed.strip_prefix("bytes=")?;
        let (start_str, end_str) = range_spec.split_once('-')?;
        let start_str = start_str.trim();
        let end_str = end_str.trim();

        if start_str.is_empty() {
            let suffix: u64 = end_str.parse().ok()?;
            Some(HttpRange::Suffix(suffix))
        } else if end_str.is_empty() {
            let start: u64 = start_str.parse().ok()?;
            Some(HttpRange::From(start))
        } else {
            let start: u64 = start_str.parse().ok()?;
            let end: u64 = end_str.parse().ok()?;
            if start <= end {
                Some(HttpRange::FromTo(start, end))
            } else {
                None
            }
        }
    }

    /// Resolves requested range against total file size.
    /// Returns `Some((start_offset, content_length, end_offset))` or `None` if unsatisfiable.
    pub fn satisfy(&self, total_size: u64) -> Option<(u64, u64, u64)> {
        if total_size == 0 {
            return None;
        }
        match *self {
            HttpRange::FromTo(start, end) => {
                if start >= total_size {
                    return None;
                }
                let end = end.min(total_size - 1);
                let length = end - start + 1;
                Some((start, length, end))
            }
            HttpRange::From(start) => {
                if start >= total_size {
                    return None;
                }
                let end = total_size - 1;
                let length = end - start + 1;
                Some((start, length, end))
            }
            HttpRange::Suffix(suffix) => {
                if suffix == 0 {
                    return None;
                }
                let length = suffix.min(total_size);
                let start = total_size - length;
                let end = total_size - 1;
                Some((start, length, end))
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UploadMediaResponse {
    pub sha256: String,
    pub disk_path: String,
    pub mime_type: String,
    pub file_size: i64,
    pub url: String,
}

#[utoipa::path(
    get,
    path = "/sessions/{id}/media/{sha256}",
    tag = "Media",
    summary = "Retrieve cached media binary",
    description = "Zero-buffer streaming of cached media with path traversal containment and RFC 7233 Range/Partial Content (HTTP 206) support.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("sha256" = String, Path, description = "Media SHA-256 hash (64 hex characters)"),
        ("Range" = Option<String>, Header, description = "Standard RFC 7233 byte range (e.g. 'bytes=0-1024')")
    ),
    responses(
        (status = 200, description = "Full media file binary stream", content_type = "application/octet-stream"),
        (status = 206, description = "Partial media chunk stream", content_type = "application/octet-stream"),
        (status = 400, description = "Invalid SHA-256 hash parameter", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 403, description = "Forbidden: path traversal detected", body = ErrorResponse),
        (status = 404, description = "Media not found in database or disk", body = ErrorResponse),
        (status = 416, description = "Requested range not satisfiable", body = ErrorResponse)
    )
)]
pub async fn get_media(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Path((_session_id, sha256)): Path<(String, String)>,
) -> AppResult<Response> {
    if sha256.len() != 64 || !sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::BadRequest(
            "Invalid sha256 hash parameter".to_string(),
        ));
    }
    let sha256 = sha256.to_ascii_lowercase();

    let media = repo::get_one(&sha256)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Media with sha256 '{}' not found", sha256)))?;

    let base_dir = PathBuf::from(&state.config.media.dir);
    tokio::fs::create_dir_all(&base_dir)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to create media dir: {}", e)))?;

    let canonical_base = tokio::fs::canonicalize(&base_dir).await.map_err(|e| {
        AppError::Internal(anyhow::anyhow!("Failed to canonicalize media dir: {}", e))
    })?;

    let file_path = PathBuf::from(&media.disk_path);
    let canonical_file = tokio::fs::canonicalize(&file_path)
        .await
        .map_err(|_| AppError::NotFound("Media file missing on disk".to_string()))?;

    // Strict path traversal containment check
    if !canonical_file.starts_with(&canonical_base) {
        return Err(AppError::Forbidden(
            "Access denied: path traversal attempt detected".to_string(),
        ));
    }

    let mut file = tokio::fs::File::open(&canonical_file)
        .await
        .map_err(|e| AppError::NotFound(format!("Failed to open media file: {}", e)))?;

    let metadata = file
        .metadata()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to read media metadata: {}", e)))?;
    let total_size = metadata.len();

    // Async LRU touch: update timestamp non-blockingly
    let sha256_touch = sha256.clone();
    tokio::spawn(async move {
        if let Err(e) = repo::touch_last_accessed(&sha256_touch).await {
            tracing::warn!(sha256 = %sha256_touch, error = %e, "Failed to touch media last_accessed_at");
        }
    });

    let mime = HeaderValue::from_str(&media.mime_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));

    let range_header = headers.get(header::RANGE).and_then(|val| val.to_str().ok());

    if let Some(parsed_range) = range_header.and_then(HttpRange::parse) {
        match parsed_range.satisfy(total_size) {
            Some((start, length, end)) => {
                file.seek(SeekFrom::Start(start)).await.map_err(|e| {
                    AppError::Internal(anyhow::anyhow!("Failed to seek media file: {}", e))
                })?;

                let limited_stream = ReaderStream::new(file.take(length));
                let content_range = format!("bytes {}-{}/{}", start, end, total_size);

                let response = Response::builder()
                    .status(StatusCode::PARTIAL_CONTENT)
                    .header(header::CONTENT_TYPE, mime)
                    .header(header::ACCEPT_RANGES, "bytes")
                    .header(header::CONTENT_LENGTH, length.to_string())
                    .header(header::CONTENT_RANGE, content_range)
                    .body(Body::from_stream(limited_stream))
                    .map_err(|e| {
                        AppError::Internal(anyhow::anyhow!("Failed to build response: {}", e))
                    })?;

                return Ok(response.into_response());
            }
            None => {
                let unsatisfiable_range = format!("bytes */{}", total_size);
                let response = Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::ACCEPT_RANGES, "bytes")
                    .header(header::CONTENT_RANGE, unsatisfiable_range)
                    .body(Body::empty())
                    .map_err(|e| {
                        AppError::Internal(anyhow::anyhow!("Failed to build range error: {}", e))
                    })?;

                return Ok(response.into_response());
            }
        }
    }

    // Default Full 200 OK Response with Zero-Buffer Streaming
    let stream = ReaderStream::new(file);
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_LENGTH, total_size.to_string())
        .body(Body::from_stream(stream))
        .map_err(|e| {
            AppError::Internal(anyhow::anyhow!("Failed to build streaming response: {}", e))
        })?;

    Ok(response.into_response())
}

#[utoipa::path(
    post,
    path = "/sessions/{id}/media/upload",
    tag = "Media",
    summary = "Direct streaming media upload",
    description = "Streams incoming binary payload to disk cache, calculates SHA-256 hash on the fly, and registers the file in media cache.",
    params(
        ("id" = String, Path, description = "Session identifier"),
        ("Content-Type" = Option<String>, Header, description = "MIME type of the media file (e.g. image/jpeg, video/mp4)")
    ),
    responses(
        (status = 201, description = "Media uploaded and cached successfully", body = UploadMediaResponse),
        (status = 400, description = "Missing or invalid payload", body = ErrorResponse),
        (status = 401, description = "Unauthorized", body = ErrorResponse),
        (status = 413, description = "File size exceeds configured limit", body = ErrorResponse),
        (status = 500, description = "Failed to store media", body = ErrorResponse)
    )
)]
pub async fn upload_media(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    headers: HeaderMap,
    request: Request,
) -> AppResult<impl IntoResponse> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let max_size = state
        .config
        .media
        .max_file_size
        .unwrap_or(100 * 1024 * 1024);
    let media_dir = PathBuf::from(&state.config.media.dir);
    tokio::fs::create_dir_all(&media_dir).await.map_err(|e| {
        AppError::Internal(anyhow::anyhow!("Failed to create media directory: {}", e))
    })?;

    // Create temporary file for streaming write
    let temp_filename = format!("tmp_{}_{}", session_id, uuid_simple());
    let temp_path = media_dir.join(&temp_filename);
    let mut temp_file = tokio::fs::File::create(&temp_path).await.map_err(|e| {
        AppError::Internal(anyhow::anyhow!("Failed to create temporary file: {}", e))
    })?;

    let body = request.into_body();
    let mut hasher = Sha256::new();
    let mut total_bytes: u64 = 0;

    use whatsapp_rust::futures::StreamExt;
    let mut stream = body.into_data_stream();

    while let Some(chunk_result) = stream.next().await {
        let data =
            chunk_result.map_err(|e| AppError::BadRequest(format!("Stream error: {}", e)))?;
        total_bytes += data.len() as u64;
        if total_bytes > max_size {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(AppError::BadRequest(format!(
                "File size exceeds maximum allowed size of {} bytes",
                max_size
            )));
        }
        hasher.update(&data);
        temp_file
            .write_all(&data)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to write chunk: {}", e)))?;
    }

    temp_file
        .flush()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to flush temp file: {}", e)))?;
    drop(temp_file);

    if total_bytes == 0 {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(AppError::BadRequest(
            "Uploaded media payload is empty".to_string(),
        ));
    }

    let sha256_hex = hex::encode(hasher.finalize());
    let ext = mime_to_extension(&content_type);
    let final_filename = format!("{}.{}", sha256_hex, ext);
    let final_path = media_dir.join(&final_filename);

    tokio::fs::rename(&temp_path, &final_path)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to finalize media file: {}", e)))?;

    let disk_path_str = final_path.to_str().unwrap_or(&final_filename).to_string();

    repo::upsert(
        &sha256_hex,
        &disk_path_str,
        &content_type,
        total_bytes as i64,
    )
    .await?;

    let resp = UploadMediaResponse {
        sha256: sha256_hex.clone(),
        disk_path: disk_path_str,
        mime_type: content_type,
        file_size: total_bytes as i64,
        url: format!("/sessions/{}/media/{}", session_id, sha256_hex),
    };

    Ok((StatusCode::CREATED, Json(resp)))
}

fn mime_to_extension(mime: &str) -> &'static str {
    match mime {
        m if m.contains("jpeg") || m.contains("jpg") => "jpg",
        m if m.contains("png") => "png",
        m if m.contains("webp") => "webp",
        m if m.contains("ogg") => "ogg",
        m if m.contains("mp3") || m.contains("mpeg") => "mp3",
        m if m.contains("mp4") => "mp4",
        m if m.contains("pdf") => "pdf",
        _ => "bin",
    }
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}_{:x}", now.as_millis(), std::process::id())
}
