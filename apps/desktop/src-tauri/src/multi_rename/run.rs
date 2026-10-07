//! The two calls the sheet makes: preview (every keystroke) and apply (Start).
//!
//! Both read the pane's rows off its cached listing, so the frontend sends row
//! numbers and the spec, never paths or names. Apply recomputes the preview and
//! renames only the rows that are `Ready` NOW, through the bulk-rename executor
//! Ask Cmdr uses (`start_renames`): chains, swaps, and case-only renames, a
//! fingerprint recheck before each step, every step journaled, so the operation
//! log's undo reverses it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tokio::time::Duration;

use serde::{Deserialize, Serialize};

use crate::deadline::blocking_typed_result_with_timeout;
use crate::file_system::listing::cached_listing::LISTING_CACHE;
use crate::file_system::listing::metadata::FileEntry;
use crate::file_system::write_operations::{BulkRenameRow, RenameStartError, SourceFingerprint, start_renames};
use crate::ignore_poison::RwLockIgnorePoison;
use crate::operation_log::types::Initiator;

use super::plan::{Compiled, MultiRenameSpec, PreviewRow, SpecError, preview};

/// Why a preview or an apply didn't answer. Typed, so the frontend words it.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum MultiRenameError {
    /// The pane's listing is no longer cached (it moved on).
    Gone { listing_id: String },
    /// The spec doesn't parse; the sheet shows it under its field.
    Spec { error: SpecError },
    /// No row is ready to rename.
    NothingToRename,
    /// No volume answers for the folder (unplugged, disconnected).
    NotConnected { volume_id: String },
    /// The executor refused before renaming anything.
    CouldntStart { reason: RenameStartError },
    /// The folder changed since the preview the user started from: re-preview.
    PreviewOutOfDate,
    /// The folder is read-only (inside an archive or a `.git` portal).
    ReadOnly,
    /// The preview didn't finish within its deadline.
    TimedOut,
    /// The preview's worker failed; `detail` is log text only.
    Internal { detail: String },
}

/// A started rename.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MultiRenameStarted {
    /// The operation, for its progress, the queue, and Undo.
    pub operation_id: String,
    /// How many rows it renames.
    pub renaming: usize,
}

/// What a preview of one pane holds.
struct Snapshot {
    volume_id: String,
    dir: PathBuf,
    rows: Vec<(usize, FileEntry)>,
    siblings: Vec<FileEntry>,
}

/// The pane's `rows` (backend row numbers, in rename order; `None` for every row
/// the pane shows) and the folder's every entry.
fn snapshot(listing_id: &str, include_hidden: bool, rows: Option<&[usize]>) -> Result<Snapshot, MultiRenameError> {
    let cache = LISTING_CACHE.read_ignore_poison();
    let listing = cache.get(listing_id).ok_or_else(|| MultiRenameError::Gone {
        listing_id: listing_id.to_string(),
    })?;
    listing.touch();
    let shown = listing.rows(include_hidden);
    let picked: Vec<(usize, FileEntry)> = match rows {
        Some(rows) => rows
            .iter()
            .filter_map(|&row| shown.get(row).map(|entry| (row, entry.clone())))
            .collect(),
        None => shown.iter().cloned().enumerate().collect(),
    };
    Ok(Snapshot {
        volume_id: listing.volume_id.clone(),
        dir: listing.path.as_path().to_path_buf(),
        rows: picked,
        siblings: listing.entries().to_vec(),
    })
}

fn previewed(snapshot: &Snapshot, spec: &MultiRenameSpec) -> Result<Vec<PreviewRow>, MultiRenameError> {
    let compiled = Compiled::new(spec).map_err(|error| MultiRenameError::Spec { error })?;
    let rows: Vec<(usize, &FileEntry)> = snapshot.rows.iter().map(|(row, entry)| (*row, entry)).collect();
    Ok(preview(&compiled, &snapshot.dir, &rows, &snapshot.siblings))
}

/// The sheet's live preview.
pub(crate) fn preview_rows(
    listing_id: &str,
    include_hidden: bool,
    rows: Option<&[usize]>,
    spec: &MultiRenameSpec,
) -> Result<Vec<PreviewRow>, MultiRenameError> {
    previewed(&snapshot(listing_id, include_hidden, rows)?, spec)
}

/// One row the user saw in the preview they started from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ExpectedRename {
    pub row: usize,
    pub old_name: String,
    pub new_name: String,
}

