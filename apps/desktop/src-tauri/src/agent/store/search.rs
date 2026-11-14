//! Cross-thread search for the threads panel: which conversations match a query, in what
//! order, and the excerpt each one shows.
//!
//! ## Matching
//!
//! A thread matches when its MESSAGE TEXT matches (`messages_fts`, through
//! [`sanitize_fts_query`]) or when its TITLE holds every query term as a word prefix. The title
//! counts on its own because a renamed thread's title is in no index, and it's the one line the
//! user wrote to find the thread again.
//!
//! ## Ranking
//!
//! [`rank`] orders candidates by: a title match first, then the best (lowest) FTS5 `bm25` score
//! among the thread's matching messages, then the thread's last activity, newest first, then
//! the id. A title-only match has no score and sorts after every scored one among the title
//! matches. All of this is in Rust over a few hundred rows (one per thread at most), so the
//! order is unit-testable and the SQL stays two flat reads.
//!
//! ## Snippets
//!
//! Each hit carries an excerpt of its best-scoring message as [`SnippetPart`]s, matched terms
//! flagged. FTS5's `snippet()` marks them with two private-use characters ([`MATCH_START`],
//! [`MATCH_END`]) that [`parse_snippet`] splits on, so no markup ever reaches the frontend and
//! it renders plain text (thread text is untrusted). A message that itself contains one of the
//! two markers only mis-highlights its own excerpt.
//!
//! Snippets are fetched for the returned page only, one `rowid`-pinned FTS read per hit.

use std::cmp::Ordering;
use std::collections::HashMap;

use rusqlite::Connection;

use super::AgentStoreError;
use super::wake_place::{WakePlace, wake_place};
use crate::agent::types::ConversationOrigin;

/// Opens a matched run inside a `snippet()` excerpt. Private-use, so it can't be typed text.
const MATCH_START: char = '\u{E000}';
/// Closes a matched run inside a `snippet()` excerpt.
const MATCH_END: char = '\u{E001}';

/// How many tokens an excerpt spans: enough to fill two lines of the rail at its default width.
const SNIPPET_TOKENS: u32 = 24;

/// One run of a search excerpt. Plain text; `matched` runs are what the query hit.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SnippetPart {
    pub text: String,
    pub matched: bool,
}

/// A conversation that matched a cross-thread search. Wire type (the threads panel's results).
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationSearchHit {
    pub conversation_id: i64,
    pub title: String,
    /// The thread's last activity (unix secs), which the row shows as its age.
    pub updated_at: i64,
    pub archived: bool,
    /// An excerpt of the best-matching message, matched terms flagged. Empty when only the
    /// title matched.
    pub snippet: Vec<SnippetPart>,
    /// `None` = user-started; a token says the agent opened it. Carried so a hit wears the same
    /// glyph the thread list gives it, rather than reading as a thread the user began and forgot.
    pub origin: Option<ConversationOrigin>,
    /// Where a wake thread's activity happened, while its title is still the folder name the
    /// wake gave it. The frontend builds the display title from this.
    pub wake_place: Option<WakePlace>,
}

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

/// The query's words, lowercased, split the way FTS5's default tokenizer splits them (on
/// anything not alphanumeric), so `report(v2)` asks for `report` and `v2`.
fn query_words(input: &str) -> Vec<String> {
    words_of(input).collect()
}

fn words_of(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase)
}

/// Whether every query word starts some word of `title`: the FTS prefix rule, applied to a
/// title no index holds.
pub(super) fn title_matches(title: &str, words: &[String]) -> bool {
    if words.is_empty() {
        return false;
    }
    let title_words: Vec<String> = words_of(title).collect();
    words.iter().all(|word| {
        title_words
            .iter()
            .any(|title_word| title_word.starts_with(word.as_str()))
    })
}

/// One thread that matched, before ordering.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Candidate {
    pub id: i64,
    pub title_match: bool,
    /// The best `bm25` among its matching messages (lower is better), and that message's id.
    pub best: Option<(f64, i64)>,
    pub updated_at: i64,
}

/// Orders candidates best first. See the module docs for the key.
pub(super) fn rank(candidates: &mut [Candidate]) {
    candidates.sort_by(|a, b| {
        b.title_match
            .cmp(&a.title_match)
            .then_with(|| match (a.best, b.best) {
                (Some((a_score, _)), Some((b_score, _))) => a_score.total_cmp(&b_score),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            })
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| b.id.cmp(&a.id))
    });
}

