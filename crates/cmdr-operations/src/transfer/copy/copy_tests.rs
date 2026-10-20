//! Unit tests for `copy_files_with_progress_inner` (local-FS copy), plus what
//! the copy's rollback ledger records about each file it writes.
//!
//! Drives the sink-based inner function directly with a `CollectorEventSink`
//! against a real tempdir, the same shape `volume/copy_tests/` uses against
//! `InMemoryVolume`.

use super::*;
use crate::Operations;
use crate::event_sinks::CollectorEventSink;
use crate::ledger::{WrittenFile, WrittenIdentity};
use crate::testing::TestOperations;
use crate::transfer_tree::TreeLeaf;
use cmdr_operation_log::types::OperationId;

fn make_state(engine: &TestOperations, progress_interval_ms: u64) -> Arc<WriteOperationState> {
    Arc::new(engine.new_operation_state(Duration::from_millis(progress_interval_ms)))
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
    let engine = TestOperations::new();
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
    let state = make_state(&engine, 200);
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
    let engine = TestOperations::new();
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let src_file = src_dir.join("file.bin");
    fs::write(&src_file, vec![0u8; 4096]).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(&engine, 200);
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

/// A local copy fsyncs every directory that gained an entry, the parents of the
/// folders it created included, so a copied tree's entries are durable when it
/// reports complete. It deletes nothing, so a flush failure doesn't stop it: the
/// copy still completes, and its completion says it couldn't confirm the copies.
#[test]
fn local_copy_fsyncs_its_created_directories_and_completes_unconfirmed_on_a_flush_failure() {
    use crate::durability::test_hook;

    let engine = TestOperations::new();

    let dir = cmdr_fs::testing::TestDir::new("copy-durability");
    let src = dir.join("src");
    let dst = dir.join("dst");
    fs::create_dir_all(src.join("tree").join("sub")).unwrap();
    fs::write(src.join("tree").join("sub").join("leaf.txt"), b"leaf").unwrap();
    fs::create_dir_all(&dst).unwrap();
    let log = test_hook::record(|_| Some(Err(std::io::Error::from_raw_os_error(libc::EIO))));

    let events = Arc::new(CollectorEventSink::new());
    let result = copy_files_with_progress_inner(
        &*events,
        "op-local-copy-durability",
        &make_state(&engine, 200),
        &[src.join("tree")],
        &dst,
        &WriteOperationConfig::default(),
    );

    assert!(result.is_ok(), "a copy's flush failure doesn't stop it, got {result:?}");
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert!(
        complete[0].durability_unconfirmed.is_some(),
        "the completion carries the unconfirmed flush"
    );
    drop(complete);
    let mut synced = log.dirs();
    synced.sort();
    let mut expected = vec![dst.clone(), dst.join("tree"), dst.join("tree").join("sub")];
    expected.sort();
    assert_eq!(synced, expected, "each directory that gained an entry, once");
    assert!(dst.join("tree").join("sub").join("leaf.txt").is_file());
}

/// A flush that confirms everything adds nothing to the completion.
#[test]
fn a_local_copy_whose_flush_holds_completes_with_nothing_unconfirmed() {
    use crate::durability::test_hook;

    let engine = TestOperations::new();

    let dir = cmdr_fs::testing::TestDir::new("copy-durability-ok");
    let src = dir.join("src");
    let dst = dir.join("dst");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("a.txt"), b"a").unwrap();
    fs::create_dir_all(&dst).unwrap();
    let _log = test_hook::record(|_| None);

    let events = Arc::new(CollectorEventSink::new());
    let result = copy_files_with_progress_inner(
        &*events,
        "op-local-copy-durability-ok",
        &make_state(&engine, 200),
        &[src.join("a.txt")],
        &dst,
        &WriteOperationConfig::default(),
    );

    assert!(result.is_ok(), "{result:?}");
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(complete[0].durability_unconfirmed, None);
}

