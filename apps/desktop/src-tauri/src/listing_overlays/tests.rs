//! The seam itself: registration, the shadowing merge rule, and the
//! zero-contributor fast path. What the GIT contributor decides is pinned
//! beside it in `file_system/git/overlay_tests.rs`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::*;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::{LocalPosixVolume, Volume};

/// An overlay that contributes one row per name it was built with, for
/// whatever directory it was pointed at.
struct FixedRows {
    id: &'static str,
    at: PathBuf,
    names: Vec<&'static str>,
}

impl ListingOverlay for FixedRows {
    fn id(&self) -> &'static str {
        self.id
    }

    fn applies_to(&self, _volume: &dyn Volume, path: &Path) -> bool {
        path == self.at
    }

    fn extra_entries(&self, _volume: &dyn Volume, path: &Path) -> Vec<FileEntry> {
        self.names
            .iter()
            .map(|name| {
                FileEntry::new(
                    (*name).to_string(),
                    path.join(name).to_string_lossy().into_owned(),
                    true,
                    false,
                )
            })
            .collect()
    }
}

fn real_row(name: &str) -> FileEntry {
    FileEntry::new(name.to_string(), format!("/anywhere/{name}"), false, false)
}

fn a_volume() -> Arc<dyn Volume> {
    Arc::new(LocalPosixVolume::new("Test", Path::new("/"))) as Arc<dyn Volume>
}

/// `decorate` folds a contributor's rows in and reports how many it added.
#[tokio::test]
async fn a_contributor_adds_its_rows_to_the_listing() {
    let at = PathBuf::from("/overlay/adds");
    register_listing_overlay(Arc::new(FixedRows {
        id: "test-adds",
        at: at.clone(),
        names: vec!["branches", "tags"],
    }));

    let mut entries = vec![real_row("HEAD"), real_row("config")];
    let added = decorate(&a_volume(), &at, &mut entries).await;

    assert_eq!(added, 2);
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"HEAD") && names.contains(&"config"), "{names:?}");
    assert!(names.contains(&"branches") && names.contains(&"tags"), "{names:?}");
}

/// The merge rule: a contributed row SHADOWS a real one of the same name, so
/// the listing never carries two rows called `branches`.
#[tokio::test]
async fn a_contributed_row_shadows_a_real_one_of_the_same_name() {
    let at = PathBuf::from("/overlay/shadows");
    register_listing_overlay(Arc::new(FixedRows {
        id: "test-shadows",
        at: at.clone(),
        names: vec!["branches"],
    }));

    // A repo made by an older git carries a real, deprecated `.git/branches/`.
    let mut entries = vec![real_row("branches"), real_row("HEAD")];
    let added = decorate(&a_volume(), &at, &mut entries).await;

    assert_eq!(added, 1);
    let branches: Vec<&FileEntry> = entries.iter().filter(|e| e.name == "branches").collect();
    assert_eq!(branches.len(), 1, "exactly one row called branches: {entries:?}");
    assert!(branches[0].is_directory, "the contributed row is the one that survived");
    assert_eq!(entries.len(), 2, "the unrelated real row is untouched");
}

/// A directory no contributor claims comes back exactly as the volume read it,
/// which is every listing in the app but a handful.
#[tokio::test]
async fn a_directory_no_contributor_claims_is_left_alone() {
    let mut entries = vec![real_row("a.txt"), real_row("b.txt")];
    let added = decorate(&a_volume(), Path::new("/overlay/untouched"), &mut entries).await;

    assert_eq!(added, 0);
    assert_eq!(entries.len(), 2);
}

/// Registering the same id twice keeps the first, so a double setup can't
/// contribute one set of rows two times.
#[tokio::test]
async fn a_contributor_registered_twice_only_counts_once() {
    let at = PathBuf::from("/overlay/twice");
    for _ in 0..2 {
        register_listing_overlay(Arc::new(FixedRows {
            id: "test-twice",
            at: at.clone(),
            names: vec!["branches"],
        }));
    }

    let mut entries = Vec::new();
    assert_eq!(decorate(&a_volume(), &at, &mut entries).await, 1);
    assert_eq!(entries.len(), 1);
}
