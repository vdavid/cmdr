//! The shape of a `directory-diff` and how to compute one.
//!
//! A diff names what happened to each row of a PANE: added, removed, modified in
//! place, or moved to a new sorted position. Rows are the pane's (`visible_rows.rs`),
//! so a change to an entry the pane shows on neither side is no change at all, and
//! a listing whose hidden entries alone changed emits nothing. `diff_emitter`
//! coalesces these into the `directory-diff` event; the single-entry cache helpers
//! build them through [`DiffChange::for_pane`], while the full re-read path derives
//! them here with [`diff_rows`] over the committed projections.
//!
//! Why `Move` is its own variant and how it stays minimal:
//! `../DETAILS.md` § "Reordered rows".

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use super::FileEntry;
#[cfg(test)]
use super::name_filter::NameFilter;
#[cfg(test)]
use super::visible_rows::shows;

/// What happened to one row of a listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DiffChangeType {
    Add,
    Remove,
    /// The row's contents changed but its sorted position didn't.
    Modify,
    /// The row's own sort key changed (an mtime bump under sort-by-date, a size change
    /// under sort-by-size), so it jumped to a new position. It carries the fresh entry,
    /// which is why it replaces rather than accompanies a [`DiffChangeType::Modify`].
    /// The frontend rides the cursor and the selection along; reporting the jump as a
    /// remove plus an add would instead leave them on whoever took the vacated row.
    Move,
}

/// A single directory diff change
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiffChange {
    #[serde(rename = "type")]
    pub change_type: DiffChangeType,
    pub entry: FileEntry,
    /// Position in the sorted listing: old listing for `Remove`, new listing for the rest.
    pub index: usize,
    /// Where the row sat before it moved. `Some` exactly on `Move`.
    pub previous_index: Option<usize>,
}

impl DiffChange {
    pub fn added(entry: FileEntry, index: usize) -> Self {
        Self {
            change_type: DiffChangeType::Add,
            entry,
            index,
            previous_index: None,
        }
    }

    pub fn removed(entry: FileEntry, index: usize) -> Self {
        Self {
            change_type: DiffChangeType::Remove,
            entry,
            index,
            previous_index: None,
        }
    }

    pub fn modified(entry: FileEntry, index: usize) -> Self {
        Self {
            change_type: DiffChangeType::Modify,
            entry,
            index,
            previous_index: None,
        }
    }

    pub fn moved(entry: FileEntry, previous_index: usize, index: usize) -> Self {
        Self {
            change_type: DiffChangeType::Move,
            entry,
            index,
            previous_index: Some(previous_index),
        }
    }

    /// What one patched entry means to the pane: the rows it sat on before the
    /// patch and sits on after it. `None` when the pane shows it on neither side,
    /// which is the whole point: a dotfile write in `~` with hidden files off.
    ///
    /// Only for a patch of this ONE entry, where a row that differs means the entry
    /// itself changed places. A batch derives its moves with [`diff_rows`].
    pub(crate) fn for_pane(entry: FileEntry, rows: PaneRows) -> Option<Self> {
        match (rows.before, rows.after) {
            (None, None) => None,
            (Some(before), None) => Some(Self::removed(entry, before)),
            (None, Some(after)) => Some(Self::added(entry, after)),
            (Some(before), Some(after)) if before == after => Some(Self::modified(entry, after)),
            (Some(before), Some(after)) => Some(Self::moved(entry, before, after)),
        }
    }
}

/// Where a patched entry shows in its pane before and after the patch; `None`
/// on a side where the pane doesn't show it (hidden, scratch, not there yet, or
/// gone). A row, never an entry index: see `CachedListing::pane_rows`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneRows {
    pub before: Option<usize>,
    pub after: Option<usize>,
}

/// `directory-diff` event sent to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryDiff {
    pub listing_id: String,
    pub batches: Vec<DirectoryDiffBatch>,
}

/// One committed transition, with indices in its own old/new row spaces.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryDiffBatch {
    pub from_sequence: u64,
    pub sequence: u64,
    pub total_count: usize,
    pub changes: Vec<DiffChange>,
}