/// Copying an EMPTY directory must create it at the destination. The copy
/// loop iterates `scan_result.files` only and creates directories lazily as
/// file parents, so a source with zero files used to complete "successfully"
/// while creating nothing — the empty dir silently never arrived.
#[test]
fn copy_creates_empty_directory_at_destination() {
    let engine = TestOperations::new();
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(src_dir.join("empty-dir")).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(&engine, 200);
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
    let engine = TestOperations::new();
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(src_dir.join("tree/populated")).unwrap();
    fs::create_dir_all(src_dir.join("tree/sub-empty/deeper-empty")).unwrap();
    fs::write(src_dir.join("tree/populated/file.txt"), b"content").unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(&engine, 200);
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
    let engine = TestOperations::new();
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(src_dir.join("clash")).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();
    fs::write(dst_dir.join("clash"), b"existing user data").unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(&engine, 200);
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
    use crate::state::{CachedScanResult, FileInfo};
    use cmdr_fs::volume::CopyScanResult;

    let engine = TestOperations::new();

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
    engine.stores().previews().insert_scan_result(
        preview_id.clone(),
        CachedScanResult::from_local_walk(
            crate::types::ScanPreviewPurpose::Transfer,
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
                    top_level_modified_at: None,
                    top_level_created_at: None,
                },
            )],
            None,
        ),
    );

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(&engine, 0);
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
    let engine = TestOperations::new();
    const FILE_COUNT: usize = 2_000;
    const FILE_BYTES: usize = 4 * 1024;
    const ROUNDS: usize = 5;

    let tmp = tempfile::tempdir().expect("tempdir");
    // Measure with the persisted in-flight-temp ledger live, since production
    // rewrites it twice per file. Without this the bench would quietly skip it.
    let ledger = crate::in_flight_temps::Ledger::recording_in(tmp.path());
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
        // Effectively no progress emits.
        let state = Arc::new(
            engine
                .new_operation_state(Duration::from_millis(1_000_000))
                .with_in_flight_ledger(ledger.clone()),
        );
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
        // allowed-pluralize-noun: `FILE_COUNT` is a fixed benchmark constant well above 1, never a live count
        "local many-small-files copy: {FILE_COUNT} files, rounds(ms)={millis:?}, median={median} ms, per-file={:.3} ms",
        median as f64 / FILE_COUNT as f64
    );
}

/// A top-level source the user's "Skip all" covered leaves the file list before
/// the source-item tracker is even built, so nothing downstream speaks for it.
/// Before this it emitted nothing at all, and a caller recording a per-source
/// verdict waited on one that was never coming.
#[test]
fn a_bulk_skipped_source_reports_itself_as_skipped() {
    use crate::types::{ConflictResolution, SourceItemOutcome};

    let engine = TestOperations::new();

    let tmp = tempfile::tempdir().expect("tempdir");
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();

    let clashing = src_dir.join("collide.bin");
    let clean = src_dir.join("fine.bin");
    fs::write(&clashing, b"source").unwrap();
    fs::write(&clean, b"source").unwrap();
    // The pre-flight already found this name at the destination, which is what
    // lets the copy bulk-skip it rather than re-stat every file under it.
    fs::write(dst_dir.join("collide.bin"), b"dest").unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(&engine, 0);
    let config = WriteOperationConfig {
        conflict_resolution: ConflictResolution::Skip,
        pre_known_conflicts: vec!["collide.bin".to_string()],
        ..WriteOperationConfig::default()
    };

    let result = copy_files_with_progress_inner(
        &*events,
        "op-bulk-skip-outcome",
        &state,
        &[clashing.clone(), clean.clone()],
        &dst_dir,
        &config,
    );
    assert!(result.is_ok(), "expected Ok, got {:?}", result);

    let items = events.source_items_done.lock().unwrap();
    let verdict = |path: &Path| {
        items
            .iter()
            .find(|item| item.source_path == path.display().to_string())
            .map(|item| item.outcome)
    };
    assert_eq!(verdict(&clashing), Some(SourceItemOutcome::Skipped));
    assert_eq!(verdict(&clean), Some(SourceItemOutcome::Done));
    assert!(
        items.iter().all(|item| !item.source_removed),
        "a copy leaves every source where it was, skipped or not"
    );
    assert_eq!(fs::read(dst_dir.join("collide.bin")).unwrap(), b"dest", "the skip held");
}

