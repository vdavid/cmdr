//! Landing a fully-written cross-volume temp at its final name.
//!
//! Both halves are one act: [`temp_sibling_path`] mints the `.cmdr-tmp-<uuid>`
//! sibling a staged write goes to, and [`finalize_safe_replace`] swaps it over
//! whatever is at the destination once the last byte is in. Kept out of
//! `conflict.rs` because the resolver only DECIDES that a replace should happen;
//! the five write sites (`copy_serial.rs`, `copy_concurrent_task.rs`,
//! `merge.rs`, `sequential_extract.rs`, `move_cross.rs`) are what call this,
//! after their stream succeeds and the resolver is long done.
//!
//! ❗ On a failure here the new bytes are committed data, not a partial. The
//! caller contract is on `finalize_safe_replace`, and the rescue that keeps them
//! out of reach of the hourly reap is `../recovered_name.rs`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::recovered_name::{FinalizeFailure, rescue_out_of_temp_space};
use crate::file_system::volume::{Volume, VolumeError};

/// Builds a temp sibling path next to `dest_path` for a staged write.
///
/// Uses the recognizable `.cmdr-tmp-<uuid>` marker (matches the project's temp
/// convention, so a leftover after a crash is identifiable and cleanup helpers
/// recognize it). The temp lives in the same parent directory as the original
/// so the finalize step's `rename` stays within one directory (no cross-dir
/// rename, which some backends refuse).
///
/// Shared with `staged_write.rs`, which stages EVERY cross-volume file write on
/// one of these, not only the conflict-driven safe-replace.
pub(super) fn temp_sibling_path(dest_path: &Path) -> PathBuf {
    let parent = dest_path.parent().unwrap_or(Path::new(""));
    let filename = dest_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    parent.join(format!("{filename}.cmdr-tmp-{}", uuid::Uuid::new_v4()))
}

/// Lands a fully-written temp at its final name: deletes whatever is at `orig`
/// (which survived the entire streaming write) and renames the temp into its
/// place.
///
/// Two callers, one shape: the conflict layer's file→file safe-replace, and
/// `staged_write.rs`'s landing of an ordinary staged write (where `orig` usually
/// doesn't exist yet and the delete is a tolerated `NotFound`).
///
/// Order matters and is the whole point of safe-replace: the temp holds the
/// COMPLETE new data the moment this is called, and `orig` still holds the
/// complete old data. We delete `orig` first, then `rename(temp, orig, false)`
/// into the now-absent slot. We do NOT use `rename(force=true)` to replace:
/// MTP's `rename(force=true)` does NOT delete an existing destination (it can
/// create a duplicate), so an explicit delete-then-rename is the only shape
/// that's correct and uniform across Local / SMB / MTP / InMemory.
///
/// There is a tiny window between the delete and the rename where neither name
/// resolves to a file on disk — but the complete new data lives in `temp`
/// throughout, so a crash in that window leaves a recoverable `.cmdr-tmp-*`
/// sibling rather than data loss. We tolerate `NotFound` on the delete (the
/// original may have vanished out from under us). If the delete fails for any
/// other reason we return the error WITHOUT deleting the temp — the new data
/// must survive so the user (or a retry) can recover it.
///
/// CALLER CONTRACT: when this returns `Err` the new data is somewhere the caller
/// must NOT clean up, and [`FinalizeFailure::new_data_at`] says where. If the
/// DELETE failed, nothing moved: the destination still holds the user's file and
/// the temp is an ordinary partial. If the delete SUCCEEDED and the rename
/// failed, the temp holds the only complete copy of the new data and the
/// original is gone, so this rescues it out of temp space (see
/// [`rescue_out_of_temp_space`]) and reports where it went. The write sites
/// enforce the no-cleanup half by stopping their partial-cleanup tracking from
/// designating the temp the moment the streaming write succeeded, before this
/// function runs. See `transfer/CLAUDE.md` § "The post-write temp is committed
/// data" and the `*_preserves_new_data_on_finalize_failure` tests.
pub(super) async fn finalize_safe_replace(
    dest_volume: &Arc<dyn Volume>,
    temp: &Path,
    orig: &Path,
) -> Result<(), FinalizeFailure> {
    match dest_volume.delete(orig).await {
        Ok(()) => {}
        Err(VolumeError::NotFound(_)) => {
            // Already gone; the rename below will land the new data anyway.
        }
        Err(e) => {
            log::warn!(
                "finalize_safe_replace: couldn't delete the original {} before the rename, so the destination still holds it and the temp {} is an ordinary partial: {}",
                orig.display(),
                temp.display(),
                e
            );
            return Err(FinalizeFailure {
                error: e,
                new_data_at: None,
            });
        }
    }
    match dest_volume.rename(temp, orig, false).await {
        Ok(()) => Ok(()),
        Err(error) => {
            let new_data_at = rescue_out_of_temp_space(dest_volume, temp, orig).await;
            log::warn!(
                "finalize_safe_replace: the original {} is gone and the new data couldn't take its name, so it is at {} now: {}",
                orig.display(),
                new_data_at.display(),
                error
            );
            Err(FinalizeFailure {
                error,
                new_data_at: Some(new_data_at),
            })
        }
    }
}
