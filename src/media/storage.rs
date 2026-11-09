use std::path::{Path, PathBuf};

/// Sanitizes folder/file name removing invalid characters for Windows/Linux filesystems.
pub fn sanitize_name(name: &str) -> String {
    let invalid_chars = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    let cleaned: String = name
        .chars()
        .map(|c| {
            if invalid_chars.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();

    let trimmed = cleaned.trim().trim_matches('.');
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Generates organized profile picture path:
/// `media/profile_pictures/{groups|contacts}/{clean_name}/{sha256}.jpg`
pub fn get_profile_picture_path(
    media_dir: &str,
    is_group: bool,
    name_or_jid: &str,
    sha256: &str,
) -> PathBuf {
    let category = if is_group { "groups" } else { "contacts" };
    let clean_name = sanitize_name(name_or_jid);
    Path::new(media_dir)
        .join("profile_pictures")
        .join(category)
        .join(clean_name)
        .join(format!("{}.jpg", sha256))
}

/// Generates organized chat media path:
/// `media/chats/{groups|direct}/{clean_chat_name}/{media_type}/{sha256}.{ext}`
pub fn get_chat_media_path(
    media_dir: &str,
    is_group: bool,
    chat_name_or_jid: &str,
    media_type: &str,
    sha256: &str,
    ext: &str,
) -> PathBuf {
    let group_or_direct = if is_group { "groups" } else { "direct" };
    let clean_chat = sanitize_name(chat_name_or_jid);
    let clean_type = sanitize_name(media_type);
    Path::new(media_dir)
        .join("chats")
        .join(group_or_direct)
        .join(clean_chat)
        .join(clean_type)
        .join(format!("{}.{}", sha256, ext))
}

#[cfg(test)]
mod tests {
    use super::{get_chat_media_path, get_profile_picture_path, sanitize_name};
    use std::path::Path;

    #[test]
    fn sanitize_name_replaces_invalid_and_control_characters() {
        assert_eq!(sanitize_name("team/name?\n"), "team_name__");
    }

    #[test]
    fn sanitize_name_trims_surrounding_space_and_dots() {
        assert_eq!(sanitize_name("  ..photo..  "), "photo");
        assert_eq!(sanitize_name("..."), "unknown");
        assert_eq!(sanitize_name(" \t "), "unknown");
    }

    #[test]
    fn generated_paths_use_sanitized_components_and_expected_category() {
        let profile = get_profile_picture_path("media", true, "../Group/Name", "abc");
        assert_eq!(profile, Path::new("media/profile_pictures/groups/.._Group_Name/abc.jpg"));

        let chat = get_chat_media_path("media", false, "Alice", "image/raw", "def", "webp");
        assert_eq!(chat, Path::new("media/chats/direct/Alice/image_raw/def.webp"));
    }
}

