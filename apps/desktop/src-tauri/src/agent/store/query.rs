//! The read/write query layer over `main.db`: conversations, messages, and the cost meter.
//! Cross-thread search is `search.rs`.
//!
//! Functions take a [`Connection`]; the caller owns its lifetime
//! (the chat runtime holds a write connection; reads can use a short-lived read-only
//! one). Writes that must stay consistent (a message insert bumps its conversation's
//! `updated_at`, and the per-conversation `seq` is derived) run inside a transaction.
//!
//! ## Wire vs. backend types
//!
//! [`ConversationRow`], [`CostSummary`], and [`CostDay`] are
//! wire types (camelCase + `specta::Type`) the IPC layer returns directly.
//! [`StoredMessage`] is deliberately NOT a wire type: it carries the fully parsed
//! [`AgentPart`]s, including the opaque provider reasoning blob, which must never cross to
//! the frontend. The IPC layer builds a display-only `MessageView` from it.

use rusqlite::Connection;

use super::super::types::ConversationOrigin;
use super::AgentStoreError;
use super::events::{ConversationEvent, EVENT_ROLE_TOKEN};
use super::rows::insert_message_row;
use super::wake_place::{WakePlace, wake_place};
use crate::agent::llm::types::{AgentPart, AgentRole, ProviderTag};

/// A conversation header row. Wire type (the thread list).
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationRow {
    pub id: i64,
    pub title: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub archived: bool,
    /// `None` = user-started (the v1 case). A non-null token is a programmatic origin.
    pub origin: Option<ConversationOrigin>,
    /// Where a wake thread's activity happened, while its title is still the wake's own; the
    /// list builds the row's display title from it (`wake_place.rs`). `None` for every other row.
    pub wake_place: Option<WakePlace>,
}

/// What one stored message row holds: a transcript message (an LLM-facing role plus its
/// ordered typed parts) or a UI-facing conversation event.
#[derive(Debug, Clone, PartialEq)]
pub enum StoredContent {
    Message { role: AgentRole, parts: Vec<AgentPart> },
    Event(ConversationEvent),
}

/// One stored message, fully decoded. Backend-only: transcript `parts` carry the opaque
/// provider reasoning blob, so this never crosses IPC (the IPC layer derives a display
/// `MessageView`).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredMessage {
    pub id: i64,
    pub seq: i64,
    pub content: StoredContent,
    pub text_for_search: String,
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub created_at: i64,
}

/// A conversation header plus a page of its messages, and the total message count so a
/// paged UI knows whether more exist. Backend-only (holds [`StoredMessage`]s).
#[derive(Debug, Clone, PartialEq)]
pub struct ConversationDetail {
    pub conversation: ConversationRow,
    pub messages: Vec<StoredMessage>,
    pub total_messages: u32,
}

/// One metering event to fold into the cost meter (per completed `respond` call).
#[derive(Debug, Clone, PartialEq)]
pub struct CostRecord {
    /// Local day, `YYYY-MM-DD`.
    pub day: String,
    pub conversation_id: i64,
    pub provider: ProviderTag,
    pub model: String,
    /// The whole input, the cached share included.
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    /// Of `prompt_tokens`, read back from the provider's prompt cache.
    pub cache_read_tokens: u64,
    /// Of `prompt_tokens`, written to the provider's prompt cache.
    pub cache_write_tokens: u64,
    /// Integer micro-USD; an honest estimate, 0 for a local/on-device model.
    pub cost_micros: i64,
    /// False when the model wasn't in the price table at metering time.
    pub priced: bool,
}

/// One day's token + cost totals across every thread and model. Wire type (the settings
/// spend display).
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CostDay {
    pub day: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    /// Of `prompt_tokens`, what the provider's prompt cache served.
    pub cache_read_tokens: u64,
    pub cost_micros: i64,
    /// True only when every contributing row was priced. False ⇒ the day's cost is a
    /// lower bound (some model was unpriced), shown as "unknown", never a silent $0
    /// (spec §2.4 honesty).
    pub fully_priced: bool,
}

/// The per-day cost rollup, newest day first. Wire type (the settings spend list).
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CostSummary {
    pub days: Vec<CostDay>,
}

// ── Conversations ────────────────────────────────────────────────────────────

