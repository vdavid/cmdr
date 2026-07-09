//! Compress: seed a valid empty zip at a target, then pack sources into it. The
//! seed is the net-new surface (the copy-into machinery is covered by
//! `copy_into_tests`), so these tests pin the seed's validity, that it's
//! load-bearing (a compress against a 0-byte target would fail), and one end-to-end
//! compress of local files.

use super::compress::{compress_start, seed_empty_zip};
use super::test_support::*;
use crate::file_system::volume::backends::archive::bytes_start_with_zip_signature;

/// The seed writes a valid empty archive: the reader opens it with zero entries,
/// and its first bytes pass the shared zip-signature check. Pre-fix (a 0-byte
/// stub) `ZipArchive::new` errors here, so this test is RED until the real
/// 22-byte EOCD seed lands.
#[test]
fn seed_empty_zip_writes_a_valid_zero_entry_archive() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("out.zip");

    seed_empty_zip(&target).expect("seed");

    // The reader opens it as a real, empty archive.
    let file = std::fs::File::open(&target).expect("open seeded zip");
    let archive = ZipArchive::new(file).expect("a 0-byte file would fail here; the seed must be a valid empty zip");
    assert_eq!(archive.len(), 0, "a fresh seed holds zero entries");

    // The magic check the routing/boundary layer uses accepts the seed.
    let mut header = [0u8; 4];
    let mut f = std::fs::File::open(&target).expect("reopen");
    f.read_exact(&mut header).expect("read header");
    assert!(
        bytes_start_with_zip_signature(&header),
        "the seed's first bytes must pass the shared zip-signature check"
    );
}

/// The seed replaces any existing file at the target atomically (temp+rename), so
/// an overwrite lands a clean empty archive, never a torn one, and leaves no temp.
#[test]
fn seed_empty_zip_overwrites_an_existing_file_atomically() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let target = tmp.path().join("out.zip");
    std::fs::write(&target, b"stale bytes that are not a zip").expect("pre-write");

    seed_empty_zip(&target).expect("seed over existing");

    let file = std::fs::File::open(&target).expect("open");
    let archive = ZipArchive::new(file).expect("the overwrite must leave a valid empty zip");
    assert_eq!(archive.len(), 0);
    // No temp sibling left behind after the atomic rename.
    let leftover = std::fs::read_dir(tmp.path())
        .expect("read_dir")
        .filter_map(Result::ok)
        .any(|e| e.file_name().to_string_lossy().contains(".cmdr-tmp-"));
    assert!(!leftover, "the temp sibling must be gone after the rename");
}

/// End-to-end: compress two local files into a new zip at a target and read both
/// entries back. This is where the seed being load-bearing shows — with the 0-byte
/// stub, `route_archive_copy_into`'s in-closure `ZipArchive::new` fails and neither
/// entry lands. (Manually verified RED by temporarily skipping the seed.)
#[tokio::test]
async fn compress_start_packs_local_files_into_a_new_zip() {
    use crate::file_system::volume::backends::LocalPosixVolume;

    let tmp = tempfile::tempdir().expect("tempdir");

    // A local source volume holding two files at its root.
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(&src_root).expect("mkdir src");
    std::fs::write(src_root.join("one.txt"), b"first").expect("w1");
    std::fs::write(src_root.join("two.txt"), b"second").expect("w2");
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.clone()));

    // The target zip doesn't exist yet — compress must seed it, then add the files.
    let dest = tmp.path().join("bundle.zip");
    assert!(!dest.exists(), "the target must not exist before compress");

    let events = Arc::new(CollectorEventSink::new());
    compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("one.txt"), PathBuf::from("two.txt")],
        dest.clone(),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
    )
    .await
    .expect("start compress");

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "compress should complete"
    );

    // Both files landed at the archive root with their exact bytes.
    assert_eq!(read_entry(&dest, "one.txt").as_deref(), Some(b"first".as_slice()));
    assert_eq!(read_entry(&dest, "two.txt").as_deref(), Some(b"second".as_slice()));

    let complete = events.complete.lock_ignore_poison();
    assert!(complete[0].files_skipped == 0, "a clean compress skips nothing");
}

/// Compressing a whole folder packs its subtree under the folder name — the common
/// "compress the directory under the cursor" case.
#[tokio::test]
async fn compress_start_packs_a_directory_subtree() {
    use crate::file_system::volume::backends::LocalPosixVolume;

    let tmp = tempfile::tempdir().expect("tempdir");
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("project/sub")).expect("mkdir");
    std::fs::write(src_root.join("project/readme.txt"), b"top").expect("w1");
    std::fs::write(src_root.join("project/sub/deep.txt"), b"nested").expect("w2");
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.clone()));

    let dest = tmp.path().join("project.zip");
    let events = Arc::new(CollectorEventSink::new());
    compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("project")],
        dest.clone(),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
    )
    .await
    .expect("start compress");

    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    assert_eq!(
        read_entry(&dest, "project/readme.txt").as_deref(),
        Some(b"top".as_slice())
    );
    assert_eq!(
        read_entry(&dest, "project/sub/deep.txt").as_deref(),
        Some(b"nested".as_slice())
    );
}
