//! Unit tests for `copy_files_with_progress_inner` (local-FS copy).
//!
//! Drives the sink-based inner function directly with a `CollectorEventSink`
//! against a real tempdir, the same shape `volume/copy_tests.rs` uses against
//! `InMemoryVolume`.

use super::*;
use crate::file_system::write_operations::types::CollectorEventSink;

fn make_state(progress_interval_ms: u64) -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(progress_interval_ms)))
}

/// Local-FS copy of a single file must emit at least one `Copying`-phase
/// progress event with `files_done == N` (the full count). Without a per-
/// file milestone emit in the sync driver's `Transferred` arm, the
/// throttled emit inside `copy_single_item` is suppressed when the chunked
/// progress callback (or an instant clonefile) just reset the throttle —
/// for single-file ops the FE's files-axis never crosses `0/1` before the
/// dialog closes on the complete event.
///
/// Uses `progress_interval_ms: 200` (production default) to keep the
/// throttle window active. Pre-fix the test reliably saw zero Copying
/// events with `files_done = 1`; post-fix the driver milestone fires
/// unconditionally so the assertion holds regardless of throttle timing.
#[test]
fn local_copy_single_file_reaches_files_done_n() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("file.bin");
    // 1 MB payload large enough to surface the throttle interaction on the
    // chunked-copy path; APFS clonefile completes instantly without firing
    // chunked progress, which is fine — the driver milestone still has to
    // land for the files-axis to cross `0/1`.
    fs::write(&src_file, vec![0u8; 1_048_576]).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig::default();

    let result = copy_files_with_progress_inner(
        &*events,
        "op-local-copy-files-n",
        &state,
        std::slice::from_ref(&src_file),
        &dst_dir,
        &config,
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let progress = events.progress.lock().unwrap();
    let copying: Vec<_> = progress
        .iter()
        .filter(|p| p.phase == WriteOperationPhase::Copying)
        .collect();
    let saw_files_done_n = copying.iter().any(|p| p.files_done == 1);
    assert!(
        saw_files_done_n,
        "local-FS copy: expected at least one Copying event with files_done = 1, got {:?}",
        copying.iter().map(|e| (e.files_done, e.bytes_done)).collect::<Vec<_>>(),
    );

    // Completion event accounts for the file.
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete[0].files_processed, 1);
    assert_eq!(complete[0].bytes_processed, 1_048_576);
}

/// A local-FS copy must emit a `Flushing`-phase progress event before the
/// `write-complete` fires. This is the user-visible "Writing the last piece…"
/// state: on slow media the end-of-op `fdatasync` over the created
/// destinations is a real multi-second pause, and the bar must not sit frozen
/// at 100% pretending the work is done. The event is the observable proxy for
/// the durability contract (the fsync itself isn't power-loss-testable in a
/// unit test).
#[test]
fn local_copy_emits_flushing_phase_before_complete() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("file.bin");
    fs::write(&src_file, vec![0u8; 4096]).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig::default();

    let result = copy_files_with_progress_inner(
        &*events,
        "op-local-copy-flushing",
        &state,
        std::slice::from_ref(&src_file),
        &dst_dir,
        &config,
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let progress = events.progress.lock().unwrap();
    let saw_flushing = progress.iter().any(|p| p.phase == WriteOperationPhase::Flushing);
    assert!(
        saw_flushing,
        "local-FS copy: expected a Flushing-phase progress event, got phases {:?}",
        progress.iter().map(|p| p.phase).collect::<Vec<_>>(),
    );

    // The flush pass made the created destination durable; we can read it back.
    let dst_file = dst_dir.join("file.bin");
    assert!(dst_file.exists(), "destination should hold the copied file");
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1, "exactly one write-complete");
}

/// Copying an EMPTY directory must create it at the destination. The copy
/// loop iterates `scan_result.files` only and creates directories lazily as
/// file parents, so a source with zero files used to complete "successfully"
/// while creating nothing — the empty dir silently never arrived.
#[test]
fn copy_creates_empty_directory_at_destination() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(src_dir.join("empty-dir")).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig::default();

    let source = src_dir.join("empty-dir");
    let result = copy_files_with_progress_inner(
        &*events,
        "op-copy-empty-dir",
        &state,
        std::slice::from_ref(&source),
        &dst_dir,
        &config,
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
    assert!(
        dst_dir.join("empty-dir").is_dir(),
        "the empty directory must exist at the destination"
    );
}

/// Empty directories NESTED inside a populated source must land too: the file
/// loop creates only ancestors of files, so `tree/sub-empty` (no files
/// anywhere under it) needs the explicit scanned-dirs pass.
#[test]
fn copy_creates_nested_empty_directories() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(src_dir.join("tree/populated")).unwrap();
    fs::create_dir_all(src_dir.join("tree/sub-empty/deeper-empty")).unwrap();
    fs::write(src_dir.join("tree/populated/file.txt"), b"content").unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig::default();

    let source = src_dir.join("tree");
    let result = copy_files_with_progress_inner(
        &*events,
        "op-copy-nested-empty-dirs",
        &state,
        std::slice::from_ref(&source),
        &dst_dir,
        &config,
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
    assert!(
        dst_dir.join("tree/populated/file.txt").is_file(),
        "the regular file must arrive"
    );
    assert!(
        dst_dir.join("tree/sub-empty/deeper-empty").is_dir(),
        "nested empty directories must exist at the destination"
    );
}

