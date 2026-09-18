use crate::models::contacts::Contact;
use anyhow::Result;
use tracing::info;
use whatsapp_rust::types::events::{ContactNumberChanged, ContactUpdated};

pub async fn handle_updated(session_id: &str, update: &ContactUpdated) -> Result<()> {
    let jid = update.jid.to_string();
    info!(
        session_id = %session_id,
        jid = %jid,
        "Contact updated notification received"
    );

    let contact = Contact {
        session_id: session_id.to_string(),
        jid: jid.clone(),
        lid_jid: None,
        phone_number: Some(update.jid.user.to_string()),
        push_name: None,
        status_text: None,
        profile_pic_url: None,
        is_business: false,
        username: None,
        devices: None,
        verified_name: None,
        updated_at: update.timestamp,
    };

    crate::repository::contacts::upsert(&contact).await?;
    Ok(())
}

pub async fn handle_changed(session_id: &str, update: &ContactNumberChanged) -> Result<()> {
    let old_jid = update.old_jid.to_string();
    let new_jid = update.new_jid.to_string();
    let new_lid = update.new_lid.as_ref().map(|j| j.to_string());

    info!(
        session_id = %session_id,
        old_jid = %old_jid,
        new_jid = %new_jid,
        new_lid = ?new_lid,
        "Contact phone number migration received"
    );

    crate::repository::contacts::migrate_jid(session_id, &old_jid, &new_jid, new_lid.as_deref())
        .await?;

    Ok(())
}
