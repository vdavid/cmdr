//! Tests for preview-in-zip temp-extraction (`archive_extract`).

use std::io::Write as _;
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::ViewerError;
use super::archive_extract::{
    EXTRACT_CAP_BYTES, extract_if_archive_inner_with, init_archive_extract_dir, is_orphan_extract_name,
    reap_orphan_extracts,
};
use super::session;

/// Serializes the tests that drive `open_session` (they share the process-wide extract
/// dir set by `init_archive_extract_dir`).
static SERIAL: Mutex<()> = Mutex::new(());

/// Registers a real local-FS "root" volume so `resolve("root", …)` finds a parent for
/// the on-demand `ArchiveVolume`. Idempotent; mirrors the pattern in `commands/rename.rs`.
fn ensure_root_volume() {
    use crate::file_system::get_volume_manager;
    use crate::file_system::volume::LocalPosixVolume;
    get_volume_manager().register_if_absent("root", Arc::new(LocalPosixVolume::new("Test root", "/")));
}

/// Writes a real zip to `path`. A `name` ending in `/` becomes a directory entry.
fn build_zip(path: &Path, entries: &[(&str, &[u8])]) {
    use zip::write::SimpleFileOptions;
    let file = std::fs::File::create(path).expect("create zip");
    let mut writer = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, content) in entries {
        if let Some(dir) = name.strip_suffix('/') {
            writer.add_directory(dir, opts).expect("add dir");
        } else {
            writer.start_file(*name, opts).expect("start file");
            writer.write_all(content).expect("write entry");
        }
    }
    writer.finish().expect("finish zip");
}

#[test]
fn orphan_predicate_matches_only_our_subdirs() {
    assert!(is_orphan_extract_name(".cmdr-viewer-abc123"));
    // A sibling name, a plain file, and the write-ops temp family must NOT match.
    assert!(!is_orphan_extract_name("notes.txt"));
    assert!(!is_orphan_extract_name(".cmdr-tmp-abc"));
    assert!(!is_orphan_extract_name("cmdr-viewer-no-dot"));
}

#[test]
fn reaper_removes_only_matching_subdirs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ours = dir.path().join(".cmdr-viewer-123");
    std::fs::create_dir_all(ours.join("inner")).expect("mk ours");
    std::fs::write(ours.join("inner/f"), b"x").expect("seed");
    let theirs = dir.path().join("keepme");
    std::fs::create_dir_all(&theirs).expect("mk theirs");

    reap_orphan_extracts(dir.path());

    assert!(!ours.exists(), "orphan .cmdr-viewer-* subdir should be reaped");
    assert!(theirs.exists(), "unrelated dir must be left alone");
}

#[test]
fn non_archive_path_returns_none() {
    ensure_root_volume();
    let dir = tempfile::tempdir().expect("tempdir");
    let plain = dir.path().join("plain.txt");
    std::fs::write(&plain, b"hi").expect("seed");
    let extract = tempfile::tempdir().expect("extract dir");

    let got = extract_if_archive_inner_with(&plain, extract.path(), EXTRACT_CAP_BYTES).expect("resolve");
    assert!(got.is_none(), "a non-archive path must not extract");
}

#[test]
fn the_zip_file_itself_returns_none_so_it_views_as_raw_bytes() {
    ensure_root_volume();
    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("bundle.zip");
    build_zip(&zip, &[("inner.txt", b"hello")]);
    let extract = tempfile::tempdir().expect("extract dir");

    // The `.zip` FILE itself is NOT temp-extracted — it views as raw bytes like any
    // binary file. (Extracting inner "" would address the archive ROOT, a directory,
    // and error — so pre-fix this would panic here.)
    let got = extract_if_archive_inner_with(&zip, extract.path(), EXTRACT_CAP_BYTES).expect("resolve the .zip file");
    assert!(got.is_none(), "the .zip file itself must not extract (raw-bytes view)");

    // A path INSIDE the archive DOES extract to a temp.
    let inner = zip.join("inner.txt");
    let extracted =
        extract_if_archive_inner_with(&inner, extract.path(), EXTRACT_CAP_BYTES).expect("resolve inner entry");
    assert!(extracted.is_some(), "an inner path extracts to a temp");
}

