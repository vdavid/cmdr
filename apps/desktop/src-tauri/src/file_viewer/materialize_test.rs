//! Tests for preview-in-zip temp-extraction (`materialize`).

use std::io::Write as _;
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::ViewerError;
use super::materialize::{
    PREVIEW_CAP_BYTES, extract_if_routed_with, init_materialize_dir, is_orphan_temp_name, materialize_for_viewer_with,
    reap_orphan_temps,
};
use super::session;

/// Serializes the tests that drive `open_session` (they share the process-wide extract
/// dir set by `init_materialize_dir`).
static SERIAL: Mutex<()> = Mutex::new(());

/// Registers a real local-FS "root" volume so `resolve("root", …)` finds a parent for
/// the on-demand `ArchiveVolume`. Idempotent; mirrors the pattern in `commands/rename.rs`.
fn ensure_root_volume() {
    use crate::file_system::volume::LocalPosixVolume;
    use crate::file_system::volume::manager::get_volume_manager;
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
    assert!(is_orphan_temp_name(".cmdr-viewer-abc123"));
    // A sibling name, a plain file, and the write-ops temp family must NOT match.
    assert!(!is_orphan_temp_name("notes.txt"));
    assert!(!is_orphan_temp_name(".cmdr-tmp-abc"));
    assert!(!is_orphan_temp_name("cmdr-viewer-no-dot"));
}

