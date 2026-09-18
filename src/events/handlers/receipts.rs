use anyhow::Result;
use tracing::{debug, error};
use whatsapp_rust::types::events::Receipt;
use whatsapp_rust::types::presence::ReceiptType;

pub async fn handle(session_id: &str, receipt: &Receipt) -> Result<()> {
    let status_str = match receipt.r#type {
        ReceiptType::Read
        | ReceiptType::ReadSelf
        | ReceiptType::Played
        | ReceiptType::PlayedSelf => "read",
        ReceiptType::Delivered => "delivered",
        ReceiptType::Sender => "sent",
        _ => return Ok(()),
    };

    let message_ids: Vec<String> = receipt
        .message_ids
        .iter()
        .map(|id| id.to_string())
        .collect();

    debug!(
        session_id = %session_id,
        status = %status_str,
        count = message_ids.len(),
        "Applying delivery receipt to messages"
    );

    if let Err(err) =
        crate::repository::messages::update_delivery_status(session_id, &message_ids, status_str)
            .await
    {
        error!(
            session_id = %session_id,
            error = %err,
            "Failed to update delivery status from receipt"
        );
    } else {
        crate::events::webhook::dispatch_event(
            "message.receipt",
            session_id,
            serde_json::json!({
                "status": status_str,
                "message_ids": message_ids,
                "chat_jid": receipt.source.chat.to_string(),
                "sender_jid": receipt.source.sender.to_string(),
            }),
        );
    }

    Ok(())
}
