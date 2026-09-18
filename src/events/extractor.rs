use whatsapp_rust::download::Downloadable;
use whatsapp_rust::waproto::whatsapp as wa;

#[derive(Debug, Default, Clone)]
pub struct ExtractedMedia {
    pub cdn_url: Option<String>,
    pub media_key: Option<Vec<u8>>,
    pub file_enc_sha256: Option<String>,
    pub media_sha256: Option<String>,
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

pub fn extract_message_content(msg: &wa::Message) -> ExtractedMessage {
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
    } else if let Some(vid) = msg.video_message.as_option() {
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