#[test]
fn reaper_removes_only_matching_subdirs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let ours = dir.path().join(".cmdr-viewer-123");
    std::fs::create_dir_all(ours.join("inner")).expect("mk ours");
    std::fs::write(ours.join("inner/f"), b"x").expect("seed");
    let theirs = dir.path().join("keepme");
    std::fs::create_dir_all(&theirs).expect("mk theirs");

    reap_orphan_temps(dir.path());

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

    let got = extract_if_routed_with(&plain, "root", extract.path(), PREVIEW_CAP_BYTES).expect("resolve");
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
    let got = extract_if_routed_with(&zip, "root", extract.path(), PREVIEW_CAP_BYTES).expect("resolve the .zip file");
    assert!(got.is_none(), "the .zip file itself must not extract (raw-bytes view)");

    // A path INSIDE the archive DOES extract to a temp.
    let inner = zip.join("inner.txt");
    let extracted =
        extract_if_routed_with(&inner, "root", extract.path(), PREVIEW_CAP_BYTES).expect("resolve inner entry");
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
    let err = extract_if_routed_with(&inner, "root", extract.path(), 10).expect_err("oversize must be refused");
    assert!(
        matches!(err, ViewerError::TooLargeToPreview { size, cap: 10 } if size == entry_len as u64),
        "expected TooLargeToPreview with the full declared size (refused before extraction), got {err:?}"
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
    let err = extract_if_routed_with(&inner, "root", extract.path(), PREVIEW_CAP_BYTES)
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
    init_materialize_dir(extract.path().to_path_buf());

    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("bundle.zip");
    let content = b"line one\nline two\n";
    build_zip(&zip, &[("notes.txt", content)]);

    let inner = zip.join("notes.txt");
    let result = session::open_session(inner.to_str().expect("utf8 path"), "root").expect("open preview");

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
        is_orphan_temp_name(&subdir_name.to_string_lossy()),
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
    init_materialize_dir(extract.path().to_path_buf());

    let src = tempfile::tempdir().expect("src dir");
    let zip = src.path().join("pics.zip");
    // A PNG signature is enough for magic-byte classification as an image.
    let png = b"\x89PNG\r\n\x1a\n and the rest doesn't matter for classification";
    build_zip(&zip, &[("logo.png", png)]);

    let inner = zip.join("logo.png");
    let result = session::open_session(inner.to_str().expect("utf8 path"), "root").expect("open image preview");

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

/// Registers a volume whose paths the OS can't open (standing in for a phone over
/// ADB or MTP, or an SFTP / WebDAV server) under `id`, holding one `notes.txt`, and
/// returns that file's app path. `InMemoryVolume` answers `paths_are_os_visible()`
/// `false` by default; the assert keeps the fixture honest about it.
fn a_volume_the_os_cant_open(id: &str, content: &[u8]) -> String {
    use crate::file_system::volume::manager::get_volume_manager;
    use crate::file_system::volume::{InMemoryVolume, Volume as _};

    let root = format!("mtp://{id}/1");
    let volume = InMemoryVolume::new("Phone").with_root(&root);
    let path = format!("{root}/notes.txt");
    tauri::async_runtime::block_on(volume.create_file(Path::new(&path), content)).expect("seed the file");
    assert!(
        !volume.paths_are_os_visible(),
        "the fixture must model a volume the OS can't open"
    );
    get_volume_manager().register(id, Arc::new(volume));
    path
}

/// A file on a volume whose paths the OS can't open is pulled through the volume
/// into the same bounded temp a routed entry uses, read from there, and the temp goes
/// on close. Pre-fix the open went straight to `std::fs`, found nothing at `mtp://…`,
/// and answered `NotFound` (F3 on a real phone over ADB did exactly that).
#[test]
fn a_file_on_a_volume_the_os_cant_open_is_pulled_into_a_temp_and_its_lines_read() {
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let extract = crate::test_support::TestDir::new("viewer_pull");
    init_materialize_dir(extract.to_path_buf());
    let path = a_volume_the_os_cant_open("viewer-pull-cell", b"first line\nsecond line\n");

    let opened = session::open_session(&path, "viewer-pull-cell").expect("the file opens");
    assert_eq!(opened.file_name, "notes.txt");
    assert_eq!(opened.initial_lines.lines[0], "first line");
    assert_eq!(opened.initial_lines.lines[1], "second line");
    let subdirs: Vec<_> = std::fs::read_dir(&extract)
        .expect("read extract dir")
        .flatten()
        .collect();
    assert_eq!(subdirs.len(), 1, "one temp subdir expected, found {subdirs:?}");

    session::close_session(&opened.session_id).expect("close");
    let after: Vec<_> = std::fs::read_dir(&extract)
        .expect("read extract dir")
        .flatten()
        .collect();
    assert!(after.is_empty(), "the temp goes on close, found {after:?}");
}

/// A file past the cap on such a volume answers the typed `TooLargeToPreview` the
/// frontend words ("too big to preview from here"), from the size the volume reports,
/// before a temp exists. 100 KiB is past one 64 KiB in-memory chunk, so the reported
/// `size` pins the refusal to the up-front guard rather than the streaming backstop.
/// Pre-fix nothing was pulled, so there was no refusal to give.
#[test]
fn a_file_past_the_cap_on_a_volume_the_os_cant_open_is_refused_before_a_temp_exists() {
    let extract = crate::test_support::TestDir::new("viewer_pull_cap");
    let entry_len = 100 * 1024;
    let path = a_volume_the_os_cant_open("viewer-pull-cap-cell", &vec![b'x'; entry_len]);

    let outcome = materialize_for_viewer_with(Path::new(&path), "viewer-pull-cap-cell", &extract, 10);
    assert!(
        matches!(outcome, Err(ViewerError::TooLargeToPreview { size, cap: 10 }) if size == entry_len as u64),
        "expected TooLargeToPreview with the declared size, got {outcome:?}"
    );
    let created: Vec<_> = std::fs::read_dir(&extract)
        .expect("read extract dir")
        .flatten()
        .collect();
    assert!(created.is_empty(), "no temp after a cap refusal, found {created:?}");
}

/// The cap fires for a `.zip` entry AND for a blob in a repo's virtual `.git`
/// snapshot, so its log line names no namespace. Pinned because "from the archive"
/// read as a plain lie once the git portal started routing through here.
#[test]
fn the_too_large_display_string_names_no_particular_routed_source() {
    let rendered = ViewerError::TooLargeToPreview { size: 9, cap: 2 }.to_string();
    assert_eq!(
        rendered,
        "This item is too large to preview from here (size 9, limit 2)"
    );
}
