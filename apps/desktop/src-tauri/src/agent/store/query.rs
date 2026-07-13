//! The read/write query layer over `main.db`: conversations, messages, the FTS5
//! cross-thread search (with its input sanitizer), and the cost meter.
//!
//! Functions take a [`Connection`](rusqlite::Connection); the caller owns its lifetime
//! (the chat runtime holds a write connection; reads can use a short-lived read-only
//! one). Writes that must stay consistent (a message insert bumps its conversation's
//! `updated_at`, and the per-conversation `seq` is derived) run inside a transaction.
//!
//! ## Wire vs. backend types
//!
//! [`ConversationRow`], [`ConversationSearchHit`], [`CostSummary`], and [`CostDay`] are
//! wire types (camelCase + `specta::Type`) the IPC layer returns directly.
//! [`StoredMessage`] is deliberately NOT a wire type: it carries the fully parsed
//! [`AgentPart`]s, including the opaque provider reasoning blob, which must never cross to
//! the frontend. The IPC layer builds a display-only `MessageView` from it.
//!
//! ## FTS5 search is sanitized, never raw
//!
//! Raw user input fed into `... MATCH ?` throws an fts5 syntax error on ordinary filename
//! fragments (`report(v2)`, `foo:bar`, a bareword `AND`/`OR`/`NOT`, an unbalanced `"`),
//! and parameter binding does not help — the string is parsed as FTS5 query syntax.
//! [`sanitize_fts_query`] turns any user text into a safe prefix-matching query.

use rusqlite::Connection;

use super::super::types::ConversationOrigin;
use super::AgentStoreError;
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
}

/// One stored message, fully decoded. Backend-only: `parts` carries the opaque provider
/// reasoning blob, so this never crosses IPC (the IPC layer derives a display `MessageView`).
#[derive(Debug, Clone, PartialEq)]
pub struct StoredMessage {
    pub id: i64,
    pub seq: i64,
    pub role: AgentRole,
    pub parts: Vec<AgentPart>,
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

/// A conversation whose messages matched a cross-thread search, newest-activity first.
/// Wire type (the search results list): the `snippet` is a plain-text excerpt from
/// the newest matching message, rendered ESCAPED on the frontend (never `{@html}`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSearchHit {
    pub conversation_id: i64,
    pub title: String,
    pub updated_at: i64,
    /// An FTS5 excerpt around the match, or the message start if the term is early.
    /// Plain text with `…` ellipses; no markup (rendered as escaped text).
    pub snippet: String,
}

