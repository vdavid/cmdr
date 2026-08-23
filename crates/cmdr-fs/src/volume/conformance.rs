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

/// [`Volume::delete`] handles ONE node, so a directory
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

/// [`Volume::rename`] with `force == false` refuses
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

/// [`Volume::create_file`] refuses a path that
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

/// [`Volume::create_directory_all`]
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

/// [`Volume::is_writable`] answers for the mutations the backend actually
/// offers, in whichever direction it claims.
///
/// `scratch_dir` must NOT exist on `volume`; the assertion creates it (or
/// watches the create be refused) and cleans up after itself.
///
/// **Why this one is worth a shared assertion.** `is_writable` is the only
/// capability predicate whose answer leaves the backend and reaches the user as
/// UI state: it decides whether New folder, New file, Rename, and Paste render
/// enabled. Every other mutation contract here is enforced by what a method
/// DOES; this one is a declaration, so nothing but a test stops it drifting from
/// the methods it speaks for. A stale `true` is an enabled button that can't
/// work; a stale `false` is a working volume the user can't write to.
pub async fn assert_writability_matches_the_mutations_offered(volume: &dyn Volume, scratch_dir: &Path) {
    assert!(
        !volume.exists(scratch_dir).await,
        "fixture precondition: {} must not exist yet",
        scratch_dir.display()
    );

    let outcome = volume.create_directory(scratch_dir).await;
    if volume.is_writable() {
        assert!(
            outcome.is_ok(),
            "{} answers is_writable() == true, so creating {} must work. Got {outcome:?}",
            volume.name(),
            scratch_dir.display(),
        );
        // Leave the volume as we found it, so a caller can reuse the fixture.
        let _ = volume.delete(scratch_dir).await;
    } else {
        assert!(
            matches!(outcome, Err(VolumeError::NotSupported)),
            "{} answers is_writable() == false, so creating {} must refuse with NotSupported. Got {outcome:?}",
            volume.name(),
            scratch_dir.display(),
        );
    }
}

/// [`Volume::supports_export`] answers for the bytes the backend actually hands
/// out, in whichever direction it claims — and
/// [`Volume::supports_streaming`] agrees with it.
///
/// `path` must already exist on `volume` and hold exactly `content`, so a
/// backend that claims export but streams the wrong bytes can't pass.
///
/// **Why this one is worth a shared assertion.** `supports_export` is the second
/// capability predicate (with `is_writable`) whose answer leaves the backend and
/// reaches the user as UI state, and the ONLY thing standing between a copy and
/// the bytes it would move: `copy_between_volumes` rejects a source that answers
/// `false` before it reads anything, and the same `false` greys out copy-from in
/// the pane. Nothing else in the trait notices, because every method the copy
/// engine would call is implemented and works — the declaration is simply
/// missing. That is exactly how a backend ships fully able to stream its bytes
/// and completely unable to be copied from, with no failing method, no
/// classification error, and no log line anywhere to find it by.
///
/// The trait default is `false`, so the failure mode is silence: a new backend
/// that implements `open_read_stream` and forgets this one predicate is refused
/// at the guard with a message about export that names nothing it did wrong.
pub async fn assert_export_matches_the_bytes_offered(volume: &dyn Volume, path: &Path, content: &[u8]) {
    let size_before = size_of(volume, path, "fixture precondition").await;
    assert_eq!(
        size_before,
        Some(content.len() as u64),
        "fixture precondition: {} must hold exactly the {} bytes the assertion compares against",
        path.display(),
        content.len(),
    );

    let opened = volume.open_read_stream(path).await;
    match opened {
        Ok(mut stream) => {
            let mut read = Vec::with_capacity(content.len());
            while let Some(chunk) = stream.next_chunk().await {
                let chunk = chunk.unwrap_or_else(|e| {
                    panic!(
                        "{} streams {}, so every chunk must arrive; got {e:?}",
                        volume.name(),
                        path.display(),
                    )
                });
                read.extend_from_slice(&chunk);
            }
            assert_eq!(
                read.len(),
                content.len(),
                "{} streamed {} bytes out of {}, but it holds {}",
                volume.name(),
                read.len(),
                path.display(),
                content.len(),
            );
            assert!(
                read == content,
                "{} streamed the wrong bytes out of {}",
                volume.name(),
                path.display(),
            );

            assert!(
                volume.supports_export(),
                "{} streams {} back byte for byte, so it MUST answer supports_export() == true. \
                 A false here is refused at `copy_between_volumes`' guard before a byte moves, \
                 and greys out copy-from in the pane — with every method involved working fine.",
                volume.name(),
                path.display(),
            );
            assert!(
                volume.supports_streaming(),
                "{} streams {} back byte for byte, so it MUST answer supports_streaming() == true",
                volume.name(),
                path.display(),
            );
            assert!(
                volume.capabilities().can_export,
                "{} answers supports_export() == true, so the published VolumeCapabilities the \
                 frontend reads must agree; capabilities() is a pure fold and must not be overridden",
                volume.name(),
            );
        }
        Err(VolumeError::NotSupported) => {
            assert!(
                !volume.supports_export(),
                "{} answers supports_export() == true, so open_read_stream({}) must work rather \
                 than refuse with NotSupported",
                volume.name(),
                path.display(),
            );
        }
        Err(other) => panic!(
            "open_read_stream({}) on {} must either stream or answer NotSupported; got {other:?}",
            path.display(),
            volume.name(),
        ),
    }
}