// ── What the copy's rollback ledger records ────────────────────────────────

/// Copies one file through `copy_single_item` and hands back the ledger it
/// recorded into, so a test can ask what the copy claims to have written.
fn copy_one_file(engine: &TestOperations, source: &Path, dest_dir: &Path) -> CopyTransaction {
    copy_one_file_knowing(engine, source, dest_dir, &mut HashSet::new())
}

/// [`copy_one_file`] over a caller-held `created_dirs`, the operation's set of
/// directories it has already proven or made.
fn copy_one_file_knowing(
    engine: &TestOperations,
    source: &Path,
    dest_dir: &Path,
    created_dirs: &mut HashSet<PathBuf>,
) -> CopyTransaction {
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(engine, 200);
    let config = WriteOperationConfig::default();
    let mut transaction = CopyTransaction::new();
    let mut files_done = 0usize;
    let mut bytes_done = 0u64;
    let write_weight = fs::symlink_metadata(source).unwrap().len();
    let is_symlink = fs::symlink_metadata(source).unwrap().file_type().is_symlink();

    copy_single_item(
        source,
        dest_dir.join(source.file_name().unwrap()),
        dest_dir,
        None,
        is_symlink,
        write_weight,
        &mut files_done,
        &mut bytes_done,
        1,
        write_weight,
        &state,
        &*events,
        "op-ledger-identity",
        WriteOperationType::Copy,
        &Duration::from_millis(200),
        &config,
        &mut transaction,
        &mut ApplyToAll::default(),
        created_dirs,
        &mut HashMap::new(),
        &mut HashSet::new(),
        &mut HashSet::new(),
        None,
        &TreeLeaf::detached(),
    )
    .expect("the copy should land");

    transaction
}

/// A file landing DIRECTLY in the destination root leaves the root in
/// `created_dirs`, which is what lets the next file skip the parent checks. The
/// link-aware chain walk looks only BELOW the root, so without this a flat copy
/// of many files paid a stat of the same folder per file.
#[test]
fn a_file_landing_in_the_destination_root_proves_the_root_once() {
    let engine = TestOperations::new();
    let tmp = tempfile::tempdir().expect("tempdir");
    let dest_dir = tmp.path().join("dst");
    fs::create_dir_all(&dest_dir).unwrap();
    let source = tmp.path().join("a.txt");
    fs::write(&source, "A").unwrap();
    let mut created_dirs = HashSet::new();

    copy_one_file_knowing(&engine, &source, &dest_dir, &mut created_dirs);

    assert!(
        created_dirs.contains(&dest_dir),
        "the root must be known after its first file, got {created_dirs:?}"
    );
}

/// The ledger entry for a copied file identifies the file that landed: the same
/// node id and size a fresh stat of the destination reports. Without that, a
/// reversal can only check that something is there, and "something is there" is
/// what deletes a file someone else replaced.
#[test]
fn a_copied_file_is_recorded_with_the_identity_it_landed_with() {
    let engine = TestOperations::new();
    let tmp = tempfile::tempdir().unwrap();
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();
    let source = src_dir.join("report.pdf");
    fs::write(&source, vec![7u8; 2048]).unwrap();

    let transaction = copy_one_file(&engine, &source, &dst_dir);

    let landed = dst_dir.join("report.pdf");
    let recorded = transaction.created_files();
    assert_eq!(recorded.len(), 1, "one file copied, one ledger entry");
    assert_eq!(recorded[0].path, landed);
    assert_eq!(
        recorded[0].identity,
        WrittenIdentity::at_local_path(&landed),
        "the ledger has to describe the file that actually landed"
    );
    assert_eq!(
        recorded[0].identity.recorded_size(),
        Some(2048),
        "the size the ledger records is the one the journal row carries"
    );
    transaction.commit();
}