/// Create a conversation, returning its new id. `now` is unix secs (`created_at` =
/// `updated_at` at birth). `origin` is `None` for a user-started thread (the v1 case).
pub fn create_conversation(
    conn: &Connection,
    title: &str,
    now: i64,
    origin: Option<ConversationOrigin>,
) -> Result<i64, AgentStoreError> {
    conn.execute(
        "INSERT INTO conversations (title, created_at, updated_at, archived, origin)
         VALUES (?1, ?2, ?2, 0, ?3)",
        rusqlite::params![title, now, origin.map(|o| o.as_token())],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Decode the nullable `conversations.origin` column. NULL is the user-started case; an unknown
/// token is a typed decode error, or a row from a newer schema reads as a thread the user began.
pub(super) fn origin_of(token: Option<String>) -> Result<Option<ConversationOrigin>, AgentStoreError> {
    token
        .map(|token| {
            ConversationOrigin::from_token(&token).ok_or(AgentStoreError::Decode {
                column: "origin",
                value: token,
            })
        })
        .transpose()
}

fn map_conversation_row(row: &rusqlite::Row<'_>) -> Result<ConversationRow, AgentStoreError> {
    let origin = origin_of(row.get(5)?)?;
    Ok(ConversationRow {
        id: row.get(0)?,
        title: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        archived: row.get::<_, i64>(4)? != 0,
        origin,
        wake_place: None,
    })
}

const CONVERSATION_COLUMNS: &str = "id, title, created_at, updated_at, archived, origin";

/// Conversations newest-activity first, paged. `include_archived = false` filters the
/// archived flag; the `conversations_updated` index serves the order.
///
/// The reserved `quiet_wakes` ledger row is filtered out either way — it is a cost total, not a
/// conversation, and `include_archived` is about threads the user set aside, not bookkeeping. ⚠️
/// `IS NOT`, ❌ never `<>`: the common case is a NULL origin, which `<>` answers NULL to.
pub fn list_conversations(
    conn: &Connection,
    limit: u32,
    offset: u32,
    include_archived: bool,
) -> Result<Vec<ConversationRow>, AgentStoreError> {
    let visible = "origin IS NOT ?3";
    let where_sql = if include_archived {
        format!("WHERE {visible}")
    } else {
        format!("WHERE archived = 0 AND {visible}")
    };
    let sql = format!(
        "SELECT {CONVERSATION_COLUMNS} FROM conversations {where_sql} \
         ORDER BY updated_at DESC, id DESC LIMIT ?1 OFFSET ?2"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(rusqlite::params![
        limit,
        offset,
        ConversationOrigin::QuietWakes.as_token()
    ])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_conversation_row(row)?);
    }
    for row in &mut out {
        row.wake_place = wake_place(conn, row.id, row.origin, &row.title)?;
    }
    Ok(out)
}

/// The id of the reserved thread that holds what quiet wakes spent. Created by migration v8,
/// so it is present on every store this build opens; the `Err` is for a DB somebody edited by
/// hand, and its callers keep the thread rather than lose its cost record.
pub fn quiet_wakes_conversation(conn: &Connection) -> Result<i64, AgentStoreError> {
    let mut stmt = conn.prepare_cached("SELECT id FROM conversations WHERE origin = ?1 ORDER BY id LIMIT 1")?;
    let mut rows = stmt.query(rusqlite::params![ConversationOrigin::QuietWakes.as_token()])?;
    match rows.next()? {
        Some(row) => Ok(row.get(0)?),
        None => Err(AgentStoreError::Decode {
            column: "origin",
            value: ConversationOrigin::QuietWakes.as_token().to_string(),
        }),
    }
}

/// Delete a conversation outright. Its messages, cost rows, and proposals cascade, and the FTS
/// delete trigger de-indexes its text.
///
/// The store's one delete: everything else archives. It exists for the thread a QUIET wake
/// opened, which the user must never see. Fold the cost first with
/// [`discard_conversation_keeping_cost`] rather than calling this on a thread that spent
/// anything.
pub fn delete_conversation(conn: &Connection, id: i64) -> Result<(), AgentStoreError> {
    conn.execute("DELETE FROM conversations WHERE id = ?1", rusqlite::params![id])?;
    Ok(())
}

/// Move one conversation's cost rows onto the reserved quiet-wakes thread, then delete the
/// conversation. Both in one transaction, so a crash between them can't drop the spend.
///
/// ⚠️ The fold is the same `ON CONFLICT (day, conversation_id, provider, model) DO UPDATE`
/// shape [`record_cost`] uses, and `priced` carries as `priced AND excluded.priced` for the
/// same reason: a reserved row that absorbed one unpriced turn is a LOWER BOUND, and claiming
/// a complete price it doesn't have is exactly the silent-$0 the meter exists to prevent.
pub fn discard_conversation_keeping_cost(conn: &Connection, id: i64) -> Result<(), AgentStoreError> {
    let reserved = quiet_wakes_conversation(conn)?;
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO cost_meter
            (day, conversation_id, provider, model, prompt_tokens, completion_tokens,
             cache_read_tokens, cache_write_tokens, cost_micros, priced)
         SELECT day, ?2, provider, model, prompt_tokens, completion_tokens,
                cache_read_tokens, cache_write_tokens, cost_micros, priced
           FROM cost_meter WHERE conversation_id = ?1
         ON CONFLICT (day, conversation_id, provider, model) DO UPDATE SET
            prompt_tokens      = prompt_tokens + excluded.prompt_tokens,
            completion_tokens  = completion_tokens + excluded.completion_tokens,
            cache_read_tokens  = cache_read_tokens + excluded.cache_read_tokens,
            cache_write_tokens = cache_write_tokens + excluded.cache_write_tokens,
            cost_micros        = cost_micros + excluded.cost_micros,
            priced            = priced AND excluded.priced",
        rusqlite::params![id, reserved],
    )?;
    tx.execute("DELETE FROM conversations WHERE id = ?1", rusqlite::params![id])?;
    tx.commit()?;
    Ok(())
}

