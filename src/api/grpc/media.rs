use std::io::SeekFrom;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use futures_util::Stream;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use tonic::{Request, Response, Status};

use crate::api::grpc::proto::media_service_server::MediaService;
use crate::api::grpc::proto::upload_media_chunk::Data as ChunkData;
use crate::api::grpc::proto::{
    DownloadMediaRequest, MediaChunk, UploadMediaChunk, UploadMediaResponse,
    UploadMediaUnaryRequest,
};
use crate::api::http::state::AppState;
use crate::repository::media_cache as repo;

const CHUNK_SIZE: usize = 64 * 1024; // 64KB

pub struct MediaServiceImpl {
    state: Arc<AppState>,
}

impl MediaServiceImpl {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }
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

#[tonic::async_trait]
impl MediaService for MediaServiceImpl {
    type DownloadMediaStream = Pin<
        Box<dyn Stream<Item = Result<MediaChunk, Status>> + Send + 'static>,
    >;

    async fn download_media(
        &self,
        request: Request<DownloadMediaRequest>,
    ) -> Result<Response<Self::DownloadMediaStream>, Status> {
        let req = request.into_inner();
        let sha256 = req.sha256.to_ascii_lowercase();

        if sha256.len() != 64 || !sha256.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(Status::invalid_argument("Invalid SHA-256 hash parameter"));
        }

        let media = repo::get_one(&sha256)
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?
            .ok_or_else(|| Status::not_found(format!("Media with sha256 '{}' not found", sha256)))?;

        let base_dir = PathBuf::from(&self.state.config.media.dir);
        tokio::fs::create_dir_all(&base_dir)
            .await
            .map_err(|e| Status::internal(format!("Failed to create media dir: {}", e)))?;

        let canonical_base = tokio::fs::canonicalize(&base_dir)
            .await
            .map_err(|e| Status::internal(format!("Failed to canonicalize media dir: {}", e)))?;

        let file_path = PathBuf::from(&media.disk_path);
        let canonical_file = tokio::fs::canonicalize(&file_path)
            .await
            .map_err(|_| Status::not_found("Media file missing on disk"))?;

        if !canonical_file.starts_with(&canonical_base) {
            return Err(Status::permission_denied(
                "Access denied: path traversal attempt detected",
            ));
        }

        let mut file = tokio::fs::File::open(&canonical_file)
            .await
            .map_err(|e| Status::not_found(format!("Failed to open media file: {}", e)))?;

        let metadata = file
            .metadata()
            .await
            .map_err(|e| Status::internal(format!("Failed to read media metadata: {}", e)))?;
        let total_size = metadata.len();

        let sha256_touch = sha256.clone();
        tokio::spawn(async move {
            if let Err(e) = repo::touch_last_accessed(&sha256_touch).await {
                tracing::warn!(sha256 = %sha256_touch, error = %e, "Failed to touch media last_accessed_at");
            }
        });

        let start = req.offset.unwrap_or(0).max(0) as u64;
        if start >= total_size && total_size > 0 {
            return Err(Status::out_of_range("Requested offset exceeds file size"));
        }

        let length = match req.length {
            Some(l) if l > 0 => (l as u64).min(total_size - start),
            _ => total_size - start,
        };

        if start > 0 {
            file.seek(SeekFrom::Start(start))
                .await
                .map_err(|e| Status::internal(format!("Failed to seek file: {}", e)))?;
        }

        let mime_type = media.mime_type;
        let stream = futures_util::stream::unfold(
            (file, start, length, mime_type, total_size),
            move |(mut file, current_offset, remaining, mime_type, total_size)| async move {
                if remaining == 0 {
                    return None;
                }
                let to_read = (remaining as usize).min(CHUNK_SIZE);
                let mut buf = vec![0u8; to_read];
                match file.read_exact(&mut buf).await {
                    Ok(_) => {
                        let chunk = MediaChunk {
                            data: buf,
                            mime_type: mime_type.clone(),
                            total_size: total_size as i64,
                            offset: current_offset as i64,
                        };
                        let next_offset = current_offset + to_read as u64;
                        let next_remaining = remaining - to_read as u64;
                        Some((
                            Ok(chunk),
                            (file, next_offset, next_remaining, mime_type, total_size),
                        ))
                    }
                    Err(e) => Some((
                        Err(Status::internal(format!("Failed to read media chunk: {}", e))),
                        (file, current_offset, 0, mime_type, total_size),
                    )),
                }
            },
        );

