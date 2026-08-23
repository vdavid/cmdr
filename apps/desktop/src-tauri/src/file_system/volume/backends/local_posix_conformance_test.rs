//! The shared `Volume` conformance promises, asserted against `LocalPosixVolume`.
//!
//! These live apart from `local_posix_test.rs` because they assert something
//! different in kind: not how this backend behaves, but that it keeps the
//! contracts `cmdr_fs::volume::conformance` holds EVERY backend to. Every other
//! backend already keeps its conformance cells in a file of their own
//! (`cmdr-smb`'s and `cmdr-sftp`'s `volume::conformance_test`, MTP's
//! `mtp_conformance_test`); this is that file for the local one.
//!
//! ❗ LocalPosix is the backend the other suites' fixtures are compared against,
//! so a contract it stops keeping is one the whole suite stops noticing.

use super::*;
use crate::test_support::TestDir;
use std::path::Path;

/// The shared `Volume::delete` non-recursion assertion. LocalPosix gets it for
/// free from `std::fs::remove_dir`'s `ENOTEMPTY`, which is exactly why it's
/// worth pinning: "free" is what makes a contract invisible until a backend
/// that doesn't get it free comes along.
#[tokio::test]
async fn delete_honors_the_shared_non_recursion_contract() {
    let test_dir = TestDir::new("delete_non_recursion_test");
    let volume = LocalPosixVolume::new("Test", &*test_dir);

    volume.create_directory(Path::new("album")).await.unwrap();
    volume
        .create_file(Path::new("album/keep.txt"), b"content")
        .await
        .unwrap();

    cmdr_fs::volume::conformance::assert_delete_leaves_a_non_empty_dir_intact(&volume, Path::new("album"), "keep.txt")
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
    let volume = LocalPosixVolume::new("Test", &*test_dir);

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
    let volume = LocalPosixVolume::new("Test", &*test_dir);

    volume
        .create_file(Path::new("notes.txt"), b"the user's notes")
        .await
        .unwrap();

    cmdr_fs::volume::conformance::assert_create_file_refuses_to_clobber(&volume, Path::new("notes.txt"), b"new").await;
}

/// The shared `Volume::create_directory_all` honesty assertion, over the trait's
/// default walk composed from LocalPosix's own `exists` + `create_directory`.
#[tokio::test]
async fn create_directory_all_honors_the_shared_honesty_contract() {
    let test_dir = TestDir::new("create_directory_all_honesty_conformance_test");
    let volume = LocalPosixVolume::new("Test", &*test_dir);

    volume.create_directory(Path::new("album")).await.unwrap();

    cmdr_fs::volume::conformance::assert_create_directory_all_reports_an_existing_dir_honestly(
        &volume,
        Path::new("album"),
    )
    .await;
}

/// The shared writability-declaration assertion: `is_writable()` and the
/// mutations LocalPosix offers say the same thing.
#[tokio::test]
async fn is_writable_honors_the_shared_declaration_contract() {
    let test_dir = TestDir::new("is_writable_declaration_conformance_test");
    let volume = LocalPosixVolume::new("Test", &*test_dir);

    cmdr_fs::volume::conformance::assert_writability_matches_the_mutations_offered(&volume, Path::new("scratch")).await;
}

/// The shared export-handshake assertion: LocalPosix streams its bytes, so it
/// must say `supports_export()`.
#[tokio::test]
async fn export_honors_the_shared_handshake_contract() {
    let test_dir = TestDir::new("export_handshake_conformance_test");
    let volume = LocalPosixVolume::new("Test", &*test_dir);
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
    let volume = LocalPosixVolume::new("Test", &*test_dir);

    cmdr_fs::volume::conformance::assert_not_found_carries_the_path(&volume, Path::new("no-such-file.txt")).await;
}
