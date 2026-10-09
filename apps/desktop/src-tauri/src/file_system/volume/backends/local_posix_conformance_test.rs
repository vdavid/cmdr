//! The shared `Volume` conformance promises, asserted against `LocalPosixVolume`.
//!
//! These live apart from `local_posix_test.rs` because they assert something
//! different in kind: not how this backend behaves, but that it keeps the
//! contracts `cmdr_fs::volume::conformance` holds EVERY backend to. Every other
//! backend already keeps its conformance cells in a file of their own
//! (`cmdr-smb`'s and `cmdr-sftp`'s `volume::conformance_test`, MTP's
//! `cmdr-mtp`'s `volume::conformance_test`); this is that file for the local one.
//!
//! ❗ LocalPosix is the backend the other suites' fixtures are compared against,
//! so a contract it stops keeping is one the whole suite stops noticing.

use super::*;
use crate::file_system::volume::{StreamLength, WriteMode};
use crate::test_support::TestDir;
use std::path::Path;

/// The shared `Volume::delete` non-recursion assertion. LocalPosix gets it for
/// free from `std::fs::remove_dir`'s `ENOTEMPTY`, which is exactly why it's
/// worth pinning: "free" is what makes a contract invisible until a backend
/// that doesn't get it free comes along.
#[tokio::test]
async fn delete_honors_the_shared_non_recursion_contract() {
    let test_dir = TestDir::new("delete_non_recursion_test");
    let volume = LocalPosixVolume::local_folder("Test", &*test_dir);

    volume.create_directory(Path::new("album")).await.unwrap();
    volume
        .create_file(Path::new("album/keep.txt"), b"content")
        .await
        .unwrap();

    cmdr_fs::volume::conformance::assert_delete_leaves_a_non_empty_dir_intact(&volume, Path::new("album"), "keep.txt")
        .await;
}

/// The batch delete a move's source sweep uses, through the trait's default.
#[tokio::test]
async fn delete_files_honors_the_shared_batch_contract() {
    let test_dir = TestDir::new("delete_files_batch_test");
    let volume = LocalPosixVolume::local_folder("Test", &*test_dir);
    volume.create_directory(Path::new("level")).await.unwrap();
    for name in ["a.txt", "b.txt", "kept.txt"] {
        volume
            .create_file(&Path::new("level").join(name), b"bytes")
            .await
            .unwrap();
    }

    cmdr_fs::volume::conformance::assert_delete_files_removes_exactly_what_it_names(
        &volume,
        [Path::new("level/a.txt"), Path::new("level/b.txt")],
        Path::new("level/kept.txt"),
    )
    .await;
}

/// The shared `Volume::rename` no-clobber assertion. LocalPosix earns it with
/// `renamex_np(RENAME_EXCL)` / `renameat2(RENAME_NOREPLACE)`, one kernel
/// operation with no TOCTOU window — a different mechanism from every other
/// backend's, which is the whole reason the promise is asserted rather than
/// assumed.
#[tokio::test]
async fn rename_honors_the_shared_no_clobber_contract() {
    let test_dir = TestDir::new("rename_no_clobber_conformance_test");
    let volume = LocalPosixVolume::local_folder("Test", &*test_dir);

    volume.create_file(Path::new("source.txt"), b"source").await.unwrap();
    volume
        .create_file(Path::new("target.txt"), b"the user's target file")
        .await
        .unwrap();

    cmdr_fs::volume::conformance::assert_rename_refuses_an_existing_destination(
        &volume,
        Path::new("source.txt"),
        Path::new("target.txt"),
    )
    .await;
}

/// The shared `Volume::create_file` no-clobber assertion. LocalPosix earns it
/// with `OpenOptions::create_new(true)`; a plain `std::fs::write` one refactor
/// away would truncate instead, with the New File command still reporting
/// success.
#[tokio::test]
async fn create_file_honors_the_shared_no_clobber_contract() {
    let test_dir = TestDir::new("create_file_no_clobber_conformance_test");
    let volume = LocalPosixVolume::local_folder("Test", &*test_dir);

    volume
        .create_file(Path::new("notes.txt"), b"the user's notes")
        .await
        .unwrap();

    cmdr_fs::volume::conformance::assert_create_file_refuses_to_clobber(&volume, Path::new("notes.txt"), b"new").await;
}

