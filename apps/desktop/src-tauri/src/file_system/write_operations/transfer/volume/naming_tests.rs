//! Unit tests for `naming.rs` (the volume-side ` (N)` namer), split out as a
//! `#[path]` child so the module itself stays readable. `super::` here is
//! `naming` and `super::super::` is `volume`, the same one-level-shallower rule
//! every other `*_tests.rs` in this directory follows.

use super::*;
use crate::file_system::volume::InMemoryVolume;

// ======================================================================
// find_unique_volume_name — TOCTOU reservation on local-FS dest volumes
// ======================================================================
//
// Volume-side sibling of `unique_name::find_unique_name`. For a Rename
// resolution the chosen `name (N)` must be atomically RESERVED with an
// `O_CREAT|O_EXCL` placeholder when the destination volume is backed by a
// local filesystem (`local_path().is_some()`), so a concurrent writer
// (second Cmdr op, cloud-sync agent, backup tool) can't land a file at the
// same name between our pick and the streaming write. A probe alone would
// leave a TOCTOU window. Mirrors the reservation suite in `unique_name_tests.rs`.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_fs_rename_reserves_the_chosen_name_on_disk() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    let temp = tempfile::TempDir::new().unwrap();
    let target = temp.path().join("notes.txt");
    std::fs::write(&target, b"original").unwrap();

    let vol: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("dst", temp.path().to_path_buf()));

    let unique = find_unique_volume_name(&vol, &target, false, &ClaimedNames::default()).await;

    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "notes (1).txt");
    // The O_EXCL placeholder must already exist on disk after the call.
    assert!(
        unique.exists(),
        "reservation must create the placeholder on a local-FS dest"
    );
    // A second call escalates to (2), proving the first reservation persisted.
    let next = find_unique_volume_name(&vol, &target, false, &ClaimedNames::default()).await;
    assert_eq!(next.file_name().unwrap().to_string_lossy(), "notes (2).txt");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_fs_rename_keeps_extension_in_the_right_place() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    let temp = tempfile::TempDir::new().unwrap();
    let target = temp.path().join("report.pdf");
    std::fs::write(&target, b"x").unwrap();

    let vol: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("dst", temp.path().to_path_buf()));
    let unique = find_unique_volume_name(&vol, &target, false, &ClaimedNames::default()).await;
    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "report (1).pdf");
    assert!(unique.exists(), "reservation must create the placeholder");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_local_dest_does_not_reserve_a_placeholder() {
    // MTP / SMB / InMemory have no exclusive-create semantics here
    // (`local_path()` is `None`), so the function must NOT try to touch the
    // real local FS. It returns the next free name based on `exists()`,
    // accepting the documented narrow residual window.
    let dst = Arc::new(InMemoryVolume::new("dst"));
    dst.create_file(Path::new("/notes.txt"), b"old").await.unwrap();
    let dst_dyn: Arc<dyn Volume> = dst.clone();

    let unique = find_unique_volume_name(&dst_dyn, Path::new("/notes.txt"), false, &ClaimedNames::default()).await;
    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "notes (1).txt");
    // No placeholder was created on the in-memory volume.
    assert!(
        !dst.exists(&unique).await,
        "non-local dest must not pre-create the renamed name"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_fs_rename_continues_a_trailing_sequence() {
    // The volume namer shares `unique_name::{split_sequence, numbered_name}` with
    // the local-FS namer, so a name that already ends in ` (N)` continues the
    // series here too instead of nesting into `notes (1) (1).txt`.
    use crate::file_system::volume::backends::LocalPosixVolume;
    let temp = tempfile::TempDir::new().unwrap();
    let target = temp.path().join("notes (1).txt");
    std::fs::write(&target, b"original").unwrap();

    let vol: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("dst", temp.path().to_path_buf()));

    let unique = find_unique_volume_name(&vol, &target, false, &ClaimedNames::default()).await;
    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "notes (2).txt");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_local_dest_continues_a_trailing_sequence_too() {
    // The `exists()`-probe branch takes the same route through the shared
    // convention, so MTP / SMB dests number identically to local ones.
    let dst = Arc::new(InMemoryVolume::new("dst"));
    dst.create_file(Path::new("/notes (1).txt"), b"old").await.unwrap();
    let dst_dyn: Arc<dyn Volume> = dst.clone();

    let unique = find_unique_volume_name(&dst_dyn, Path::new("/notes (1).txt"), false, &ClaimedNames::default()).await;
    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "notes (2).txt");
}

/// A DIRECTORY never takes the `O_CREAT|O_EXCL` reservation, even on a local-FS
/// destination: the placeholder is a file, and one sitting where the copy is
/// about to create a directory makes the merge walker's `create_directory`
/// report `AlreadyExists` and try to merge into it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_directory_name_is_never_reserved_with_a_file_placeholder() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    let temp = tempfile::TempDir::new().unwrap();
    let target = temp.path().join("docs");
    std::fs::create_dir(&target).unwrap();

    let vol: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("dst", temp.path().to_path_buf()));

    let unique = find_unique_volume_name(&vol, &target, true, &ClaimedNames::default()).await;

    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "docs (1)");
    assert!(
        !unique.exists(),
        "the picked directory name must be left free for the merge walker to create"
    );
}

/// A directory has no extension, so `is_directory` picks the candidate KIND too:
/// the number goes after the whole name, never inside it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dot_in_a_directory_name_is_part_of_the_name_on_a_volume() {
    let dst = Arc::new(InMemoryVolume::new("dst"));
    dst.create_directory(Path::new("/backup.2024")).await.unwrap();
    let dst_dyn: Arc<dyn Volume> = dst.clone();

    let unique = find_unique_volume_name(&dst_dyn, Path::new("/backup.2024"), true, &ClaimedNames::default()).await;
    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "backup.2024 (1)");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_non_numeric_parenthetical_is_not_a_sequence_on_a_volume() {
    let dst = Arc::new(InMemoryVolume::new("dst"));
    dst.create_file(Path::new("/Report (final).pdf"), b"old").await.unwrap();
    let dst_dyn: Arc<dyn Volume> = dst.clone();

    let unique = find_unique_volume_name(
        &dst_dyn,
        Path::new("/Report (final).pdf"),
        false,
        &ClaimedNames::default(),
    )
    .await;
    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "Report (final) (1).pdf");
}