/// Splits a `snippet()` excerpt on the match markers. A stray marker (one the message itself
/// held) only flips highlighting; it never drops text.
pub(super) fn parse_snippet(raw: &str) -> Vec<SnippetPart> {
    fn flush(text: &mut String, matched: bool, parts: &mut Vec<SnippetPart>) {
        if text.is_empty() {
            return;
        }
        match parts.last_mut() {
            Some(last) if last.matched == matched => last.text.push_str(text),
            _ => parts.push(SnippetPart {
                text: std::mem::take(text),
                matched,
            }),
        }
        text.clear();
    }
    let mut parts: Vec<SnippetPart> = Vec::new();
    let mut matched = false;
    let mut current = String::new();
    for c in raw.chars() {
        match c {
            MATCH_START | MATCH_END => {
                flush(&mut current, matched, &mut parts);
                matched = c == MATCH_START;
            }
            // An excerpt is one line in the row: newlines read as spaces.
            '\n' | '\r' | '\t' => current.push(' '),
            _ => current.push(c),
        }
    }
    flush(&mut current, matched, &mut parts);
    parts
}

/// Conversations matching `query`, best first (title match, then `bm25`, then recency), paged.
/// The reserved quiet-wakes row never appears. An empty or punctuation-only query returns no
/// hits.
pub fn search_conversations(
    conn: &Connection,
    query: &str,
    limit: u32,
    offset: u32,
) -> Result<Vec<ConversationSearchHit>, AgentStoreError> {
    let Some(match_query) = sanitize_fts_query(query) else {
        return Ok(Vec::new());
    };
    let words = query_words(query);
    let [quiet_wakes, about_this] = ConversationOrigin::LEDGERS.map(ConversationOrigin::as_token);

    // Every thread's header: a few hundred short rows, read once, so title matches and the
    // recency tie-break need no second trip.
    struct Header {
        title: String,
        updated_at: i64,
        archived: bool,
        origin: Option<String>,
    }
    let mut headers: HashMap<i64, Header> = HashMap::new();
    {
        let mut stmt = conn.prepare_cached(
            "SELECT id, title, updated_at, archived, origin FROM conversations \
             WHERE origin IS NULL OR origin NOT IN (?1, ?2)",
        )?;
        let mut rows = stmt.query(rusqlite::params![quiet_wakes, about_this])?;
        while let Some(row) = rows.next()? {
            headers.insert(
                row.get(0)?,
                Header {
                    title: row.get(1)?,
                    updated_at: row.get(2)?,
                    archived: row.get::<_, i64>(3)? != 0,
                    origin: row.get(4)?,
                },
            );
        }
    }

    // Each thread's best-scoring matching message. `bm25()` is only callable in the query that
    // runs the MATCH, so it's scored in the inner select and ranked per thread outside it.
    let mut best: HashMap<i64, (f64, i64)> = HashMap::new();
    {
        let mut stmt = conn.prepare_cached(
            "SELECT cid, score, mid FROM (
                 SELECT m.conversation_id AS cid, f.score AS score, m.id AS mid,
                        ROW_NUMBER() OVER (PARTITION BY m.conversation_id ORDER BY f.score ASC, m.id DESC) AS rn
                 FROM (SELECT rowid AS rid, bm25(messages_fts) AS score
                       FROM messages_fts WHERE messages_fts MATCH ?1) f
                 JOIN messages m ON m.id = f.rid
             ) WHERE rn = 1",
        )?;
        let mut rows = stmt.query(rusqlite::params![match_query])?;
        while let Some(row) = rows.next()? {
            best.insert(row.get(0)?, (row.get(1)?, row.get(2)?));
        }
    }

    let mut candidates: Vec<Candidate> = headers
        .iter()
        .filter_map(|(&id, header)| {
            let title_match = title_matches(&header.title, &words);
            let best = best.get(&id).copied();
            (title_match || best.is_some()).then_some(Candidate {
                id,
                title_match,
                best,
                updated_at: header.updated_at,
            })
        })
        .collect();
    rank(&mut candidates);

    let mut snippet_stmt = conn.prepare_cached(&format!(
        "SELECT snippet(messages_fts, 0, '{MATCH_START}', '{MATCH_END}', '…', {SNIPPET_TOKENS})
         FROM messages_fts WHERE messages_fts MATCH ?1 AND rowid = ?2"
    ))?;
    let mut out = Vec::new();
    for candidate in candidates.into_iter().skip(offset as usize).take(limit as usize) {
        let Some(header) = headers.remove(&candidate.id) else {
            continue;
        };
        let snippet = match candidate.best {
            Some((_, message_id)) => {
                let raw: Option<String> = snippet_stmt
                    .query_row(rusqlite::params![match_query, message_id], |row| row.get(0))
                    .map(Some)
                    .or_else(|e| match e {
                        rusqlite::Error::QueryReturnedNoRows => Ok(None),
                        other => Err(other),
                    })?;
                raw.map(|raw| parse_snippet(&raw)).unwrap_or_default()
            }
            None => Vec::new(),
        };
        let origin = super::query::origin_of(header.origin)?;
        let wake_place = wake_place(conn, candidate.id, origin, &header.title)?;
        out.push(ConversationSearchHit {
            conversation_id: candidate.id,
            title: header.title,
            updated_at: header.updated_at,
            archived: header.archived,
            snippet,
            origin,
            wake_place,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests;
