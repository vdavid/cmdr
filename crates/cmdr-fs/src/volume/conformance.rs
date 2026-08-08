//! Shared conformance assertions every `Volume` implementation runs.
//!
//! A trait contract no test enforces is a comment. The promises in
//! [`Volume`](crate::volume::Volume)'s doc comments are load-bearing for data safety,
//! and a backend that talks to a device rather than to a filesystem can stop
//! honoring one with no compile error and no visible symptom — right up until a
//! user loses a file.
//!
//! Each assertion takes an already-seeded fixture, because seeding is the one
//! part that can't be shared: a local volume needs a temp dir, MTP needs a
//! backing dir plus a rescan, SMB needs a share. What the assertion checks is
//! identical everywhere, which is the point.

use std::path::Path;

use super::Volume;

/// [`Volume::delete`](crate::volume::Volume::delete) handles ONE node, so a directory
/// that still holds anything is refused and left completely intact.
///
/// `dir` must already exist on `volume` and hold `child_name` directly inside
/// it. The assertion checks that precondition first, so a fixture that seeded
/// nothing can't pass by accident.
///
/// **Why this one is worth a shared assertion.** Real data-safety logic leans on
/// the refusal rather than on a check of its own: the same-volume move's
/// inside-out source cleanup keeps a skipped child's only copy purely by letting
/// the parent's delete fail, and rollback's created-dirs prune leaves a
/// directory standing for the same reason. A backend that quietly recurses turns
/// both of those into deletions of data the user asked to keep.
pub async fn assert_delete_leaves_a_non_empty_dir_intact(volume: &dyn Volume, dir: &Path, child_name: &str) {
    let before = volume
        .list_directory(dir, None)
        .await
        .unwrap_or_else(|e| panic!("fixture precondition: listing {} must work, got {e:?}", dir.display()));
    assert!(
        before.iter().any(|e| e.name == child_name),
        "fixture precondition: {} must hold {child_name}, found {:?}",
        dir.display(),
        before.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    let outcome = volume.delete(dir).await;
    assert!(
        outcome.is_err(),
        "delete of the non-empty directory {} must refuse; it returned Ok, so it recursed",
        dir.display()
    );

    let after = volume.list_directory(dir, None).await.unwrap_or_else(|e| {
        panic!(
            "the refused directory {} must still be listable, got {e:?}",
            dir.display()
        )
    });
    assert!(
        after.iter().any(|e| e.name == child_name),
        "a refused delete must destroy nothing, but {child_name} is gone from {}; found {:?}",
        dir.display(),
        after.iter().map(|e| &e.name).collect::<Vec<_>>()
    );
}
