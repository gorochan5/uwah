use anyhow::{Context, Result};
use sqlx::sqlite::SqliteQueryResult;
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

pub async fn get_by_group(session_id: &str, group_jid: &str) -> Result<Vec<GroupParticipant>> {
    info!(
        session_id = session_id,
        group_jid = group_jid,
        "Fetching participants for group from DB"
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
        AND group_jid = $2
        "#,
        session_id,
        group_jid
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch group participants from DB")
}

pub async fn get_one(
    session_id: &str,
    group_jid: &str,
    user_jid: &str,
) -> Result<Option<GroupParticipant>> {
    info!(
        session_id = session_id,
        group_jid = group_jid,
        user_jid = user_jid,
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
        AND group_jid = $2
        AND user_jid = $3
        "#,
        session_id,
        group_jid,
        user_jid
    )
    .fetch_optional(pool)
    .await
    .context("Failed to fetch group participant from DB")
}

pub async fn create(participant: &GroupParticipant) -> Result<SqliteQueryResult> {
    info!(
        session_id = participant.session_id,
        group_jid = participant.group_jid,
        user_jid = participant.user_jid,
        "Creating group participant in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        INSERT INTO group_participants ( 
            session_id,
            group_jid,
            user_jid,
            role,
            joined_at
        )
        VALUES (
            $1, $2, $3, $4, $5
        )
        "#,
        participant.session_id,
        participant.group_jid,
        participant.user_jid,
        participant.role,
        participant.joined_at
    )
    .execute(pool)
    .await
    .context("Failed to create new group participant on DB")
}

pub async fn delete(
    session_id: &str,
    group_jid: &str,
    user_jid: &str,
) -> Result<SqliteQueryResult> {
    info!(
        session_id = session_id,
        group_jid = group_jid,
        user_jid = user_jid,
        "Deleting group participant in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        DELETE FROM group_participants
        WHERE session_id = $1
        AND group_jid = $2
        AND user_jid = $3
        "#,
        session_id,
        group_jid,
        user_jid
    )
    .execute(pool)
    .await
    .context("Failed to delete group participant from DB")
}

pub async fn upsert(participant: &GroupParticipant) -> Result<SqliteQueryResult> {
    // Defensively ensure parent group chat exists
    crate::repository::chats::ensure_exists(&participant.group_jid, &participant.session_id)
        .await?;

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        INSERT INTO group_participants (session_id, group_jid, user_jid, role, joined_at)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT(session_id, group_jid, user_jid) DO UPDATE SET
            role = excluded.role,
            joined_at = MIN(group_participants.joined_at, excluded.joined_at)
        "#,
        participant.session_id,
        participant.group_jid,
        participant.user_jid,
        participant.role,
        participant.joined_at
    )
    .execute(pool)
    .await
    .context("Failed to upsert group participant in DB")
}

pub async fn update_role(
    session_id: &str,
    group_jid: &str,
    user_jid: &str,
    role: &str,
) -> Result<()> {
    let pool = crate::database::get();
    sqlx::query!(
        r#"
        UPDATE group_participants
        SET role = $1
        WHERE session_id = $2 AND group_jid = $3 AND user_jid = $4
        "#,
        role,
        session_id,
        group_jid,
        user_jid
    )
    .execute(pool)
    .await
    .context("Failed to update group participant role in DB")?;

    Ok(())
}
