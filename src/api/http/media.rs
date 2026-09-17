use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use std::path::PathBuf;
use std::sync::Arc;
use crate::api::http::{
    error::{AppError, AppResult},
    state::AppState,
};
use crate::repository::media_cache as repo;

pub async fn get_media(
    State(state): State<Arc<AppState>>,
    Path((_session_id, sha256)): Path<(String, String)>,
) -> AppResult<Response> {
    if sha256.len() != 64 || !sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AppError::BadRequest("Invalid sha256 hash parameter".to_string()));
    }

    let media = repo::get_one(&sha256)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Media with sha256 '{}' not found", sha256)))?;

    let base_dir = PathBuf::from(&state.config.media.dir);
    let file_path = PathBuf::from(&media.disk_path);

    if !file_path.starts_with(&base_dir) && !file_path.exists() {
        return Err(AppError::NotFound("Media file missing on disk".to_string()));
    }

    let bytes = tokio::fs::read(&file_path)
        .await
        .map_err(|e| AppError::NotFound(format!("Failed to read media file: {}", e)))?;

    let mime = HeaderValue::from_str(&media.mime_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));

    let response = Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CONTENT_LENGTH, bytes.len())
        .body(Body::from(bytes))
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Failed to build response: {}", e)))?;

    Ok(response.into_response())
}

