//! A `preview_id` is not authorization to act on a path set.
//!
//! The frontend hands an operation a `preview_id` and a source list. The id
//! proves a scan once happened; it says nothing about WHICH selection was
//! scanned. The local delete walker takes the cached result wholesale and
//! iterates `scan_result.files` — it never looks at its own `sources` again —
//! so an id pointing at a preview of a different tree deletes that other tree.
//! Delete has no rollback, so this is the worst place in the app for a believed
//! fact, and these tests are its fence.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::super::state::{CachedScanResult, FileInfo, WriteOperationState, insert_scan_result};
use super::super::test_support::TestOperationGuard;
use super::super::types::{CollectorEventSink, WriteOperationConfig};
use super::walker::{delete_files_with_progress_inner, delete_volume_files_with_progress_inner};
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{CopyScanResult, InMemoryVolume, Volume};
use crate::test_support::TestDir;

fn unique(tag: &str) -> String {
    format!(
        "preview-binding-{tag}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos()
    )
}

/// Reads a file back off the volume, so the assertion checks CONTENT survival
/// rather than just a name. `None` when the path is gone.
async fn read_back(volume: &InMemoryVolume, path: &str) -> Option<Vec<u8>> {
    let mut stream = volume.open_read_stream(std::path::Path::new(path)).await.ok()?;
    let mut collected = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        collected.extend_from_slice(&chunk);
    }
    Some(collected)
}

fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(10)))
}

/// Builds the shape `run_scan_preview` (the LOCAL `std::fs` walk) caches: a
/// per-file `FileInfo` list plus one `CopyScanResult` per top-level source.
fn local_preview_of(source_dir: &std::path::Path, files: &[PathBuf]) -> CachedScanResult {
    let total: u64 = files
        .iter()
        .map(|f| fs::metadata(f).map(|m| m.len()).unwrap_or(0))
        .sum();
    CachedScanResult {
        sources: vec![source_dir.to_path_buf()],
        files: files
            .iter()
            .map(|f| {
                let metadata = fs::symlink_metadata(f).expect("scanned file exists");
                FileInfo::new(
                    f.clone(),
                    source_dir.parent().expect("source has a parent").to_path_buf(),
                    &metadata,
                )
            })
            .collect(),
        dirs: vec![source_dir.to_path_buf()],
        file_count: files.len(),
        total_bytes: total,
        dedup_bytes: total,
        per_path: vec![(
            source_dir.to_path_buf(),
            CopyScanResult {
                file_count: files.len(),
                dir_count: 0,
                total_bytes: total,
                dedup_bytes: total,
                top_level_is_directory: true,
            },
        )],
        estimated_compressed_bytes: None,
        inserted_at: Instant::now(),
    }
}

/// **The destructive one.** A delete asked to remove `/keep_me` while the cache
/// holds a preview of `/delete_me` must not touch `/delete_me`.
///
/// The local walker's cache-hit branch takes the whole cached `ScanResult` and
/// then only ever iterates `scan_result.files`, so an unbound cache turns the
/// user's "delete B" into "delete A" with no prompt, no progress line naming A,
/// and nothing to roll back. Pre-fix this fails by `delete_me` disappearing.
#[test]
fn a_local_delete_never_acts_on_a_preview_of_a_different_selection() {
    let root = TestDir::new(&unique("local-delete"));

    let untouched = root.join("delete_me");
    fs::create_dir_all(&untouched).expect("create untouched dir");
    let untouched_file = untouched.join("precious.txt");
    fs::write(&untouched_file, b"the user did not ask for this").expect("write untouched file");

    let requested = root.join("keep_me");
    fs::create_dir_all(&requested).expect("create requested dir");
    let requested_file = requested.join("chosen.txt");
    fs::write(&requested_file, b"this is what was selected").expect("write requested file");

    // A completed preview of the OTHER folder, exactly as the local scan
    // preview would have cached it if the user had scanned that one.
    let preview_id = unique("stale-preview");
    insert_scan_result(
        preview_id.clone(),
        local_preview_of(&untouched, &[untouched_file.clone()]),
    );

    let op_id = unique("op");
    let op = TestOperationGuard::register_as(op_id.clone(), make_state());
    let sink = CollectorEventSink::new();
    let config = WriteOperationConfig {
        preview_id: Some(preview_id),
        ..WriteOperationConfig::default()
    };

    let result = delete_files_with_progress_inner(&sink, &op_id, op.state(), &[requested.clone()], &config);

    assert!(
        result.is_ok(),
        "the delete of the requested folder must succeed: {result:?}"
    );
    assert!(
        untouched_file.exists(),
        "a preview of another folder must never authorize deleting it"
    );
    assert!(
        !requested_file.exists(),
        "the folder the user actually selected must be deleted"
    );
}