/// A copied symlink is recorded as the LINK, not as whatever it points at — the
/// one snapshot that survives a dangling target.
#[test]
fn a_copied_symlink_is_recorded_as_the_link() {
    let engine = TestOperations::new();
    let tmp = tempfile::tempdir().unwrap();
    let src_dir = tmp.path().join("src");
    let dst_dir = tmp.path().join("dst");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&dst_dir).unwrap();
    let target = src_dir.join("target.bin");
    fs::write(&target, vec![3u8; 999]).unwrap();
    let link = src_dir.join("shortcut");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let transaction = copy_one_file(&engine, &link, &dst_dir);

    let landed = dst_dir.join("shortcut");
    let recorded = transaction.created_files();
    assert_eq!(recorded.len(), 1);
    assert_eq!(
        recorded[0].identity,
        WrittenIdentity::at_local_path(&landed),
        "a link's snapshot is its own, taken with symlink_metadata"
    );
    assert_ne!(
        recorded[0].identity.recorded_size(),
        Some(999),
        "the link's size is the link's, never the target's"
    );
    transaction.commit();
}

/// The ledger is a stack: a reversal the user stops halfway leaves it claiming
/// exactly the files still on disk, so what stayed behind can be reported
/// honestly (and a second pass can't try to delete the same file twice).
#[test]
fn a_stopped_reversal_leaves_the_ledger_claiming_what_is_still_on_disk() {
    let engine = TestOperations::new();
    let tmp = tempfile::tempdir().unwrap();
    let state = make_state(&engine, 0);
    let guard = engine.register_state("ledger-stop", Arc::clone(&state));

    let mut transaction = CopyTransaction::new();
    let files: Vec<PathBuf> = (0..4)
        .map(|i| {
            let path = tmp.path().join(format!("file{i}.bin"));
            fs::write(&path, b"payload").unwrap();
            transaction.record_file(WrittenFile::local(path.clone()));
            path
        })
        .collect();

    // Rolling back, and the user stops the reversal once it reports progress.
    engine.cancel_write_operation(guard.id(), true);
    let events = StopTheReversalSink {
        inner: CollectorEventSink::new(),
        engine: Operations::clone(&engine),
        operation_id: guard.id().clone(),
    };
    let completed = rollback_with_progress(
        &mut transaction,
        &events,
        guard.id().as_str(),
        &WindDown::of(&state),
        WriteOperationType::Copy,
        files.len(),
        0,
        files.len(),
        0,
    );

    assert_eq!(
        completed.into_cancel_rollback().outcome,
        crate::types::CancelRollbackOutcome::PartiallyRolledBack,
        "the user stopped it partway, so some of what the copy wrote is still there"
    );
    let still_claimed = transaction.created_files();
    assert!(
        !still_claimed.is_empty() && still_claimed.len() < files.len(),
        "the reversal should have stopped partway, got {}/{} still claimed",
        still_claimed.len(),
        files.len()
    );
    for entry in still_claimed {
        assert!(
            entry.path.exists(),
            "{} is still claimed, so it must still be on disk",
            entry.path.display()
        );
    }
    let claimed: HashSet<&Path> = still_claimed.iter().map(|f| f.path.as_path()).collect();
    for file in &files {
        assert_eq!(
            file.exists(),
            claimed.contains(file.as_path()),
            "{} is on disk exactly when the ledger still claims it",
            file.display()
        );
    }
    transaction.commit();
}

/// Stops the reversal at its first progress report, the way a user clicking
/// Cancel mid-rollback does.
struct StopTheReversalSink {
    inner: CollectorEventSink,
    engine: Operations,
    operation_id: OperationId,
}

