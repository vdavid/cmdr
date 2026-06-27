//! Tests for `transfer_driver.rs`'s `build_pre_skip_set`.
//!
//! Coverage: bulk-skip set population is filename-only matching, gated to the
//! `Skip` resolution, and never bulk-skips a directory-typed source (a merging
//! folder must fall through to per-child resolution, never be dropped wholesale).

use super::super::super::types::ConflictResolution;
use super::build_pre_skip_set;
use super::test_support::paths;
use std::collections::HashSet;
use std::path::PathBuf;

#[test]
fn build_pre_skip_set_empty_when_not_skip() {
    let sources = paths(&["/a.txt", "/b.txt"]);
    let empty_dirs = HashSet::new();
    for resolution in [
        ConflictResolution::Stop,
        ConflictResolution::Overwrite,
        ConflictResolution::Rename,
    ] {
        let set = build_pre_skip_set(&sources, resolution, &["a.txt".into()], &empty_dirs);
        assert!(
            set.is_empty(),
            "non-Skip resolution {resolution:?} must not populate pre-skip set"
        );
    }
}

#[test]
fn build_pre_skip_set_empty_when_pre_known_list_empty() {
    let sources = paths(&["/a.txt", "/b.txt"]);
    let empty_dirs = HashSet::new();
    let set = build_pre_skip_set(&sources, ConflictResolution::Skip, &[], &empty_dirs);
    assert!(set.is_empty());
}

#[test]
fn build_pre_skip_set_matches_by_filename_only() {
    // Pre-known list contains FILE NAMES (the FE only knows leaf names from
    // the conflict scan). The driver must match by `file_name()`, not full
    // path.
    let sources = paths(&["/photos/a.txt", "/docs/b.txt", "/docs/c.txt"]);
    let empty_dirs = HashSet::new();
    let set = build_pre_skip_set(
        &sources,
        ConflictResolution::Skip,
        &["a.txt".into(), "c.txt".into()],
        &empty_dirs,
    );
    assert_eq!(set.len(), 2);
    assert!(set.contains(&PathBuf::from("/photos/a.txt")));
    assert!(set.contains(&PathBuf::from("/docs/c.txt")));
    assert!(!set.contains(&PathBuf::from("/docs/b.txt")));
}

/// Directory-typed top-level sources are excluded from the bulk-skip set
/// even when their filenames match a pre-known conflict. Bulk-skip would
/// drop the whole subtree; for directories the right behavior is to fall
/// through to per-iter conflict resolution so the conflicting children get
/// skipped individually while the non-conflicting ones still copy.
#[test]
fn build_pre_skip_set_excludes_known_directory_paths() {
    let sources = paths(&["/photos/a.txt", "/docs", "/notes/c.txt"]);
    let mut known_dirs = HashSet::new();
    known_dirs.insert(PathBuf::from("/docs"));
    let set = build_pre_skip_set(
        &sources,
        ConflictResolution::Skip,
        &["a.txt".into(), "docs".into(), "c.txt".into()],
        &known_dirs,
    );
    assert_eq!(set.len(), 2);
    assert!(set.contains(&PathBuf::from("/photos/a.txt")));
    assert!(set.contains(&PathBuf::from("/notes/c.txt")));
    assert!(
        !set.contains(&PathBuf::from("/docs")),
        "known-directory path /docs must be excluded from bulk-skip"
    );
}

/// A dir-vs-dir collision (a source folder landing on a same-named dest folder)
/// must NEVER enter the file bulk-skip set, even under `Skip all`. Folders
/// always merge; "Skip all" governs the clashing FILES inside the merge, not
/// the folder wholesale. The upfront FE forwards the folder's name as a
/// pre-known conflict, but the preflight scan also reports it via
/// `known_directory_paths`, so it falls through to per-child resolution. This
/// pins that the merge-not-skip-wholesale semantics hold at the bulk-skip gate.
#[test]
fn build_pre_skip_set_never_bulk_skips_a_merging_directory() {
    let sources = paths(&["/photos", "/notes.txt"]);
    let mut known_dirs = HashSet::new();
    // `/photos` is a directory (a dir-dir merge at the destination).
    known_dirs.insert(PathBuf::from("/photos"));
    let set = build_pre_skip_set(
        &sources,
        ConflictResolution::Skip,
        // Both names arrive as pre-known conflicts from the FE.
        &["photos".into(), "notes.txt".into()],
        &known_dirs,
    );
    // Only the file is bulk-skipped; the merging folder is left to per-child
    // resolution so its non-clashing children still copy.
    assert_eq!(set.len(), 1);
    assert!(set.contains(&PathBuf::from("/notes.txt")));
    assert!(
        !set.contains(&PathBuf::from("/photos")),
        "a merging directory must never be bulk-skipped wholesale"
    );
}
