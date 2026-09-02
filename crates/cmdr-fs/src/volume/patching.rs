//! Keeping a pane honest after a mutation a watcher will never report.
//!
//! A backend with no change watcher (SFTP, WebDAV) has exactly one way to keep a
//! displayed listing in step with a write it just made: patch the listing cache
//! itself. ❗ That patch is the ONLY thing standing between a successful rename
//! and a pane still showing the old name, so the arithmetic below is worth
//! having in one place rather than one copy per protocol.
//!
//! ❗ **A patch is a courtesy and ❌ must never fail the mutation that earned
//! it.** Every function here returns `()`: a path that doesn't translate, or a
//! stat that doesn't answer, means no patch, and the pane re-lists eventually.
//! The write already landed, and reporting it as a failure would be a lie.

use std::path::{Path, PathBuf};

use crate::entry::FileEntry;
use crate::volume::host::listings::ListingHost;
use crate::volume::scan_walk::Walking;
use crate::volume::{DirectoryChange, MutationEvent};

/// What patching a listing needs from a backend.
pub trait PatchSource: Sync {
    /// The id every cached listing on this volume is filed under.
    fn patch_volume_id(&self) -> &str;

    /// The listing cache to patch.
    fn patch_listings(&self) -> &dyn ListingHost;

    /// One entry, freshly read. ❗ The backend's own stat, so the patch carries
    /// what the server actually holds rather than what the caller hoped.
    fn patch_stat<'a>(&'a self, path: &'a Path) -> Walking<'a, FileEntry>;

    /// The path the APP addresses `path` by, or `None` when `path` isn't on this
    /// volume. A refusal here is "no patch to make", never an error.
    fn patch_display_path(&self, path: &Path) -> Option<PathBuf>;
}

/// Patches the listing that held `path`'s parent, for one change.
pub async fn patch_mutation(source: &dyn PatchSource, parent_path: &Path, mutation: MutationEvent) {
    let listings = source.patch_listings();
    let volume_id = source.patch_volume_id();
    match mutation {
        MutationEvent::Created(ref name) | MutationEvent::Modified(ref name) => {
            // One stat, and a failure is simply no patch: the pane re-lists
            // eventually, and the mutation itself has already succeeded.
            let Ok(entry) = source.patch_stat(&parent_path.join(name)).await else {
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
            let Ok(entry) = source.patch_stat(&parent_path.join(&to)).await else {
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

/// Patches for a `path` that has just appeared.
pub async fn patch_created(source: &dyn PatchSource, path: &Path) {
    let Some((parent, name)) = parent_and_name(source, path) else {
        return;
    };
    patch_mutation(source, &parent, MutationEvent::Created(name)).await;
}

/// Patches for a `path` that has just gone.
pub async fn patch_deleted(source: &dyn PatchSource, path: &Path) {
    let Some((parent, name)) = parent_and_name(source, path) else {
        return;
    };
    patch_mutation(source, &parent, MutationEvent::Deleted(name)).await;
}

/// Patches for a rename, which is ❗ two changes when it crosses directories: a
/// pane showing the source has lost an entry and one showing the destination has
/// gained one, and a single `Renamed` would leave whichever it wasn't sent to
/// stale.
pub async fn patch_renamed(source: &dyn PatchSource, from: &Path, to: &Path) {
    let (Some((from_parent, from_name)), Some((to_parent, to_name))) =
        (parent_and_name(source, from), parent_and_name(source, to))
    else {
        return;
    };
    if from_parent == to_parent {
        patch_mutation(
            source,
            &from_parent,
            MutationEvent::Renamed {
                from: from_name,
                to: to_name,
            },
        )
        .await;
    } else {
        patch_mutation(source, &from_parent, MutationEvent::Deleted(from_name)).await;
        patch_mutation(source, &to_parent, MutationEvent::Created(to_name)).await;
    }
}

/// `path`'s parent as the app addresses it, plus `path`'s own name. `None` when
/// either is missing, which is the "nothing to patch" answer.
fn parent_and_name(source: &dyn PatchSource, path: &Path) -> Option<(PathBuf, String)> {
    let parent = source.patch_display_path(path.parent()?)?;
    Some((parent, path.file_name()?.to_string_lossy().into_owned()))
}

#[cfg(test)]
#[path = "patching_test.rs"]
mod patching_test;
