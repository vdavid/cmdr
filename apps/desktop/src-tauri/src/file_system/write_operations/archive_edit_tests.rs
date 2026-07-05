//! Headless tests for the archive-edit driver: it runs the mutator as a managed
//! op and emits the right terminal events, with no Tauri runtime (a
//! `CollectorEventSink` captures events).

use std::io::{Read, Write};
use std::sync::Arc;
use std::time::Duration;

use zip::write::SimpleFileOptions;
use zip::{ZipArchive, ZipWriter};

use super::super::types::CollectorEventSink;
use super::*;
use crate::file_system::volume::backends::archive::mutator::{AddEntry, AddSource};
use crate::ignore_poison::IgnorePoison;

/// Builds a one-entry zip at `path`.
fn write_simple_zip(path: &Path, entry: &str, content: &[u8]) {
    let file = std::fs::File::create(path).expect("create zip");
    let mut writer = ZipWriter::new(file);
    writer.start_file(entry, SimpleFileOptions::default()).expect("start entry");
    writer.write_all(content).expect("write entry");
    writer.finish().expect("finish zip");
}

/// Reads one entry's decompressed bytes back, or `None` if absent.
fn read_entry(path: &Path, name: &str) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
    let mut entry = archive.by_name(name).ok()?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// Polls until `predicate` holds or a bounded timeout elapses, yielding to the
/// runtime so the spawned op makes progress.
async fn wait_until(mut predicate: impl FnMut() -> bool) -> bool {
    for _ in 0..3000 {
        if predicate() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(1)).await;
    }
    false
}

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

    let complete = events.complete.lock_ignore_poison();
    assert_eq!(complete.len(), 1);
    assert_eq!(complete[0].operation_type, WriteOperationType::ArchiveEdit);
    // No error, and settle fired for the same op.
    assert!(events.errors.lock_ignore_poison().is_empty(), "no write-error on success");
    assert!(
        wait_until(|| !events.settled.lock_ignore_poison().is_empty()).await,
        "write-settled should fire"
    );
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
async fn copy_into_adds_a_local_directory_tree_and_skips_conflicts() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    use crate::file_system::write_operations::route_archive_copy_into;

    let tmp = tempfile::tempdir().expect("tempdir");
    // The archive already holds `payload/existing.txt`, so a Skip-policy copy of a
    // colliding file leaves it untouched while adding the new ones.
    let archive = tmp.path().join("a.zip");
    {
        let file = std::fs::File::create(&archive).expect("create zip");
        let mut writer = ZipWriter::new(file);
        writer.start_file("payload/existing.txt", SimpleFileOptions::default()).expect("start");
        writer.write_all(b"OLD").expect("write");
        writer.finish().expect("finish");
    }

    // A local source tree: payload/{existing.txt, fresh.txt, sub/deep.txt}.
    let src_root = tmp.path().join("src");
    std::fs::create_dir_all(src_root.join("payload/sub")).expect("mkdir src");
    std::fs::write(src_root.join("payload/existing.txt"), b"NEW").expect("w1");
    std::fs::write(src_root.join("payload/fresh.txt"), b"fresh").expect("w2");
    std::fs::write(src_root.join("payload/sub/deep.txt"), b"deep").expect("w3");

    // A local-FS source volume rooted at src_root (drives `local_path()`).
    let source_volume: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", src_root.clone()));

    let events = Arc::new(CollectorEventSink::new());
    // Destination is the archive ROOT, so the source dir `payload` lands as `payload/`.
    let dest = archive.clone();
    route_archive_copy_into(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("payload")],
        dest,
        "root".to_string(),
        ConflictResolution::Skip,
        0,
        false,
    )
    .await
    .expect("start copy-into");

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "copy-into should complete"
    );
    // The colliding file kept its OLD bytes (Skip), the new files were added.
    assert_eq!(read_entry(&archive, "payload/existing.txt").as_deref(), Some(b"OLD".as_slice()));
    assert_eq!(read_entry(&archive, "payload/fresh.txt").as_deref(), Some(b"fresh".as_slice()));
    assert_eq!(read_entry(&archive, "payload/sub/deep.txt").as_deref(), Some(b"deep".as_slice()));
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
    };

    archive_edit_start(Arc::clone(&events) as Arc<dyn OperationEventSink>, request, 0)
        .await
        .expect("start archive edit");

    assert!(
        wait_until(|| !events.errors.lock_ignore_poison().is_empty()).await,
        "a missing archive should surface a write-error"
    );
    assert!(events.complete.lock_ignore_poison().is_empty(), "no write-complete on failure");
    // Settle still fires (torn-down cleanly, no hang).
    assert!(
        wait_until(|| !events.settled.lock_ignore_poison().is_empty()).await,
        "write-settled fires even on the error path"
    );
}