/// The same binding, one layer up: the requested source is a SUBSET of what the
/// preview walked. The cached file list is a superset, so believing it deletes
/// a sibling the user deselected between the scan and the confirm.
#[test]
fn a_local_delete_never_acts_on_a_preview_covering_more_than_was_asked_for() {
    let root = TestDir::new(&unique("local-delete-subset"));

    let requested = root.join("selected");
    fs::create_dir_all(&requested).expect("create selected dir");
    let requested_file = requested.join("a.txt");
    fs::write(&requested_file, b"selected").expect("write selected file");

    let deselected = root.join("deselected");
    fs::create_dir_all(&deselected).expect("create deselected dir");
    let deselected_file = deselected.join("b.txt");
    fs::write(&deselected_file, b"deselected").expect("write deselected file");

    // The preview walked BOTH folders; the operation asks for one.
    let preview_id = unique("superset-preview");
    let mut cached = local_preview_of(&requested, &[requested_file.clone()]);
    cached.sources.push(deselected.clone());
    let deselected_metadata = fs::symlink_metadata(&deselected_file).expect("deselected file exists");
    cached.files.push(FileInfo::new(
        deselected_file.clone(),
        root.to_path_buf(),
        &deselected_metadata,
    ));
    cached.file_count = 2;
    insert_scan_result(preview_id.clone(), cached);

    let op_id = unique("op-subset");
    let op = TestOperationGuard::register_as(op_id.clone(), make_state());
    let sink = CollectorEventSink::new();
    let config = WriteOperationConfig {
        preview_id: Some(preview_id),
        ..WriteOperationConfig::default()
    };

    let result = delete_files_with_progress_inner(&sink, &op_id, op.state(), &[requested.clone()], &config);

    assert!(result.is_ok(), "the delete must succeed: {result:?}");
    assert!(
        deselected_file.exists(),
        "a source dropped from the selection after the scan must survive"
    );
    assert!(!requested_file.exists(), "the selected source must be deleted");
}

/// Regression fence, not the red: the VOLUME delete walker iterates
/// `for source in sources` and falls through to a fresh recursion when
/// `by_path` has no entry for a source, so it was already bound to its request.
/// This pins that it stays that way, and that the binding didn't break the
/// fall-through.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_volume_delete_stays_bound_to_its_own_sources() {
    let volume_id = unique("vol");
    let volume = Arc::new(InMemoryVolume::new("binding-vol"));
    volume
        .create_file(std::path::Path::new("/requested.txt"), b"requested")
        .await
        .expect("seed requested file");
    volume
        .create_file(std::path::Path::new("/foreign.txt"), b"foreign")
        .await
        .expect("seed foreign file");
    get_volume_manager().register(&volume_id, volume.clone() as Arc<dyn Volume>);

    // A volume-batch-shaped preview of the file the operation was NOT asked
    // to delete.
    let preview_id = unique("vol-preview");
    insert_scan_result(
        preview_id.clone(),
        CachedScanResult {
            sources: vec![PathBuf::from("/foreign.txt")],
            files: Vec::new(),
            dirs: Vec::new(),
            file_count: 1,
            total_bytes: 7,
            dedup_bytes: 7,
            per_path: vec![(
                PathBuf::from("/foreign.txt"),
                CopyScanResult {
                    file_count: 1,
                    dir_count: 0,
                    total_bytes: 7,
                    dedup_bytes: 7,
                    top_level_is_directory: false,
                },
            )],
            estimated_compressed_bytes: None,
            inserted_at: Instant::now(),
        },
    );

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = WriteOperationConfig {
        preview_id: Some(preview_id),
        ..WriteOperationConfig::default()
    };

    let result = delete_volume_files_with_progress_inner(
        volume.clone() as Arc<dyn Volume>,
        &volume_id,
        events.as_ref(),
        &unique("vol-op"),
        &state,
        &[PathBuf::from("/requested.txt")],
        &config,
    )
    .await;

    assert!(result.is_ok(), "the volume delete must succeed: {result:?}");
    assert_eq!(
        read_back(&volume, "/foreign.txt").await.as_deref(),
        Some(&b"foreign"[..]),
        "a preview of another path must never authorize deleting it"
    );
    assert_eq!(
        read_back(&volume, "/requested.txt").await,
        None,
        "the requested path must be deleted"
    );

    get_volume_manager().unregister(&volume_id);
}
