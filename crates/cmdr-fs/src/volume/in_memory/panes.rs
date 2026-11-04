//! Keeping the panes honest after an in-memory write, when asked to: the browser
//! demo shows its one in-memory volume in both panes, and nothing watches a
//! `HashMap`. With [`InMemoryVolume::with_pane_patches`] set, every create,
//! delete, rename, and streamed write patches the listings through the shared
//! patcher (`crate::volume::patching`), the way SFTP and S3 do; without it (every
//! test that only cares about the store) nothing is patched, as before.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::InMemoryVolume;
use crate::entry::FileEntry;
use crate::volume::host::listings::ListingHost;
use crate::volume::patching::{PatchSource, patch_created, patch_deleted, patch_mutation, patch_renamed};
use crate::volume::scan_walk::Walking;
use crate::volume::{MutationEvent, Volume};

/// The id the panes file this volume's listings under, and the panes.
pub(super) struct PanePatches {
    volume_id: String,
    listings: Arc<dyn ListingHost>,
}

/// One volume as the patcher sees it.
struct Patcher<'a> {
    volume: &'a InMemoryVolume,
    patches: &'a PanePatches,
}

impl PatchSource for Patcher<'_> {
    fn patch_volume_id(&self) -> &str {
        &self.patches.volume_id
    }

    fn patch_listings(&self) -> &dyn ListingHost {
        self.patches.listings.as_ref()
    }

    fn patch_stat<'a>(&'a self, path: &'a Path) -> Walking<'a, FileEntry> {
        self.volume.get_metadata(path)
    }

    fn patch_display_path(&self, path: &Path) -> Option<PathBuf> {
        Some(path.to_path_buf())
    }
}

impl InMemoryVolume {
    /// Patches `listings` (the panes, which file this volume under `volume_id`)
    /// after every write, so a pane showing a folder sees a copy land in it.
    pub fn with_pane_patches(mut self, volume_id: impl Into<String>, listings: Arc<dyn ListingHost>) -> Self {
        self.pane_patches = Some(PanePatches {
            volume_id: volume_id.into(),
            listings,
        });
        self
    }

    fn patcher(&self) -> Option<Patcher<'_>> {
        self.pane_patches
            .as_ref()
            .map(|patches| Patcher { volume: self, patches })
    }

    /// `path` appeared.
    pub(super) async fn patch_created_at(&self, path: &Path) {
        if let Some(patcher) = self.patcher() {
            patch_created(&patcher, path).await;
        }
    }

    /// `path` went away.
    pub(super) async fn patch_deleted_at(&self, path: &Path) {
        if let Some(patcher) = self.patcher() {
            patch_deleted(&patcher, path).await;
        }
    }

    /// `from` is now `to`.
    pub(super) async fn patch_renamed_at(&self, from: &Path, to: &Path) {
        if let Some(patcher) = self.patcher() {
            patch_renamed(&patcher, from, to).await;
        }
    }

    /// A change the engine reports itself (`Volume::notify_mutation`).
    pub(super) async fn patch_reported(&self, parent_path: &Path, mutation: MutationEvent) {
        if let Some(patcher) = self.patcher() {
            patch_mutation(&patcher, parent_path, mutation).await;
        }
    }
}
