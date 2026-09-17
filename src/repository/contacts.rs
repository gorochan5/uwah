use anyhow::{Context, Result};
use sqlx::sqlite::SqliteQueryResult;
use tracing::info;

type Contact = crate::models::contacts::Contact;

pub async fn get_all() -> Result<Vec<Contact>> {
    info!("Fetching all contacts from DB");

    let pool = crate::database::get();
    sqlx::query_as!(
        Contact,
        r#"
        SELECT
            session_id,
            jid,
            lid_jid,
            phone_number,
            push_name,
            status_text,
            profile_pic_url,
            is_business,
            username,
            devices,
            verified_name,
            updated_at AS "updated_at: chrono::DateTime<chrono::Utc>"
        FROM contacts
        "#
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch contacts from DB")
}

pub async fn get_by_session(session_id: &str) -> Result<Vec<Contact>> {
    info!(session_id = session_id, "Fetching contacts for session from DB");

    let pool = crate::database::get();
    sqlx::query_as!(
        Contact,
        r#"
        SELECT
            session_id,
            jid,
            lid_jid,
            phone_number,
            push_name,
            status_text,
            profile_pic_url,
            is_business,
            username,
            devices,
            verified_name,
            updated_at AS "updated_at: chrono::DateTime<chrono::Utc>"
        FROM contacts
        WHERE session_id = $1
        ORDER BY push_name ASC, jid ASC
        "#,
        session_id
    )
    .fetch_all(pool)
    .await
    .context("Failed to fetch contacts for session from DB")
}

pub async fn get_one(jid: &str, session_id: &str) -> Result<Option<Contact>> {
    info!(
        contact_jid = jid,
        session_id = session_id,
        "Fetching contact from DB"
    );
    
    let pool = crate::database::get();
    sqlx::query_as!(
        Contact,
        r#"
        SELECT
            session_id,
            jid,
            lid_jid,
            phone_number,
            push_name,
            status_text,
            profile_pic_url,
            is_business,
            username,
            devices,
            verified_name,
            updated_at AS "updated_at: chrono::DateTime<chrono::Utc>"
        FROM contacts
        WHERE jid = $1
        AND session_id = $2
        "#,
        jid,
        session_id
    )
    .fetch_optional(pool)
    .await
    .context("Failed to fetch contact from DB")
}

pub async fn create(jid: &str, session_id: &str) -> Result<SqliteQueryResult> {
    info!(
        contact_jid = jid,
        session_id = session_id,
        "Creating contact in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        INSERT INTO contacts ( 
                jid,
                session_id
            )
        VALUES (
                $1,
                $2
            )
        "#,
        jid,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to create new contact on DB")
}

pub async fn upsert(contact: &Contact) -> Result<SqliteQueryResult> {
    info!(
        contact_jid = contact.jid,
        session_id = contact.session_id,
        "Upserting contact in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        INSERT INTO contacts (
            session_id,
            jid,
            lid_jid,
            phone_number,
            push_name,
            status_text,
            profile_pic_url,
            is_business,
            username,
            devices,
            verified_name,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, CURRENT_TIMESTAMP)
        ON CONFLICT(session_id, jid) DO UPDATE SET
            lid_jid = excluded.lid_jid,
            phone_number = excluded.phone_number,
            push_name = excluded.push_name,
            status_text = excluded.status_text,
            profile_pic_url = excluded.profile_pic_url,
            is_business = excluded.is_business,
            username = excluded.username,
            devices = excluded.devices,
            verified_name = excluded.verified_name,
            updated_at = CURRENT_TIMESTAMP
        "#,
        contact.session_id,
        contact.jid,
        contact.lid_jid,
        contact.phone_number,
        contact.push_name,
        contact.status_text,
        contact.profile_pic_url,
        contact.is_business,
        contact.username,
        contact.devices,
        contact.verified_name,
    )
    .execute(pool)
    .await
    .context("Failed to upsert contact on DB")
}

pub async fn delete(jid: &str, session_id: &str) -> Result<SqliteQueryResult> {
    info!(
        contact_jid = jid,
        session_id = session_id,
        "Deleting contact in DB"
    );

    let pool = crate::database::get();
    sqlx::query!(
        r#"
        DELETE FROM contacts
        WHERE jid = $1
        AND session_id = $2
        "#,
        jid,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to delete contact from DB")
}
