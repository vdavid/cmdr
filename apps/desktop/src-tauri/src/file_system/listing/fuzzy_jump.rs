//! Type-to-jump: highest-scoring fuzzy filename match within a cached listing.
//!
//! Powers the in-directory navigation feature where the user types a few characters
//! in a focused file pane and the cursor jumps to the best-matching entry.
//!
//! ## Crate choice — why `nucleo-matcher`
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
//! `find_first_match` is a pure function over `&[FileEntry]` — no `LISTING_CACHE`
//! lock, no `tokio`. That makes it trivial to unit-test against in-memory fixtures
//! and keeps the Tauri command layer (`commands/file_system/listing.rs`) a thin
//! pass-through that just grabs the read lock and delegates here.

use std::time::Instant;

use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};

use crate::file_system::listing::caching::LISTING_CACHE;
use crate::file_system::listing::metadata::FileEntry;

/// Returns the **visible-space** index of the highest-scoring fuzzy match for `query`,
/// or `None` if no entry matches.
///
/// Rules:
/// - When `include_hidden` is `false`, dotfiles (`name.starts_with('.')`) are skipped.
/// - The match runs against the whole filename (including extension) — fuzzy scoring
///   already rewards prefix and word-boundary matches, so we don't split on the dot.
/// - Smart-case: an all-lowercase query matches case-insensitively; any uppercase
///   character makes that character case-sensitive (delegated to nucleo-matcher).
/// - Ties (equal score) resolve to the lower index, which matches the listing's
///   active sort order.
/// - Empty query → `None`. Empty listing → `None`.
/// - The synthetic `..` parent entry is **not** in `LISTING_CACHE` (it's prepended
///   by the frontend), so there's no special case for it here.
///
/// ## Index space
///
/// The returned index counts entries in the **visible** sequence — the same
/// sequence `operations::get_file_at` / `get_file_range` produce when called
/// with the same `include_hidden` flag. When `include_hidden` is `false` and
/// the entries vec contains hidden files before the match, the returned index
/// will be **smaller** than the absolute vec position. The frontend uses this
/// directly as a cursor index (plus the `+1` parent-entry offset when
/// `hasParent`), so the indexing space must line up with `getFileAt` /
/// `getFileRange`.
pub fn find_first_match(entries: &[FileEntry], query: &str, include_hidden: bool) -> Option<usize> {
    if query.is_empty() || entries.is_empty() {
        return None;
    }

    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);

    let mut best: Option<(usize, u32)> = None;
    let mut haystack_buf: Vec<char> = Vec::new();

    // Iterate the visible sequence directly so the returned index matches the
    // cursor space used by `getFileAt` / `getFileRange` (which iterate via
    // `visible_entries(...).nth(index)` in `operations.rs`).
    let visible = entries.iter().filter(|e| include_hidden || !e.name.starts_with('.'));

    for (visible_idx, entry) in visible.enumerate() {
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

/// Convenience wrapper that grabs the `LISTING_CACHE` read lock, runs
/// `find_first_match`, and emits a single `type_to_jump` debug log line with
/// the per-call timing. The Tauri command in `commands::file_system::listing`
/// is a thin async pass-through over this.
pub fn fuzzy_find_first_match_in_listing(
    listing_id: &str,
    query: &str,
    include_hidden: bool,
) -> Result<Option<usize>, String> {
    let started = Instant::now();
    let cache = LISTING_CACHE
        .read()
        .map_err(|_| "Failed to acquire cache lock".to_string())?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| format!("Listing not found: {}", listing_id))?;

    let result = find_first_match(&listing.entries, query, include_hidden);
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
    use crate::file_system::listing::metadata::FileEntry;

    fn entry(name: &str) -> FileEntry {
        FileEntry::new(name.to_string(), format!("/{}", name), false, false)
    }

    #[test]
    fn empty_listing_returns_none() {
        let entries: Vec<FileEntry> = Vec::new();
        assert_eq!(find_first_match(&entries, "abc", true), None);
    }

    #[test]
    fn empty_query_returns_none() {
        let entries = vec![entry("README.md"), entry("AGENTS.md")];
        assert_eq!(find_first_match(&entries, "", true), None);
    }

    #[test]
    fn no_matches_returns_none() {
        let entries = vec![entry("README.md"), entry("AGENTS.md")];
        // "xyz" shares no characters with either name.
        assert_eq!(find_first_match(&entries, "xyz", true), None);
    }

    #[test]
    fn single_match_returns_its_index() {
        let entries = vec![entry("README.md"), entry("AGENTS.md"), entry("Cargo.toml")];
        // Only "Cargo.toml" contains the subsequence "crg" / "cargo".
        let idx = find_first_match(&entries, "cargo", true).expect("should match");
        assert_eq!(idx, 2);
    }

    #[test]
    fn multiple_matches_pick_highest_scored() {
        // "tests" fuzzy-matches both — but "tests.js" is the better (prefix) match
        // than "my_tests_helper.rs" so it should win.
        let entries = vec![entry("my_tests_helper.rs"), entry("tests.js"), entry("other.txt")];
        let idx = find_first_match(&entries, "tests", true).expect("should match");
        assert_eq!(idx, 1, "prefix match 'tests.js' should outscore 'my_tests_helper.rs'");
    }

    #[test]
    fn ties_resolve_to_lower_index() {
        // Two identical names → identical scores → lower index wins.
        let entries = vec![entry("hello.txt"), entry("hello.txt")];
        let idx = find_first_match(&entries, "hello", true).expect("should match");
        assert_eq!(idx, 0);
    }

    #[test]
    fn hidden_entry_excluded_when_include_hidden_false() {
        let entries = vec![entry(".env"), entry("env_setup.sh")];
        // With hidden excluded, only "env_setup.sh" is a candidate. The dotfile
        // is invisible, so "env_setup.sh" sits at visible-index 0.
        let idx = find_first_match(&entries, "env", false).expect("should match");
        assert_eq!(idx, 0);
    }

    #[test]
    fn hidden_entry_included_when_include_hidden_true() {
        // Deterministic case: two clearly distinct names. The only entry that can
        // match "alpha" is ".alpha.txt" — "zeta.bin" shares no characters with the
        // query. The match must be found AND must land at the dotfile's visible
        // index (0 when hidden is on, since the dotfile is then visible).
        let entries = vec![entry(".alpha.txt"), entry("zeta.bin")];
        let idx = find_first_match(&entries, "alpha", true).expect("should match");
        assert_eq!(
            idx, 0,
            "hidden '.alpha.txt' must be considered when include_hidden=true"
        );
    }

    /// Regression test for the visible-space indexing contract.
    ///
    /// Before this fix, `find_first_match` returned the absolute index into the
    /// `entries` vec. With a hidden file sitting before the match in the vec,
    /// the frontend (which uses the index in the visible sequence — same as
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

        let idx = find_first_match(&entries, "target", false).expect("should match");
        // The visible-space index of "target.txt" is 0, not the absolute 2.
        assert_eq!(
            idx, 0,
            "must return visible-space index (0), not absolute vec index (2), so the frontend cursor doesn't skip rows"
        );

        // Sanity check: with include_hidden=true, the same match lands at
        // visible-index 2 because the two dotfiles are now visible too.
        let idx_with_hidden = find_first_match(&entries, "target", true).expect("should match");
        assert_eq!(idx_with_hidden, 2);
    }

    #[test]
    fn case_insensitive_with_lowercase_query() {
        // Lowercase query → smart case → matches against UPPERCASE filename.
        let entries = vec![entry("README.md"), entry("TESTS.txt"), entry("other.bin")];
        let idx = find_first_match(&entries, "tes", true).expect("should match");
        assert_eq!(idx, 1);
    }

    #[test]
    fn unicode_filename_is_matchable() {
        // Nucleo normalizes Unicode (Normalization::Smart). Typing the ASCII form
        // should still find the accented filename. We document the observed behavior
        // here rather than asserting a strict score — what matters is "some match
        // is found and it's the Résumé entry, not the unrelated one".
        let entries = vec![entry("notes.txt"), entry("Résumé.pdf"), entry("photo.jpg")];
        let idx = find_first_match(&entries, "resume", true).expect("should match");
        assert_eq!(idx, 1, "ASCII 'resume' should fold into 'Résumé.pdf'");
    }
}
