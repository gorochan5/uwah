use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, QueryBuilder, SqlitePool, sqlite::SqliteQueryResult};
use tracing::info;

type GroupParticipant = crate::models::group_participants::GroupParticipant;

pub async fn get_all() -> Result<Vec<GroupParticipant>> {
    info!("Fetching all group participants from DB");

    let pool = crate::database::get();
    sqlx::query_as!(
        GroupParticipant,
        r#"
        SELECT
            session_id,
            group_jid,
            user_jid,
            role,
            joined_at AS "joined_at: chrono::DateTime<chrono::Utc>"
        FROM group_participants
        "#
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch group participants from DB")
}

pub async fn get_one(id: &str) -> Result<Option<GroupParticipant>> {
    info!(
        group_participant_id = id,
        "Fetching group participant from DB"
    );
    
    let pool = crate::database::get();
    sqlx::query_as!(
        GroupParticipant,
        r#"
        SELECT
            session_id,
            group_jid,
            user_jid,
            role,
            joined_at AS "joined_at: chrono::DateTime<chrono::Utc>"
        FROM group_participants
        WHERE session_id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to fetch group participant from DB")
}

pub async fn create(id: &str) -> Result<SqliteQueryResult> {
    info!(
        group_participant_id = id,
        "Creating group participant in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        INSERT INTO group_participants ( 
                session_id
            )
        VALUES (
                $1
            )
        "#,
        id
    )
    .execute(pool)
    .await
    .context("Failed to create new group participant on DB")
}

pub async fn delete(id: &str) -> Result<SqliteQueryResult>  {
    info!(
        group_participant_id = id,
        "Deleting group participant in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        DELETE FROM group_participants
        WHERE session_id = $1
        "#,
        id,
    )
    .execute(pool)
    .await
    .context("Failed to delete group participant from DB")
}
