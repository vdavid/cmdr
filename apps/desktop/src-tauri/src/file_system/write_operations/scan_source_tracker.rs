//! Per-TOP-LEVEL-source bookkeeping over a flat scan result.
//!
//! A scan hands back one flat list of files; the drivers report progress and
//! outcomes per top-level source the user actually selected. `SourceItemTracker`
//! is the bridge: it counts down each source's files and says when one is
//! finished and with what verdict, so `write-source-item-done` fires once per
//! source rather than once per file.
//!
//! ❗ A source's identity here is `source_root` + the first component of the
//! file's path relative to it (`top_level_source_path`), ❌ never the file's own
//! parent. Getting that wrong reports a deep child as a finished source.

use std::collections::HashSet;
use std::path::PathBuf;

use super::state::FileInfo;
use super::types::{SourceItemOutcome, TopLevelSkipped};
use crate::file_system::listing::{SortColumn, SortOrder};
/// Builds a map from top-level source path to the number of files it contains in the scan result.
///
/// Each `FileInfo` has a `source_root` (the parent of the top-level source) and a `path` (the full
/// file path). The top-level source is reconstructed as `source_root + first component of (path
/// relative to source_root)`.
pub(super) fn build_source_file_counts(files: &[FileInfo]) -> std::collections::HashMap<PathBuf, usize> {
    let mut counts = std::collections::HashMap::new();
    for file_info in files {
        let top_level_source = top_level_source_path(file_info);
        *counts.entry(top_level_source).or_insert(0) += 1;
    }
    counts
}

/// Reconstructs the top-level source path from a `FileInfo`.
///
/// For a file at `/home/user/docs/mydir/sub/file.txt` with `source_root = /home/user/docs`,
/// returns `/home/user/docs/mydir`.
/// For a single file `/home/user/docs/file.txt` with `source_root = /home/user/docs`,
/// returns `/home/user/docs/file.txt`.
pub(super) fn top_level_source_path(file_info: &FileInfo) -> PathBuf {
    if let Ok(relative) = file_info.path.strip_prefix(&file_info.source_root)
        && let Some(first_component) = relative.components().next()
    {
        return file_info.source_root.join(first_component);
    }
    // Fallback: use the path itself (shouldn't happen with well-formed FileInfo)
    file_info.path.clone()
}

/// What an engine did with ONE file it processed.
///
/// ❗ Every engine that resolves conflicts mid-flight owes this upward. A copy
/// decides per-file Skips inside `copy_single_item`, after the driver's
/// pre-flight skip pass has run, so returning "done" for both outcomes is what
/// let a refused item be counted as copied: the toast said "Copied 2 files."
/// having copied one, and `write-source-item-done` said `Done`.
///
/// ⚠️ It says what the POLICY decided, ❌ never what is on disk. A caller
/// authorizing a destructive act — a cross-FS move deciding whether to delete
/// an original — asks the LEDGER, which records what actually landed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum FileVerdict {
    /// The operation was carried out on it: bytes copied, entry unlinked.
    CarriedOut,
    /// Walked past untouched: a conflict resolved to Skip, a type-mismatch
    /// parent Skip, or a source that already IS its destination.
    Skipped,
}

/// A top-level source whose last file has just been processed, and the verdict
/// to report for it.
pub(super) struct FinishedSource {
    pub source_path: PathBuf,
    pub outcome: SourceItemOutcome,
}

/// Tracks per-source-item file counts and emits when all files for a source are done.
pub(super) struct SourceItemTracker {
    totals: std::collections::HashMap<PathBuf, usize>,
    processed: std::collections::HashMap<PathBuf, usize>,
    /// Sources at least one of whose files actually landed. A source missing
    /// here when its count completes had every file walked past.
    landed_something: HashSet<PathBuf>,
    /// Running tally of the top-level sources that finished having landed
    /// nothing, for `WriteCompleteEvent::top_level_skipped`.
    skipped: TopLevelSkipped,
}

impl SourceItemTracker {
    pub fn new(files: &[FileInfo]) -> Self {
        Self {
            totals: build_source_file_counts(files),
            processed: std::collections::HashMap::new(),
            landed_something: HashSet::new(),
            skipped: TopLevelSkipped { files: 0, folders: 0 },
        }
    }

    /// The top-level sources that landed nothing, split by kind. Read after the
    /// per-file loop; a source only lands in it once its LAST file is in.
    ///
    /// ⚠️ Sources dropped before the tracker was built (a copy's bulk pre-known
    /// -conflict skip) are not in here — nothing ever calls `record` for them.
    /// Their caller adds them.
    pub fn skipped_top_level(&self) -> TopLevelSkipped {
        self.skipped
    }

    /// Records a processed file and what happened to it. Returns the source
    /// when all of its files are done.
    ///
    /// A source counts as `Done` if ANY of its files landed: the skip is
    /// per-file, so one refused child inside a folder doesn't make the whole
    /// folder a skip. Only a source that landed nothing reports `Skipped`.
    pub fn record(&mut self, file_info: &FileInfo, verdict: FileVerdict) -> Option<FinishedSource> {
        let source_path = top_level_source_path(file_info);
        if verdict == FileVerdict::CarriedOut {
            self.landed_something.insert(source_path.clone());
        }
        let count = self.processed.entry(source_path.clone()).or_insert(0);
        *count += 1;
        if self.totals.get(&source_path) != Some(count) {
            return None;
        }
        let outcome = if self.landed_something.contains(&source_path) {
            SourceItemOutcome::Done
        } else {
            SourceItemOutcome::Skipped
        };
        if outcome == SourceItemOutcome::Skipped {
            // A source that IS the file it walked was a top-level FILE; one
            // whose path differs from the file's is the folder above it. No
            // stat needed, and it stays right for a source that has since
            // vanished.
            if source_path == file_info.path {
                self.skipped.files += 1;
            } else {
                self.skipped.folders += 1;
            }
        }
        Some(FinishedSource { source_path, outcome })
    }
}

// ============================================================================
// Scanning helpers
// ============================================================================

/// Sorts files according to the specified column and order.
pub(super) fn sort_files(files: &mut [FileInfo], column: SortColumn, order: SortOrder) {
    files.sort_by(|a, b| {
        let cmp = match column {
            SortColumn::Name => a.name_lower().cmp(&b.name_lower()),
            SortColumn::Extension => a
                .extension()
                .cmp(&b.extension())
                .then_with(|| a.name_lower().cmp(&b.name_lower())),
            SortColumn::Size => a.size.cmp(&b.size),
            SortColumn::Modified => a.modified.cmp(&b.modified),
            SortColumn::Created => a.created.cmp(&b.created),
        };
        match order {
            SortOrder::Ascending => cmp,
            SortOrder::Descending => cmp.reverse(),
        }
    });
}
