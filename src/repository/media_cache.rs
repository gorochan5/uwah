use anyhow::{Context, Result};
use sqlx::sqlite::SqliteQueryResult;
use tracing::info;

type MediaCache = crate::models::media_cache::MediaCache;

pub async fn get_all() -> Result<Vec<MediaCache>> {
    info!("Fetching all media cache entries from DB");

    let pool = crate::database::get();
    sqlx::query_as!(
        MediaCache,
        r#"
        SELECT
            sha256 AS "sha256!",
            disk_path,
            mime_type,
            file_size,
            ref_count,
            last_accessed_at AS "last_accessed_at!: chrono::DateTime<chrono::Utc>",
            created_at AS "created_at!: chrono::DateTime<chrono::Utc>"
        FROM media_cache
        "#
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch media cache entries from DB")
}

pub async fn get_one(sha256: &str) -> Result<Option<MediaCache>> {
    info!(
        media_sha256 = sha256,
        "Fetching media cache entry from DB"
    );
    
    let pool = crate::database::get();
    sqlx::query_as!(
        MediaCache,
        r#"
        SELECT
            sha256 AS "sha256!",
            disk_path,
            mime_type,
            file_size,
            ref_count,
            last_accessed_at AS "last_accessed_at!: chrono::DateTime<chrono::Utc>",
            created_at AS "created_at!: chrono::DateTime<chrono::Utc>"
        FROM media_cache
        WHERE sha256 = $1
        "#,
        sha256
    )
    .fetch_optional(pool)
    .await
    .context("Failed to fetch media cache entry from DB")
}

pub async fn create(sha256: &str, disk_path: &str, mime_type: &str, file_size: i64) -> Result<SqliteQueryResult> {
    info!(
        media_sha256 = sha256,
        disk_path = disk_path,
        "Creating media cache entry in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        INSERT INTO media_cache ( 
            sha256,
            disk_path,
            mime_type,
            file_size
        )
        VALUES (
            $1, $2, $3, $4
        )
        "#,
        sha256,
        disk_path,
        mime_type,
        file_size
    )
    .execute(pool)
    .await
    .context("Failed to create new media cache entry on DB")
}

pub async fn delete(sha256: &str) -> Result<SqliteQueryResult> {
    info!(
        media_sha256 = sha256,
        "Deleting media cache entry in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        DELETE FROM media_cache
        WHERE sha256 = $1
        "#,
        sha256
    )
    .execute(pool)
    .await
    .context("Failed to delete media cache entry from DB")
}
