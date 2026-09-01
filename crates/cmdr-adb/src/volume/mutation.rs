//! The listing-cache patch each write leaves behind.
//!
//! There is no watcher on this backend, so `notify_mutation` is the ONLY thing
//! that keeps a pane honest after a write. ❗ One call per changed DIRECTORY,
//! ❌ never one per entry: the host walks every cached listing on the volume.

use std::path::Path;

use cmdr_fs::volume::{DirectoryChange, MutationEvent, Volume};

use super::AdbVolume;

impl AdbVolume {
    /// The one patch a create leaves behind. A path that doesn't translate
    /// skips it: the write already succeeded, and a patch must never fail it.
    pub(super) async fn notify_created(&self, path: &Path) {
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return;
        };
        let Some(parent_display) = self.display_path_for(parent) else {
            return;
        };
        self.notify_mutation(
            self.volume_id(),
            &parent_display,
            MutationEvent::Created(name.to_string_lossy().to_string()),
        )
        .await;
    }

    /// The same for a delete.
    pub(super) async fn notify_deleted(&self, path: &Path) {
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return;
        };
        let Some(parent_display) = self.display_path_for(parent) else {
            return;
        };
        self.notify_mutation(
            self.volume_id(),
            &parent_display,
            MutationEvent::Deleted(name.to_string_lossy().to_string()),
        )
        .await;
    }

    /// One `Renamed` when both ends share a parent, otherwise a `Deleted` at
    /// the source and a `Created` at the destination.
    pub(super) async fn notify_renamed(&self, from: &Path, to: &Path) {
        if from.parent() == to.parent() {
            let (Some(parent), Some(from_name), Some(to_name)) = (from.parent(), from.file_name(), to.file_name())
            else {
                return;
            };
            let Some(parent_display) = self.display_path_for(parent) else {
                return;
            };
            self.notify_mutation(
                self.volume_id(),
                &parent_display,
                MutationEvent::Renamed {
                    from: from_name.to_string_lossy().to_string(),
                    to: to_name.to_string_lossy().to_string(),
                },
            )
            .await;
            return;
        }
        self.notify_deleted(from).await;
        self.notify_created(to).await;
    }

    /// Patches the cached listing of ONE directory to match a mutation that
    /// has already landed on the device.
    pub(super) async fn notify_mutation_impl(&self, parent_path: &Path, mutation: MutationEvent) {
        let listings = self.inner.host.listings();
        let volume_id = self.volume_id();
        match mutation {
            MutationEvent::Created(ref name) | MutationEvent::Modified(ref name) => {
                // One stat, and a failure is simply no patch: the pane re-lists
                // eventually, and the mutation itself has already succeeded.
                let Ok(entry) = self.get_metadata_impl(&parent_path.join(name)).await else {
                    return;
                };
                let change = if matches!(mutation, MutationEvent::Created(_)) {
                    DirectoryChange::Added(entry)
                } else {
                    DirectoryChange::Modified(entry)
                };
                listings.directory_changed(volume_id, parent_path, change);
            }
            MutationEvent::Deleted(name) => {
                listings.directory_changed(volume_id, parent_path, DirectoryChange::Removed(name));
            }
            MutationEvent::Renamed { from, to } => {
                let Ok(entry) = self.get_metadata_impl(&parent_path.join(&to)).await else {
                    return;
                };
                listings.directory_changed(
                    volume_id,
                    parent_path,
                    DirectoryChange::Renamed {
                        old_name: from,
                        new_entry: entry,
                    },
                );
            }
        }
    }
}
