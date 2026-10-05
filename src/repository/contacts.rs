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
    get_by_session_paginated(session_id, None, None, None).await
}

pub async fn get_by_session_paginated(
    session_id: &str,
    limit: Option<i64>,
    offset: Option<i64>,
    search: Option<&str>,
) -> Result<Vec<Contact>> {
    info!(
        session_id = session_id,
        limit = ?limit,
        offset = ?offset,
        search = ?search,
        "Fetching contacts for session from DB"
    );

    let pool = crate::database::get();
    let mut builder = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
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
            updated_at
        FROM contacts
        WHERE session_id = 
        "#
    );
    builder.push_bind(session_id);

    if let Some(s) = search {
        let s = s.trim();
        if !s.is_empty() {
            let pattern = format!("%{}%", s);
            builder.push(" AND (push_name LIKE ");
            builder.push_bind(pattern.clone());
            builder.push(" OR phone_number LIKE ");
            builder.push_bind(pattern.clone());
            builder.push(" OR jid LIKE ");
            builder.push_bind(pattern.clone());
            builder.push(" OR lid_jid LIKE ");
            builder.push_bind(pattern);
            builder.push(")");
        }
    }

    builder.push(" ORDER BY (push_name IS NULL OR push_name = '') ASC, push_name COLLATE NOCASE ASC, phone_number ASC, jid ASC");

    if let Some(l) = limit {
        let limit_val = l.clamp(1, 100);
        let offset_val = offset.unwrap_or(0).max(0);
        builder.push(" LIMIT ");
        builder.push_bind(limit_val);
        builder.push(" OFFSET ");
        builder.push_bind(offset_val);
    }

    let query = builder.build_query_as::<Contact>();
    query
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
    let pool = crate::database::get();
    info!(
        contact_jid = contact.jid,
        session_id = contact.session_id,
        "Upserting contact in DB"
    );

    // Defensively ensure root session exists
    sqlx::query!(
        r#"
        INSERT INTO sessions (id, status, created_at, updated_at)
        VALUES ($1, 'QRCODE', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO NOTHING
        "#,
        contact.session_id
    )
    .execute(pool)
    .await
    .context("Failed to assert session exists for contact in DB")?;

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
            lid_jid = COALESCE(excluded.lid_jid, contacts.lid_jid),
            phone_number = COALESCE(excluded.phone_number, contacts.phone_number),
            push_name = COALESCE(excluded.push_name, contacts.push_name),
            status_text = COALESCE(excluded.status_text, contacts.status_text),
            profile_pic_url = COALESCE(excluded.profile_pic_url, contacts.profile_pic_url),
            is_business = excluded.is_business,
            username = COALESCE(excluded.username, contacts.username),
            devices = COALESCE(excluded.devices, contacts.devices),
            verified_name = COALESCE(excluded.verified_name, contacts.verified_name),
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

pub async fn get_by_jid_or_lid(session_id: &str, identifier: &str) -> Result<Option<Contact>> {
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
        WHERE session_id = $1 AND (jid = $2 OR lid_jid = $2 OR phone_number = $2)
        LIMIT 1
        "#,
        session_id,
        identifier
    )
    .fetch_optional(pool)
    .await
    .context("Failed to fetch contact by JID/LID/Phone")
}

