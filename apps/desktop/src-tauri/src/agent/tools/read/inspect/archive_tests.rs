//! Tests for the archive kind: the `.zip` itself, a directory inside it, a file inside it
//! (read through the viewer's bounded temp), the encryption and cap refusals, and the
//! `unsupportedVolume` half that needs a registered volume.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use serde_json::json;

use super::archive::{ArchiveContent, ArchiveEntry, MAX_ARCHIVE_ENTRIES, TempCleanup};
use super::tests::assert_text_only;
use super::*;
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{InMemoryVolume, LocalPosixVolume};
use crate::file_viewer::archive_extract::{EXTRACT_CAP_BYTES, extract_if_archive_inner_with};
use crate::test_support::TestDir;
use cmdr_archive::test_fixtures::{
    build_encrypted_7z, build_zip, build_zipcrypto_zip, dir as zip_dir, encrypted_entry, plain_entry, stored,
};

/// Registers a real local-FS "root" volume so `resolve("root", …)` finds a parent for the
/// on-demand `ArchiveVolume`. Idempotent; the shape `archive_extract_test.rs` uses.
fn ensure_root_volume() {
    get_volume_manager().register_if_absent("root", Arc::new(LocalPosixVolume::new("Test root", "/")));
}

fn write_bytes(dir: &TestDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

/// A zip with a file at the root, a directory, and a file inside it.
fn write_bundle(dir: &TestDir) -> PathBuf {
    write_bytes(
        dir,
        "bundle.zip",
        &build_zip(&[
            stored("readme.txt", b"hello".to_vec()),
            zip_dir("docs/"),
            stored("docs/notes.txt", b"line one\nline two\n".to_vec()),
        ]),
    )
}

fn inspect(path: &Path) -> FileRow {
    ensure_root_volume();
    inspect_path(
        path.to_str().unwrap(),
        &TextAsk::Window(WindowOpts {
            start_line: 1,
            max_lines: 200,
        }),
        &AtomicBool::new(false),
    )
}

/// Inspect with the extract step pointed at `extract_dir` under `cap`, so a test can see
/// the temp and shrink the cap.
fn inspect_extracting_to(path: &Path, extract_dir: &Path, cap: u64) -> FileRow {
    ensure_root_volume();
    let extract =
        |requested: &Path, volume_id: &str| extract_if_archive_inner_with(requested, volume_id, extract_dir, cap);
    inspect_path_with(
        path.to_str().unwrap(),
        &TextAsk::Window(WindowOpts {
            start_line: 1,
            max_lines: 200,
        }),
        &AtomicBool::new(false),
        &extract,
    )
}

fn file_of(row: &FileRow) -> &InspectedFile {
    match row {
        FileRow::Ok(file) => file,
        other => panic!("expected an ok row, got {other:?}"),
    }
}

fn archive_of(row: &FileRow) -> &ArchiveContent {
    match &file_of(row).content {
        Content::Archive(a) => a,
        other => panic!("expected an archive row, got {other:?}"),
    }
}

fn names(entries: &[ArchiveEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.name.as_str()).collect()
}

fn entries_in(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).collect()
}

// ── The archive itself ────────────────────────────────────────────────────────

#[test]
fn the_zip_path_itself_lists_the_root_dirs_first_with_sizes_and_dates_spoken() {
    let dir = TestDir::new("inspect_zip_root");
    let zip = write_bundle(&dir);

    let row = inspect(&zip);
    let file = file_of(&row);
    // The row's metadata is the `.zip` file's own: size from the disk, `zip` extension,
    // the extension's MIME beside the kind the bytes confirmed.
    assert_eq!(file.name, "bundle.zip");
    assert_eq!(file.extension.as_deref(), Some("zip"));
    assert_eq!(file.size_bytes, Some(std::fs::metadata(&zip).unwrap().len()));
    assert_eq!(file.mime.as_deref(), Some("application/zip"));

    let archive = archive_of(&row);
    assert_eq!(archive.format, "zip");
    assert_eq!(archive.inner, "", "the root");
    assert_eq!(
        names(&archive.entries),
        ["docs", "readme.txt"],
        "dirs first, as the pane lists"
    );
    assert_eq!((archive.total, archive.returned, archive.truncated), (2, 2, false));
    assert!(!archive.has_encrypted_entries);

    let docs = &archive.entries[0];
    assert!(docs.is_dir);
    assert_eq!(
        docs.size, None,
        "a directory has no size, and the row doesn't invent one"
    );
    let readme = &archive.entries[1];
    assert!(!readme.is_dir);
    assert_eq!((readme.size, readme.size_human.as_deref()), (Some(5), Some("5 B")));
    assert!(readme.modified.is_some(), "the zip records an mtime per entry");
    assert!(
        readme.modified_human.as_deref().is_some_and(|d| d.len() == 10),
        "spoken as YYYY-MM-DD: {:?}",
        readme.modified_human
    );
    assert!(!readme.encrypted);
}

