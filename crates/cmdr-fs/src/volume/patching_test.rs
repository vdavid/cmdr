//! The patch arithmetic, against a recording listing host.
//!
//! ❗ A wrong patch here is a pane showing a file that no longer exists, or
//! missing one that does, after an operation the user watched succeed.

use std::collections::HashMap;
use std::sync::Arc;

use super::*;
use crate::volume::VolumeError;
use crate::volume::host::listings::RecordingListings;

/// A backend whose paths are already the app's, with a fixed set of entries.
struct FakeBackend {
    listings: Arc<RecordingListings>,
    entries: HashMap<String, bool>,
    /// Paths this backend refuses to translate, standing in for "not on this
    /// volume".
    off_volume: Vec<String>,
}

impl FakeBackend {
    fn new(entries: &[(&str, bool)]) -> Self {
        Self {
            listings: Arc::new(RecordingListings::default()),
            entries: entries.iter().map(|(p, d)| ((*p).to_string(), *d)).collect(),
            off_volume: Vec::new(),
        }
    }

    fn refusing(mut self, path: &str) -> Self {
        self.off_volume.push(path.to_string());
        self
    }

    fn changes(&self) -> Vec<(String, PathBuf, DirectoryChange)> {
        self.listings.changes()
    }
}

impl PatchSource for FakeBackend {
    fn patch_volume_id(&self) -> &str {
        "test-volume"
    }

    fn patch_listings(&self) -> &dyn ListingHost {
        self.listings.as_ref()
    }

    fn patch_stat<'a>(&'a self, path: &'a Path) -> Walking<'a, FileEntry> {
        Box::pin(async move {
            let key = path.to_string_lossy().into_owned();
            match self.entries.get(&key) {
                Some(is_dir) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    Ok(FileEntry::new(name, key, *is_dir, false))
                }
                None => Err(VolumeError::NotFound(key)),
            }
        })
    }

    fn patch_display_path(&self, path: &Path) -> Option<PathBuf> {
        let key = path.to_string_lossy().into_owned();
        (!self.off_volume.contains(&key)).then(|| path.to_path_buf())
    }
}

#[tokio::test]
async fn a_new_file_is_added_to_its_parents_listing() {
    let backend = FakeBackend::new(&[("/dir/new.txt", false)]);

    patch_created(&backend, Path::new("/dir/new.txt")).await;

    let changes = backend.changes();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].0, "test-volume");
    assert_eq!(changes[0].1, PathBuf::from("/dir"));
    assert!(matches!(&changes[0].2, DirectoryChange::Added(entry) if entry.name == "new.txt"));
}

#[tokio::test]
async fn a_deleted_file_needs_no_stat_to_leave_its_listing() {
    // ❗ Deliberately not in `entries`: the thing is gone, so a stat that had to
    // succeed would mean a delete could never be patched at all.
    let backend = FakeBackend::new(&[]);

    patch_deleted(&backend, Path::new("/dir/gone.txt")).await;

    let changes = backend.changes();
    assert_eq!(changes.len(), 1);
    assert!(matches!(&changes[0].2, DirectoryChange::Removed(name) if name == "gone.txt"));
}

#[tokio::test]
async fn a_rename_inside_one_directory_is_a_single_renamed_change() {
    let backend = FakeBackend::new(&[("/dir/after.txt", false)]);

    patch_renamed(&backend, Path::new("/dir/before.txt"), Path::new("/dir/after.txt")).await;

    let changes = backend.changes();
    assert_eq!(changes.len(), 1, "one listing moved, so one change");
    assert!(matches!(
        &changes[0].2,
        DirectoryChange::Renamed { old_name, new_entry } if old_name == "before.txt" && new_entry.name == "after.txt"
    ));
}

#[tokio::test]
async fn a_rename_across_directories_is_a_loss_here_and_a_gain_there() {
    // ❗ A single `Renamed` would leave whichever pane it wasn't sent to stale.
    let backend = FakeBackend::new(&[("/to/moved.txt", false)]);

    patch_renamed(&backend, Path::new("/from/moved.txt"), Path::new("/to/moved.txt")).await;

    let changes = backend.changes();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0].1, PathBuf::from("/from"));
    assert!(matches!(&changes[0].2, DirectoryChange::Removed(name) if name == "moved.txt"));
    assert_eq!(changes[1].1, PathBuf::from("/to"));
    assert!(matches!(&changes[1].2, DirectoryChange::Added(entry) if entry.name == "moved.txt"));
}

#[tokio::test]
async fn a_stat_that_does_not_answer_is_no_patch_rather_than_a_failure() {
    // ❗ The mutation already landed. A patch is a courtesy, and this function
    // has no way to report a problem even if it wanted to.
    let backend = FakeBackend::new(&[]);

    patch_created(&backend, Path::new("/dir/never-stats.txt")).await;

    assert!(backend.changes().is_empty());
}

#[tokio::test]
async fn a_parent_that_is_not_on_this_volume_is_no_patch() {
    let backend = FakeBackend::new(&[("/elsewhere/f.txt", false)]).refusing("/elsewhere");

    patch_created(&backend, Path::new("/elsewhere/f.txt")).await;

    assert!(backend.changes().is_empty());
}

#[tokio::test]
async fn a_path_with_no_parent_is_no_patch() {
    let backend = FakeBackend::new(&[("/", true)]);

    patch_created(&backend, Path::new("/")).await;

    assert!(backend.changes().is_empty());
}
