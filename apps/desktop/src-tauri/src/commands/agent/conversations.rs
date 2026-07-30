//! Thread history: read one conversation, list them, search them, rename, archive.

use tauri::AppHandle;

use super::views::to_message_view;
use super::{ConversationDetailView, with_read_connection, with_write_connection};
use crate::agent::store::{self, ConversationRow, ConversationSearchHit};

/// One conversation's header plus a page of its display messages (oldest first). `None`
/// when the thread is absent or the store never opened.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_get_conversation(
    app: AppHandle,
    id: i64,
    msg_limit: u32,
    msg_offset: u32,
) -> Result<Option<ConversationDetailView>, String> {
    with_read_connection(app, None, move |conn| {
        let Some(detail) = store::get_conversation(conn, id, msg_limit, msg_offset)? else {
            return Ok(None);
        };
        // The gauge's figure survives a restart, so reopening a thread shows what its last turn
        // actually cost rather than an empty bar.
        let last_context_usage =
            store::conversation_context_usage(conn, id)?.map(|(tokens, budget)| super::ContextUsageView {
                estimated_tokens: tokens as u32,
                budget_tokens: budget as u32,
            });
        Ok(Some(ConversationDetailView {
            conversation: detail.conversation,
            messages: detail.messages.into_iter().map(to_message_view).collect(),
            total_messages: detail.total_messages,
            last_context_usage,
        }))
    })
    .await
}

/// Conversations newest-activity first, paged. Empty when the store never opened.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_list_conversations(
    app: AppHandle,
    limit: u32,
    offset: u32,
    include_archived: bool,
) -> Result<Vec<ConversationRow>, String> {
    with_read_connection(app, Vec::new(), move |conn| {
        store::list_conversations(conn, limit, offset, include_archived)
    })
    .await
}

/// Conversations whose messages match `query` (FTS5, sanitized), newest-match first,
/// paged. Each hit carries a plain-text snippet around the match. Empty when the store
/// never opened or the query has no searchable term.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_search_conversations(
    app: AppHandle,
    query: String,
    limit: u32,
    offset: u32,
) -> Result<Vec<ConversationSearchHit>, String> {
    with_read_connection(app, Vec::new(), move |conn| {
        store::search_conversations(conn, &query, limit, offset)
    })
    .await
}

/// Rename a conversation. A no-op when the store never opened.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_rename_conversation(app: AppHandle, id: i64, title: String) -> Result<(), String> {
    with_write_connection(app, move |conn| store::rename_conversation(conn, id, &title)).await
}

/// Archive or unarchive a conversation (no delete in v1 — the flag filters the list). A
/// no-op when the store never opened.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_archive_conversation(app: AppHandle, id: i64, archived: bool) -> Result<(), String> {
    with_write_connection(app, move |conn| store::archive_conversation(conn, id, archived)).await
}
