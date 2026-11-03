//! Thread history: read one conversation, list them, search them, rename, archive.

use tauri::AppHandle;

use super::{
    AskCmdrToolResultError, ConversationDetailView, ToolResultDetailView, with_read_connection, with_write_connection,
};
use crate::agent::chat::cancel::{paused_spend, turn_in_flight};
use crate::agent::chat::message_view::thread::{PageContext, project_page, tool_result_detail};
use crate::agent::store::{self, ConversationRow, ConversationSearchHit, TurnContext};

/// One conversation's header plus a page of its display messages (oldest first). `None`
/// when the thread is absent or the store never opened.
///
/// Each tool line on the page says what it looked at, what came back, and whether it repeats an
/// earlier call; each turn's last assistant view carries what the turn spent. All of that is
/// turn-scoped, so the rows completing the page's first and last turns are read too
/// (`store::turn_context`), never the whole thread.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_get_conversation(
    app: AppHandle,
    id: i64,
    msg_limit: u32,
    msg_offset: u32,
) -> Result<Option<ConversationDetailView>, String> {
    let memory_root = crate::agent::memory::store_for(&app).map(|memory| memory.root().to_path_buf());
    with_read_connection(app, None, move |conn| {
        // Read before the rows, so a turn that ends in between reads as running for one load
        // rather than as settled with its last results missing.
        let in_flight = turn_in_flight(id);
        let turn_in_flight_since_seq = in_flight.map(|turn| turn.since_seq);
        let Some(detail) = store::get_conversation(conn, id, msg_limit, msg_offset)? else {
            return Ok(None);
        };
        let context = match (detail.messages.first(), detail.messages.last()) {
            (Some(first), Some(last)) => store::turn_context(conn, id, first.seq, last.seq)?,
            _ => TurnContext::default(),
        };
        let messages = project_page(
            context,
            detail.messages,
            PageContext {
                in_flight_since_seq: turn_in_flight_since_seq,
                memory_root: memory_root.as_deref(),
            },
        );
        // The gauge's figure survives a restart, so reopening a thread shows what its last turn
        // actually cost rather than an empty bar.
        let last_context_usage =
            store::conversation_context_usage(conn, id)?.map(|(tokens, budget)| super::ContextUsageView {
                estimated_tokens: tokens as u32,
                budget_tokens: budget as u32,
            });
        Ok(Some(ConversationDetailView {
            conversation: detail.conversation,
            messages,
            total_messages: detail.total_messages,
            last_context_usage,
            turn_in_flight_since_seq,
            turn_in_flight_run: in_flight.map(|turn| turn.run),
            spend_paused_micros: paused_spend(id),
        }))
    })
    .await
}

/// What one tool line's call returned, for its expanded view: the curated preview plus the raw
/// result for Copy. The line is `(message_id, call_id)`, since call ids collide across rounds, and
/// `occurrence` picks among calls one row names with the same id (0 for the first).
/// ❌ The content is private (file text, EXIF incl. GPS): only the store's failure is logged.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_tool_result(
    app: AppHandle,
    conversation_id: i64,
    message_id: i64,
    call_id: String,
    occurrence: u32,
) -> Result<ToolResultDetailView, AskCmdrToolResultError> {
    let Some(db_path) = super::db_path(&app) else {
        return Err(AskCmdrToolResultError::StoreUnavailable);
    };
    let read = tauri::async_runtime::spawn_blocking(move || {
        let conn = store::open_read_connection(&db_path)?;
        store::row_and_rest_of_turn(&conn, conversation_id, message_id)
    })
    .await;
    match read {
        Ok(Ok(rows)) => rows
            .and_then(|rows| tool_result_detail(rows, &call_id, occurrence as usize))
            .ok_or(AskCmdrToolResultError::NotFound),
        Ok(Err(error)) => {
            log::warn!(target: super::LOG_TARGET, "ask_cmdr_tool_result: the store wouldn't read: {error}");
            Err(AskCmdrToolResultError::StoreUnavailable)
        }
        Err(error) => {
            log::warn!(target: super::LOG_TARGET, "ask_cmdr_tool_result: the read task ended early: {error}");
            Err(AskCmdrToolResultError::StoreUnavailable)
        }
    }
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

/// Conversations matching `query` (title or FTS5 message text, sanitized), best first, paged
/// (`agent/store/search.rs`). Each hit carries its excerpt as plain-text runs, matches flagged. Empty when the store
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
