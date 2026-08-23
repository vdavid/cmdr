//! The one insert every `messages` row goes through.
//!
//! Its own module because both writers need it and neither may depend on the other: `query.rs`
//! appends transcript messages and `events.rs` appends timeline events, while `query.rs`
//! already names `ConversationEvent` for `StoredContent`. Leaving this in `query.rs` puts the
//! two modules in a cycle (`module-cycles` catches it), and the shared piece is genuinely
//! neither one's.

use rusqlite::Connection;

use super::AgentStoreError;

/// The shared insert for message and event rows: derive the per-conversation `seq` and
/// bump the conversation's `updated_at`, all inside one transaction so the seq can't race
/// and the two writes commit together.
#[allow(
    clippy::too_many_arguments,
    reason = "one row's full column set; a params struct would just relocate the arity"
)]
pub(super) fn insert_message_row(
    conn: &Connection,
    conversation_id: i64,
    role_token: &str,
    content_blocks: &str,
    text_for_search: &str,
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    now: i64,
) -> Result<(i64, i64), AgentStoreError> {
    let tx = conn.unchecked_transaction()?;
    let seq: i64 = tx.query_row(
        "SELECT COALESCE(MAX(seq), -1) + 1 FROM messages WHERE conversation_id = ?1",
        rusqlite::params![conversation_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO messages
            (conversation_id, seq, role, content_blocks, text_for_search, prompt_tokens, completion_tokens, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        rusqlite::params![
            conversation_id,
            seq,
            role_token,
            content_blocks,
            text_for_search,
            prompt_tokens,
            completion_tokens,
            now,
        ],
    )?;
    let message_id = tx.last_insert_rowid();
    tx.execute(
        "UPDATE conversations SET updated_at = ?2 WHERE id = ?1",
        rusqlite::params![conversation_id, now],
    )?;
    tx.commit()?;
    Ok((message_id, seq))
}