#[test]
fn refuses_oversize_entry_before_extracting() {
    ensure_root_volume();
    let extract = tempfile::tempdir().expect("extract dir");
    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("big.zip");
    // 200 KiB: bigger than one 128 KiB decompression chunk, so the reported `size`
    // pins the refusal to the up-front declared-size guard (the full 204,800 bytes from
    // the index) rather than the streaming byte-cap backstop, which could only ever
    // report a chunk-bounded count. That makes "refuse BEFORE extraction" an observable
    // contract, not just "refuse eventually".
    let entry_len = 200 * 1024;
    build_zip(&zip, &[("data.bin", &vec![0u8; entry_len])]);

    let inner = zip.join("data.bin");
    let err = extract_if_archive_inner_with(&inner, extract.path(), 10).expect_err("oversize must be refused");
    assert!(
        matches!(err, ViewerError::ExtractTooLarge { size, cap: 10 } if size == entry_len as u64),
        "expected ExtractTooLarge with the full declared size (refused before extraction), got {err:?}"
    );

    // Refused from the index's declared size, BEFORE any temp subdir was created.
    let created: Vec<_> = std::fs::read_dir(extract.path())
        .expect("read extract dir")
        .flatten()
        .collect();
    assert!(
        created.is_empty(),
        "no temp should exist after a cap refusal, found {created:?}"
    );
}

#[test]
fn directory_entry_in_zip_is_rejected() {
    ensure_root_volume();
    let extract = tempfile::tempdir().expect("extract dir");
    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("d.zip");
    build_zip(&zip, &[("sub/", b""), ("sub/f.txt", b"x")]);

    let inner = zip.join("sub");
    let err = extract_if_archive_inner_with(&inner, extract.path(), EXTRACT_CAP_BYTES)
        .expect_err("a directory entry can't be previewed");
    assert!(
        matches!(err, ViewerError::IsDirectory),
        "expected IsDirectory, got {err:?}"
    );
}

#[test]
fn text_file_in_zip_round_trips_and_temp_is_deleted_on_close() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    ensure_root_volume();
    let extract = tempfile::tempdir().expect("extract dir");
    init_archive_extract_dir(extract.path().to_path_buf());

    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("bundle.zip");
    let content = b"line one\nline two\n";
    build_zip(&zip, &[("notes.txt", content)]);

    let inner = zip.join("notes.txt");
    let result = session::open_session(inner.to_str().expect("utf8 path")).expect("open preview");

    // The viewer shows the entry's basename, not the uuid temp name.
    assert_eq!(result.file_name, "notes.txt");

    // Exactly one `.cmdr-viewer-*` subdir under the reaper-covered extract dir, and its
    // extracted file's bytes match the original entry (round-trip).
    let subdirs: Vec<_> = std::fs::read_dir(extract.path())
        .expect("read extract dir")
        .flatten()
        .collect();
    assert_eq!(subdirs.len(), 1, "one temp subdir expected, found {subdirs:?}");
    let subdir_name = subdirs[0].file_name();
    assert!(
        is_orphan_extract_name(&subdir_name.to_string_lossy()),
        "temp subdir must match the reaper glob: {subdir_name:?}"
    );
    let temp_file = subdirs[0].path().join("notes.txt");
    assert_eq!(
        std::fs::read(&temp_file).expect("read temp"),
        content,
        "extracted bytes must match"
    );

    // Closing the session deletes the temp subdir (both close paths funnel here).
    session::close_session(&result.session_id).expect("close");
    let after: Vec<_> = std::fs::read_dir(extract.path())
        .expect("read extract dir")
        .flatten()
        .collect();
    assert!(
        after.is_empty(),
        "temp must be deleted on session close, found {after:?}"
    );
}

#[test]
fn image_in_zip_opens_as_media_and_temp_is_deleted_on_close() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    ensure_root_volume();
    let extract = tempfile::tempdir().expect("extract dir");
    init_archive_extract_dir(extract.path().to_path_buf());

    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("pics.zip");
    // A PNG signature is enough for magic-byte classification as an image.
    let png = b"\x89PNG\r\n\x1a\n and the rest doesn't matter for classification";
    build_zip(&zip, &[("logo.png", png)]);

    let inner = zip.join("logo.png");
    let result = session::open_session(inner.to_str().expect("utf8 path")).expect("open image preview");

    // The extracted image renders inline (media session): a media token, right title.
    assert!(
        matches!(result.kind, super::ViewerContentKind::Image),
        "expected Image, got {:?}",
        result.kind
    );
    assert!(
        result.media_token.is_some(),
        "an image preview must mint a cmdr-media:// token"
    );
    assert_eq!(result.file_name, "logo.png");

    // Closing deletes the extracted temp (the media session inherited the cleanup).
    session::close_session(&result.session_id).expect("close");
    let after: Vec<_> = std::fs::read_dir(extract.path())
        .expect("read extract dir")
        .flatten()
        .collect();
    assert!(
        after.is_empty(),
        "media temp must be deleted on session close, found {after:?}"
    );
}