/// The shared `write_from_stream(CreateNew)` no-clobber assertion. LocalPosix
/// earns it with `OpenOptions::create_new(true)` (`O_EXCL`).
#[tokio::test]
async fn write_from_stream_create_new_honors_the_shared_no_clobber_contract() {
    let test_dir = TestDir::new("write_create_new_conformance_test");
    let volume = LocalPosixVolume::local_folder("Test", &*test_dir);

    volume
        .create_file(Path::new("notes.txt"), b"the user's notes")
        .await
        .unwrap();

    cmdr_fs::volume::conformance::assert_write_from_stream_create_new_refuses_to_clobber(
        &volume,
        Path::new("notes.txt"),
        Path::new("fresh.txt"),
        b"new",
    )
    .await;
}

/// The shared date assertions. Every copy landing on local disk from another
/// volume goes through `write_from_stream`, so this is the cell that keeps a
/// phone's photos dated when they land in `~/Pictures`.
#[tokio::test]
async fn a_copy_keeps_the_source_date_per_the_shared_contract() {
    let test_dir = TestDir::new("dated_write_conformance_test");
    let volume = LocalPosixVolume::local_folder("Test", &*test_dir);

    cmdr_fs::volume::conformance::assert_write_from_stream_keeps_the_source_date(
        &volume,
        Path::new("dated.txt"),
        std::time::Duration::ZERO,
    )
    .await;
    cmdr_fs::volume::conformance::assert_read_stream_reports_the_listed_date(&volume, Path::new("dated.txt")).await;
}

/// A copy onto local disk dates the folders it created, once their contents
/// landed.
#[tokio::test]
async fn set_modified_honors_the_shared_folder_date_contract() {
    let test_dir = TestDir::new("folder_date_conformance_test");
    let volume = LocalPosixVolume::local_folder("Test", &*test_dir);

    cmdr_fs::volume::conformance::assert_set_modified_dates_a_folder(
        &volume,
        Path::new("dated"),
        std::time::Duration::ZERO,
    )
    .await;
}

#[tokio::test]
async fn unknown_write_streams_all_bytes_and_reports_the_accepted_count() {
    let test_dir = TestDir::new("unknown_write_conformance_test");
    let volume = LocalPosixVolume::local_folder("Test", &*test_dir);
    let (tx, rx) = tokio::sync::mpsc::channel(1);
    let (cancel_tx, _cancel_rx) = tokio::sync::oneshot::channel();
    tx.send(Ok(b"first".to_vec())).await.unwrap();
    let sender = tokio::spawn(async move {
        tx.send(Ok(b"-second".to_vec())).await.unwrap();
    });
    let progress = std::sync::Mutex::new(Vec::new());
    let written = volume
        .write_from_stream(
            Path::new("notes.txt"),
            WriteMode::CreateNew,
            StreamLength::Unknown,
            Box::new(cmdr_fs::volume::ChannelReadStream::new(
                rx,
                cancel_tx,
                StreamLength::Unknown,
            )),
            &|tick| {
                progress.lock().unwrap().push(tick);
                std::ops::ControlFlow::Continue(())
            },
        )
        .await;
    sender.await.unwrap();

    assert_eq!(written.unwrap(), 12);
    assert_eq!(std::fs::read(test_dir.join("notes.txt")).unwrap(), b"first-second");
    let progress = progress.lock().unwrap();
    assert_eq!(progress.last().unwrap().bytes_written, 12);
    assert_eq!(progress.last().unwrap().expected_length, StreamLength::Unknown);
}

/// The shared `Volume::create_directory_all` honesty assertion, over the trait's
/// default walk composed from LocalPosix's own `exists` + `create_directory`.
#[tokio::test]
async fn create_directory_all_honors_the_shared_honesty_contract() {
    let test_dir = TestDir::new("create_directory_all_honesty_conformance_test");
    let volume = LocalPosixVolume::local_folder("Test", &*test_dir);

    volume.create_directory(Path::new("album")).await.unwrap();

    cmdr_fs::volume::conformance::assert_create_directory_all_reports_an_existing_dir_honestly(
        &volume,
        Path::new("album"),
    )
    .await;
}