#[test]
fn a_mislabeled_zip_falls_through_to_the_plain_pipeline() {
    let dir = TestDir::new("inspect_zip_mislabeled");
    let fake = write_bytes(&dir, "notes.zip", b"plain text, definitely not a zip\n");

    let row = inspect(&fake);
    assert!(
        matches!(&file_of(&row).content, Content::Text(_)),
        "the bytes decide, not the extension: {row:?}"
    );
    assert_eq!(
        file_of(&row).mime.as_deref(),
        Some("application/zip"),
        "the lying extension shows beside the kind"
    );
}

#[test]
fn a_root_listing_is_cut_at_the_entry_cap_and_says_so() {
    let dir = TestDir::new("inspect_zip_many");
    let fixtures: Vec<_> = (0..MAX_ARCHIVE_ENTRIES + 5)
        .map(|i| stored(format!("f-{i:03}.txt"), b"x".to_vec()))
        .collect();
    let zip = write_bytes(&dir, "many.zip", &build_zip(&fixtures));

    let archive = archive_of(&inspect(&zip)).clone();
    assert_eq!(archive.total, MAX_ARCHIVE_ENTRIES + 5);
    assert_eq!(archive.returned, MAX_ARCHIVE_ENTRIES);
    assert!(archive.truncated);
    assert_eq!(archive.entries.len(), MAX_ARCHIVE_ENTRIES);
}

// ── Inside the archive ────────────────────────────────────────────────────────

#[test]
fn an_inner_directory_lists_its_children_and_carries_no_size() {
    let dir = TestDir::new("inspect_zip_inner_dir");
    let zip = write_bundle(&dir);

    let row = inspect(&zip.join("docs"));
    let file = file_of(&row);
    assert_eq!(file.name, "docs");
    assert_eq!(file.extension, None);
    assert_eq!(
        file.size_bytes, None,
        "a directory inside an archive has no size; never a wrong zero"
    );
    assert_eq!(file.size_human, None);
    let archive = archive_of(&row);
    assert_eq!(archive.inner, "docs");
    assert_eq!(names(&archive.entries), ["notes.txt"]);
    assert_eq!(archive.total, 1);
}

#[test]
fn an_inner_text_file_reads_through_a_temp_that_is_gone_afterwards() {
    let dir = TestDir::new("inspect_zip_inner_file");
    let zip = write_bundle(&dir);
    let extract_dir = TestDir::new("inspect_zip_extract");

    let row = inspect_extracting_to(&zip.join("docs/notes.txt"), &extract_dir, EXTRACT_CAP_BYTES);
    let file = file_of(&row);
    assert_eq!(file.name, "notes.txt");
    assert_eq!(file.extension.as_deref(), Some("txt"));
    assert_eq!(file.mime.as_deref(), Some("text/plain"));
    assert_eq!(
        (file.size_bytes, file.size_human.as_deref()),
        (Some(18), Some("18 B")),
        "the entry's uncompressed size, from the central directory"
    );
    assert!(file.modified.is_some());
    let Content::Text(text) = &file.content else {
        panic!("expected text, got {:?}", file.content);
    };
    assert_eq!(text.encoding, "UTF-8");
    assert_eq!(text.window.as_ref().unwrap().content, "line one\nline two\n");

    assert!(
        entries_in(&extract_dir).is_empty(),
        "the temp is removed once the row is shaped, found {:?}",
        entries_in(&extract_dir)
    );
}

