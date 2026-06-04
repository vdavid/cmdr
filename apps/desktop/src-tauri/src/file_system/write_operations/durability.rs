//! Targeted durability: `fdatasync` per created destination so "complete"
//! means "durable on disk", not "buffered in the OS page cache". Also holds
//! the drive-index size lookup used to render directory sizes in conflict UI.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::state::WriteOperationState;
use super::types::OperationEventSink;

/// Emits a `Flushing`-phase progress event, then `fdatasync`s every freshly
/// created destination so "complete" means "durable on disk", not "buffered in
/// the OS page cache". Blocks until the flush finishes.
///
/// Reuses the operation's own list of created destinations (`created_files`
/// from `CopyTransaction`); no parallel path tracking. `already_synced` holds
/// the destinations the per-file copy strategy already made durable (chunked
/// copy's inline `sync_data`) or for which a flush is moot (APFS clonefile /
/// reflink — the data shares copy-on-write extents with the source, so there's
/// nothing of our own to flush). Those are skipped here, so a long chunked
/// batch keeps its "durable-as-each-file-completes" property without a
/// redundant second `fdatasync` at the end.
///
/// `fdatasync` (Rust `File::sync_data`) is the floor: file data + size become
/// durable; mtime may lag, which is fine. We also best-effort `fsync` each
/// distinct parent directory so the directory entry (the rename-into-place from
/// the temp+rename / staging paths) is durable too. Directory fsync failures
/// are logged and ignored — not every filesystem supports it, and the file
/// data is already safe by then.
///
/// All steps are best-effort on error: a failed `sync_data` is logged, not
/// propagated. The bytes are written either way; failing the whole operation
/// at the final flush would be worse UX than a logged warning, and the typed
/// error paths already covered the cases where the write itself failed.
#[allow(
    clippy::too_many_arguments,
    reason = "These are the natural operation-wide values a progress emit needs; bundling them into a struct adds ceremony without cleaning anything up, matching WriteProgressEvent::new."
)]
pub(super) fn flush_created_destinations(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: super::types::WriteOperationType,
    state: &Arc<WriteOperationState>,
    files_done: usize,
    files_total: usize,
    bytes_done: u64,
    bytes_total: u64,
    created_files: &[PathBuf],
    already_synced: &HashSet<PathBuf>,
) {
    use super::types::{WriteOperationPhase, WriteProgressEvent};

    // Announce the closing flush so the FE can show "Writing the last piece…"
    // instead of a bar frozen at 100% on slow media.
    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            operation_type,
            WriteOperationPhase::Flushing,
            None,
            files_done,
            files_total,
            bytes_done,
            bytes_total,
        ),
    );

    let mut synced_dirs: HashSet<PathBuf> = HashSet::new();
    for dest in created_files {
        if already_synced.contains(dest) {
            continue;
        }
        // Skip symlinks: opening one follows it, and a symlink carries no file
        // data of its own to flush. `symlink_metadata` doesn't follow.
        match fs::symlink_metadata(dest) {
            Ok(meta) if meta.file_type().is_symlink() => continue,
            Ok(_) => {}
            Err(e) => {
                log::warn!(
                    target: "write_durability",
                    "flush: couldn't stat {} before fdatasync: {e}",
                    dest.display()
                );
                continue;
            }
        }

        match fs::File::open(dest) {
            Ok(f) => {
                if let Err(e) = f.sync_data() {
                    log::warn!(
                        target: "write_durability",
                        "flush: fdatasync failed for {}: {e}",
                        dest.display()
                    );
                }
            }
            Err(e) => {
                log::warn!(
                    target: "write_durability",
                    "flush: couldn't open {} for fdatasync: {e}",
                    dest.display()
                );
            }
        }

        // Best-effort: fsync the parent directory so the directory entry is
        // durable too. De-duped across files in the same directory.
        if let Some(parent) = dest.parent()
            && synced_dirs.insert(parent.to_path_buf())
            && let Err(e) = fsync_dir(parent)
        {
            log::debug!(
                target: "write_durability",
                "flush: parent dir fsync skipped for {}: {e}",
                parent.display()
            );
        }
    }
}

/// Opens a directory and `fsync`s it so directory-entry changes (the final
/// rename-into-place) are durable. Best-effort; some filesystems reject this.
fn fsync_dir(dir: &Path) -> std::io::Result<()> {
    let f = fs::File::open(dir)?;
    f.sync_all()
}

/// Looks up a directory's recursive size from the drive index. Returns `None`
/// when the index doesn't cover the path (network mount, MTP, outside-scope
/// path, indexer not yet initialised). The BE intentionally never *walks*
/// the tree to compute this — `(unknown)` on the FE is the legitimate
/// fallback when the cached value isn't available.
pub(super) fn lookup_indexed_size(path: &Path) -> Option<u64> {
    crate::indexing::get_dir_stats(&path.to_string_lossy())
        .ok()
        .flatten()
        .map(|s| s.recursive_size)
}
