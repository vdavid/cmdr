//! Compare directories (⇧F2): which files each pane marks against the other.
//!
//! Total Commander's "Compare directories": files are matched by name, folders are
//! left alone, and each pane marks the files the other pane lacks plus, per
//! [`CompareDirectoriesMode`], the copies that are newer or differ in size. Equal
//! files and the older copy stay unmarked, so after a compare the marked rows
//! are exactly what F5 would bring the other side up to date with.
//!
//! It reads the two cached listings under ONE lock and answers in each pane's own
//! row space ([`CachedListing::rows`](super::cached_listing::CachedListing::rows)),
//! so the indices are ready to become a selection, and a row the pane doesn't
//! show (a hidden file, scratch) is never marked. Row numbers only fit a pane
//! showing the same committed state, so the answer carries each listing's revision.
//! Scratch drift is committed before reading; `settled` confirms the requested visibility.
//! The frontend marks nothing unless its own applied revisions also match.

use std::collections::HashMap;
use std::sync::atomic::Ordering;

use serde::{Deserialize, Serialize};

use cmdr_fs::name_fold::fold_name;

use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::listing::operations::ListingLookupError;
use crate::file_system::listing::visible_rows::VisibleRows;

/// How far apart two modification times can be and still count as the same, in
/// seconds. FAT and many network shares store time in 2 s steps, so a faithful
/// copy can read a second or two off its original.
const SAME_TIME_TOLERANCE_SECS: u64 = 2;

/// Which copies count as different, beyond the files missing on the other side
/// (marked in every mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CompareDirectoriesMode {
    /// Total Commander's default: the newer copy of a file is marked, the older
    /// one isn't.
    NewerAndMissing,
    /// Only the files the other side doesn't have.
    Missing,
    /// Both copies of a file whose size differs, whichever is newer.
    SizeAndMissing,
}

/// Why a comparison didn't answer. Typed, so the frontend never reads a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum CompareDirectoriesError {
    /// A pane's listing is no longer cached (its pane moved on).
    Gone {
        listing_id: String,
    },
    Changed {
        listing_id: String,
    },
    /// The comparison didn't finish within its deadline.
    TimedOut,
    /// The comparison's worker failed; `detail` is log text only.
    Internal {
        detail: String,
    },
}

impl From<ListingLookupError> for CompareDirectoriesError {
    fn from(err: ListingLookupError) -> Self {
        match err {
            ListingLookupError::Gone { listing_id } => Self::Gone { listing_id },
            ListingLookupError::Changed { listing_id } => Self::Changed { listing_id },
        }
    }
}

/// The rows to mark in each pane, in that pane's row space (no `..` offset), and
/// which state of each listing they were read from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CompareDirectoriesResult {
    pub left: Vec<usize>,
    pub right: Vec<usize>,
    /// The committed visible revision the rows were read at. A pane may mark them
    /// only while its applied revision is exactly this.
    pub left_sequence: u64,
    pub right_sequence: u64,
    /// Requested visibility matches both reconciled committed listings.
    /// Publication latency is irrelevant.
    pub settled: bool,
}

/// Compares the files two panes show and answers which rows each should mark.
pub fn compare_directories(
    left_listing_id: &str,
    left_include_hidden: bool,
    right_listing_id: &str,
    right_include_hidden: bool,
    mode: CompareDirectoriesMode,
) -> Result<CompareDirectoriesResult, ListingLookupError> {
    let cache = super::operations::reconciled_cache(&[left_listing_id, right_listing_id]);
    let left = cache
        .get(left_listing_id)
        .ok_or_else(|| ListingLookupError::gone(left_listing_id))?;
    let right = cache
        .get(right_listing_id)
        .ok_or_else(|| ListingLookupError::gone(right_listing_id))?;
    left.touch();
    right.touch();
    let left_sequence = left.sequence.load(Ordering::Acquire);
    let right_sequence = right.sequence.load(Ordering::Acquire);
    let settled = left.include_hidden() == left_include_hidden && right.include_hidden() == right_include_hidden;

    let left_rows = left.rows(left_include_hidden);
    let right_rows = right.rows(right_include_hidden);
    let left_files = Counterparts::of(&left_rows);
    let right_files = Counterparts::of(&right_rows);
    let left_marked = rows_to_mark(&left_rows, &left_files, &right_files, mode);
    let right_marked = rows_to_mark(&right_rows, &right_files, &left_files, mode);
    drop(left_rows);
    drop(right_rows);
    drop(cache);

    Ok(CompareDirectoriesResult {
        left: left_marked,
        right: right_marked,
        left_sequence,
        right_sequence,
        settled,
    })
}

/// The files one pane shows, looked up by name the way the other pane's names
/// would find them.
struct Counterparts<'a> {
    exact: HashMap<&'a str, &'a FileEntry>,
    /// Folded (case and Unicode form) name → the file, or `None` when two files
    /// on this side fold to the same key (a case-sensitive volume holding both
    /// `Report` and `report`): then only an exact name finds either.
    folded: HashMap<String, Option<&'a FileEntry>>,
}

impl<'a> Counterparts<'a> {
    fn of(rows: &VisibleRows<'a>) -> Self {
        let mut exact = HashMap::new();
        let mut folded: HashMap<String, Option<&'a FileEntry>> = HashMap::new();
        for entry in rows.iter().filter(|entry| !entry.is_directory) {
            exact.insert(entry.name.as_str(), entry);
            folded
                .entry(fold_name(&entry.name).into_owned())
                .and_modify(|slot| *slot = None)
                .or_insert(Some(entry));
        }
        Self { exact, folded }
    }

    /// Whether exactly one file on this side folds to `key`.
    fn folds_uniquely(&self, key: &str) -> bool {
        matches!(self.folded.get(key), Some(Some(_)))
    }

    /// The counterpart on this side (`self`) of a file named `name` from the side
    /// `own`: the exact spelling first, else a folded match, which counts only
    /// when the key is unique on BOTH sides. Mutual, so the two directions always
    /// agree: with `Report` and `report` on one side and `REPORT` on the other,
    /// none of the three finds a counterpart by folding.
    fn find(&self, name: &str, own: &Counterparts<'_>) -> Option<&'a FileEntry> {
        if let Some(entry) = self.exact.get(name) {
            return Some(entry);
        }
        let key = fold_name(name);
        if !own.folds_uniquely(&key) {
            return None;
        }
        self.folded.get(key.as_ref()).copied().flatten()
    }
}

fn rows_to_mark(
    rows: &VisibleRows<'_>,
    own: &Counterparts<'_>,
    other: &Counterparts<'_>,
    mode: CompareDirectoriesMode,
) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, entry)| !entry.is_directory)
        .filter(|(_, entry)| match other.find(&entry.name, own) {
            None => true,
            Some(counterpart) => differs(entry, counterpart, mode),
        })
        .map(|(row, _)| row)
        .collect()
}

/// Whether `this` copy is marked against its `other` copy. An unknown time or
/// size never marks: we only claim a difference we can see.
fn differs(this: &FileEntry, other: &FileEntry, mode: CompareDirectoriesMode) -> bool {
    match mode {
        CompareDirectoriesMode::Missing => false,
        CompareDirectoriesMode::NewerAndMissing => match (this.modified_at, other.modified_at) {
            (Some(this), Some(other)) => this > other.saturating_add(SAME_TIME_TOLERANCE_SECS),
            _ => false,
        },
        CompareDirectoriesMode::SizeAndMissing => match (this.size, other.size) {
            (Some(this), Some(other)) => this != other,
            _ => false,
        },
    }
}
