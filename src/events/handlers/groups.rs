use crate::models::group_participants::GroupParticipant;
use anyhow::Result;
use tracing::{error, info};
use whatsapp_rust::types::events::GroupUpdate;
use whatsapp_rust::wacore::stanza::groups::GroupNotificationAction;

pub async fn handle(session_id: &str, update: &GroupUpdate) -> Result<()> {
    let group_jid = update.group_jid.to_string();

    info!(
        session_id = %session_id,
        group_jid = %group_jid,
        "Processing WhatsApp group update"
    );

    // 1. Ensure parent group chat exists in DB
    if let Err(err) = crate::repository::chats::ensure_exists(&group_jid, session_id).await {
        error!(
            session_id = %session_id,
            group_jid = %group_jid,
            error = %err,
            "Failed to ensure group chat exists in DB"
        );
        return Err(err);
    }

    // 2. Dispatch specific group notification action
    match &update.action {
        GroupNotificationAction::Subject {
            subject,
            subject_owner,
            subject_time,
            ..
        } => {
            let owner_str = subject_owner.as_ref().map(|j| j.to_string());
            let _ = crate::repository::chats::update_subject(
                session_id,
                &group_jid,
                Some(subject.as_str()),
                owner_str.as_deref(),
                subject_time.map(|t| t as i64),
            )
            .await;
        }
        GroupNotificationAction::Add { participants, .. } => {
            for p in participants {
                let participant_jid = p.jid.to_string();
                info!(
                    session_id = %session_id,
                    group_jid = %group_jid,
                    user_jid = %participant_jid,
                    "Adding participant to group"
                );
                let participant = GroupParticipant {
                    session_id: session_id.to_string(),
                    group_jid: group_jid.clone(),
                    user_jid: participant_jid,
                    role: "member".to_string(),
                    joined_at: update.timestamp,
                };
                let _ = crate::repository::group_participants::upsert(&participant).await;
            }
        }
        GroupNotificationAction::Remove { participants, .. } => {
            for p in participants {
                let participant_jid = p.jid.to_string();
                info!(
                    session_id = %session_id,
                    group_jid = %group_jid,
                    user_jid = %participant_jid,
                    "Removing participant from group"
                );
                let _ = crate::repository::group_participants::delete(
                    session_id,
                    &group_jid,
                    &participant_jid,
                )
                .await;
            }
        }
        GroupNotificationAction::Promote { participants } => {
            for p in participants {
                let participant_jid = p.jid.to_string();
                info!(
                    session_id = %session_id,
                    group_jid = %group_jid,
                    user_jid = %participant_jid,
                    "Promoting participant to admin"
                );
                let _ = crate::repository::group_participants::update_role(
                    session_id,
                    &group_jid,
                    &participant_jid,
                    "admin",
                )
                .await;
            }
        }
        GroupNotificationAction::Demote { participants } => {
            for p in participants {
                let participant_jid = p.jid.to_string();
                info!(
                    session_id = %session_id,
                    group_jid = %group_jid,
                    user_jid = %participant_jid,
                    "Demoting participant to member"
                );
                let _ = crate::repository::group_participants::update_role(
                    session_id,
                    &group_jid,
                    &participant_jid,
                    "member",
                )
                .await;
            }
        }
        _ => {}
    }

    Ok(())
}
