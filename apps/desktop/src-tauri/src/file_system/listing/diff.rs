//! The shape of a `directory-diff` and how to compute one.
//!
//! A diff names what happened to each row of a cached listing: added, removed,
//! modified in place, or moved to a new sorted position. `diff_emitter` coalesces
//! these into the `directory-diff` event; the incremental watcher path builds them
//! row by row from the cache helpers, while the full re-read path derives them here
//! with [`compute_diff`].
//!
//! Why `Move` is its own variant and how it stays minimal:
//! `../DETAILS.md` § "Reordered rows".

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use super::FileEntry;

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
}

/// `directory-diff` event sent to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, tauri_specta::Event)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryDiff {
    pub listing_id: String,
    /// Monotonic.
    pub sequence: u64,
    pub changes: Vec<DiffChange>,
}

/// Computes the diff between old and new directory listings.
///
/// Used by both local file watcher and MTP file watcher to generate
/// incremental updates for the frontend.
///
/// A row that survived is reported as `Move` only when it genuinely jumped the
/// queue, ❌ never for the index shift every row below an add or a remove takes.
/// The rows that kept their relative order are the longest increasing run of old
/// positions read in new order, so the smallest possible set is called moved.
pub fn compute_diff(old: &[FileEntry], new: &[FileEntry]) -> Vec<DiffChange> {
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
            changes.push(DiffChange::added(new_entry.clone(), new_index));
            continue;
        };
        let rank = survivor_rank;
        survivor_rank += 1;
        if !in_order.contains(&rank) {
            changes.push(DiffChange::moved(new_entry.clone(), old_index, new_index));
        } else if is_entry_modified(&old[old_index], new_entry) {
            changes.push(DiffChange::modified(new_entry.clone(), new_index));
        }
    }

    // Find removals (index refers to position in old listing)
    for (old_index, old_entry) in old.iter().enumerate() {
        if !new_map.contains(old_entry.path.as_str()) {
            changes.push(DiffChange::removed(old_entry.clone(), old_index));
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

/// Check if a file entry has been modified.
fn is_entry_modified(old: &FileEntry, new: &FileEntry) -> bool {
    old.size != new.size
        || old.modified_at != new.modified_at
        || old.permissions != new.permissions
        || old.is_directory != new.is_directory
        || old.is_symlink != new.is_symlink
}