        Ok(Response::new(Box::pin(stream)))
    }

    async fn upload_media(
        &self,
        request: Request<tonic::Streaming<UploadMediaChunk>>,
    ) -> Result<Response<UploadMediaResponse>, Status> {
        let mut stream = request.into_inner();
        let max_size = self
            .state
            .config
            .media
            .max_file_size
            .unwrap_or(100 * 1024 * 1024);

        let media_dir = PathBuf::from(&self.state.config.media.dir);
        tokio::fs::create_dir_all(&media_dir)
            .await
            .map_err(|e| Status::internal(format!("Failed to create media directory: {}", e)))?;

        let mut session_id = "default".to_string();
        let mut mime_type = "application/octet-stream".to_string();

        let temp_filename = format!("tmp_grpc_{}", uuid::Uuid::new_v4());
        let temp_path = media_dir.join(&temp_filename);
        let mut temp_file = tokio::fs::OpenOptions::new().write(true).create_new(true).open(&temp_path)
            .await
            .map_err(|e| Status::internal(format!("Failed to create temporary file: {}", e)))?;

        let mut hasher = Sha256::new();
        let mut total_bytes: u64 = 0;

        while let Some(chunk_res) = stream.message().await.map_err(|e| {
            Status::invalid_argument(format!("Error reading upload stream: {}", e))
        })? {
            if let Some(data) = chunk_res.data {
                match data {
                    ChunkData::Metadata(meta) => {
                        session_id = meta.session_id;
                        if !meta.mime_type.is_empty() {
                            mime_type = meta.mime_type;
                        }
                    }
                    ChunkData::Chunk(chunk_bytes) => {
                        if !chunk_bytes.is_empty() {
                            total_bytes += chunk_bytes.len() as u64;
                            if total_bytes > max_size {
                                let _ = tokio::fs::remove_file(&temp_path).await;
                                return Err(Status::invalid_argument(format!(
                                    "File size exceeds maximum allowed size of {} bytes",
                                    max_size
                                )));
                            }
                            hasher.update(&chunk_bytes);
                            temp_file
                                .write_all(&chunk_bytes)
                                .await
                                .map_err(|e| Status::internal(format!("Failed to write chunk: {}", e)))?;
                        }
                    }
                }
            }
        }

        temp_file
            .flush()
            .await
            .map_err(|e| Status::internal(format!("Failed to flush temp file: {}", e)))?;
        drop(temp_file);

        if total_bytes == 0 {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(Status::invalid_argument("Uploaded media payload is empty"));
        }

        let sha256_hex = hex::encode(hasher.finalize());
        let ext = mime_to_extension(&mime_type);
        let final_filename = format!("{}.{}", sha256_hex, ext);
        let final_path = media_dir.join(&final_filename);

        tokio::fs::rename(&temp_path, &final_path)
            .await
            .map_err(|e| Status::internal(format!("Failed to finalize media file: {}", e)))?;

        let disk_path_str = final_path.to_str().unwrap_or(&final_filename).to_string();

        repo::upsert(
            &sha256_hex,
            &disk_path_str,
            &mime_type,
            total_bytes as i64,
        )
        .await
        .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

        let url = format!("/sessions/{}/media/{}", session_id, sha256_hex);

        Ok(Response::new(UploadMediaResponse {
            sha256: sha256_hex,
            disk_path: disk_path_str,
            mime_type,
            file_size: total_bytes as i64,
            url,
        }))
    }

    async fn upload_media_unary(
        &self,
        request: Request<UploadMediaUnaryRequest>,
    ) -> Result<Response<UploadMediaResponse>, Status> {
        let req = request.into_inner();
        let session_id = req.session_id;
        let bytes = req.data;
        let mime_type = if req.mime_type.is_empty() {
            "application/octet-stream".to_string()
        } else {
            req.mime_type
        };

        if bytes.is_empty() {
            return Err(Status::invalid_argument("Media binary payload cannot be empty"));
        }

        let max_size = self
            .state
            .config
            .media
            .max_file_size
            .unwrap_or(100 * 1024 * 1024);

        if bytes.len() as u64 > max_size {
            return Err(Status::invalid_argument(format!(
                "File size exceeds maximum allowed size of {} bytes",
                max_size
            )));
        }

        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let sha256_hex = hex::encode(hasher.finalize());

        let media_dir = PathBuf::from(&self.state.config.media.dir);
        tokio::fs::create_dir_all(&media_dir)
            .await
            .map_err(|e| Status::internal(format!("Failed to create media directory: {}", e)))?;

        let ext = if let Some(ref fname) = req.filename
            && let Some(parsed_ext) = fname.split('.').next_back()
        {
            parsed_ext
        } else {
            mime_to_extension(&mime_type)
        };

        let final_filename = format!("{}.{}", sha256_hex, ext);
        let final_path = media_dir.join(&final_filename);
        let disk_path_str = final_path.to_str().unwrap_or(&final_filename).to_string();

        let (final_disk_path, final_mime) = if let Ok(Some(existing)) = repo::get_one(&sha256_hex).await
            && tokio::fs::try_exists(&existing.disk_path).await.unwrap_or(false)
        {
            let _ = repo::touch_last_accessed(&sha256_hex).await;
            tracing::info!(
                media_sha256 = %sha256_hex,
                disk_path = %existing.disk_path,
                "Media cache hit, skipping disk write and DB upsert"
            );
            (existing.disk_path, existing.mime_type)
        } else {
            tokio::fs::write(&final_path, &bytes)
                .await
                .map_err(|e| Status::internal(format!("Failed to persist media: {}", e)))?;

            repo::upsert(
                &sha256_hex,
                &disk_path_str,
                &mime_type,
                bytes.len() as i64,
            )
            .await
            .map_err(|e| Status::internal(format!("Database error: {}", e)))?;

            (disk_path_str, mime_type)
        };

        let url = format!("/sessions/{}/media/{}", session_id, sha256_hex);

        Ok(Response::new(UploadMediaResponse {
            sha256: sha256_hex,
            disk_path: final_disk_path,
            mime_type: final_mime,
            file_size: bytes.len() as i64,
            url,
        }))
    }
}
