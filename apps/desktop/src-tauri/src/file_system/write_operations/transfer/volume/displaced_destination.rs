//! The destination a same-volume Overwrite is replacing, held aside until the
//! rename that replaces it has landed.
//!
//! The cross-volume side's twin is `conflict.rs::finalize_safe_replace` (a temp
//! holding the NEW bytes); the local-FS side's is
//! `write_operations/overwrite.rs::DisplacedEntry`. This is the one for a move
//! that replaces by RENAMING, where the new bytes need no temp at all and the
//! only thing at risk is the file being replaced.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::super::super::types::WriteOperationError;
use super::naming::rescue_out_of_temp_space;
use super::transfer_error::{PathRole, map_volume_error};
use crate::file_system::staging::StagingTemp;
use crate::file_system::volume::{Volume, VolumeError};

/// The destination a same-volume Overwrite is about to replace, renamed out of
/// the way rather than deleted.
///
/// A same-volume move replaces by RENAMING the source onto the name, and a
/// rename can't clobber (`force = false`, and MTP's `force = true` doesn't
/// delete an existing dest either), so the name has to be free first. Deleting
/// to free it is what makes the gap fatal: the replacing rename is a separate
/// call, and an SMB `STATUS_SHARING_VIOLATION`, an MTP `MoveObject` refusal, or
/// a session blip fails it after the delete has already succeeded — the
/// destination gone, the source not moved. One rename aside costs one call on
/// every backend and turns that into a restore.
///
/// The guard rides along so the aside stays hidden from the pane for exactly as
/// long as it is on disk. ❗ It wears the `.cmdr-temp-` marker, which
/// `cleanup.rs::reap_stale_transfer_temps` does NOT match (it reaps `.cmdr-tmp-`
/// only), so an aside nothing could put back survives for the user to find.
pub(super) struct DisplacedDestination {
    aside: StagingTemp,
    original: PathBuf,
}

/// Renames whatever is at `original` to a `.cmdr-temp-<uuid>` sibling, so the
/// name is free for the rename that replaces it.
///
/// `Ok(None)` ⇒ nothing was there, so there is nothing to put back.
pub(super) async fn displace_destination(
    volume: &Arc<dyn Volume>,
    original: &Path,
    owner: Option<std::sync::Weak<()>>,
) -> Result<Option<DisplacedDestination>, WriteOperationError> {
    let aside = StagingTemp::mint_aside(original, uuid::Uuid::new_v4(), owner);
    match volume.rename(original, aside.path(), false).await {
        Ok(()) => Ok(Some(DisplacedDestination {
            aside,
            original: original.to_path_buf(),
        })),
        Err(VolumeError::NotFound(_)) => Ok(None),
        Err(e) => Err(map_volume_error(
            &original.display().to_string(),
            PathRole::Destination,
            e,
        )),
    }
}

impl DisplacedDestination {
    /// The replacement landed, so the file it replaced goes. Best-effort: a
    /// leftover wears the recognizable `.cmdr-temp-<uuid>` name and becomes
    /// visible in the pane once the operation ends.
    pub(super) async fn discard(self, volume: &Arc<dyn Volume>) {
        if let Err(e) = volume.delete(self.aside.path()).await {
            log::warn!(
                target: "copy",
                "couldn't remove the displaced destination at {}: {e}",
                self.aside.path().display()
            );
        }
    }

    /// The replacement never landed, so the user's file comes home.
    ///
    /// `None` when it did. `Some(kept_at)` when even THAT rename refused: the
    /// bytes then wear a ` (recovered)` name beside where they belong (or, if
    /// nothing landed at all, still the aside's), and the caller has to name
    /// that path in the failure the user reads, because it is the only place
    /// their file is.
    pub(super) async fn restore(self, volume: &Arc<dyn Volume>) -> Option<PathBuf> {
        match volume.rename(self.aside.path(), &self.original, false).await {
            Ok(()) => None,
            Err(e) => {
                log::warn!(
                    target: "copy",
                    "couldn't put {} back at {}: {e}",
                    self.aside.path().display(),
                    self.original.display()
                );
                Some(rescue_out_of_temp_space(volume, self.aside.path(), &self.original).await)
            }
        }
    }
}
