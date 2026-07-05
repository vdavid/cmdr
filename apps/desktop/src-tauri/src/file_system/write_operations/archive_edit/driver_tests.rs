//! Headless tests for the generic archive-edit driver (`archive_edit_start`) and
//! the in-archive delete route: they run the mutator as a managed op and emit the
//! right terminal events, with no Tauri runtime (a `CollectorEventSink` captures
//! events).

use super::test_support::*;
use super::*;
use crate::file_system::volume::backends::archive::mutator::{AddEntry, AddSource};

#[tokio::test]
async fn a_successful_edit_rewrites_the_archive_and_emits_complete_then_settled() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("a.zip");
    write_simple_zip(&path, "keep.txt", b"keep");

    let events = Arc::new(CollectorEventSink::new());
    let request = ArchiveEditRequest {
        archive_path: path.clone(),
        parent_volume_id: "root".to_string(),
        changeset: Changeset {
            adds: vec![AddEntry {
                inner_path: "added.txt".to_string(),
                source: AddSource::Bytes(b"new bytes".to_vec()),
            }],
            ..Default::default()
        },
        summary: OperationSummaryText::default(),
        move_sources_to_delete: vec![],
        skipped_count: 0,
    };

    let start = archive_edit_start(Arc::clone(&events) as Arc<dyn OperationEventSink>, request, 0)
        .await
        .expect("start archive edit");
    assert_eq!(start.operation_type, WriteOperationType::ArchiveEdit);

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "a write-complete should fire"
    );

    // The archive was actually rewritten.
    assert_eq!(read_entry(&path, "keep.txt").as_deref(), Some(b"keep".as_slice()));
    assert_eq!(read_entry(&path, "added.txt").as_deref(), Some(b"new bytes".as_slice()));

    {
        let complete = events.complete.lock_ignore_poison();
        assert_eq!(complete.len(), 1);
        assert_eq!(complete[0].operation_type, WriteOperationType::ArchiveEdit);
    }
    // No error, and settle fired for the same op.
    assert!(
        events.errors.lock_ignore_poison().is_empty(),
        "no write-error on success"
    );
    assert!(
        wait_until(|| !events.settled.lock_ignore_poison().is_empty()).await,
        "write-settled should fire"
    );
    // A `root`-parent edit settles with no volume id (`None`, not `"root"`).
    assert_eq!(events.settled.lock_ignore_poison()[0].volume_id, None);
}

#[tokio::test]
async fn route_archive_delete_removes_entries_and_completes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("a.zip");
    {
        let file = std::fs::File::create(&path).expect("create zip");
        let mut writer = ZipWriter::new(file);
        for name in ["keep.txt", "drop.txt"] {
            writer.start_file(name, SimpleFileOptions::default()).expect("start");
            writer.write_all(name.as_bytes()).expect("write");
        }
        writer.finish().expect("finish");
    }

    let events = Arc::new(CollectorEventSink::new());
    // The FE sends full paths inside the archive.
    let sources = vec![path.join("drop.txt")];
    route_archive_delete(Arc::clone(&events) as Arc<dyn OperationEventSink>, &sources, "root", 0)
        .await
        .expect("start delete");

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "the delete should complete"
    );
    assert!(read_entry(&path, "drop.txt").is_none(), "the entry was removed");
    assert_eq!(read_entry(&path, "keep.txt").as_deref(), Some(b"keep.txt".as_slice()));
}

#[tokio::test]
async fn route_archive_delete_reports_the_deleted_count_not_the_retained_count() {
    // Deleting ONE entry from a 3-entry zip must report `files_processed == 1`
    // (the number DELETED), not the retained-entry count (2). Pins the archive
    // edit's `files_processed` semantics against the "Delete complete: 2 files" bug.
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("a.zip");
    write_multi_zip(
        &path,
        &[("drop.txt", b"drop"), ("keep1.txt", b"one"), ("keep2.txt", b"two")],
    );

    let events = Arc::new(CollectorEventSink::new());
    route_archive_delete(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        &[path.join("drop.txt")],
        "root",
        0,
    )
    .await
    .expect("start delete");

    assert!(wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await);
    assert!(read_entry(&path, "drop.txt").is_none(), "the entry was removed");
    let complete = events.complete.lock_ignore_poison();
    assert_eq!(
        complete[0].files_processed, 1,
        "a one-file delete must report 1 processed file, not the retained-entry count"
    );
}

#[tokio::test]
async fn a_missing_archive_emits_a_write_error_not_a_panic() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("ghost.zip"); // never created

    let events = Arc::new(CollectorEventSink::new());
    let request = ArchiveEditRequest {
        archive_path: path.clone(),
        parent_volume_id: "root".to_string(),
        changeset: Changeset {
            mkdirs: vec!["dir".to_string()],
            ..Default::default()
        },
        summary: OperationSummaryText::default(),
        move_sources_to_delete: vec![],
        skipped_count: 0,
    };

    archive_edit_start(Arc::clone(&events) as Arc<dyn OperationEventSink>, request, 0)
        .await
        .expect("start archive edit");

    assert!(
        wait_until(|| !events.errors.lock_ignore_poison().is_empty()).await,
        "a missing archive should surface a write-error"
    );
    assert!(
        events.complete.lock_ignore_poison().is_empty(),
        "no write-complete on failure"
    );
    // Settle still fires (torn-down cleanly, no hang).
    assert!(
        wait_until(|| !events.settled.lock_ignore_poison().is_empty()).await,
        "write-settled fires even on the error path"
    );
}