impl OperationEventSink for StopTheReversalSink {
    fn emit_settled(&self, e: crate::types::WriteSettledEvent) {
        self.inner.emit_settled(e);
    }
    fn emit_progress(&self, event: WriteProgressEvent) {
        if event.phase == WriteOperationPhase::RollingBack && event.current_file.is_some() {
            self.engine.cancel_write_operation(&self.operation_id, false);
        }
        self.inner.emit_progress(event);
    }
    fn emit_complete(&self, e: WriteCompleteEvent) {
        self.inner.emit_complete(e);
    }
    fn emit_cancelled(&self, e: WriteCancelledEvent) {
        self.inner.emit_cancelled(e);
    }
    fn emit_error(&self, e: crate::types::WriteErrorEvent) {
        self.inner.emit_error(e);
    }
    fn emit_conflict(&self, e: crate::ArmedConflict) {
        self.inner.emit_conflict(e);
    }
    fn emit_conflict_resolved(&self, e: crate::types::WriteConflictResolvedEvent) {
        self.inner.emit_conflict_resolved(e);
    }
    fn emit_source_item_done(&self, e: WriteSourceItemDoneEvent) {
        self.inner.emit_source_item_done(e);
    }
    fn emit_scan_progress(&self, e: crate::types::ScanProgressEvent) {
        self.inner.emit_scan_progress(e);
    }
    fn emit_scan_conflict(&self, c: crate::types::ConflictInfo) {
        self.inner.emit_scan_conflict(c);
    }
    fn emit_dry_run_complete(&self, r: crate::types::DryRunResult) {
        self.inner.emit_dry_run_complete(r);
    }
}

/// The scanned-dirs pass runs AFTER the per-file loop, so "Paused" is already on
/// screen by the time it starts: it has to park like every other loop, or a
/// paused copy keeps building the destination skeleton.
#[test]
fn a_paused_copy_stops_landing_scanned_dirs_until_it_resumes() {
    use crate::ledger::CopyTransaction;
    use std::collections::{HashMap, HashSet};

    let engine = TestOperations::new();

    let tmp = tempfile::tempdir().expect("tempdir");
    let source = tmp.path().join("src/album");
    let destination = tmp.path().join("dst");
    let empties = ["e1", "e2", "e3", "e4"];
    for name in empties {
        fs::create_dir_all(source.join(name)).unwrap();
    }
    fs::create_dir_all(&destination).unwrap();

    // Deepest-first, the order the scan hands them over, and the order this pass
    // reverses so parents land before children.
    let mut scanned_dirs: Vec<PathBuf> = empties.iter().map(|n| source.join(n)).collect();
    scanned_dirs.push(source.clone());

    let state = make_state(&engine, 0);
    state.pause_gate.pause();

    let sources = vec![source.clone()];
    let destination_for_pass = destination.clone();
    let state_for_pass = Arc::clone(&state);
    let pass = std::thread::spawn(move || {
        let mut transaction = CopyTransaction::new();
        let mut created_dirs: HashSet<PathBuf> = HashSet::new();
        let result = create_scanned_dirs_at_destination(
            &scanned_dirs,
            &sources,
            &destination_for_pass,
            &state_for_pass,
            &mut transaction,
            &mut created_dirs,
            &HashMap::new(),
        );
        // The production caller commits what the pass created on every terminal
        // path; an uncommitted transaction's `Drop` net would delete the very
        // directories this test is about to look for.
        transaction.commit();
        result
    });

    // Parking has no "parked now" signal, so hold a window open: an ungated pass
    // would have built the whole skeleton many times over inside it.
    // allowed-test-sleep: negative assertion over a window; the park has nothing to await.
    std::thread::sleep(Duration::from_millis(150));
    assert!(
        !destination.join("album").exists(),
        "a paused copy lands no destination directory at all"
    );

    state.pause_gate.resume();

    let result = pass.join().expect("the pass thread joins");
    assert!(result.is_ok(), "the resumed pass completes, got {result:?}");
    for name in empties {
        assert!(
            destination.join("album").join(name).exists(),
            "and every scanned dir lands once the user resumes: {name}"
        );
    }
}

