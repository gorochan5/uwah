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
    info!(media_sha256 = sha256, "Fetching media cache entry from DB");

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

pub async fn create(
    sha256: &str,
    disk_path: &str,
    mime_type: &str,
    file_size: i64,
) -> Result<SqliteQueryResult> {
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

pub async fn upsert(
    sha256: &str,
    disk_path: &str,
    mime_type: &str,
    file_size: i64,
) -> Result<SqliteQueryResult> {
    info!(
        media_sha256 = sha256,
        disk_path = disk_path,
        "Upserting media cache entry in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        INSERT INTO media_cache (
            sha256,
            disk_path,
            mime_type,
            file_size,
            last_accessed_at
        )
        VALUES (
            $1, $2, $3, $4, CURRENT_TIMESTAMP
        )
        ON CONFLICT(sha256) DO UPDATE SET
            disk_path = excluded.disk_path,
            mime_type = excluded.mime_type,
            file_size = excluded.file_size,
            last_accessed_at = CURRENT_TIMESTAMP
        "#,
        sha256,
        disk_path,
        mime_type,
        file_size
    )
    .execute(pool)
    .await
    .context("Failed to upsert media cache entry on DB")
}

pub async fn delete(sha256: &str) -> Result<SqliteQueryResult> {
    info!(media_sha256 = sha256, "Deleting media cache entry in DB");

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

pub async fn touch_last_accessed(sha256: &str) -> Result<SqliteQueryResult> {
    let pool = crate::database::get();
    sqlx::query!(
        r#"
        UPDATE media_cache
        SET last_accessed_at = CURRENT_TIMESTAMP
        WHERE sha256 = $1
        "#,
        sha256
    )
    .execute(pool)
    .await
    .context("Failed to touch media cache last_accessed_at in DB")
}

pub async fn get_expired(ttl_seconds: i64, limit: i64) -> Result<Vec<MediaCache>> {
    let cutoff = chrono::Utc::now() - chrono::Duration::seconds(ttl_seconds);
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
        WHERE last_accessed_at < $1
        ORDER BY last_accessed_at ASC
        LIMIT $2
        "#,
        cutoff,
        limit
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch expired media cache entries from DB")
}

pub async fn get_unreferenced(limit: i64) -> Result<Vec<MediaCache>> {
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
        WHERE ref_count <= 0
           OR NOT EXISTS (
               SELECT 1 FROM messages WHERE messages.media_sha256 = media_cache.sha256
           )
        ORDER BY last_accessed_at ASC
        LIMIT $1
        "#,
        limit
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch unreferenced media cache entries")
}

pub async fn prune_unreferenced() -> Result<Vec<MediaCache>> {
    let pool = crate::database::get();
    let unreferenced = get_unreferenced(500).await?;
    if unreferenced.is_empty() {
        return Ok(Vec::new());
    }

    // 1. Physically delete files on disk first
    for entry in &unreferenced {
        if let Err(err) = tokio::fs::remove_file(&entry.disk_path).await
            && err.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(
                sha256 = %entry.sha256,
                path = %entry.disk_path,
                error = %err,
                "Failed to delete pruned media file from disk"
            );
        }
    }

    // 2. Delete database rows within transaction
    let mut tx = pool
        .begin()
        .await
        .context("Failed to start prune transaction")?;
    for entry in &unreferenced {
        sqlx::query!(
            r#"
            DELETE FROM media_cache
            WHERE sha256 = $1
            "#,
            entry.sha256
        )
        .execute(&mut *tx)
        .await
        .context("Failed to delete unreferenced media cache row")?;
    }
    tx.commit()
        .await
        .context("Failed to commit prune transaction")?;

    info!(
        count = unreferenced.len(),
        "Pruned unreferenced media records and physical files"
    );
    Ok(unreferenced)
}