pub async fn upsert_batch(session_id: &str, contacts: &[Contact]) -> Result<usize> {
    if contacts.is_empty() {
        return Ok(0);
    }
    let pool = crate::database::get();
    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin transaction for batch contact sync")?;

    sqlx::query!(
        r#"
        INSERT INTO sessions (id, status, created_at, updated_at)
        VALUES ($1, 'QRCODE', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO NOTHING
        "#,
        session_id
    )
    .execute(&mut *tx)
    .await
    .context("Failed to assert session exists in batch")?;

    for contact in contacts {
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
        .execute(&mut *tx)
        .await
        .context("Failed to upsert contact in batch")?;
    }

    tx.commit()
        .await
        .context("Failed to commit batch contact sync transaction")?;
    Ok(contacts.len())
}

pub async fn migrate_jid(
    session_id: &str,
    old_jid: &str,
    new_jid: &str,
    new_lid: Option<&str>,
) -> Result<()> {
    let pool = crate::database::get();
    sqlx::query!(
        r#"
        UPDATE contacts
        SET jid = $1,
            lid_jid = COALESCE($2, lid_jid),
            updated_at = CURRENT_TIMESTAMP
        WHERE jid = $3 AND session_id = $4
        "#,
        new_jid,
        new_lid,
        old_jid,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to migrate contact JID in DB")?;

    Ok(())
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

pub async fn link_lid_to_pn(session_id: &str, pn_jid: &str, lid_jid: &str) -> Result<()> {
    let pool = crate::database::get();
    let mut tx = pool
        .begin()
        .await
        .context("Failed to begin transaction to link LID to PN")?;

    // Check if orphaned LID row exists
    let orphaned = sqlx::query!(
        r#"
        SELECT profile_pic_url, status_text, verified_name, push_name
        FROM contacts
        WHERE session_id = $1 AND jid = $2
        "#,
        session_id,
        lid_jid
    )
    .fetch_optional(&mut *tx)
    .await
    .context("Failed to check for orphaned LID contact")?;

    let phone = pn_jid.split('@').next();

    if let Some(orphan) = orphaned {
        // Merge orphaned LID fields into pn_jid row (inserting if not exists, or updating)
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
                verified_name,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, CURRENT_TIMESTAMP)
            ON CONFLICT(session_id, jid) DO UPDATE SET
                lid_jid = $3,
                phone_number = COALESCE(contacts.phone_number, excluded.phone_number),
                push_name = COALESCE(contacts.push_name, excluded.push_name),
                status_text = COALESCE(contacts.status_text, excluded.status_text),
                profile_pic_url = COALESCE(contacts.profile_pic_url, excluded.profile_pic_url),
                verified_name = COALESCE(contacts.verified_name, excluded.verified_name),
                updated_at = CURRENT_TIMESTAMP
            "#,
            session_id,
            pn_jid,
            lid_jid,
            phone,
            orphan.push_name,
            orphan.status_text,
            orphan.profile_pic_url,
            orphan.verified_name,
        )
        .execute(&mut *tx)
        .await
        .context("Failed to merge LID contact into PN contact")?;

        // Delete the orphaned LID row
        sqlx::query!(
            r#"
            DELETE FROM contacts
            WHERE session_id = $1 AND jid = $2
            "#,
            session_id,
            lid_jid
        )
        .execute(&mut *tx)
        .await
        .context("Failed to delete orphaned LID contact")?;
    } else {
        // Update pn_jid row's lid_jid (or insert if not exists)
        sqlx::query!(
            r#"
            INSERT INTO contacts (
                session_id,
                jid,
                lid_jid,
                phone_number,
                updated_at
            )
            VALUES ($1, $2, $3, $4, CURRENT_TIMESTAMP)
            ON CONFLICT(session_id, jid) DO UPDATE SET
                lid_jid = $3,
                phone_number = COALESCE(contacts.phone_number, excluded.phone_number),
                updated_at = CURRENT_TIMESTAMP
            "#,
            session_id,
            pn_jid,
            lid_jid,
            phone
        )
        .execute(&mut *tx)
        .await
        .context("Failed to link LID to PN contact")?;
    }

    tx.commit()
        .await
        .context("Failed to commit link LID to PN transaction")?;

    Ok(())
}

/// Idempotently ensures a contact exists for the session.
/// If the JID already exists in contacts: does nothing.
/// If the JID is a LID and already mapped to a contact (via lid_jid): does nothing.
/// If the JID does NOT exist: adds new contact using pushname as contact name if present, or None if absent.
pub async fn ensure_exists(
    session_id: &str,
    jid: &str,
    phone_number: Option<&str>,
    push_name: Option<&str>,
    lid_jid: Option<&str>,
) -> Result<SqliteQueryResult> {
    let pool = crate::database::get();

    // 1. Defensively assert root session exists
    sqlx::query!(
        r#"
        INSERT INTO sessions (id, status, created_at, updated_at)
        VALUES ($1, 'QRCODE', CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)
        ON CONFLICT(id) DO NOTHING
        "#,
        session_id
    )
    .execute(pool)
    .await
    .context("Failed to assert session exists for contact")?;

    // 2. If jid is a LID, check if a contact row already exists with lid_jid = jid
    if jid.ends_with("@lid") {
        let existing = sqlx::query!(
            r#"
            SELECT jid FROM contacts
            WHERE session_id = $1 AND lid_jid = $2
            LIMIT 1
            "#,
            session_id,
            jid
        )
        .fetch_optional(pool)
        .await
        .context("Failed to check for existing contact by lid_jid")?;

        if existing.is_some() {
            return Ok(SqliteQueryResult::default());
        }
    }

    // 3. Select contact name: pushname if present and non-empty, otherwise None
    let contact_name = push_name
        .map(str::trim)
        .filter(|s| !s.is_empty());

    // 4. Insert new contact; if already exists, do nothing
    sqlx::query!(
        r#"
        INSERT INTO contacts (
            session_id,
            jid,
            lid_jid,
            phone_number,
            push_name,
            updated_at
        )
        VALUES ($1, $2, $3, $4, $5, CURRENT_TIMESTAMP)
        ON CONFLICT(session_id, jid) DO NOTHING
        "#,
        session_id,
        jid,
        lid_jid,
        phone_number,
        contact_name
    )
    .execute(pool)
    .await
    .context("Failed to ensure contact exists in DB")
}

pub async fn update_profile_pic_url(
    session_id: &str,
    jid: &str,
    profile_pic_url: &str,
) -> Result<()> {
    let pool = crate::database::get();
    sqlx::query!(
        r#"
        UPDATE contacts
        SET profile_pic_url = $1,
            updated_at = CURRENT_TIMESTAMP
        WHERE session_id = $2 AND jid = $3
        "#,
        profile_pic_url,
        session_id,
        jid
    )
    .execute(pool)
    .await
    .context("Failed to update contact profile_pic_url in DB")?;

    Ok(())
}

