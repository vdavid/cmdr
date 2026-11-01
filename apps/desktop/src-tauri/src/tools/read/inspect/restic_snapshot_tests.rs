//! `inspect_file` inside a restic repo's `Browse snapshots` view: a text file in
//! a snapshot reads through the same bounded temp the viewer uses, the way a
//! file in a git snapshot does (`git_snapshot_tests.rs`). No new kind of
//! content leaves: a text window, as for any text file.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::*;
use crate::file_system::restic::test_repo::{SourceFile, backed_up_drive};
use crate::file_viewer::materialize::{PREVIEW_CAP_BYTES, extract_if_routed_with};
use crate::test_support::{TestDir, isolate_secrets};

const NOTES: SourceFile = SourceFile {
    path: "notes.txt",
    bytes: b"line one\nline two\n",
    mode: 0o644,
    modified: 1_600_000_000,
};

/// Inspect with the materialization pointed at `extract_dir`, so the test can see
/// whether the temp was cleaned up.
fn inspect_extracting_to(path: &Path, extract_dir: &Path) -> FileRow {
    let extract = |requested: &Path, volume_id: &str, _cancel: &AtomicBool| {
        extract_if_routed_with(requested, volume_id, extract_dir, PREVIEW_CAP_BYTES)
    };
    inspect_path_with(
        path.to_str().unwrap(),
        &TextAsk::Window(WindowOpts::default()),
        &AtomicBool::new(false),
        &extract,
    )
}

#[test]
fn a_text_file_in_a_snapshot_reads_through_a_temp_that_is_gone_afterwards() {
    let _secrets = isolate_secrets();
    let drive = tauri::async_runtime::block_on(async {
        let drive = backed_up_drive("inspect-restic", &[NOTES], &[]).await;
        drive.unlock().await;
        drive
    });
    let extract_dir = TestDir::new("inspect_restic_extract");

    let row = inspect_extracting_to(&drive.backed_up.join("notes.txt"), &extract_dir);
    let FileRow::Ok(file) = &row else {
        panic!("expected an ok row, got {row:?}");
    };
    assert_eq!(file.name, "notes.txt");
    let Content::Text(text) = &file.content else {
        panic!("expected text, got {:?}", file.content);
    };
    assert_eq!(text.window.as_ref().unwrap().content, "line one\nline two\n");

    let leftovers: Vec<_> = std::fs::read_dir(&*extract_dir)
        .expect("read the extract dir")
        .flatten()
        .map(|e| e.path())
        .collect();
    assert!(leftovers.is_empty(), "the temp is removed, found {leftovers:?}");

    // A folder in the snapshot is a folder, and a name that isn't in it is missing.
    assert!(matches!(
        inspect_extracting_to(&drive.backed_up, &extract_dir),
        FileRow::Folder { .. }
    ));
    assert!(matches!(
        inspect_extracting_to(&drive.backed_up.join("nope.txt"), &extract_dir),
        FileRow::Missing { .. }
    ));
}