/// One metering event to fold into the cost meter (per completed `respond` call).
#[derive(Debug, Clone, PartialEq)]
pub struct CostRecord {
    /// Local day, `YYYY-MM-DD`.
    pub day: String,
    pub conversation_id: i64,
    pub provider: ProviderTag,
    pub model: String,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
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

fn map_conversation_row(row: &rusqlite::Row<'_>) -> Result<ConversationRow, AgentStoreError> {
    let origin_token: Option<String> = row.get(5)?;
    let origin = match origin_token {
        Some(token) => Some(
            ConversationOrigin::from_token(&token).ok_or_else(|| AgentStoreError::Decode {
                column: "origin",
                value: token,
            })?,
        ),
        None => None,
    };
    Ok(ConversationRow {
        id: row.get(0)?,
        title: row.get(1)?,
        created_at: row.get(2)?,
        updated_at: row.get(3)?,
        archived: row.get::<_, i64>(4)? != 0,
        origin,
    })
}

const CONVERSATION_COLUMNS: &str = "id, title, created_at, updated_at, archived, origin";

/// Conversations newest-activity first, paged. `include_archived = false` filters the
/// archived flag; the `conversations_updated` index serves the order.
pub fn list_conversations(
    conn: &Connection,
    limit: u32,
    offset: u32,
    include_archived: bool,
) -> Result<Vec<ConversationRow>, AgentStoreError> {
    let where_sql = if include_archived { "" } else { "WHERE archived = 0" };
    let sql = format!(
        "SELECT {CONVERSATION_COLUMNS} FROM conversations {where_sql} \
         ORDER BY updated_at DESC, id DESC LIMIT ?1 OFFSET ?2"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let mut rows = stmt.query(rusqlite::params![limit, offset])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(map_conversation_row(row)?);
    }
    Ok(out)
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

const MESSAGE_COLUMNS: &str =
    "id, seq, role, content_blocks, text_for_search, prompt_tokens, completion_tokens, created_at";

fn map_message_row(row: &rusqlite::Row<'_>) -> Result<StoredMessage, AgentStoreError> {
    let role_token: String = row.get(2)?;
    let role = AgentRole::from_token(&role_token).ok_or_else(|| AgentStoreError::Decode {
        column: "role",
        value: role_token,
    })?;
    let content_blocks: String = row.get(3)?;
    let parts: Vec<AgentPart> = serde_json::from_str(&content_blocks).map_err(AgentStoreError::ContentBlocks)?;
    Ok(StoredMessage {
        id: row.get(0)?,
        seq: row.get(1)?,
        role,
        parts,
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
            role.as_token(),
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

// ── Cross-thread FTS5 search ─────────────────────────────────────────────────

/// Turn arbitrary user input into a safe, prefix-matching FTS5 query, or `None` when the
/// input carries no searchable term (empty, whitespace, or pure punctuation) — the caller
/// then returns no hits rather than running a match.
///
/// Each whitespace-separated token becomes an FTS5 **string literal** with embedded double
/// quotes doubled (the FTS5 escape), then a trailing `*` for prefix search. Wrapping in a
/// string literal neutralizes every FTS5 metacharacter: a bareword `AND`/`OR`/`NOT`
/// becomes a literal term, `foo:bar`'s column-filter `:` and `report(v2)`'s parentheses
/// are just token separators inside the literal, and an unbalanced `"` can't break out.
/// Tokens with no alphanumeric content are dropped so they never produce an empty phrase.
pub fn sanitize_fts_query(input: &str) -> Option<String> {
    let mut terms = Vec::new();
    for raw in input.split_whitespace() {
        if !raw.chars().any(char::is_alphanumeric) {
            continue;
        }
        let escaped = raw.replace('"', "\"\"");
        terms.push(format!("\"{escaped}\"*"));
    }
    (!terms.is_empty()).then(|| terms.join(" "))
}

/// Conversations whose messages match `query`, newest-activity first (by the most recent
/// matching message), paged. Distinct conversations even when several of a thread's
/// messages match. An empty/uses-only-punctuation query returns no hits.
pub fn search_conversations(
    conn: &Connection,
    query: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<ConversationSearchHit>, AgentStoreError> {
    let Some(match_query) = sanitize_fts_query(query) else {
        return Ok(Vec::new());
    };
    // Per conversation: the max matching message id (the newest matching message,
    // ids being insert-monotonic) drives a deterministic snippet, and MAX(created_at)
    // orders threads by most recent match. The `hit` subquery yields a snippet for
    // every matching rowid; joining it on that max id picks one excerpt per thread.
    // `snippet(messages_fts, 0, '', '', '…', 10)`: column 0 (`text_for_search`), no
    // highlight markers (the frontend escapes + styles), `…` ellipsis, ~10 tokens.
    let mut stmt = conn.prepare_cached(
        "SELECT c.id, c.title, c.updated_at, hit.snippet
         FROM conversations c
         JOIN (
             SELECT m.conversation_id AS cid,
                    MAX(m.created_at)  AS latest,
                    MAX(m.id)          AS max_id
             FROM messages m
             WHERE m.id IN (SELECT rowid FROM messages_fts WHERE messages_fts MATCH ?1)
             GROUP BY m.conversation_id
         ) grouped ON grouped.cid = c.id
         JOIN (
             SELECT messages_fts.rowid AS rid,
                    snippet(messages_fts, 0, '', '', '…', 10) AS snippet
             FROM messages_fts WHERE messages_fts MATCH ?1
         ) hit ON hit.rid = grouped.max_id
         ORDER BY grouped.latest DESC, c.id DESC
         LIMIT ?2 OFFSET ?3",
    )?;
    let mut rows = stmt.query(rusqlite::params![match_query, limit, offset])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(ConversationSearchHit {
            conversation_id: row.get(0)?,
            title: row.get(1)?,
            updated_at: row.get(2)?,
            snippet: row.get(3)?,
        });
    }
    Ok(out)
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
            (day, conversation_id, provider, model, prompt_tokens, completion_tokens, cost_micros, priced)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (day, conversation_id, provider, model) DO UPDATE SET
            prompt_tokens     = prompt_tokens + excluded.prompt_tokens,
            completion_tokens = completion_tokens + excluded.completion_tokens,
            cost_micros       = cost_micros + excluded.cost_micros,
            priced            = priced AND excluded.priced",
        rusqlite::params![
            record.day,
            record.conversation_id,
            record.provider.as_token(),
            record.model,
            record.prompt_tokens as i64,
            record.completion_tokens as i64,
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
    let (prompt_tokens, completion_tokens, cost_micros, fully_priced) = conn.query_row(
        "SELECT COALESCE(SUM(prompt_tokens), 0),
                COALESCE(SUM(completion_tokens), 0),
                COALESCE(SUM(cost_micros), 0),
                COALESCE(MIN(priced), 1)
         FROM cost_meter WHERE conversation_id = ?1",
        rusqlite::params![conversation_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)? as u64,
                row.get::<_, i64>(1)? as u64,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)? != 0,
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
        cost_micros,
        fully_priced,
        providers,
    })
}

/// The per-day cost rollup across every thread and model, newest day first. A day reads
/// `fully_priced = false` when any contributing row was unpriced (`MIN(priced) = 0`).
pub fn cost_summary(conn: &Connection) -> Result<CostSummary, AgentStoreError> {
    let mut stmt = conn.prepare_cached(
        "SELECT day,
                SUM(prompt_tokens),
                SUM(completion_tokens),
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
            cost_micros: row.get(3)?,
            fully_priced: row.get::<_, i64>(4)? != 0,
        });
    }
    Ok(CostSummary { days })
}

// ── Consent (meta rows) ──────────────────────────────────────────────────────

/// The `meta` key holding the accepted consent-copy version (as text).
const CONSENT_VERSION_KEY: &str = "ask_cmdr_consent_version";
/// The `meta` key holding the unix-secs timestamp the user accepted consent.
const CONSENT_AT_KEY: &str = "ask_cmdr_consent_at";

/// A recorded consent: which copy version the user accepted, and when. Stored in the
/// durable `main.db` (agent state, not a preference — agent-spec D56), so it lives beside
/// the chats it governs and is `sqlite3`-inspectable. Wire type (the consent record).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AskCmdrConsent {
    /// The `CONSENT_COPY_VERSION` the user accepted. A future copy change bumps the
    /// constant, so a stale-version record no longer counts as current consent.
    pub version: u32,
    /// Unix secs when consent was recorded.
    pub at: i64,
}

/// Read a `meta` value by key.
fn read_meta(conn: &Connection, key: &str) -> Result<Option<String>, AgentStoreError> {
    let mut stmt = conn.prepare_cached("SELECT value FROM meta WHERE key = ?1")?;
    let mut rows = stmt.query(rusqlite::params![key])?;
    match rows.next()? {
        Some(row) => Ok(Some(row.get(0)?)),
        None => Ok(None),
    }
}

/// The recorded consent, or `None` if the user has never accepted (both keys must be
/// present and parseable). A partial/garbage record reads as no consent, so the gate
/// re-shows the consent screen rather than silently proceeding.
pub fn get_consent(conn: &Connection) -> Result<Option<AskCmdrConsent>, AgentStoreError> {
    let (Some(version_str), Some(at_str)) = (read_meta(conn, CONSENT_VERSION_KEY)?, read_meta(conn, CONSENT_AT_KEY)?)
    else {
        return Ok(None);
    };
    match (version_str.parse::<u32>(), at_str.parse::<i64>()) {
        (Ok(version), Ok(at)) => Ok(Some(AskCmdrConsent { version, at })),
        _ => Ok(None),
    }
}

/// Record consent for copy `version` at `now` (unix secs). Idempotent — re-accepting
/// overwrites the stored version + timestamp.
pub fn set_consent(conn: &Connection, version: u32, now: i64) -> Result<(), AgentStoreError> {
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
        rusqlite::params![CONSENT_VERSION_KEY, version.to_string()],
    )?;
    conn.execute(
        "INSERT OR REPLACE INTO meta (key, value) VALUES (?1, ?2)",
        rusqlite::params![CONSENT_AT_KEY, now.to_string()],
    )?;
    Ok(())
}

/// Clear any recorded consent (the settings "turn off Ask Cmdr" path). The next rail open
/// re-shows the consent screen.
pub fn clear_consent(conn: &Connection) -> Result<(), AgentStoreError> {
    conn.execute(
        "DELETE FROM meta WHERE key IN (?1, ?2)",
        rusqlite::params![CONSENT_VERSION_KEY, CONSENT_AT_KEY],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