/// What apply recomputed, matched against what the user saw.
struct Prepared {
    volume_id: String,
    dir: PathBuf,
    ready: Vec<PreviewRow>,
}

/// The blocking half of apply: recompute the preview from the listing as it is
/// now and require its ready rows to be EXACTLY the ones the user saw, so a row
/// number that shifted (a file appeared above it) never renames a different file.
fn prepare(
    listing_id: &str,
    include_hidden: bool,
    rows: Option<&[usize]>,
    spec: &MultiRenameSpec,
    expected: &[ExpectedRename],
) -> Result<Prepared, MultiRenameError> {
    let snapshot = snapshot(listing_id, include_hidden, rows)?;
    let ready: Vec<PreviewRow> = previewed(&snapshot, spec)?
        .into_iter()
        .filter(|row| row.status.is_ready())
        .collect();
    if ready.is_empty() {
        return Err(MultiRenameError::NothingToRename);
    }
    let now: HashSet<ExpectedRename> = ready
        .iter()
        .map(|r| ExpectedRename {
            row: r.row,
            old_name: r.old_name.clone(),
            new_name: r.new_name.clone(),
        })
        .collect();
    let seen: HashSet<ExpectedRename> = expected.iter().cloned().collect();
    if now != seen {
        return Err(MultiRenameError::PreviewOutOfDate);
    }
    Ok(Prepared {
        volume_id: snapshot.volume_id,
        dir: snapshot.dir,
        ready,
    })
}

fn bulk_row(dir: &Path, row: &PreviewRow, expected_fingerprint: SourceFingerprint) -> BulkRenameRow {
    BulkRenameRow {
        row_id: row.row.to_string(),
        source: dir.join(&row.old_name),
        destination: dir.join(&row.new_name),
        expected_fingerprint,
    }
}

/// Renames the rows the user saw as ready, if they're still exactly that.
/// Returns once the operation has started; progress, the queue, and Undo follow
/// it by its id.
pub(crate) async fn apply(
    events: Arc<dyn crate::file_system::write_operations::OperationEventSink>,
    listing_id: String,
    include_hidden: bool,
    rows: Option<Vec<usize>>,
    spec: MultiRenameSpec,
    expected: Vec<ExpectedRename>,
) -> Result<MultiRenameStarted, MultiRenameError> {
    // Off the IPC thread: the listing clone and a mask and regex per row.
    let prepared = blocking_typed_result_with_timeout(
        Duration::from_secs(5),
        || MultiRenameError::TimedOut,
        |detail| MultiRenameError::Internal { detail },
        move || prepare(&listing_id, include_hidden, rows.as_deref(), &spec, &expected),
    )
    .await?;

    // An archive or a `.git` portal serves its folder read-only.
    let resolved = crate::file_system::volume::manager::get_volume_manager()
        .resolve(&prepared.volume_id, &prepared.dir)
        .await;
    if resolved.routed.is_some() {
        return Err(MultiRenameError::ReadOnly);
    }

    let Prepared { volume_id, dir, ready } = prepared;
    let bulk: Vec<BulkRenameRow> = if volume_id == "root" {
        let dir_for_capture = dir.clone();
        blocking_typed_result_with_timeout(
            Duration::from_secs(5),
            || MultiRenameError::TimedOut,
            |detail| MultiRenameError::Internal { detail },
            move || {
                // A row whose file vanished since the preview is left out, not failed.
                Ok(ready
                    .iter()
                    .filter_map(|row| {
                        SourceFingerprint::capture_local(&dir_for_capture.join(&row.old_name))
                            .map(|fingerprint| bulk_row(&dir_for_capture, row, fingerprint))
                    })
                    .collect())
            },
        )
        .await?
    } else {
        let volume = resolved.volume.ok_or_else(|| MultiRenameError::NotConnected {
            volume_id: volume_id.clone(),
        })?;
        let mut bulk = Vec::with_capacity(ready.len());
        for row in &ready {
            if let Some(fingerprint) =
                SourceFingerprint::capture_remote(volume.as_ref(), &dir.join(&row.old_name)).await
            {
                bulk.push(bulk_row(&dir, row, fingerprint));
            }
        }
        bulk
    };
    if bulk.is_empty() {
        return Err(MultiRenameError::NothingToRename);
    }
    let renaming = bulk.len();
    let started = start_renames(events, volume_id, bulk, Initiator::User)
        .await
        .map_err(|reason| MultiRenameError::CouldntStart { reason })?;
    Ok(MultiRenameStarted {
        operation_id: started.operation.operation_id,
        renaming,
    })
}