/// A local copy's scan becomes the operation's live tree of the selection, every
/// file under the folder it sits in, so the progress views can show which
/// folder is in flight.
#[test]
fn a_local_copy_hands_its_operation_the_tree_of_its_selection() {
    use crate::transfer_tree::test_support::outline;

    let engine = TestOperations::new();

    let tmp = tempfile::tempdir().expect("tempdir");
    let src = tmp.path().join("src");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(src.join("photos/2024")).unwrap();
    fs::create_dir_all(&dst).unwrap();
    fs::write(src.join("photos/a.jpg"), b"aaa").unwrap();
    fs::write(src.join("photos/2024/b.jpg"), b"bbbbb").unwrap();
    fs::write(src.join("notes.txt"), b"nn").unwrap();
    let state = make_state(&engine, 0);

    copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "op-local-copy-tree",
        &state,
        &[src.join("photos"), src.join("notes.txt")],
        &dst,
        &WriteOperationConfig::default(),
    )
    .expect("the copy succeeds");

    let tree = state.transfer_tree.get().expect("the scan installed a tree");
    assert_eq!(
        tree.read(outline).expect("the tree is on"),
        vec![
            "photos/ 2 files 8 B",
            "  2024/ 1 files 5 B",
            "    b.jpg 5 B",
            "  a.jpg 3 B",
            "notes.txt 2 B",
        ]
    );
}

/// Every row a local copy reaches ends the way its file did: landed, or skipped
/// on a clash, and the empty folder once the scanned-dirs pass made it. The
/// folder holding the skip still landed something, so it reads done.
#[test]
fn a_local_copy_settles_every_row_the_way_its_file_ended() {
    use crate::transfer_tree::test_support::row_states;
    use crate::types::ConflictResolution;

    let engine = TestOperations::new();

    let tmp = tempfile::tempdir().expect("tempdir");
    let src = tmp.path().join("src");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(src.join("photos/empty")).unwrap();
    fs::create_dir_all(dst.join("photos")).unwrap();
    fs::write(src.join("photos/a.jpg"), b"aaa").unwrap();
    fs::write(src.join("photos/b.jpg"), b"bbbbb").unwrap();
    fs::write(dst.join("photos/a.jpg"), b"theirs").unwrap();
    let state = make_state(&engine, 0);

    copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "op-local-copy-rows",
        &state,
        &[src.join("photos")],
        &dst,
        &WriteOperationConfig {
            conflict_resolution: ConflictResolution::Skip,
            ..WriteOperationConfig::default()
        },
    )
    .expect("the copy succeeds");

    let tree = state.transfer_tree.get().expect("the scan installed a tree");
    assert_eq!(
        tree.read(row_states).expect("the tree is on"),
        vec!["photos/ done", "  empty/ done", "  a.jpg skipped", "  b.jpg done"]
    );
}

/// A local copy STARTS its files in tree order, the order the progress tree
/// shows: each folder's subfolders first, then its files, by name, depth first,
/// and the top-level sources the same way whatever order they came in. A flat
/// sort on the file name alone would copy `b/aaa.txt` before `a/zzz.txt`, and
/// the folders would interleave.
#[test]
fn a_local_copy_starts_its_files_in_tree_order() {
    let engine = TestOperations::new();
    let tmp = tempfile::tempdir().expect("tempdir");
    let src = tmp.path().join("src");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(src.join("a/sub")).unwrap();
    fs::create_dir_all(src.join("b")).unwrap();
    fs::create_dir_all(&dst).unwrap();
    fs::write(src.join("a/zzz.txt"), b"1").unwrap();
    fs::write(src.join("a/sub/mmm.txt"), b"2").unwrap();
    fs::write(src.join("b/aaa.txt"), b"3").unwrap();
    fs::write(src.join("Y.txt"), b"4").unwrap();
    fs::write(src.join("z.txt"), b"5").unwrap();
    let events = CollectorEventSink::new();
    let state = make_state(&engine, 0);

    copy_files_with_progress_inner(
        &events,
        "op-local-copy-tree-order",
        &state,
        &[src.join("z.txt"), src.join("b"), src.join("Y.txt"), src.join("a")],
        &dst,
        &WriteOperationConfig::default(),
    )
    .expect("the copy succeeds");

    // Each file's milestone names it as it lands, and a local copy lands one
    // file at a time, so the milestones are the start order.
    let mut landed: Vec<String> = Vec::new();
    let mut files_done = 0;
    for event in events.progress.lock().unwrap().iter() {
        if event.phase == WriteOperationPhase::Copying && event.files_done > files_done {
            files_done = event.files_done;
            landed.push(event.current_file.clone().unwrap_or_default());
        }
    }
    assert_eq!(landed, ["mmm.txt", "zzz.txt", "aaa.txt", "Y.txt", "z.txt"]);
}

