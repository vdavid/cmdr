//! Extract-out tests for `copy_volumes_with_progress`, split out of
//! `volume_copy_tests.rs`. A cross-volume copy that pulls a file and a
//! directory subtree OUT of a zip archive through the transfer engine, plus the
//! Zip-Slip guard that writes a symlink entry as a regular file.
//!
//! Shared fixture `make_state` lives in `volume_copy_tests.rs` (`super::tests`).

use super::tests::make_state;
use super::*;
use crate::file_system::volume::{InMemoryVolume, LocalPosixVolume};
use crate::file_system::write_operations::types::CollectorEventSink;

// ========================================================================
// Extract-out: copy a file + a directory subtree OUT of a zip archive
// (headless repro of the extract-out flow through the transfer engine).
// ========================================================================

/// Builds a real zip with a top-level file and a two-file directory, returning
/// the tempdir (keep it alive) and the `.zip` path.
fn build_extract_out_fixture() -> (tempfile::TempDir, PathBuf) {
    use std::io::Write;
    let dir = tempfile::tempdir().expect("tempdir");
    let zip_path = dir.path().join("bundle.zip");
    let file = std::fs::File::create(&zip_path).expect("create zip");
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    writer.start_file("readme.txt", options).expect("start readme");
    writer.write_all(b"hello").expect("write readme");
    writer.add_directory("docs/", options).expect("add docs dir");
    writer.start_file("docs/a.txt", options).expect("start a");
    writer.write_all(b"aaa").expect("write a");
    writer.start_file("docs/b.txt", options).expect("start b");
    writer.write_all(b"bbb").expect("write b");
    writer.finish().expect("finish zip");
    (dir, zip_path)
}

async fn read_dest_file(dest: &Arc<dyn Volume>, path: &str) -> Vec<u8> {
    let mut stream = dest
        .open_read_stream(Path::new(path))
        .await
        .unwrap_or_else(|e| panic!("dest missing {path}: {e:?}"));
    let mut out = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        out.extend_from_slice(&chunk.expect("chunk"));
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn extract_out_copies_a_file_and_a_directory_subtree_out_of_a_zip() {
    use crate::file_system::volume::backends::archive::{ArchiveFormat, ArchiveVolume};

    let (_tmp, zip_path) = build_extract_out_fixture();

    // Source = the read-only ArchiveVolume over the zip; dest = in-memory.
    let parent: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Parent").with_local_fs_access());
    let source: Arc<dyn Volume> = Arc::new(ArchiveVolume::new(parent, zip_path.clone(), ArchiveFormat::Zip));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    // The FE sends FULL paths that cross the archive boundary (what resolve
    // returns unchanged): a top-level file and a directory.
    let sources = vec![zip_path.join("readme.txt"), zip_path.join("docs")];

    let result = copy_volumes_with_progress(
        events.clone(),
        "extract-out-op",
        &state,
        Arc::clone(&source),
        &sources,
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "extract-out should succeed: {result:?}");

    // The file and both subtree files land at the destination with their bytes.
    assert_eq!(read_dest_file(&dest, "/readme.txt").await, b"hello");
    assert_eq!(read_dest_file(&dest, "/docs/a.txt").await, b"aaa");
    assert_eq!(read_dest_file(&dest, "/docs/b.txt").await, b"bbb");

    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1, "one completion event");
    // `files_processed` counts TOP-LEVEL source items (the file + the directory),
    // not leaves — same as any local↔local directory copy. `bytes_processed` is
    // the reliable full-transfer measure: all three inner files' bytes.
    assert_eq!(complete[0].files_processed, 2, "two top-level sources");
    assert_eq!(complete[0].bytes_processed, 5 + 3 + 3, "all inner-file bytes copied");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn extracting_a_symlink_entry_writes_a_regular_file_never_a_symlink() {
    use crate::file_system::volume::backends::archive::{ArchiveFormat, ArchiveVolume};

    // Pins that extraction never CREATES a symlink from archive data: a symlink
    // entry's content is its target path, and writing those bytes verbatim as a
    // regular file is what stops Zip Slip through the back door (a symlink entry
    // pointing outside the extraction root).
    let src_dir = tempfile::tempdir().expect("src tempdir");
    let zip_path = src_dir.path().join("bundle.zip");
    {
        let file = std::fs::File::create(&zip_path).expect("create zip");
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        // The hostile case: a symlink whose target is an absolute path OUTSIDE any
        // extraction root. Extraction must write these bytes as a plain file, never
        // materialize a link to `/etc/passwd`.
        writer.add_symlink("link", "/etc/passwd", options).expect("add symlink");
        writer.finish().expect("finish zip");
    }

    let parent: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Parent").with_local_fs_access());
    let source: Arc<dyn Volume> = Arc::new(ArchiveVolume::new(parent, zip_path.clone(), ArchiveFormat::Zip));

    // A real-filesystem destination so we can stat the landed entry's kind.
    let dst_dir = tempfile::tempdir().expect("dst tempdir");
    let dest: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("Dest", dst_dir.path().to_str().unwrap()));

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "symlink-extract-op",
        &state,
        Arc::clone(&source),
        &[zip_path.join("link")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "symlink extraction should succeed: {result:?}");

    // (a) The destination is a REGULAR file, never a symlink.
    let landed = dst_dir.path().join("link");
    let meta = std::fs::symlink_metadata(&landed).expect("dest entry exists");
    assert!(
        meta.file_type().is_file(),
        "extracted symlink entry must be a regular file, not a symlink"
    );
    assert!(
        !meta.file_type().is_symlink(),
        "must NOT be a symlink — that would be Zip Slip through the back door"
    );
    // (b) Its content is the target-path bytes, written verbatim (never followed).
    assert_eq!(std::fs::read(&landed).expect("read dest"), b"/etc/passwd");
}
