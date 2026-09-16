use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, SqlitePool, sqlite::SqliteQueryResult};
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

pub async fn get_one(id: &str) -> Result<Option<MediaCache>> {
    info!(
        media_id = id,
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
        id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to fetch media cache entry from DB")
}

pub async fn create(id: &str) -> Result<SqliteQueryResult> {
    info!(
        media_id = id,
        "Creating media cache entry in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        INSERT INTO media_cache ( 
                sha256
            )
        VALUES (
                $1
            )
        "#,
        id
    )
    .execute(pool)
    .await
    .context("Failed to create new media cache entry on DB")
}

pub async fn delete(id: &str) -> Result<SqliteQueryResult>  {
    info!(
        media_id = id,
        "Deleting media cache entry in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        DELETE FROM media_cache
        WHERE sha256 = $1
        "#,
        id,
    )
    .execute(pool)
    .await
    .context("Failed to delete media cache entry from DB")
}