/// Rename a conversation. Does not touch `updated_at` (that tracks message activity, not
/// a metadata edit, so a rename doesn't reorder the thread list).
pub fn rename_conversation(conn: &Connection, id: i64, title: &str) -> Result<(), AgentStoreError> {
    conn.execute(
        "UPDATE conversations SET title = ?2 WHERE id = ?1",
        rusqlite::params![id, title],
    )?;
    Ok(())
}

/// Set a conversation's archived flag (no delete in v1 — archive filters the list).
pub fn archive_conversation(conn: &Connection, id: i64, archived: bool) -> Result<(), AgentStoreError> {
    conn.execute(
        "UPDATE conversations SET archived = ?2 WHERE id = ?1",
        rusqlite::params![id, archived as i64],
    )?;
    Ok(())
}

/// One conversation's header plus a page of its messages (seq ascending), and the total
/// message count. `None` if the conversation is absent.
pub fn get_conversation(
    conn: &Connection,
    id: i64,
    msg_limit: u32,
    msg_offset: u32,
) -> Result<Option<ConversationDetail>, AgentStoreError> {
    let sql = format!("SELECT {CONVERSATION_COLUMNS} FROM conversations WHERE id = ?1");
    let conversation = {
        let mut stmt = conn.prepare_cached(&sql)?;
        let mut rows = stmt.query(rusqlite::params![id])?;
        match rows.next()? {
            Some(row) => map_conversation_row(row)?,
            None => return Ok(None),
        }
    };
    let conversation = ConversationRow {
        wake_place: wake_place(conn, conversation.id, conversation.origin, &conversation.title)?,
        ..conversation
    };
    let total_messages: u32 = conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE conversation_id = ?1",
        rusqlite::params![id],
        |row| row.get::<_, i64>(0).map(|n| n as u32),
    )?;
    let messages = list_messages(conn, id, msg_limit, msg_offset)?;
    Ok(Some(ConversationDetail {
        conversation,
        messages,
        total_messages,
    }))
}

// ── Messages ─────────────────────────────────────────────────────────────────

pub(super) const MESSAGE_COLUMNS: &str =
    "id, seq, role, content_blocks, text_for_search, prompt_tokens, completion_tokens, created_at";