/// Computes the diff a pane with this `include_hidden` and quick filter sees
/// between two sorted readings of its directory.
///
/// It diffs the rows the pane shows on each side (`visible_rows::shows`), so the
/// indices are rows, an entry hidden on both sides is left out, and one that turns
/// hidden or visible arrives as a remove or an add. Committed cache transitions
/// use [`diff_rows`] with their pinned projections instead of sampling live state.
///
/// A row that survived is reported as `Move` only when it genuinely jumped the
/// queue, ❌ never for the index shift every row below an add or a remove takes.
/// The rows that kept their relative order are the longest increasing run of old
/// positions read in new order, so the smallest possible set is called moved.
#[cfg(test)]
pub fn compute_diff(
    old: &[FileEntry],
    new: &[FileEntry],
    include_hidden: bool,
    name_filter: Option<&NameFilter>,
) -> Vec<DiffChange> {
    let old: Vec<&FileEntry> = old.iter().filter(|e| shows(e, include_hidden, name_filter)).collect();
    let new: Vec<&FileEntry> = new.iter().filter(|e| shows(e, include_hidden, name_filter)).collect();
    diff_rows(&old, &new)
}

/// Diff over rows already picked out, so an index is a row.
pub(crate) fn diff_rows(old: &[&FileEntry], new: &[&FileEntry]) -> Vec<DiffChange> {
    let mut changes = Vec::new();

    // Create lookup maps by path
    let old_map: HashMap<&str, usize> = old.iter().enumerate().map(|(i, e)| (e.path.as_str(), i)).collect();
    let new_map: HashSet<&str> = new.iter().map(|e| e.path.as_str()).collect();

    // Survivors in NEW order, each carrying where it sat in the old listing.
    let survivors: Vec<(usize, usize)> = new
        .iter()
        .enumerate()
        .filter_map(|(new_index, entry)| {
            old_map
                .get(entry.path.as_str())
                .map(|&old_index| (new_index, old_index))
        })
        .collect();
    let old_positions: Vec<usize> = survivors.iter().map(|&(_, old_index)| old_index).collect();
    let in_order: HashSet<usize> = longest_increasing_subsequence(&old_positions).into_iter().collect();

    // Additions and survivors, in new-listing order.
    let mut survivor_rank = 0usize;
    for (new_index, new_entry) in new.iter().enumerate() {
        let Some(&old_index) = old_map.get(new_entry.path.as_str()) else {
            changes.push(DiffChange::added((*new_entry).clone(), new_index));
            continue;
        };
        let rank = survivor_rank;
        survivor_rank += 1;
        if !in_order.contains(&rank) {
            changes.push(DiffChange::moved((*new_entry).clone(), old_index, new_index));
        } else if is_entry_modified(old[old_index], new_entry) {
            changes.push(DiffChange::modified((*new_entry).clone(), new_index));
        }
    }

    // Find removals (index refers to position in old listing)
    for (old_index, old_entry) in old.iter().enumerate() {
        if !new_map.contains(old_entry.path.as_str()) {
            changes.push(DiffChange::removed((*old_entry).clone(), old_index));
        }
    }

    changes
}

/// Positions (into `values`) of one longest strictly increasing subsequence, via
/// patience sorting: O(n log n), which matters because this runs over every row of
/// a re-read listing.
fn longest_increasing_subsequence(values: &[usize]) -> Vec<usize> {
    // `tails[k]` is the position of the smallest tail among the increasing
    // subsequences of length `k + 1` found so far.
    let mut tails: Vec<usize> = Vec::new();
    let mut predecessor: Vec<Option<usize>> = vec![None; values.len()];

    for (position, &value) in values.iter().enumerate() {
        let length = tails.partition_point(|&tail| values[tail] < value);
        if length > 0 {
            predecessor[position] = Some(tails[length - 1]);
        }
        if length == tails.len() {
            tails.push(position);
        } else {
            tails[length] = position;
        }
    }

    let mut result = Vec::with_capacity(tails.len());
    let mut cursor = tails.last().copied();
    while let Some(position) = cursor {
        result.push(position);
        cursor = predecessor[position];
    }
    result.reverse();
    result
}

/// Check if a file entry has been modified. `is_hidden` counts: a pane showing
/// hidden files dims the row, and one hiding them loses it.
fn is_entry_modified(old: &FileEntry, new: &FileEntry) -> bool {
    old.size != new.size
        || old.modified_at != new.modified_at
        || old.permissions != new.permissions
        || old.is_directory != new.is_directory
        || old.is_symlink != new.is_symlink
        || old.is_hidden != new.is_hidden
}
