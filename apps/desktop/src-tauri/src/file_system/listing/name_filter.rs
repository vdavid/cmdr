//! Quick filter: the pattern a pane narrows its rows to while the user types.
//!
//! The pane's rows are the entries [`super::visible_rows`] shows,
//! so the filter is one more input to THAT predicate, never a second filter
//! point: counts, ranges, selection indices, type-to-jump, and `directory-diff`
//! rows all agree on what a filtered pane is showing.
//!
//! Matching is Total Commander's: the pattern matches ANYWHERE in the name,
//! ignoring case and Unicode form (`cmdr_fs::name_fold`), and `*` / `?` stand
//! for any run of characters / any one character. So `rep` finds
//! `Annual report.pdf`, and `*.pdf` (or just `.pdf`) finds every PDF.

use std::borrow::Cow;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use cmdr_fs::ignore_poison::RwLockIgnorePoison;

use crate::file_system::listing::cached_listing::LISTING_CACHE;
use crate::file_system::listing::operations::ListingLookupError;

use cmdr_fs::name_fold::fold_name;

/// A non-empty quick-filter pattern, folded once so a match folds only the name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NameFilter {
    /// The pattern, case- and form-folded, as characters for the wildcard walk.
    pattern: Vec<char>,
    /// The same pattern as a string, for the wildcard-free fast path.
    folded: String,
    /// Whether the pattern holds a `*` or `?`.
    has_wildcards: bool,
}

impl NameFilter {
    /// The filter for `raw`, or `None` when there's nothing to filter by (an
    /// empty pattern, or one of only `*`s, matches every name).
    pub(crate) fn new(raw: &str) -> Option<Self> {
        if raw.chars().all(|c| c == '*') {
            return None;
        }
        let folded = fold_name(raw).into_owned();
        Some(Self {
            pattern: folded.chars().collect(),
            has_wildcards: folded.contains(['*', '?']),
            folded,
        })
    }

    /// Whether a row named `name` survives the filter.
    pub(crate) fn matches(&self, name: &str) -> bool {
        let name: Cow<'_, str> = fold_name(name);
        if !self.has_wildcards {
            return name.contains(self.folded.as_str());
        }
        let name: Vec<char> = name.chars().collect();
        contains_glob(&name, &self.pattern)
    }
}

/// Whether `pattern` (with `*` and `?`) matches some run of `text`: a glob with
/// an implied `*` on both ends. Iterative, backtracking only to the last `*`,
/// so it's linear-ish and never recurses on a hostile pattern.
fn contains_glob(text: &[char], pattern: &[char]) -> bool {
    // An implied leading `*`: every start position is the "last star".
    let (mut t, mut p) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = Some((0, 0));
    loop {
        if p == pattern.len() {
            // An implied trailing `*`: matching the whole pattern is enough.
            return true;
        }
        if pattern[p] == '*' {
            star = Some((p + 1, t));
            p += 1;
            continue;
        }
        if t < text.len() && (pattern[p] == '?' || pattern[p] == text[t]) {
            t += 1;
            p += 1;
            continue;
        }
        match star {
            Some((star_p, star_t)) if star_t < text.len() => {
                star = Some((star_p, star_t + 1));
                p = star_p;
                t = star_t + 1;
            }
            _ => return false,
        }
    }
}

/// Where the pane's cursor and selection land after a quick-filter change, in
/// the new row space.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NameFilterResult {
    /// Whether the listing took the new pattern. `false` only when the caller
    /// asked to refuse a pattern nothing matches: the listing then keeps its
    /// previous filter, and the rest of this answer describes that one.
    pub accepted: bool,
    /// How many rows the pane shows under the new filter.
    pub total_count: usize,
    /// The row of the file that was under the cursor, or `None` when the new
    /// filter leaves it out (or no file was given).
    pub new_cursor_index: Option<usize>,
    /// The rows of the previously selected files the new filter still shows. A
    /// selected file the filter leaves out drops out of the selection, so no
    /// operation ever acts on a row the user can't see.
    pub new_selected_indices: Vec<usize>,
    /// The diff sequence the new row space starts at, when the filter changed. Every
    /// `directory-diff` numbered up to it describes the old rows: the pane takes it
    /// as its last applied sequence and skips them. `None` when nothing changed.
    pub sequence: Option<u64>,
}