#[test]
fn an_inner_file_over_the_extract_cap_is_too_large_to_extract_and_no_temp_is_made() {
    let dir = TestDir::new("inspect_zip_too_large");
    let zip = write_bytes(
        &dir,
        "big.zip",
        &build_zip(&[stored("data.bin", vec![0u8; 200 * 1024])]),
    );
    let extract_dir = TestDir::new("inspect_zip_extract_cap");

    let row = inspect_extracting_to(&zip.join("data.bin"), &extract_dir, 10);
    assert!(
        matches!(
            &row,
            FileRow::Unreadable {
                reason: UnreadableReason::TooLargeToExtract,
                ..
            }
        ),
        "got {row:?}"
    );
    assert!(
        entries_in(&extract_dir).is_empty(),
        "refused before any temp was created"
    );
}

#[test]
fn a_missing_inner_path_is_missing() {
    let dir = TestDir::new("inspect_zip_missing_inner");
    let zip = write_bundle(&dir);
    assert!(matches!(inspect(&zip.join("nope/x.txt")), FileRow::Missing { .. }));
}

#[test]
fn a_zip_inside_a_zip_reads_as_binary_not_as_a_second_archive() {
    let dir = TestDir::new("inspect_zip_nested");
    let inner_zip = build_zip(&[stored("deep.txt", b"deep".to_vec())]);
    let outer = write_bytes(&dir, "outer.zip", &build_zip(&[stored("inner.zip", inner_zip)]));
    let extract_dir = TestDir::new("inspect_zip_nested_extract");

    let row = inspect_extracting_to(&outer.join("inner.zip"), &extract_dir, EXTRACT_CAP_BYTES);
    // Nested archives aren't browsable in the pane either (the boundary is the leftmost
    // archive component); the honest kind for the extracted bytes is `binary`.
    assert!(matches!(file_of(&row).content, Content::Binary {}), "got {row:?}");
    assert!(entries_in(&extract_dir).is_empty());
}

#[test]
fn the_cleanup_guard_removes_the_temp_dir_when_dropped() {
    let extract_dir = TestDir::new("inspect_zip_guard");
    let sub = extract_dir.join(".cmdr-viewer-guard");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(sub.join("f"), b"x").unwrap();

    drop(TempCleanup(sub.clone()));
    assert!(!sub.exists(), "an early return or a panic can't leak the temp");
}

// ── Encryption ────────────────────────────────────────────────────────────────

#[test]
fn an_encrypted_entry_is_flagged_in_the_listing_and_refused_before_extraction() {
    let dir = TestDir::new("inspect_zip_encrypted_entry");
    let zip = write_bytes(
        &dir,
        "locked.zip",
        &build_zipcrypto_zip(
            &[
                encrypted_entry("secret.txt", b"top secret".to_vec()),
                plain_entry("open.txt", b"public".to_vec()),
            ],
            "hunter2",
        ),
    );
    let extract_dir = TestDir::new("inspect_zip_encrypted_extract");

    let archive = archive_of(&inspect(&zip)).clone();
    assert!(archive.has_encrypted_entries);
    let by_name = |name: &str| archive.entries.iter().find(|e| e.name == name).unwrap();
    assert!(by_name("secret.txt").encrypted);
    assert!(!by_name("open.txt").encrypted);

    // The tool has no password path, so the entry is refused typed, with no temp written.
    let row = inspect_extracting_to(&zip.join("secret.txt"), &extract_dir, EXTRACT_CAP_BYTES);
    assert!(
        matches!(
            &row,
            FileRow::Unreadable {
                reason: UnreadableReason::Encrypted,
                ..
            }
        ),
        "got {row:?}"
    );
    assert!(entries_in(&extract_dir).is_empty());

    // The plain sibling still reads.
    let row = inspect_extracting_to(&zip.join("open.txt"), &extract_dir, EXTRACT_CAP_BYTES);
    assert!(matches!(file_of(&row).content, Content::Text(_)), "got {row:?}");
}