pub(super) fn map_message_row(row: &rusqlite::Row<'_>) -> Result<StoredMessage, AgentStoreError> {
    let role_token: String = row.get(2)?;
    let content_blocks: String = row.get(3)?;
    let content = if role_token == EVENT_ROLE_TOKEN {
        let event: ConversationEvent = serde_json::from_str(&content_blocks).map_err(AgentStoreError::ContentBlocks)?;
        StoredContent::Event(event)
    } else {
        let role = AgentRole::from_token(&role_token).ok_or_else(|| AgentStoreError::Decode {
            column: "role",
            value: role_token,
        })?;
        let parts: Vec<AgentPart> = serde_json::from_str(&content_blocks).map_err(AgentStoreError::ContentBlocks)?;
        StoredContent::Message { role, parts }
    };
    Ok(StoredMessage {
        id: row.get(0)?,
        seq: row.get(1)?,
        content,
        text_for_search: row.get(4)?,
        prompt_tokens: row.get::<_, Option<i64>>(5)?.map(|n| n as u32),
        completion_tokens: row.get::<_, Option<i64>>(6)?.map(|n| n as u32),
        created_at: row.get(7)?,
    })
}

/// A page of a conversation's messages, seq ascending (chronological).
pub fn list_messages(
    conn: &Connection,
    conversation_id: i64,
    limit: u32,
    offset: u32,
) -> Result<Vec<StoredMessage>, AgentStoreError> {
    let sql = format!(
        "SELECT {MESSAGE_COLUMNS} FROM messages WHERE conversation_id = ?1 \
         ORDER BY seq ASC LIMIT ?2 OFFSET ?3"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(rusqlite::params![conversation_id, limit, offset])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_message_row(row)?);
    }
    Ok(out)
}

/// The conversation's highest `seq` so far, or `-1` for a thread with no rows yet, so "rows
/// after it" is every row either way.
pub fn last_seq(conn: &Connection, conversation_id: i64) -> Result<i64, AgentStoreError> {
    Ok(conn.query_row(
        "SELECT COALESCE(MAX(seq), -1) FROM messages WHERE conversation_id = ?1",
        rusqlite::params![conversation_id],
        |row| row.get(0),
    )?)
}

/// Append a message to a conversation, returning `(message_id, seq)`. The per-conversation
/// `seq` is derived (max + 1) and the conversation's `updated_at` is bumped, both inside
/// one transaction so the seq can't race and the two writes commit together. `parts` are
/// persisted as `content_blocks` JSON; `text_for_search` is the plain text the FTS index
/// folds (extract it at the call site — user + assistant prose, never tool blobs).
#[allow(
    clippy::too_many_arguments,
    reason = "one message's full column set; a params struct would just relocate the arity"
)]
pub fn append_message(
    conn: &Connection,
    conversation_id: i64,
    role: AgentRole,
    parts: &[AgentPart],
    text_for_search: &str,
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    now: i64,
) -> Result<(i64, i64), AgentStoreError> {
    let content_blocks = serde_json::to_string(parts).map_err(AgentStoreError::ContentBlocks)?;
    insert_message_row(
        conn,
        conversation_id,
        role.as_token(),
        &content_blocks,
        text_for_search,
        prompt_tokens,
        completion_tokens,
        now,
    )
}

/// The model name the conversation's most recent completed turn (or recorded model-change
/// event) used. `None` means no turn has run yet.
pub fn conversation_last_model(conn: &Connection, conversation_id: i64) -> Result<Option<String>, AgentStoreError> {
    let mut stmt = conn.prepare_cached("SELECT last_model FROM conversations WHERE id = ?1")?;
    let mut rows = stmt.query(rusqlite::params![conversation_id])?;
    match rows.next()? {
        Some(row) => Ok(row.get(0)?),
        None => Ok(None),
    }
}

/// Record the model a conversation last used (stamped per completed turn, and when a
/// model-change event is recorded).
pub fn set_conversation_last_model(
    conn: &Connection,
    conversation_id: i64,
    model: &str,
) -> Result<(), AgentStoreError> {
    conn.execute(
        "UPDATE conversations SET last_model = ?2 WHERE id = ?1",
        rusqlite::params![conversation_id, model],
    )?;
    Ok(())
}

