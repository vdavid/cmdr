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
//! `LISTING_CACHE` lock, no `tokio`. That makes it trivial to unit-test against
//! in-memory fixtures and keeps the Tauri command layer
//! (`commands/file_system/listing.rs`) a thin pass-through that just grabs the
//! read lock and delegates here.

use crate::ignore_poison::RwLockIgnorePoison as _;
use serde::{Deserialize, Serialize};
use std::time::Instant;

use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};

use crate::file_system::listing::cached_listing::LISTING_CACHE;
use crate::file_system::listing::metadata::FileEntry;

/// Returns the row number of the highest-scoring fuzzy match for `query` among the
/// rows a pane is showing, or `None` if no entry matches.
///
/// Rules:
/// - `rows` is the pane's visible sequence, so what it leaves out (dotfiles, in-flight scratch)
///   never matches and never shifts the answer.
/// - The match runs against the whole filename (including extension); fuzzy scoring already rewards
///   prefix and word-boundary matches, so we don't split on the dot.
/// - Smart-case: an all-lowercase query matches case-insensitively; any uppercase character makes
///   that character case-sensitive (delegated to nucleo-matcher).
/// - Ties (equal score) resolve to the lower index, which matches the listing's active sort order.
/// - Empty query → `None`. Empty listing → `None`.
/// - The synthetic `..` parent entry is **not** in `LISTING_CACHE` (it's prepended by the
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
pub fn find_first_match<'a>(rows: impl Iterator<Item = &'a FileEntry>, query: &str) -> Option<usize> {
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

/// Convenience wrapper that grabs the `LISTING_CACHE` read lock, runs
/// `find_first_match`, and emits a single `type_to_jump` debug log line with
/// the per-call timing. The Tauri command in `commands::file_system::listing`
/// is a thin async pass-through over this.
pub fn fuzzy_find_first_match_in_listing(
    listing_id: &str,
    query: &str,
    include_hidden: bool,
) -> Result<Option<usize>, FuzzyJumpError> {
    let started = Instant::now();
    // Recovering is right for a read of a cache: a panic elsewhere left the map
    // intact, and refusing every jump afterwards would be worse than reading it.
    let cache = LISTING_CACHE.read_ignore_poison();

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::listing::caching_test_support::TestListing;
    use crate::file_system::listing::metadata::FileEntry;
    use crate::file_system::staging::{ShowTempsGuard, StagingTemp};
    use std::sync::Arc;

    fn entry(name: &str) -> FileEntry {
        FileEntry::new(name.to_string(), format!("/{}", name), false, false)
    }

    /// The rows a pane shows for a fixture of ordinary names, matching what
    /// `CachedListing::rows` produces. The dotfile half only; the scratch half
    /// needs a real listing, which `the_jump_lands_on_the_row_the_pane_shows`
    /// covers.
    fn rows(entries: &[FileEntry], include_hidden: bool) -> impl Iterator<Item = &FileEntry> {
        entries
            .iter()
            .filter(move |e| include_hidden || !e.name.starts_with('.'))
    }

    #[test]
    fn empty_listing_returns_none() {
        let entries: Vec<FileEntry> = Vec::new();
        assert_eq!(find_first_match(rows(&entries, true), "abc"), None);
    }

    #[test]
    fn empty_query_returns_none() {
        let entries = vec![entry("README.md"), entry("AGENTS.md")];
        assert_eq!(find_first_match(rows(&entries, true), ""), None);
    }

    #[test]
    fn no_matches_returns_none() {
        let entries = vec![entry("README.md"), entry("AGENTS.md")];
        // "xyz" shares no characters with either name.
        assert_eq!(find_first_match(rows(&entries, true), "xyz"), None);
    }

    #[test]
    fn single_match_returns_its_index() {
        let entries = vec![entry("README.md"), entry("AGENTS.md"), entry("Cargo.toml")];
        // Only "Cargo.toml" contains the subsequence "crg" / "cargo".
        let idx = find_first_match(rows(&entries, true), "cargo").expect("should match");
        assert_eq!(idx, 2);
    }

    #[test]
    fn multiple_matches_pick_highest_scored() {
        // "tests" fuzzy-matches both, but "tests.js" is the better (prefix) match
        // than "my_tests_helper.rs" so it should win.
        let entries = vec![entry("my_tests_helper.rs"), entry("tests.js"), entry("other.txt")];
        let idx = find_first_match(rows(&entries, true), "tests").expect("should match");
        assert_eq!(idx, 1, "prefix match 'tests.js' should outscore 'my_tests_helper.rs'");
    }

    #[test]
    fn ties_resolve_to_lower_index() {
        // Two identical names → identical scores → lower index wins.
        let entries = vec![entry("hello.txt"), entry("hello.txt")];
        let idx = find_first_match(rows(&entries, true), "hello").expect("should match");
        assert_eq!(idx, 0);
    }

    #[test]
    fn hidden_entry_excluded_when_include_hidden_false() {
        let entries = vec![entry(".env"), entry("env_setup.sh")];
        // With hidden excluded, only "env_setup.sh" is a candidate. The dotfile
        // is invisible, so "env_setup.sh" sits at visible-index 0.
        let idx = find_first_match(rows(&entries, false), "env").expect("should match");
        assert_eq!(idx, 0);
    }

    #[test]
    fn hidden_entry_included_when_include_hidden_true() {
        // Deterministic case: two clearly distinct names. The only entry that can
        // match "alpha" is ".alpha.txt" since "zeta.bin" shares no characters with the
        // query. The match must be found AND must land at the dotfile's visible
        // index (0 when hidden is on, since the dotfile is then visible).
        let entries = vec![entry(".alpha.txt"), entry("zeta.bin")];
        let idx = find_first_match(rows(&entries, true), "alpha").expect("should match");
        assert_eq!(
            idx, 0,
            "hidden '.alpha.txt' must be considered when include_hidden=true"
        );
    }

    /// Regression test for the visible-space indexing contract.
    ///
    /// Before this fix, `find_first_match` returned the absolute index into the
    /// `entries` vec. With a hidden file sitting before the match in the vec,
    /// the frontend (which uses the index in the visible sequence, same as
    /// `get_file_at` / `get_file_range`) landed one row too far down per
    /// skipped dotfile. This test exercises exactly that scenario.
    #[test]
    fn returns_visible_space_index_when_hidden_precedes_match() {
        // Vec layout: [hidden, hidden, target, other]
        // Absolute indices:   0       1       2       3
        // Visible indices:    -       -       0       1
        let entries = vec![
            entry(".hidden_a"),
            entry(".hidden_b"),
            entry("target.txt"),
            entry("other.bin"),
        ];

        let idx = find_first_match(rows(&entries, false), "target").expect("should match");
        // The visible-space index of "target.txt" is 0, not the absolute 2.
        assert_eq!(
            idx, 0,
            "must return visible-space index (0), not absolute vec index (2), so the frontend cursor doesn't skip rows"
        );

        // Sanity check: with include_hidden=true, the same match lands at
        // visible-index 2 because the two dotfiles are now visible too.
        let idx_with_hidden = find_first_match(rows(&entries, true), "target").expect("should match");
        assert_eq!(idx_with_hidden, 2);
    }

    #[test]
    fn case_insensitive_with_lowercase_query() {
        // Lowercase query → smart case → matches against UPPERCASE filename.
        let entries = vec![entry("README.md"), entry("TESTS.txt"), entry("other.bin")];
        let idx = find_first_match(rows(&entries, true), "tes").expect("should match");
        assert_eq!(idx, 1);
    }

    #[test]
    fn unicode_filename_is_matchable() {
        // Nucleo normalizes Unicode (Normalization::Smart). Typing the ASCII form
        // should still find the accented filename. We document the observed behavior
        // here rather than asserting a strict score. What matters is "some match
        // is found and it's the Résumé entry, not the unrelated one".
        let entries = vec![entry("notes.txt"), entry("Résumé.pdf"), entry("photo.jpg")];
        let idx = find_first_match(rows(&entries, true), "resume").expect("should match");
        assert_eq!(idx, 1, "ASCII 'resume' should fold into 'Résumé.pdf'");
    }

    /// The contract that matters in the app: the number the jump hands back is a
    /// row number in the pane's own space, so the cursor lands on the file the
    /// user is looking at.
    ///
    /// Both things the pane leaves out sit before the target here. Deriving the
    /// sequence inside this module skipped only the dotfile, so an in-flight
    /// scratch file next to a copy in progress moved the cursor one row down.
    #[test]
    fn the_jump_lands_on_the_row_the_pane_shows() {
        let _show = ShowTempsGuard::set(false);
        let operation = Arc::new(());
        let temp = StagingTemp::mint(
            std::path::Path::new("/aaa-copying.bin"),
            Some(Arc::downgrade(&operation)),
        );
        let temp_name = temp
            .path()
            .file_name()
            .expect("a minted temp always has a file name")
            .to_string_lossy()
            .into_owned();

        // Rows the pane shows: only "target.txt" and "zzz.bin", at 0 and 1.
        let listing = TestListing::new()
            .volume("test")
            .path("/")
            .entries(vec![
                entry(".hidden"),
                entry(&temp_name),
                entry("target.txt"),
                entry("zzz.bin"),
            ])
            .insert("fuzzy-visible-rows");

        assert_eq!(
            fuzzy_find_first_match_in_listing(listing.id(), "target", false).expect("listing is cached"),
            Some(0),
            "the target is the pane's first row, whatever sits above it in the listing"
        );
    }
}
