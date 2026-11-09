use whatsapp_rust::download::Downloadable;
use whatsapp_rust::waproto::whatsapp as wa;

#[derive(Debug, Default, Clone)]
pub struct ExtractedMedia {
    pub cdn_url: Option<String>,
    pub media_key: Option<Vec<u8>>,
    pub file_enc_sha256: Option<String>,
    pub media_sha256: Option<String>,
    pub raw_file_sha256: Option<Vec<u8>>,
    pub raw_file_enc_sha256: Option<Vec<u8>>,
    pub file_length: Option<u64>,
}

pub fn extract_media<T: Downloadable>(item: &T) -> ExtractedMedia {
    ExtractedMedia {
        cdn_url: item
            .direct_path()
            .map(str::to_string)
            .or_else(|| item.static_url().map(str::to_string)),
        media_key: item.media_key().map(|k| k.to_vec()),
        file_enc_sha256: item
            .file_enc_sha256()
            .map(|bytes| bytes.iter().map(|b| format!("{:02x}", b)).collect()),
        // NOTE: Inbound messages keep media_sha256 as None until media download service
        // writes the file to disk and registers the SHA-256 into media_cache table.
        // This strictly prevents SQLite foreign key constraint violation (787).
        media_sha256: None,
        raw_file_sha256: item.file_sha256().map(|b| b.to_vec()),
        raw_file_enc_sha256: item.file_enc_sha256().map(|b| b.to_vec()),
        file_length: item.file_length(),
    }
}

#[derive(Debug, Default, Clone)]
pub struct ExtractedMessage {
    pub message_type: String,
    pub body: Option<String>,
    pub media: Option<ExtractedMedia>,
    pub quoted_message_id: Option<String>,
    pub edit_target_id: Option<String>,
    pub is_edited: bool,
    pub is_revoked: bool,
}

pub fn unwrap_base_message(mut msg: &wa::Message) -> &wa::Message {
    loop {
        if let Some(ephem) = msg.ephemeral_message.as_option()
            && let Some(inner) = ephem.message.as_option() {
            msg = inner;
            continue;
        }
        if let Some(vo) = msg.view_once_message.as_option()
            && let Some(inner) = vo.message.as_option() {
            msg = inner;
            continue;
        }
        if let Some(vo2) = msg.view_once_message_v2.as_option()
            && let Some(inner) = vo2.message.as_option() {
            msg = inner;
            continue;
        }
        if let Some(vo2_ext) = msg.view_once_message_v2_extension.as_option()
            && let Some(inner) = vo2_ext.message.as_option() {
            msg = inner;
            continue;
        }
        if let Some(doc) = msg.document_with_caption_message.as_option()
            && let Some(inner) = doc.message.as_option() {
            msg = inner;
            continue;
        }
        if let Some(dev) = msg.device_sent_message.as_option()
            && let Some(inner) = dev.message.as_option() {
            msg = inner;
            continue;
        }
        break;
    }
    msg
}