/// The chat-memory size (the resolved prompt-token budget) the conversation's most recent
/// completed turn — or recorded chat-memory event — was assembled against. `None` means no
/// turn has run yet, so there is nothing to have changed from.
///
/// ⚠️ Not [`conversation_context_usage`]'s budget, which carries the same number for the
/// gauge: that one is written only beside the prompt size it must be read with, and the live
/// settings path has no prompt size to record.
pub fn conversation_last_chat_memory(
    conn: &Connection,
    conversation_id: i64,
) -> Result<Option<usize>, AgentStoreError> {
    let mut stmt = conn.prepare_cached("SELECT last_chat_memory FROM conversations WHERE id = ?1")?;
    let mut rows = stmt.query(rusqlite::params![conversation_id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let tokens: Option<i64> = row.get(0)?;
    Ok(tokens.filter(|t| *t > 0).map(|t| t as usize))
}

/// Record the chat-memory size a conversation last used (stamped per completed turn, and
/// when a chat-memory-change event is recorded).
pub fn set_conversation_last_chat_memory(
    conn: &Connection,
    conversation_id: i64,
    chat_memory_tokens: usize,
) -> Result<(), AgentStoreError> {
    conn.execute(
        "UPDATE conversations SET last_chat_memory = ?2 WHERE id = ?1",
        rusqlite::params![conversation_id, chat_memory_tokens as i64],
    )?;
    Ok(())
}

/// What the conversation's last completed turn sent, and the budget it was assembled against.
/// `None` means no turn has finished yet, which the gauge shows as "not measured" rather than
/// as an empty bar.
///
/// Read as a pair on purpose: a size without the budget it was measured against can't be
/// turned into a percentage, so a half-recorded row is no measurement at all.
pub fn conversation_context_usage(
    conn: &Connection,
    conversation_id: i64,
) -> Result<Option<(usize, usize)>, AgentStoreError> {
    let mut stmt =
        conn.prepare_cached("SELECT last_prompt_tokens, last_prompt_budget FROM conversations WHERE id = ?1")?;
    let mut rows = stmt.query(rusqlite::params![conversation_id])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let tokens: Option<i64> = row.get(0)?;
    let budget: Option<i64> = row.get(1)?;
    match (tokens, budget) {
        (Some(tokens), Some(budget)) if budget > 0 => Ok(Some((tokens.max(0) as usize, budget as usize))),
        _ => Ok(None),
    }
}

/// Record what a completed turn's prompt cost against its budget, for the thread's gauge.
pub fn set_conversation_context_usage(
    conn: &Connection,
    conversation_id: i64,
    prompt_tokens: usize,
    prompt_budget: usize,
) -> Result<(), AgentStoreError> {
    conn.execute(
        "UPDATE conversations SET last_prompt_tokens = ?2, last_prompt_budget = ?3 WHERE id = ?1",
        rusqlite::params![conversation_id, prompt_tokens as i64, prompt_budget as i64],
    )?;
    Ok(())
}

// ── Cost meter ───────────────────────────────────────────────────────────────

/// Fold one metering event into the cost meter. Accumulates onto the existing
/// `(day, conversation_id, provider, model)` row (`ON CONFLICT DO UPDATE ... + excluded`)
/// rather than inserting a duplicate — the upsert the NOT NULL `conversation_id` PK
/// protects (a NULL in the PK would make every write insert a new row). `priced` ANDs, so
/// a day/thread/model that ever took an unpriced contribution reads unpriced (its cost is
/// a lower bound).
pub fn record_cost(conn: &Connection, record: &CostRecord) -> Result<(), AgentStoreError> {
    conn.execute(
        "INSERT INTO cost_meter
            (day, conversation_id, provider, model, prompt_tokens, completion_tokens,
             cache_read_tokens, cache_write_tokens, cost_micros, priced)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT (day, conversation_id, provider, model) DO UPDATE SET
            prompt_tokens      = prompt_tokens + excluded.prompt_tokens,
            completion_tokens  = completion_tokens + excluded.completion_tokens,
            cache_read_tokens  = cache_read_tokens + excluded.cache_read_tokens,
            cache_write_tokens = cache_write_tokens + excluded.cache_write_tokens,
            cost_micros        = cost_micros + excluded.cost_micros,
            priced             = priced AND excluded.priced",
        rusqlite::params![
            record.day,
            record.conversation_id,
            record.provider.as_token(),
            record.model,
            record.prompt_tokens as i64,
            record.completion_tokens as i64,
            record.cache_read_tokens as i64,
            record.cache_write_tokens as i64,
            record.cost_micros,
            record.priced as i64,
        ],
    )?;
    Ok(())
}

/// One conversation's cumulative token + cost totals across every day and model it used.
/// Wire type (the per-thread footer).
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationCost {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    /// Of `prompt_tokens`, what the provider's prompt cache served.
    pub cache_read_tokens: u64,
    pub cost_micros: i64,
    /// True only when every contributing row was priced. False ⇒ the cost is a lower
    /// bound (some model was unpriced), shown "unknown", never a silent $0 (spec §2.4).
    pub fully_priced: bool,
    /// The distinct providers that contributed. Empty when the thread has no metered
    /// turn yet (a brand-new or local-only-before-any-answer thread). The footer stays
    /// honest about a local/on-device thread by reading the tokens + this list.
    pub providers: Vec<ProviderTag>,
}

/// The cumulative token + cost total for one conversation (all days, all models). Zeroed
/// when the thread has metered no turn yet. Drives the per-thread footer.
pub fn conversation_cost(conn: &Connection, conversation_id: i64) -> Result<ConversationCost, AgentStoreError> {
    let (prompt_tokens, completion_tokens, cache_read_tokens, cost_micros, fully_priced) = conn.query_row(
        "SELECT COALESCE(SUM(prompt_tokens), 0),
                COALESCE(SUM(completion_tokens), 0),
                COALESCE(SUM(cache_read_tokens), 0),
                COALESCE(SUM(cost_micros), 0),
                COALESCE(MIN(priced), 1)
         FROM cost_meter WHERE conversation_id = ?1",
        rusqlite::params![conversation_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)? as u64,
                row.get::<_, i64>(1)? as u64,
                row.get::<_, i64>(2)? as u64,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)? != 0,
            ))
        },
    )?;
    let mut providers = Vec::new();
    {
        let mut stmt = conn
            .prepare_cached("SELECT DISTINCT provider FROM cost_meter WHERE conversation_id = ?1 ORDER BY provider")?;
        let mut rows = stmt.query(rusqlite::params![conversation_id])?;
        while let Some(row) = rows.next()? {
            let token: String = row.get(0)?;
            if let Some(tag) = ProviderTag::from_token(&token) {
                providers.push(tag);
            }
        }
    }
    Ok(ConversationCost {
        prompt_tokens,
        completion_tokens,
        cache_read_tokens,
        cost_micros,
        fully_priced,
        providers,
    })
}

