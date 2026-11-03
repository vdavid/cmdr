//! Where a wake thread's activity happened, for its display title.
//!
//! A wake names its thread after the busiest folder's BASENAME (`wake::job::thread_title`), so a
//! list of them reads `veszelovszki`, `tmp`, `Downloads`: data with no clue what the thread is.
//! The stored title stays that way (a backend English sentence would be untranslatable copy
//! frozen into `main.db`), and this hands the frontend the structure to say "Activity in
//! ~/tmp and 2 other folders" in the user's language instead.
//!
//! Only while the title is still the wake's own: a thread the user renamed shows their name.

use rusqlite::Connection;

use super::AgentStoreError;
use crate::agent::llm::types::{AgentPart, WakeDigest};
use crate::agent::types::ConversationOrigin;

/// The place a wake thread's digest opens with. Wire type.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WakePlace {
    /// The first folder the digest names, `~`-relative when under the home folder.
    pub folder: String,
    /// How many more folders the digest covers, rolled-up ones included.
    pub other_folders: u32,
}

/// `path` with the home folder written as `~`, or unchanged outside it.
pub(super) fn home_relative(path: &str, home: Option<&str>) -> String {
    let Some(home) = home
        .map(|home| home.trim_end_matches('/'))
        .filter(|home| !home.is_empty())
    else {
        return path.to_string();
    };
    if path == home {
        return "~".to_string();
    }
    match path.strip_prefix(home) {
        Some(rest) if rest.starts_with('/') => format!("~{rest}"),
        _ => path.to_string(),
    }
}

/// The place for a digest whose thread is titled `title`, or `None` when the user renamed it
/// (the title no longer equals the wake's own) or the digest names nowhere.
pub(super) fn place_of(digest: &WakeDigest, title: &str, home: Option<&str>) -> Option<WakePlace> {
    let path = digest.place()?;
    if WakeDigest::title_for(path) != title {
        return None;
    }
    let other_folders = digest.folder_count().saturating_sub(1);
    Some(WakePlace {
        folder: home_relative(path, home),
        other_folders: u32::try_from(other_folders).unwrap_or(u32::MAX),
    })
}

/// The wake place for conversation `id`: `None` unless the agent opened it for a wake and its
/// title is still the one the wake gave it. Reads the thread's first message (the digest).
pub(super) fn wake_place(
    conn: &Connection,
    id: i64,
    origin: Option<ConversationOrigin>,
    title: &str,
) -> Result<Option<WakePlace>, AgentStoreError> {
    if origin != Some(ConversationOrigin::Notification) {
        return Ok(None);
    }
    let content_blocks: Option<String> = {
        let mut stmt = conn.prepare_cached(
            "SELECT content_blocks FROM messages WHERE conversation_id = ?1 AND role = 'user' ORDER BY seq ASC LIMIT 1",
        )?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        match rows.next()? {
            Some(row) => Some(row.get(0)?),
            None => None,
        }
    };
    let Some(content_blocks) = content_blocks else {
        return Ok(None);
    };
    // A first message that won't parse (a newer build's part) just keeps the stored title:
    // a list row isn't worth failing over.
    let Ok(parts) = serde_json::from_str::<Vec<AgentPart>>(&content_blocks) else {
        return Ok(None);
    };
    let home = dirs::home_dir();
    let home = home.as_deref().and_then(|home| home.to_str());
    Ok(parts.iter().find_map(|part| match part {
        AgentPart::WakeDigest(digest) => place_of(digest, title, home),
        _ => None,
    }))
}