pub fn extract_message_content(raw_msg: &wa::Message) -> ExtractedMessage {
    let msg = unwrap_base_message(raw_msg);
    if let Some(text) = &msg.conversation {
        ExtractedMessage {
            message_type: "text".to_string(),
            body: Some(text.clone()),
            media: None,
            quoted_message_id: None,
            edit_target_id: None,
            is_edited: false,
            is_revoked: false,
        }
    } else if let Some(ext) = msg.extended_text_message.as_option() {
        let quoted_id = ext
            .context_info
            .as_option()
            .and_then(|c| c.stanza_id.clone());
        ExtractedMessage {
            message_type: "text".to_string(),
            body: ext.text.clone(),
            media: None,
            quoted_message_id: quoted_id,
            edit_target_id: None,
            is_edited: false,
            is_revoked: false,
        }
    } else if let Some(img) = msg.image_message.as_option() {
        let quoted_id = img
            .context_info
            .as_option()
            .and_then(|c| c.stanza_id.clone());
        ExtractedMessage {
            message_type: "image".to_string(),
            body: img.caption.clone(),
            media: Some(extract_media(img)),
            quoted_message_id: quoted_id,
            edit_target_id: None,
            is_edited: false,
            is_revoked: false,
        }
    } else if let Some(vid) = msg.video_message.as_option().or_else(|| msg.ptv_message.as_option()) {
        let quoted_id = vid
            .context_info
            .as_option()
            .and_then(|c| c.stanza_id.clone());
        ExtractedMessage {
            message_type: "video".to_string(),
            body: vid.caption.clone(),
            media: Some(extract_media(vid)),
            quoted_message_id: quoted_id,
            edit_target_id: None,
            is_edited: false,
            is_revoked: false,
        }
    } else if let Some(aud) = msg.audio_message.as_option() {
        let quoted_id = aud
            .context_info
            .as_option()
            .and_then(|c| c.stanza_id.clone());
        ExtractedMessage {
            message_type: "audio".to_string(),
            body: None,
            media: Some(extract_media(aud)),
            quoted_message_id: quoted_id,
            edit_target_id: None,
            is_edited: false,
            is_revoked: false,
        }
    } else if let Some(doc) = msg.document_message.as_option() {
        let quoted_id = doc
            .context_info
            .as_option()
            .and_then(|c| c.stanza_id.clone());
        let title = doc.title.clone().or_else(|| doc.file_name.clone());
        ExtractedMessage {
            message_type: "document".to_string(),
            body: title,
            media: Some(extract_media(doc)),
            quoted_message_id: quoted_id,
            edit_target_id: None,
            is_edited: false,
            is_revoked: false,
        }
    } else if let Some(stk) = msg.sticker_message.as_option() {
        let quoted_id = stk
            .context_info
            .as_option()
            .and_then(|c| c.stanza_id.clone());
        ExtractedMessage {
            message_type: "sticker".to_string(),
            body: None,
            media: Some(extract_media(stk)),
            quoted_message_id: quoted_id,
            edit_target_id: None,
            is_edited: false,
            is_revoked: false,
        }
    } else if let Some(pack) = msg.sticker_pack_message.as_option() {
        let quoted_id = pack
            .context_info
            .as_option()
            .and_then(|c| c.stanza_id.clone());
        ExtractedMessage {
            message_type: "sticker_pack".to_string(),
            body: pack.name.clone().or_else(|| pack.caption.clone()),
            media: Some(extract_media(pack)),
            quoted_message_id: quoted_id,
            edit_target_id: None,
            is_edited: false,
            is_revoked: false,
        }
    } else if let Some(loc) = msg.location_message.as_option() {
        let quoted_id = loc
            .context_info
            .as_option()
            .and_then(|c| c.stanza_id.clone());
        let loc_info = serde_json::json!({
            "latitude": loc.degrees_latitude,
            "longitude": loc.degrees_longitude,
            "name": loc.name,
            "address": loc.address,
            "url": loc.url,
            "comment": loc.comment,
        });
        ExtractedMessage {
            message_type: "location".to_string(),
            body: Some(loc_info.to_string()),
            media: None,
            quoted_message_id: quoted_id,
            edit_target_id: None,
            is_edited: false,
            is_revoked: false,
        }
    } else if let Some(live_loc) = msg.live_location_message.as_option() {
        let quoted_id = live_loc
            .context_info
            .as_option()
            .and_then(|c| c.stanza_id.clone());
        let loc_info = serde_json::json!({
            "latitude": live_loc.degrees_latitude,
            "longitude": live_loc.degrees_longitude,
            "caption": live_loc.caption,
            "accuracy": live_loc.accuracy_in_meters,
            "speed": live_loc.speed_in_mps,
        });
        ExtractedMessage {
            message_type: "live_location".to_string(),
            body: Some(loc_info.to_string()),
            media: None,
            quoted_message_id: quoted_id,
            edit_target_id: None,
            is_edited: false,
            is_revoked: false,
        }
    } else if let Some(contact) = msg.contact_message.as_option() {
        let quoted_id = contact
            .context_info
            .as_option()
            .and_then(|c| c.stanza_id.clone());
        let contact_info = serde_json::json!({
            "display_name": contact.display_name,
            "vcard": contact.vcard,
        });
        ExtractedMessage {
            message_type: "contact".to_string(),
            body: Some(contact_info.to_string()),
            media: None,
            quoted_message_id: quoted_id,
            edit_target_id: None,
            is_edited: false,
            is_revoked: false,
        }
    } else if let Some(contacts) = msg.contacts_array_message.as_option() {
        let quoted_id = contacts
            .context_info
            .as_option()
            .and_then(|c| c.stanza_id.clone());
        let contact_list: Vec<_> = contacts
            .contacts
            .iter()
            .map(|c| {
                serde_json::json!({
                    "display_name": c.display_name,
                    "vcard": c.vcard,
                })
            })
            .collect();
        let contact_info = serde_json::json!({
            "display_name": contacts.display_name,
            "contacts": contact_list,
        });
        ExtractedMessage {
            message_type: "contacts_array".to_string(),
            body: Some(contact_info.to_string()),
            media: None,
            quoted_message_id: quoted_id,
            edit_target_id: None,
            is_edited: false,
            is_revoked: false,
        }
    } else if let Some(reaction) = msg.reaction_message.as_option() {
        let target_id = reaction.key.as_option().and_then(|k| k.id.clone());
        ExtractedMessage {
            message_type: "reaction".to_string(),
            body: reaction.text.clone(),
            media: None,
            quoted_message_id: None,
            edit_target_id: target_id,
            is_edited: false,
            is_revoked: false,
        }
    } else if let Some(proto) = msg.protocol_message.as_option() {
        let target_id = proto.key.as_option().and_then(|k| k.id.clone());
        let is_revoked = matches!(
            proto.r#type,
            Some(wa::message::protocol_message::Type::REVOKE)
        );
        let is_edited = matches!(
            proto.r#type,
            Some(wa::message::protocol_message::Type::MESSAGE_EDIT)
        );
        let edit_body = proto.edited_message.as_option().and_then(|em| {
            em.conversation.clone().or_else(|| {
                em.extended_text_message
                    .as_option()
                    .and_then(|et| et.text.clone())
            })
        });

        ExtractedMessage {
            message_type: "protocol".to_string(),
            body: edit_body,
            media: None,
            quoted_message_id: None,
            edit_target_id: target_id,
            is_edited,
            is_revoked,
        }
    } else {
        ExtractedMessage {
            message_type: "other".to_string(),
            body: None,
            media: None,
            quoted_message_id: None,
            edit_target_id: None,
            is_edited: false,
            is_revoked: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_message_content, unwrap_base_message};
    use whatsapp_rust::waproto::whatsapp as wa;
    use whatsapp_rust::prelude::MessageField;

    #[test]
    fn extracts_conversation_text() {
        let raw = wa::Message { conversation: Some("hello".into()), ..Default::default() };
        let extracted = extract_message_content(&raw);
        assert_eq!(extracted.message_type, "text");
        assert_eq!(extracted.body.as_deref(), Some("hello"));
        assert!(extracted.media.is_none());
    }

    #[test]
    fn unwraps_ephemeral_message() {
        let inner = wa::Message { conversation: Some("wrapped".into()), ..Default::default() };
        let raw = wa::Message {
            ephemeral_message: MessageField::some(wa::message::FutureProofMessage {
                message: MessageField::some(inner),
            }),
            ..Default::default()
        };
        assert_eq!(unwrap_base_message(&raw).conversation.as_deref(), Some("wrapped"));
        assert_eq!(extract_message_content(&raw).body.as_deref(), Some("wrapped"));
    }

    #[test]
    fn unknown_message_has_empty_fallback() {
        let extracted = extract_message_content(&wa::Message::default());
        assert_eq!(extracted.message_type, "other");
        assert!(extracted.body.is_none());
        assert!(extracted.media.is_none());
    }
}