/// The shared file-in-the-way assertion, over the trait's default walk. The OS
/// answers a create under a file with `ENOTDIR`, which reaches the user as a
/// generic refusal unless the walk names the file.
#[tokio::test]
async fn create_directory_all_honors_the_shared_file_in_the_way_contract() {
    let test_dir = TestDir::new("create_directory_all_file_in_the_way_conformance_test");
    let volume = LocalPosixVolume::local_folder("Test", &*test_dir);

    volume
        .create_file(Path::new("notes"), b"the user's notes")
        .await
        .unwrap();

    cmdr_fs::volume::conformance::assert_create_directory_all_refuses_a_file_in_the_way(&volume, Path::new("notes"))
        .await;
}

/// The shared through-a-link assertion. ❗ The cell that keeps the refusal above
/// from overreaching: `/tmp` and `/var` are links on macOS, so a walk that
/// judged an occupied name by `lstat` would refuse most of the disk.
#[tokio::test]
async fn create_directory_all_honors_the_shared_through_a_link_contract() {
    let test_dir = TestDir::new("create_directory_all_through_a_link_conformance_test");
    let volume = LocalPosixVolume::local_folder("Test", &*test_dir);

    volume.create_directory(Path::new("real")).await.unwrap();
    std::os::unix::fs::symlink(test_dir.join("real"), test_dir.join("link")).unwrap();

    cmdr_fs::volume::conformance::assert_create_directory_all_goes_through_a_link_to_a_folder(
        &volume,
        Path::new("link"),
        Path::new("real"),
    )
    .await;
}

/// The shared writability-declaration assertion: `is_writable()` and the
/// mutations LocalPosix offers say the same thing.
#[tokio::test]
async fn is_writable_honors_the_shared_declaration_contract() {
    let test_dir = TestDir::new("is_writable_declaration_conformance_test");
    let volume = LocalPosixVolume::local_folder("Test", &*test_dir);

    cmdr_fs::volume::conformance::assert_writability_matches_the_mutations_offered(&volume, Path::new("scratch")).await;
}

/// The shared export-handshake assertion: LocalPosix streams its bytes, so it
/// must say `supports_export()`.
#[tokio::test]
async fn export_honors_the_shared_handshake_contract() {
    let test_dir = TestDir::new("export_handshake_conformance_test");
    let volume = LocalPosixVolume::local_folder("Test", &*test_dir);
    let content = b"the bytes a copy would move";
    volume.create_file(Path::new("exported.txt"), content).await.unwrap();

    cmdr_fs::volume::conformance::assert_export_matches_the_bytes_offered(&volume, Path::new("exported.txt"), content)
        .await;
}

/// The shared `NotFound`-payload assertion: the string the frontend renders as
/// the missing file's name really is its path.
///
/// LocalPosix earns it from `VolumeError::from_io_at`, applied at every site that
/// touches the filesystem. There is deliberately no `From<std::io::Error>` to
/// reach for, so a new `?` site can't quietly go back to putting the errno's
/// sentence where the frontend renders a filename.
#[tokio::test]
async fn not_found_honors_the_shared_path_payload_contract() {
    let test_dir = TestDir::new("not_found_payload_conformance_test");
    let volume = LocalPosixVolume::local_folder("Test", &*test_dir);

    cmdr_fs::volume::conformance::assert_not_found_carries_the_path(&volume, Path::new("no-such-file.txt")).await;
}

/// The shared conflict-scan assertion. LocalPosix gets it for free from the
/// per-item `exists()` probe, which simply finds nothing under a destination
/// directory that isn't there. "Free" is what makes the contract invisible until
/// a backend that has to spend a listing on it comes along, and two of them did.
#[tokio::test]
async fn conflict_scan_honors_the_shared_missing_destination_contract() {
    let test_dir = TestDir::new("conflict_scan_missing_destination_conformance_test");
    let volume = LocalPosixVolume::local_folder("Test", &*test_dir);

    cmdr_fs::volume::conformance::assert_conflict_scan_reads_a_missing_destination_as_empty(
        &volume,
        Path::new("not-created-yet"),
    )
    .await;
}