/// A bug while building the tree costs the tree, never the copy: every file
/// lands, in the walk's own order (the order a copy without a tree would take),
/// and the operation has no tree, so its views say there's none.
#[test]
fn a_tree_that_fails_to_build_leaves_the_local_copy_as_it_would_run_without_one() {
    use crate::transfer_tree::test_support::TREE_BUG_NAME;

    let engine = TestOperations::new();

    let tmp = tempfile::tempdir().expect("tempdir");
    let src = tmp.path().join("src");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(src.join("b")).unwrap();
    fs::create_dir_all(&dst).unwrap();
    fs::write(src.join("z.txt"), b"1").unwrap();
    fs::write(src.join("b").join(TREE_BUG_NAME), b"2").unwrap();
    fs::write(src.join("Y.txt"), b"3").unwrap();
    let events = CollectorEventSink::new();
    let state = make_state(&engine, 0);

    copy_files_with_progress_inner(
        &events,
        "op-local-copy-tree-bug",
        &state,
        &[src.join("z.txt"), src.join("b"), src.join("Y.txt")],
        &dst,
        &WriteOperationConfig::default(),
    )
    .expect("the copy succeeds");

    let mut landed: Vec<String> = Vec::new();
    let mut files_done = 0;
    for event in events.progress.lock().unwrap().iter() {
        if event.phase == WriteOperationPhase::Copying && event.files_done > files_done {
            files_done = event.files_done;
            landed.push(event.current_file.clone().unwrap_or_default());
        }
    }
    // Tree order would be `b/…`, `Y.txt`, `z.txt`; the walk takes the sources as given.
    assert_eq!(landed, ["z.txt", TREE_BUG_NAME, "Y.txt"]);
    assert_eq!(fs::read(dst.join("b").join(TREE_BUG_NAME)).unwrap(), b"2");
    assert!(
        state.transfer_tree.get().is_none(),
        "no tree, so views answer `ready: false`"
    );
}

/// A write the destination refuses (here a folder that won't take new files;
/// a full disk names its path the same way) marks the row of the file it was
/// writing, found from the destination path through the landing folder. The
/// failure names the file the user knows, never its staging temp.
#[cfg(unix)]
#[test]
fn a_write_the_destination_refuses_marks_the_row_it_was_writing() {
    use crate::transfer_tree::test_support::row_states;
    use std::os::unix::fs::PermissionsExt;

    let engine = TestOperations::new();

    // Root writes through a read-only folder, so the refusal never happens. The CI
    // Linux suite runs as root in Docker, like the other permission tests skip for.
    // SAFETY: (test) `geteuid` takes no arguments, shares no memory, and can't fail — it just
    // returns the caller's effective uid. We compare the returned integer to 0 to detect root.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }

    let tmp = tempfile::tempdir().expect("tempdir");
    let src = tmp.path().join("src");
    let dst = tmp.path().join("dst");
    fs::create_dir_all(src.join("photos")).unwrap();
    fs::create_dir_all(dst.join("photos")).unwrap();
    fs::write(src.join("photos/a.jpg"), b"aaa").unwrap();
    fs::set_permissions(dst.join("photos"), fs::Permissions::from_mode(0o555)).unwrap();
    let state = make_state(&engine, 0);

    let result = copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "op-local-copy-dest-refused",
        &state,
        &[src.join("photos")],
        &dst,
        &WriteOperationConfig::default(),
    );
    fs::set_permissions(dst.join("photos"), fs::Permissions::from_mode(0o755)).unwrap();

    let error = result.expect_err("the destination refuses the write");
    let landing = dst.join("photos/a.jpg").display().to_string();
    assert_eq!(error.item_path(), Some(landing.as_str()), "{error:?}");
    let tree = state.transfer_tree.get().expect("the scan installed a tree");
    assert_eq!(
        tree.read(row_states).expect("the tree is on"),
        vec!["photos/ failed", "  a.jpg failed"]
    );
}
