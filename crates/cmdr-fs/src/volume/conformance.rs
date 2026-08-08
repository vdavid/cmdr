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

use super::{DirectoryCreation, Volume, VolumeError};

/// The size `path` reports right now, for a fixture precondition or an
/// after-the-fact "nothing was overwritten" check.
///
/// Size travels on every backend's `FileEntry`, which is what makes it the
/// portable way to ask "are these still the original bytes?" — reading content
/// back would need `open_read_stream`, and not every mutable backend exports.
async fn size_of(volume: &dyn Volume, path: &Path, what: &str) -> Option<u64> {
    volume
        .get_metadata(path)
        .await
        .unwrap_or_else(|e| panic!("{what}: {} must be stattable, got {e:?}", path.display()))
        .size
}

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

/// [`Volume::rename`](crate::volume::Volume::rename) with `force == false` refuses
/// a destination that already exists, and takes nothing away in the process.
///
/// `from` and `to` must both already exist on `volume` and must differ in size,
/// so a backend that overwrote `to` can't slip past the size check. The
/// assertion verifies both preconditions first.
///
/// **Why this one is worth a shared assertion.** `force` is the ONLY thing
/// standing between a move and the destination file it would replace: every
/// caller that hasn't yet asked the user passes `false`, and reads the
/// `AlreadyExists` back as "stop, there's something here" — the conflict dialog,
/// the same-volume move, the New Folder / rename commands. A backend that
/// silently overwrote instead would turn every one of those prompts into a
/// destroyed file, with no error anywhere to notice. Each backend earns the
/// refusal a different way (`renamex_np(RENAME_EXCL)`, an SMB `stat` plus the
/// server's `ReplaceIfExists == false`, an MTP `exists` probe, a map lookup), so
/// there's no shared mechanism to trust — only a shared promise.
pub async fn assert_rename_refuses_an_existing_destination(volume: &dyn Volume, from: &Path, to: &Path) {
    let from_size = size_of(volume, from, "fixture precondition").await;
    let to_size = size_of(volume, to, "fixture precondition").await;
    assert!(
        from_size != to_size,
        "fixture precondition: {} and {} must differ in size so an overwrite is visible; both report {from_size:?}",
        from.display(),
        to.display(),
    );

    let outcome = volume.rename(from, to, false).await;
    assert!(
        matches!(outcome, Err(VolumeError::AlreadyExists(_))),
        "rename({}, {}, force = false) must refuse with AlreadyExists; got {outcome:?}",
        from.display(),
        to.display(),
    );

    assert!(
        volume.exists(from).await,
        "a refused rename must leave the source in place, but {} is gone",
        from.display(),
    );
    let to_size_after = size_of(volume, to, "after the refused rename").await;
    assert_eq!(
        to_size_after,
        to_size,
        "a refused rename must not touch the destination, but {} went from {to_size:?} to {to_size_after:?} bytes",
        to.display(),
    );
}

/// [`Volume::create_file`](crate::volume::Volume::create_file) refuses a path that
/// already exists, rather than truncating what's there.
///
/// `path` must already exist on `volume` and hold a different number of bytes
/// than `content`, so a backend that clobbered it can't pass the size check.
///
/// **Why this one is worth a shared assertion.** The New File command hands a
/// user-typed name straight to `create_file`, and the IPC layer above it reads
/// `AlreadyExists` as "that name is taken" and says so. Nothing between the
/// keystroke and the backend re-checks: a `create_file` that truncated on
/// collision would silently empty a file the user only meant to name, and the
/// command would report success. `std::fs::write` and a plain SMB `FileCreate`
/// disposition differ on exactly this point, which is how a backend gets it
/// wrong without anybody choosing to.
pub async fn assert_create_file_refuses_to_clobber(volume: &dyn Volume, path: &Path, content: &[u8]) {
    let size_before = size_of(volume, path, "fixture precondition").await;
    assert!(
        size_before != Some(content.len() as u64),
        "fixture precondition: {} must differ in length from the clobbering content so an overwrite is visible; both are {size_before:?} bytes",
        path.display(),
    );

    let outcome = volume.create_file(path, content).await;
    assert!(
        matches!(outcome, Err(VolumeError::AlreadyExists(_))),
        "create_file over the existing {} must refuse with AlreadyExists; got {outcome:?}",
        path.display(),
    );

    let size_after = size_of(volume, path, "after the refused create_file").await;
    assert_eq!(
        size_after,
        size_before,
        "a refused create_file must not touch the file, but {} went from {size_before:?} to {size_after:?} bytes",
        path.display(),
    );
}

/// [`Volume::create_directory_all`](crate::volume::Volume::create_directory_all)
/// reports a directory that was ALREADY there as
/// [`DirectoryCreation::AlreadyExisted`], never as `Created`.
///
/// `dir` must already exist on `volume`; the assertion checks that first.
///
/// **Why this one is worth a shared assertion.** `Created` is a promise that the
/// directory was empty at that instant, and the transfer driver spends it: on a
/// `Created` answer it skips the per-file destination conflict probe for
/// everything it then writes inside. So a backend that answered `Created` for a
/// directory it merely found turns "would have prompted" into "overwrote", for
/// every file in the copy. Only the dangerous direction is pinned here — a
/// backend that answers `AlreadyExisted` when it did create the leaf is merely
/// slower, which is why the trait says "when in doubt, answer `AlreadyExisted`".
pub async fn assert_create_directory_all_reports_an_existing_dir_honestly(volume: &dyn Volume, dir: &Path) {
    assert!(
        volume.exists(dir).await,
        "fixture precondition: {} must already exist",
        dir.display()
    );

    let outcome = volume.create_directory_all(dir).await;
    assert!(
        matches!(outcome, Ok(DirectoryCreation::AlreadyExisted)),
        "create_directory_all over the existing {} must answer AlreadyExisted; \
         a Created answer tells the transfer driver it may skip every destination conflict probe inside. Got {outcome:?}",
        dir.display(),
    );
}
