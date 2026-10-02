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

            crate::events::webhook::dispatch_event(
                "group.subject_updated",
                session_id,
                serde_json::json!({
                    "group_jid": group_jid,
                    "subject": subject,
                    "owner_jid": owner_str,
                    "timestamp": subject_time,
                }),
            );
        }
        GroupNotificationAction::Add { participants, .. } => {
            let mut added_jids = Vec::new();
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
                    user_jid: participant_jid.clone(),
                    role: "member".to_string(),
                    joined_at: update.timestamp,
                };
                let _ = crate::repository::group_participants::upsert(&participant).await;
                added_jids.push(participant_jid);
            }

            crate::events::webhook::dispatch_event(
                "group.participant_added",
                session_id,
                serde_json::json!({
                    "group_jid": group_jid,
                    "participants": added_jids,
                    "timestamp": update.timestamp,
                }),
            );
        }
        GroupNotificationAction::Remove { participants, .. } => {
            let mut removed_jids = Vec::new();
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
                removed_jids.push(participant_jid);
            }

            crate::events::webhook::dispatch_event(
                "group.participant_removed",
                session_id,
                serde_json::json!({
                    "group_jid": group_jid,
                    "participants": removed_jids,
                    "timestamp": update.timestamp,
                }),
            );
        }
        GroupNotificationAction::Promote { participants } => {
            let mut promoted_jids = Vec::new();
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
                promoted_jids.push(participant_jid);
            }

            crate::events::webhook::dispatch_event(
                "group.participant_promoted",
                session_id,
                serde_json::json!({
                    "group_jid": group_jid,
                    "participants": promoted_jids,
                    "timestamp": update.timestamp,
                }),
            );
        }
        GroupNotificationAction::Demote { participants } => {
            let mut demoted_jids = Vec::new();
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
                demoted_jids.push(participant_jid);
            }

            crate::events::webhook::dispatch_event(
                "group.participant_demoted",
                session_id,
                serde_json::json!({
                    "group_jid": group_jid,
                    "participants": demoted_jids,
                    "timestamp": update.timestamp,
                }),
            );
        }
        _ => {}
    }

    Ok(())
}