/// An empty source dir whose destination already holds a same-named FILE must
/// not destroy that file (folders merge; a type clash on an empty dir is left
/// alone rather than silently replacing user data).
#[test]
fn copy_empty_directory_does_not_clobber_same_named_dest_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(src_dir.join("clash")).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();
    fs::write(dst_dir.join("clash"), b"existing user data").unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(200);
    let config = WriteOperationConfig::default();

    let source = src_dir.join("clash");
    let result = copy_files_with_progress_inner(
        &*events,
        "op-copy-empty-dir-clash",
        &state,
        std::slice::from_ref(&source),
        &dst_dir,
        &config,
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);
    let dest = dst_dir.join("clash");
    assert!(dest.is_file(), "the existing dest file must survive");
    assert_eq!(fs::read(&dest).unwrap(), b"existing user data");
}

// ============================================================================
// The preview cache is bound to the operation's own sources
// ============================================================================

/// A local copy asked to copy `selected/` while the cache holds a preview of
/// `other/` must copy `selected/`, not `other/`.
///
/// The file LIST comes from the cache, but `mod.rs` re-reads `sources`
/// afterwards for the bulk-skip set, so the two disagree on a mismatched
/// preview and the copy silently writes the wrong tree to the destination.
/// Nothing is lost here (copy creates), but the user gets files they never
/// asked for and not the ones they did.
#[test]
fn a_local_copy_never_acts_on_a_preview_of_a_different_selection() {
    use crate::file_system::volume::CopyScanResult;
    use crate::file_system::write_operations::state::{CachedScanResult, FileInfo, insert_scan_result};

    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).expect("create src");
    fs::create_dir_all(&dst_dir).expect("create dst");

    let selected = src_dir.join("selected.bin");
    fs::write(&selected, b"the file the user picked").expect("write selected");
    let other = src_dir.join("other.bin");
    fs::write(&other, b"a file from an earlier scan").expect("write other");

    // A completed local preview of the OTHER file.
    let preview_id = "copy-binding-foreign-preview".to_string();
    let other_metadata = fs::symlink_metadata(&other).expect("other exists");
    insert_scan_result(
        preview_id.clone(),
        CachedScanResult::from_local_walk(
            vec![other.clone()],
            vec![FileInfo::new(other.clone(), src_dir.clone(), &other_metadata)],
            Vec::new(),
            other_metadata.len(),
            other_metadata.len(),
            vec![(
                other.clone(),
                CopyScanResult {
                    file_count: 1,
                    dir_count: 0,
                    total_bytes: other_metadata.len(),
                    dedup_bytes: other_metadata.len(),
                    top_level_is_directory: false,
                },
            )],
            None,
        ),
    );

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(0);
    let config = WriteOperationConfig {
        preview_id: Some(preview_id),
        ..WriteOperationConfig::default()
    };

    let result = copy_files_with_progress_inner(
        &*events,
        "op-copy-preview-binding",
        &state,
        std::slice::from_ref(&selected),
        &dst_dir,
        &config,
    );

    assert!(result.is_ok(), "the copy must succeed: {result:?}");
    assert!(
        dst_dir.join("selected.bin").exists(),
        "the file the user selected must reach the destination"
    );
    assert!(
        !dst_dir.join("other.bin").exists(),
        "a preview of another file must never authorize copying it"
    );
}

// ============================================================================
// Local many-small-files bench (what the staging rename costs)
// ============================================================================

/// Wall-clock for a local copy of many small files: the shape a per-file
/// rename shows up in, where per-file overhead dominates and byte throughput
/// doesn't.
///
/// `#[ignore]`d — it's a measurement, not an assertion. Run it with:
///
/// ```text
/// cd apps/desktop/src-tauri && cargo test --release --lib \
///   local_copy_bench_many_small_files -- --ignored --nocapture --test-threads=1
/// ```
#[test]
#[ignore = "benchmark: run on demand with --ignored --nocapture"]
#[allow(
    clippy::print_stdout,
    reason = "Bench prints its timing report by design (run with --nocapture)."
)]
fn local_copy_bench_many_small_files() {
    const FILE_COUNT: usize = 2_000;
    const FILE_BYTES: usize = 4 * 1024;
    const ROUNDS: usize = 5;

    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    fs::create_dir_all(&src_dir).unwrap();
    let payload = vec![0xAB_u8; FILE_BYTES];
    let sources: Vec<PathBuf> = (0..FILE_COUNT)
        .map(|i| {
            let p = src_dir.join(format!("f_{i:05}.bin"));
            fs::write(&p, &payload).unwrap();
            p
        })
        .collect();

    let mut millis: Vec<u128> = Vec::with_capacity(ROUNDS);
    for round in 0..ROUNDS {
        let dst_dir = tmp.path().join(format!("dst-{round}"));
        fs::create_dir_all(&dst_dir).unwrap();

        let events = Arc::new(CollectorEventSink::new());
        let state = make_state(1_000_000); // effectively no progress emits
        let config = WriteOperationConfig::default();

        let started = std::time::Instant::now();
        let result = copy_files_with_progress_inner(
            &*events,
            &format!("op-local-copy-bench-{round}"),
            &state,
            &sources,
            &dst_dir,
            &config,
        );
        let elapsed = started.elapsed();
        result.expect("bench copy must succeed");
        millis.push(elapsed.as_millis());
        println!("round {round}: {FILE_COUNT} × {FILE_BYTES} B in {elapsed:?}");
    }

    millis.sort_unstable();
    let median = millis[millis.len() / 2];
    println!(
        "local many-small-files copy: {FILE_COUNT} files, rounds(ms)={millis:?}, median={median} ms, per-file={:.3} ms",
        median as f64 / FILE_COUNT as f64
    );
}