/// [`VolumeError::NotFound`] carries the PATH that was missing, not the
/// backend's own wording for "missing".
///
/// `missing` must NOT exist on `volume`; the assertion checks that first.
///
/// **Why this one is worth a shared assertion.** The variant's doc says "carries
/// the path", and the transfer layer takes it literally: `map_volume_error`
/// forwards the string straight into `SourceNotFound { path }` /
/// `DestinationNotFound { path }`, which the frontend renders as the name of the
/// file the user just lost. A backend that puts its protocol's diagnostic there
/// instead doesn't fail anything — it just renders the server's sentence where a
/// filename belongs, and the user goes hunting for a file by a name that was
/// never on their disk.
///
/// Matching on the payload's TEXT here is not error classification (nothing
/// branches on it); it's the only way to check what a variant carries, the same
/// way the rename and create_file assertions check what a refusal left behind.
///
/// ⚠️ **Two backends do NOT keep this contract yet, so this assertion is wired
/// into `InMemoryVolume`, `cmdr-archive`, MTP, and `cmdr-sftp` only.** Both open
/// gaps leak their layer's own wording where a filename belongs, and both are
/// bigger than the backend that surfaced them:
/// - **`LocalPosixVolume`** reports `NotFound("No such file or directory (os
///   error 2)")`. The cause is shared: `types.rs`'s
///   `impl From<std::io::Error> for VolumeError` fills all three path-carrying
///   variants with `err.to_string()`, and a bare `std::io::Error` has no path to
///   do better with. Its cell exists and is `#[ignore]`d with the details
///   (`local_posix_conformance_test.rs`).
/// - **`SmbVolume`** reports `NotFound("Protocol error:
///   STATUS_OBJECT_NAME_NOT_FOUND during Create")`, and `AlreadyExists` beside
///   it has the same leak. Carried as prose rather than an ignored cell because
///   the SMB integration lane runs `--run-ignored only`, so an ignored cell
///   there still runs. `cmdr-smb/DETAILS.md` § "The `NotFound` payload gap".
///
/// ❌ Don't relax this assertion to bring either of them in; the assertion is
/// right, and `cmdr-sftp`'s `map_sftp_error` shows the fix shape.
pub async fn assert_not_found_carries_the_path(volume: &dyn Volume, missing: &Path) {
    assert!(
        !volume.exists(missing).await,
        "fixture precondition: {} must not exist",
        missing.display()
    );
    let name = missing
        .file_name()
        .expect("fixture precondition: the missing path must have a final component")
        .to_string_lossy()
        .into_owned();

    let outcome = volume.get_metadata(missing).await;
    let Err(VolumeError::NotFound(carried)) = outcome else {
        panic!(
            "get_metadata({}) on {} must answer NotFound; got {outcome:?}",
            missing.display(),
            volume.name(),
        );
    };
    assert!(
        carried.contains(&name),
        "NotFound must carry the path, and {} carried {carried:?}, which doesn't name {name}. \
         That string is what `map_volume_error` hands the frontend as SourceNotFound.path, \
         so the user reads it as the name of their missing file.",
        volume.name(),
    );
}