/// Sets (or, with an empty or `None` pattern, clears) the pane's quick filter,
/// and carries the cursor and selection into the new row space, all under ONE
/// write lock so no diff or read can land between the old row space and the new.
///
/// With `refuse_empty`, a pattern that matches no row is refused and the old
/// filter stays (`accepted: false`): Total Commander's rule that typing only
/// ever narrows down to the last match, never past it. Deciding it here, under
/// the same lock, is what makes the refusal exact; the frontend can't know the
/// count before asking.
///
/// Superseded pending transitions are dropped under the cache write lock, before
/// a watcher can publish a transition in the new row space.
#[cfg(test)]
pub fn set_listing_name_filter(
    listing_id: &str,
    pattern: Option<&str>,
    include_hidden: bool,
    cursor_filename: Option<&str>,
    selected_indices: &[usize],
    refuse_empty: bool,
) -> Result<NameFilterResult, ListingLookupError> {
    set_listing_name_filter_inner(
        listing_id,
        pattern,
        include_hidden,
        cursor_filename,
        selected_indices,
        refuse_empty,
        None,
        #[cfg(test)]
        || {},
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "Filter reconfiguration carries a guarded selection"
)]
pub fn set_listing_name_filter_guarded(
    listing_id: &str,
    pattern: Option<&str>,
    include_hidden: bool,
    cursor_filename: Option<&str>,
    selected_indices: &[usize],
    refuse_empty: bool,
    expected_sequence: Option<u64>,
) -> Result<NameFilterResult, ListingLookupError> {
    set_listing_name_filter_inner(
        listing_id,
        pattern,
        include_hidden,
        cursor_filename,
        selected_indices,
        refuse_empty,
        expected_sequence,
        #[cfg(test)]
        || {},
    )
}

#[allow(
    clippy::too_many_arguments,
    reason = "Filter reconfiguration carries a guarded selection and test hook"
)]
pub(super) fn set_listing_name_filter_inner(
    listing_id: &str,
    pattern: Option<&str>,
    include_hidden: bool,
    cursor_filename: Option<&str>,
    selected_indices: &[usize],
    refuse_empty: bool,
    expected_sequence: Option<u64>,
    #[cfg(test)] after_unlock: impl FnOnce(),
) -> Result<NameFilterResult, ListingLookupError> {
    let result = {
        let mut cache = LISTING_CACHE.write_ignore_poison();
        let listing = cache
            .get_mut(listing_id)
            .ok_or_else(|| ListingLookupError::gone(listing_id))?;
        listing.reconcile_scratch(listing_id);
        super::operations::check_expected(listing_id, listing, expected_sequence)?;
        if expected_sequence.is_some() && listing.include_hidden() != include_hidden {
            return Err(ListingLookupError::changed(listing_id));
        }
        listing.touch();

        let selected_names: Vec<String> = {
            let rows = listing.rows(include_hidden);
            selected_indices
                .iter()
                .filter_map(|&row| rows.get(row).map(|entry| entry.name.clone()))
                .collect()
        };

        let previous = listing.name_filter().cloned();
        let next = pattern.and_then(NameFilter::new);
        // Asked of every entry, not of the current rows: the new pattern needn't
        // narrow the old one, so a row the old filter hides may be its only match.
        let refused = refuse_empty
            && next.is_some()
            && next != previous
            && !listing
                .entries()
                .iter()
                .any(|entry| listing.shows_with_filter(entry, include_hidden, next.as_ref()));
        let changed = !refused && listing.set_name_filter(next);
        // Under the write lock, so no diff can be sequenced between the switch and this.
        let sequence = if changed {
            let sequence = listing.advance_sequence();
            super::diff_emitter::drop_pending(listing_id);
            Some(sequence)
        } else {
            expected_sequence
        };

        let rows = listing.rows(include_hidden);
        let names_to_rows: HashMap<&str, usize> = rows
            .iter()
            .enumerate()
            .map(|(row, entry)| (entry.name.as_str(), row))
            .collect();
        NameFilterResult {
            accepted: !refused,
            total_count: rows.len(),
            new_cursor_index: cursor_filename.and_then(|name| names_to_rows.get(name).copied()),
            new_selected_indices: selected_names
                .iter()
                .filter_map(|name| names_to_rows.get(name.as_str()).copied())
                .collect(),
            sequence,
        }
    };
    #[cfg(test)]
    after_unlock();
    Ok(result)
}