/// Every token the agent spent on its OWN initiative on `day`, prompt plus completion.
///
/// ⚠️ **Scoped by `conversations.origin`, ❌ never by the whole meter.** Capping all agent spend
/// would let a chatty afternoon on the rail starve the wake loop, and a busy wake loop eat the
/// user's own budget; the two are different money. The two proactive origins are
/// [`ConversationOrigin::Notification`] (a wake's thread) and [`ConversationOrigin::QuietWakes`]
/// (the reserved row a quiet wake's spend is folded onto before its thread is deleted). A
/// user-started thread has a NULL origin and never counts.
pub fn proactive_tokens_for_day(conn: &Connection, day: &str) -> Result<u64, AgentStoreError> {
    let total: i64 = conn.query_row(
        "SELECT COALESCE(SUM(cost_meter.prompt_tokens + cost_meter.completion_tokens), 0)
         FROM cost_meter
         JOIN conversations ON conversations.id = cost_meter.conversation_id
         WHERE cost_meter.day = ?1 AND conversations.origin IN (?2, ?3)",
        rusqlite::params![
            day,
            ConversationOrigin::Notification.as_token(),
            ConversationOrigin::QuietWakes.as_token()
        ],
        |row| row.get(0),
    )?;
    Ok(total.max(0) as u64)
}

/// The per-day cost rollup across every thread and model, newest day first. A day reads
/// `fully_priced = false` when any contributing row was unpriced (`MIN(priced) = 0`).
pub fn cost_summary(conn: &Connection) -> Result<CostSummary, AgentStoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT day,
                SUM(prompt_tokens),
                SUM(completion_tokens),
                SUM(cache_read_tokens),
                SUM(cost_micros),
                MIN(priced)
         FROM cost_meter
         GROUP BY day
         ORDER BY day DESC",
    )?;
    let mut rows = stmt.query([])?;
    let mut days = Vec::new();
    while let Some(row) = rows.next()? {
        days.push(CostDay {
            day: row.get(0)?,
            prompt_tokens: row.get::<_, i64>(1)? as u64,
            completion_tokens: row.get::<_, i64>(2)? as u64,
            cache_read_tokens: row.get::<_, i64>(3)? as u64,
            cost_micros: row.get(4)?,
            fully_priced: row.get::<_, i64>(5)? != 0,
        });
    }
    Ok(CostSummary { days })
}

#[cfg(test)]
mod tests;
