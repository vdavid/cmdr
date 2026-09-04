//! What the shared listing-cache patcher needs from this backend.
//!
//! ❗ There is no watcher here, so a patch is the ONLY thing that keeps a pane
//! honest after a write. ❗ One call per changed DIRECTORY, ❌ never one per
//! entry: the host walks every cached listing on the volume. The rules and the
//! created / deleted / renamed shapes: `cmdr_fs::volume::patching`.

use std::path::{Path, PathBuf};

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::host::listings::ListingHost;
use cmdr_fs::volume::patching::{PatchSource, patch_created, patch_deleted, patch_renamed};
use cmdr_fs::volume::scan_walk::Walking;

use super::AdbVolume;

impl PatchSource for AdbVolume {
    fn patch_volume_id(&self) -> &str {
        self.volume_id()
    }

    fn patch_listings(&self) -> &dyn ListingHost {
        self.inner.host.listings()
    }

    fn patch_stat<'a>(&'a self, path: &'a Path) -> Walking<'a, FileEntry> {
        Box::pin(self.get_metadata_impl(path))
    }

    fn patch_display_path(&self, path: &Path) -> Option<PathBuf> {
        self.display_path_for(path)
    }
}

impl AdbVolume {
    /// The one patch a create leaves behind.
    pub(super) async fn notify_created(&self, path: &Path) {
        patch_created(self, path).await;
    }

    /// The same for a delete.
    pub(super) async fn notify_deleted(&self, path: &Path) {
        patch_deleted(self, path).await;
    }

    /// One `Renamed` when both ends share a parent, otherwise a `Deleted` at the
    /// source and a `Created` at the destination.
    pub(super) async fn notify_renamed(&self, from: &Path, to: &Path) {
        patch_renamed(self, from, to).await;
    }
}