#[test]
fn a_header_encrypted_7z_is_unreadable_encrypted_and_a_content_encrypted_one_lists() {
    let dir = TestDir::new("inspect_7z");
    let files: &[(&str, &[u8])] = &[("a.txt", b"hi"), ("b.txt", b"there")];

    // `-mhe=on`: the metadata itself is encrypted, so even the listing needs a password.
    let sealed = write_bytes(&dir, "sealed.7z", &build_encrypted_7z(files, "pw", true));
    let row = inspect(&sealed);
    assert!(
        matches!(
            &row,
            FileRow::Unreadable {
                reason: UnreadableReason::Encrypted,
                ..
            }
        ),
        "got {row:?}"
    );

    // `-mhe=off`: names and sizes are plaintext; every entry is flagged.
    let open = write_bytes(&dir, "open.7z", &build_encrypted_7z(files, "pw", false));
    let archive = archive_of(&inspect(&open)).clone();
    assert_eq!(archive.format, "7z");
    assert!(archive.has_encrypted_entries);
    assert_eq!(names(&archive.entries), ["a.txt", "b.txt"]);
    assert!(archive.entries.iter().all(|e| e.encrypted));
}

// ── Not local ─────────────────────────────────────────────────────────────────

#[test]
fn a_path_on_a_volume_without_local_fs_access_is_unsupported() {
    // A registered volume whose paths `std::fs` can't open (an MTP device would be the
    // real case); `is_virtual_path` alone doesn't catch it because the path has no scheme.
    let root = "/inspect-file-remote-volume";
    get_volume_manager().register_if_absent(
        "inspect-remote",
        Arc::new(InMemoryVolume::new("Remote device").with_root(root)),
    );
    let row = inspect(Path::new(&format!("{root}/DCIM/photo.jpg")));
    assert!(matches!(row, FileRow::UnsupportedVolume { .. }), "got {row:?}");
}

// ── Through the call ──────────────────────────────────────────────────────────

#[tokio::test]
async fn archive_rows_are_text_only_and_join_by_path_with_plain_rows() {
    ensure_root_volume();
    let dir = TestDir::new("inspect_zip_call");
    let zip = write_bundle(&dir);
    let plain = write_bytes(&dir, "plain.txt", b"a plain file\n");
    let paths = vec![
        zip.to_string_lossy().into_owned(),
        zip.join("docs").to_string_lossy().into_owned(),
        zip.join("docs/notes.txt").to_string_lossy().into_owned(),
        plain.to_string_lossy().into_owned(),
    ];

    let result = run(&json!({ "paths": paths })).await.unwrap();
    let json = serde_json::to_value(&result).unwrap();
    assert_text_only(&json, "result");
    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 4);
    assert_eq!(files[0]["content"]["kind"], "archive");
    assert_eq!(files[0]["content"]["entries"][0]["name"], "docs");
    assert!(
        files[0]["content"]["entries"][0].get("encrypted").is_none(),
        "false flags stay off the wire"
    );
    assert_eq!(files[1]["content"]["inner"], "docs");
    assert!(
        files[1].get("sizeBytes").is_none(),
        "no size for a directory inside an archive"
    );
    assert_eq!(files[2]["content"]["kind"], "text");
    assert_eq!(files[2]["content"]["window"]["content"], "line one\nline two\n");
    assert_eq!(files[3]["content"]["kind"], "text");
}

#[tokio::test]
async fn find_reaches_inside_an_archive() {
    ensure_root_volume();
    let dir = TestDir::new("inspect_zip_find");
    let zip = write_bundle(&dir);
    let result = run(&json!({
        "paths": [zip.join("docs/notes.txt").to_string_lossy()],
        "find": { "query": "two" }
    }))
    .await
    .unwrap();
    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(json["files"][0]["content"]["find"]["totalMatches"], 1);
    assert_eq!(json["files"][0]["content"]["find"]["lines"][0]["line"], 2);
}
