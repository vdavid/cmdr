//! Type-to-jump: highest-scoring fuzzy filename match within a cached listing.
//!
//! Powers the in-directory navigation feature where the user types a few characters
//! in a focused file pane and the cursor jumps to the best-matching entry.
//!
//! ## Crate choice: why `nucleo-matcher`
//!
//! Picked `nucleo-matcher = "0.3.1"` (Helix editor's matcher, also used by Zellij).
//! Pros: microsecond-scale per-match cost, smart-case behavior (lowercase query =
//! case-insensitive, uppercase letter in the query opts into case-sensitive matching
//! for that character), Unicode normalization, MIT-style scoring that prefers prefix /
//! word-boundary matches. Pinned at 0.3.1 (published 2024-02-20, comfortably older
//! than the 1-month minimum). License is MPL-2.0, which is allowed by `deny.toml`.
//! The crate is small (~3 kLOC) and has no async runtime / heavy transitive deps.
//!
//! `sublime_fuzzy` (MIT) was the documented fallback if `nucleo-matcher` failed
//! license / `cargo deny` review. That didn't happen, so we shipped nucleo-matcher.
//!
//! ## Why a separate module
//!
//! `find_first_match` is a pure function over an iterator of entries with no
//! the listing cache lock, no `tokio`. That makes it trivial to unit-test against
//! in-memory fixtures and keeps the Tauri command layer
//! (`commands/file_system/listing.rs`) a thin pass-through that just grabs the
//! read lock and delegates here.

use crate::rt::Instant;
use serde::{Deserialize, Serialize};

use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};

use crate::Inner;
use cmdr_fs::entry::FileEntry;

/// Returns the row number of the highest-scoring fuzzy match for `query` among the
/// rows a pane is showing, or `None` if no entry matches.
///
/// Rules:
/// - `rows` is the pane's visible sequence, so what it leaves out (hidden entries
///   per `FileEntry::is_hidden`, in-flight scratch) never matches and never shifts the answer.
/// - The match runs against the whole filename (including extension); fuzzy scoring already rewards
///   prefix and word-boundary matches, so we don't split on the dot.
/// - Smart-case: an all-lowercase query matches case-insensitively; any uppercase character makes
///   that character case-sensitive (delegated to nucleo-matcher).
/// - Ties (equal score) resolve to the lower index, which matches the listing's active sort order.
/// - Empty query → `None`. Empty listing → `None`.
/// - The synthetic `..` parent entry is **not** in the listing cache (it's prepended by the
///   frontend), so there's no special case for it here.
///
/// ## Index space
///
/// The returned index is a ROW number, the same space `operations::get_file_at` /
/// `get_file_range` answer in. When the pane is hiding entries the number is
/// **smaller** than the absolute position in the listing's entries. The frontend
/// uses it directly as a cursor index (plus the `+1` parent-entry offset when
/// `hasParent`), so the two spaces must line up: taking the rows from the caller
/// rather than re-deriving them here is what guarantees they do.
pub(crate) fn find_first_match<'a>(rows: impl Iterator<Item = &'a FileEntry>, query: &str) -> Option<usize> {
    if query.is_empty() {
        return None;
    }

    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);

    let mut best: Option<(usize, u32)> = None;
    let mut haystack_buf: Vec<char> = Vec::new();

    for (visible_idx, entry) in rows.enumerate() {
        let haystack = Utf32Str::new(&entry.name, &mut haystack_buf);
        let Some(score) = pattern.score(haystack, &mut matcher) else {
            continue;
        };

        // Strictly greater so ties resolve to the lower index (the first match wins).
        match best {
            Some((_, best_score)) if score <= best_score => {}
            _ => best = Some((visible_idx, score)),
        }
    }

    best.map(|(idx, _)| idx)
}

/// The one way a fuzzy jump can't answer at all.
///
/// ❌ Not prose: the frontend logs the variant and leaves the cursor where it
/// was. Kept typed so a future surface can word it, and so a test asserts the
/// refusal by variant.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FuzzyJumpError {
    /// The pane's cached listing is gone (it navigated away, or the cache was
    /// evicted between the keystroke and this call).
    ListingNotFound {
        /// The listing the caller asked about.
        listing_id: String,
    },
}

impl std::fmt::Display for FuzzyJumpError {
    /// ❗ For logs and debugging only.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ListingNotFound { listing_id } => write!(f, "listing not found: {listing_id}"),
        }
    }
}

impl std::error::Error for FuzzyJumpError {}

impl Inner {
    /// Convenience wrapper that grabs the cache read lock, runs
    /// `find_first_match`, and emits a single `type_to_jump` debug log line with
    /// the per-call timing. The Tauri command in `commands::file_system::listing`
    /// is a thin async pass-through over this.
    pub(crate) fn fuzzy_find_first_match_in_listing(
        &self,
        listing_id: &str,
        query: &str,
        include_hidden: bool,
    ) -> Result<Option<usize>, FuzzyJumpError> {
        let started = Instant::now();
        // Recovering is right for a read of a cache: a panic elsewhere left the map
        // intact, and refusing every jump afterwards would be worse than reading it.
        let cache = self.reconciled_cache(&[listing_id]);

        let listing = cache.get(listing_id).ok_or_else(|| FuzzyJumpError::ListingNotFound {
            listing_id: listing_id.to_string(),
        })?;

        let rows = listing.rows(include_hidden);
        let result = find_first_match(rows.iter(), query);
        let elapsed_us = started.elapsed().as_micros();
        log::debug!(
            target: "type_to_jump",
            "listing_id={} query_len={} include_hidden={} result_index={:?} elapsed_us={}",
            listing_id,
            query.chars().count(),
            include_hidden,
            result,
            elapsed_us,
        );
        Ok(result)
    }
}
