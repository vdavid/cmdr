//! Folder-merge engine tests: per-file conflict resolution INSIDE a directory
//! merge (scan-as-you-merge). Every file policy over a merging tree, what a deep
//! skip credits to the progress bars and to the rate, the byte total the
//! recursion reports, and the destination size a deep clash carries.
//!
//! These drive the real `copy_volumes_with_progress` pipeline against
//! `InMemoryVolume` pairs + `CollectorEventSink`, so the whole stack (preflight,
//! the serial/concurrent split, `copy_directory_streaming`, the resolver) runs
//! exactly as in production. Shared fixtures `make_state` / `make_volumes` live in
//! `volume/copy_tests.rs` (`super::tests`). Sibling suites: the dir-vs-dir
//! contract in `volume/merge_dir_vs_dir_tests.rs`, the conflict-dispatch mutex in
//! `volume/merge_dispatch_mutex_tests.rs`.

use super::super::super::conflict_responder_test_support::{
    ConflictResponderSink, file_conflict_count, folder_conflict_count_both_dirs,
};
use super::super::safety_oracle::{SafetySpec, assert_operation_was_safe};
use super::tests::{make_state, make_volumes};
use super::*;
use crate::file_system::volume::Volume;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::state::cancel_write_operation;
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::{
    ConflictResolution, WriteCancelledEvent, WriteCompleteEvent, WriteConflictEvent, WriteConflictResolvedEvent,
    WriteErrorEvent, WriteSourceItemDoneEvent,
};
use std::sync::atomic::AtomicU8;

// ============================================================================
// Helpers
// ============================================================================

/// Reads a whole file from a volume into a `Vec<u8>`.
async fn read_all(vol: &Arc<dyn Volume>, path: &str) -> Vec<u8> {
    let mut stream = vol.open_read_stream(Path::new(path)).await.unwrap();
    let mut out = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        out.extend_from_slice(&chunk);
    }
    out
}

/// A merge fixture: a source tree and a dest tree of the same top-level name
/// (`/album`) with overlapping AND non-overlapping content at several depths.
///
/// Dest-only files (`keep*`) must survive every policy. Source-only files
/// (`fresh*`) must always arrive. Clashing files (`clash*`) follow the policy.
/// `/album/sub` is a nested merge with its own clash + dest-only file. There's
/// also a type mismatch: source `/album/swap` is a FILE, dest `/album/swap` is a
/// DIRECTORY.
async fn make_rich_merge() -> (Arc<dyn Volume>, Arc<dyn Volume>) {
    let (source, dest) = make_volumes();

    // Source tree.
    source.create_directory(Path::new("/album")).await.unwrap();
    source
        .create_file(Path::new("/album/fresh.txt"), b"SRC-fresh")
        .await
        .unwrap();
    source
        .create_file(Path::new("/album/clash.txt"), b"SRC-clash-larger")
        .await
        .unwrap();
    source.create_directory(Path::new("/album/sub")).await.unwrap();
    source
        .create_file(Path::new("/album/sub/fresh2.txt"), b"SRC-fresh2")
        .await
        .unwrap();
    source
        .create_file(Path::new("/album/sub/clash2.txt"), b"SRC-clash2")
        .await
        .unwrap();
    // Type mismatch: source FILE named `swap`.
    source
        .create_file(Path::new("/album/swap"), b"SRC-swap-file")
        .await
        .unwrap();

    // Dest tree (pre-existing).
    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_file(Path::new("/album/keep.txt"), b"DEST-keep")
        .await
        .unwrap();
    dest.create_file(Path::new("/album/clash.txt"), b"DEST-clash")
        .await
        .unwrap();
    dest.create_directory(Path::new("/album/sub")).await.unwrap();
    dest.create_file(Path::new("/album/sub/keep2.txt"), b"DEST-keep2")
        .await
        .unwrap();
    dest.create_file(Path::new("/album/sub/clash2.txt"), b"DEST-clash2")
        .await
        .unwrap();
    // Type mismatch: dest DIR named `swap` with a file inside it.
    dest.create_directory(Path::new("/album/swap")).await.unwrap();
    dest.create_file(Path::new("/album/swap/inner.txt"), b"DEST-swap-inner")
        .await
        .unwrap();

    (source, dest)
}

/// Every source file `make_rich_merge` creates, relative to `/album`. The clash
/// contents are deliberately LARGER than their dest counterparts here (the move
/// fixture's are smaller), which is what makes `OverwriteSmaller` resolve to an
/// overwrite on this suite and to a skip on that one.
const RICH_MERGE_SOURCE_FILES: &[(&str, &[u8])] = &[
    ("/fresh.txt", b"SRC-fresh"),
    ("/clash.txt", b"SRC-clash-larger"),
    ("/sub/fresh2.txt", b"SRC-fresh2"),
    ("/sub/clash2.txt", b"SRC-clash2"),
    ("/swap", b"SRC-swap-file"),
];

/// The source-only files: nothing shadows them, so they arrive under every
/// policy. That's the merge's whole point.
const RICH_MERGE_DELIVERED: &[(&str, &[u8])] = &[("/fresh.txt", b"SRC-fresh"), ("/sub/fresh2.txt", b"SRC-fresh2")];

/// The dest-only files, which no policy may touch. `/album/swap/inner.txt` is
/// deliberately absent: the source shadows `/album/swap` across types, and a
/// cross-type Overwrite replaces the destination wholesale by design.
const RICH_MERGE_UNTOUCHED_DEST: &[(&str, &[u8])] = &[("/keep.txt", b"DEST-keep"), ("/sub/keep2.txt", b"DEST-keep2")];

/// The oracle spec for a finished copy over the rich merge fixture.
fn rich_merge_spec(label: &str) -> SafetySpec<'_> {
    SafetySpec {
        label,
        source_root: "/album",
        dest_root: "/album",
        source_files: RICH_MERGE_SOURCE_FILES,
        delivered: RICH_MERGE_DELIVERED,
        untouched_dest: RICH_MERGE_UNTOUCHED_DEST,
    }
}

// ============================================================================
// The invariant property test
// ============================================================================

/// THE INVARIANT: a merge never deletes or overwrites a dest file the source
/// doesn't shadow — under EVERY file policy, including ask-mode with scripted
/// answers.
///
/// We enumerate every file policy over the rich merge fixture and assert that
/// every dest-only file (`keep*`) is byte-identical afterward, every time. The
/// cancel / rollback mid-merge slice of the same invariant lives in the sibling
/// tests `merge_cancel_mid_stream_preserves_unshadowed_dest_files` (this file)
/// and the `cancel_mid_merge_stream_*` / `rollback_mid_merge_stream_*` cases in
/// `volume/copy_rollback_tests.rs`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn merge_never_deletes_unshadowed_dest_files_under_every_policy() {
    // (policy, scripted Stop answer or None, apply_to_all)
    let cases: &[(ConflictResolution, Option<ConflictResolution>)] = &[
        (ConflictResolution::Skip, None),
        (ConflictResolution::Overwrite, None),
        (ConflictResolution::Rename, None),
        (ConflictResolution::OverwriteSmaller, None),
        (ConflictResolution::OverwriteOlder, None),
        (ConflictResolution::Stop, Some(ConflictResolution::Skip)),
        (ConflictResolution::Stop, Some(ConflictResolution::Overwrite)),
        (ConflictResolution::Stop, Some(ConflictResolution::Rename)),
    ];

    for (policy, scripted) in cases {
        let (source, dest) = make_rich_merge().await;
        let state = make_state();

        // The responder sink IS the events sink: it forwards every event to its
        // inner collector and auto-answers any Stop-mode prompt. For non-Stop
        // policies no prompt is ever emitted, so the scripted answer (defaulted
        // to Skip here) never fires — the sink is a plain collector in that case.
        let events = Arc::new(ConflictResponderSink::new(
            &state,
            scripted.unwrap_or(ConflictResolution::Skip),
            true,
        ));
        let config = VolumeCopyConfig {
            conflict_resolution: *policy,
            progress_interval_ms: 0,
            ..VolumeCopyConfig::default()
        };

        let result = copy_volumes_with_progress(
            events.clone(),
            &format!("op-invariant-{policy:?}-{scripted:?}"),
            &state,
            Arc::clone(&source),
            &[PathBuf::from("/album")],
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await;

        assert!(
            result.is_ok(),
            "policy {policy:?}/{scripted:?} should complete, got {result:?}"
        );

        // ❗ THE INVARIANT, through the shared oracle: every dest-only file is
        // byte-identical, every source-only file arrived, and no source byte is
        // gone from both sides.
        let label = format!("policy {policy:?}/{scripted:?}");
        assert_operation_was_safe(&source, &dest, &rich_merge_spec(&label)).await;

        // Zero folder-level prompts under EVERY policy, even Stop.
        assert_eq!(
            folder_conflict_count_both_dirs(&events.inner),
            0,
            "policy {policy:?}/{scripted:?}: a dir-vs-dir merge wrongly emitted a folder conflict"
        );
    }
}

/// Cancel-mid-merge variant of the invariant: flip intent partway through (via
/// the public cancel path, never `state.intent.store` directly), and the
/// dest-only sentinel must still survive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn merge_cancel_mid_stream_preserves_unshadowed_dest_files() {
    let (source, dest) = make_rich_merge().await;
    let state = make_state();

    // Register the op in the global cache so `cancel_write_operation` (the
    // public path) can find and transition it — exercising the real cancel
    // machinery, not a direct intent store.
    let op = TestOperationGuard::register_state("merge-cancel-public", Arc::clone(&state));
    let op_id = op.id();

    // A sink that cancels (public path) once any byte has been copied.
    struct CancelOnByteSink {
        inner: CollectorEventSink,
        op_id: String,
        fired: AtomicU8,
    }
    impl OperationEventSink for CancelOnByteSink {
        fn emit_settled(&self, e: crate::file_system::write_operations::types::WriteSettledEvent) {
            self.inner.emit_settled(e);
        }
        fn emit_progress(&self, event: WriteProgressEvent) {
            if event.phase == WriteOperationPhase::Copying
                && event.bytes_done > 0
                && self.fired.swap(1, Ordering::Relaxed) == 0
            {
                // Public cancel path (Stopped, keep partials).
                cancel_write_operation(&self.op_id, false);
            }
            self.inner.emit_progress(event);
        }
        fn emit_complete(&self, e: WriteCompleteEvent) {
            self.inner.emit_complete(e);
        }
        fn emit_cancelled(&self, e: WriteCancelledEvent) {
            self.inner.emit_cancelled(e);
        }
        fn emit_error(&self, e: WriteErrorEvent) {
            self.inner.emit_error(e);
        }
        fn emit_conflict(&self, e: WriteConflictEvent) {
            self.inner.emit_conflict(e);
        }
        fn emit_conflict_resolved(&self, e: WriteConflictResolvedEvent) {
            self.inner.emit_conflict_resolved(e);
        }
        fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
        fn emit_scan_progress(&self, _e: crate::file_system::write_operations::types::ScanProgressEvent) {}
        fn emit_scan_conflict(&self, _c: crate::file_system::write_operations::types::ConflictInfo) {}
        fn emit_dry_run_complete(&self, _r: crate::file_system::write_operations::types::DryRunResult) {}
    }

    let events = Arc::new(CancelOnByteSink {
        inner: CollectorEventSink::new(),
        op_id: op_id.to_string(),
        fired: AtomicU8::new(0),
    });
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        op_id,
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_err(), "expected a cancelled result, got {result:?}");
    // Cancel keeps partials but must never destroy a dest-only file.
    assert_eq!(
        read_all(&dest, "/album/keep.txt").await,
        b"DEST-keep",
        "cancel mid-merge clobbered a dest-only file"
    );
}

// ============================================================================
// "Skip all" merges folders, skips only clashing files (old behavior GONE)
// ============================================================================

/// THE GOTCHA FIX: under Skip, a top-level dir-vs-dir clash used to skip the
/// ENTIRE subtree (the documented `transfer/CLAUDE.md` gotcha). Now it merges:
/// the folder is merged, only the clashing FILE is skipped, and non-clashing
/// source files still arrive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn skip_all_merges_folder_and_skips_only_clashing_files() {
    let (source, dest) = make_rich_merge().await;
    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-skip-all-merge",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    // The folder MERGED (old behavior would have skipped the whole subtree):
    // non-clashing source files arrived at both depths.
    assert_eq!(read_all(&dest, "/album/fresh.txt").await, b"SRC-fresh");
    assert_eq!(read_all(&dest, "/album/sub/fresh2.txt").await, b"SRC-fresh2");

    // Clashing files were SKIPPED — dest keeps its own bytes.
    assert_eq!(read_all(&dest, "/album/clash.txt").await, b"DEST-clash");
    assert_eq!(read_all(&dest, "/album/sub/clash2.txt").await, b"DEST-clash2");

    // Dest-only files untouched.
    assert_eq!(read_all(&dest, "/album/keep.txt").await, b"DEST-keep");
    assert_eq!(read_all(&dest, "/album/sub/keep2.txt").await, b"DEST-keep2");
}

// ============================================================================
// Deep skips move the bars, and stay out of the rate
// ============================================================================

/// A child a deep merge skips is DONE, and both bars have to say so.
///
/// A merge whose children all clash used to credit nothing at all until the
/// operation ended: `MergeChildDecision::Skip` recorded the skip in the
/// rollback ledger and moved on, so a person watched `0 of 119,204` for the
/// whole run while the walk worked perfectly. That is what a user reported on
/// 2026-08-27 (`ERR-AYVM4`); they cancelled a healthy transfer because nothing
/// on screen moved.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deep_skip_credits_both_progress_bars() {
    let (source, dest) = make_rich_merge().await;
    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    copy_volumes_with_progress(
        events.clone(),
        "op-deep-skip-progress",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await
    .expect("the merge completes");

    // The copying phase only: the scan emits its findings as `files_done`, which
    // is a different meaning of the same field.
    let progress: Vec<_> = events
        .progress
        .lock()
        .unwrap()
        .iter()
        .filter(|e| e.phase == WriteOperationPhase::Copying)
        .cloned()
        .collect();
    let last = progress.last().expect("the copy emits progress");

    // `/album` holds five source files: two fresh ones that copy, two that
    // clash and are skipped, and `swap` (a file landing on a dir) that the type
    // mismatch skips as well. All five are accounted for, so the bar arrives.
    assert_eq!(
        (last.files_done, last.files_total),
        (5, 5),
        "every child is done — skipped or copied — and the file bar counts all of them"
    );
    assert_eq!(
        (last.bytes_done, last.bytes_total),
        (58, 58),
        "the byte bar counts a skipped child's bytes too, so it can reach its total"
    );
}

/// ...and the rate must not read those skipped bytes as throughput.
///
/// Nothing moved for a skip, so charging it to the EWMA reads as an
/// instantaneous burst. On a merge that skips thousands of children in a row
/// that is not a blip, it IS the reported speed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_deep_skip_stays_out_of_the_rate() {
    let (source, dest) = make_rich_merge().await;
    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    copy_volumes_with_progress(
        events.clone(),
        "op-deep-skip-rate",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await
    .expect("the merge completes");

    let (skipped_files, skipped_bytes) = state.skipped_totals();
    // `clash.txt` (16) + `sub/clash2.txt` (10) + `swap` (13), the three children
    // the Skip policy declined.
    assert_eq!(skipped_files, 3, "every declined child is recorded as skipped");
    assert_eq!(
        skipped_bytes, 39,
        "with its bytes, so the estimator can subtract them from the sample"
    );
}

// ============================================================================
// Stop-mode deep file clash emits a conflict with correct paths/flags, resumes
// ============================================================================

/// A deep file clash under Stop emits a `write-conflict` carrying the right
/// per-file paths (file, not folder) and resumes on response. We answer the deep
/// clash with Overwrite (no apply-to-all) and assert only that one deep file
/// changed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_mode_deep_file_clash_emits_conflict_and_resumes() {
    let (source, dest) = make_volumes();
    // One deep clash, plus a dest-only sibling that must survive.
    source.create_directory(Path::new("/album")).await.unwrap();
    source.create_directory(Path::new("/album/sub")).await.unwrap();
    source
        .create_file(Path::new("/album/sub/clash.txt"), b"SRC-deep")
        .await
        .unwrap();
    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_directory(Path::new("/album/sub")).await.unwrap();
    dest.create_file(Path::new("/album/sub/clash.txt"), b"DEST-deep")
        .await
        .unwrap();
    dest.create_file(Path::new("/album/sub/keep.txt"), b"DEST-keep")
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Overwrite, false));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-stop-deep",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");
    // The sink-recorded file prompts ARE the race-free count once the op future
    // has completed — exactly one Stop prompt for the deep clash.
    assert_eq!(
        file_conflict_count(&events.inner),
        1,
        "exactly one Stop prompt expected for the deep clash"
    );

    // Exactly one conflict, and it's a FILE clash with the right deep paths.
    // Clone the fields we need out of the guarded vec in a tight scope so the
    // lock guard is fully dropped before the awaits below (clippy
    // `await_holding_lock` flags a guard alive across `.await` even with an
    // explicit `drop`, so we end the borrow by leaving the block instead).
    let (src_path, dst_path, src_is_dir, dst_is_dir, n_conflicts) = {
        let conflicts = events.inner.conflicts.lock().unwrap();
        let c = conflicts.first().expect("exactly one deep file conflict");
        (
            c.source_path.clone(),
            c.destination_path.clone(),
            c.source_is_directory,
            c.destination_is_directory,
            conflicts.len(),
        )
    };
    assert_eq!(n_conflicts, 1, "exactly one deep file conflict");
    assert!(src_path.ends_with("clash.txt"), "conflict source path: {src_path}");
    assert!(dst_path.ends_with("clash.txt"), "conflict dest path: {dst_path}");
    assert!(!src_is_dir && !dst_is_dir, "deep clash is file-vs-file");

    // Overwrite applied to the deep clash; dest-only sibling untouched.
    assert_eq!(read_all(&dest, "/album/sub/clash.txt").await, b"SRC-deep");
    assert_eq!(read_all(&dest, "/album/sub/keep.txt").await, b"DEST-keep");
}

// ============================================================================
// Byte-total accounting through the merge recursion
// ============================================================================

/// The merge's returned byte total — which flows into the complete event's
/// `bytes_processed` — must be the exact sum of every file written across all
/// merged levels. Distinct, non-trivial per-file sizes make any
/// accumulation-operator corruption (`+=` → `*=` / `-=`) produce a wrong total.
/// Overwrite so every clashing file is also written (counts toward the sum).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn merge_reports_exact_byte_total_across_levels() {
    let (source, dest) = make_volumes();
    // Source: top-level file (7), a deep file (11), and a deeper file (13).
    source.create_directory(Path::new("/album")).await.unwrap();
    source.create_file(Path::new("/album/a.txt"), &[1u8; 7]).await.unwrap();
    source.create_directory(Path::new("/album/sub")).await.unwrap();
    source
        .create_file(Path::new("/album/sub/b.txt"), &[2u8; 11])
        .await
        .unwrap();
    source.create_directory(Path::new("/album/sub/deep")).await.unwrap();
    source
        .create_file(Path::new("/album/sub/deep/c.txt"), &[3u8; 13])
        .await
        .unwrap();
    // Dest pre-exists at every level so all three levels MERGE (each takes the
    // AlreadyExists branch and the byte total accumulates through recursion).
    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_directory(Path::new("/album/sub")).await.unwrap();
    dest.create_directory(Path::new("/album/sub/deep")).await.unwrap();
    // A clashing dest file at the deepest level, Overwrite ⇒ written ⇒ counted.
    dest.create_file(Path::new("/album/sub/deep/c.txt"), b"old")
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-byte-total",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    let complete = events.complete.lock().unwrap();
    let total = complete.first().expect("a complete event").bytes_processed;
    assert_eq!(
        total,
        7 + 11 + 13,
        "merge must report the exact summed byte total, got {total}"
    );
}

// ============================================================================
// The destination size a deep clash reports
// ============================================================================

/// Builds a one-deep-clash merge where the DESTINATION file is much bigger than
/// the source's. Both the size the dialog reports and the `OverwriteSmaller`
/// comparison hang off that difference.
async fn make_deep_clash_with_bigger_dest() -> (Arc<dyn Volume>, Arc<dyn Volume>) {
    let (source, dest) = make_volumes();
    source.create_directory(Path::new("/album")).await.unwrap();
    source.create_directory(Path::new("/album/sub")).await.unwrap();
    source
        .create_file(Path::new("/album/sub/clash.txt"), b"SRC")
        .await
        .unwrap();
    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_directory(Path::new("/album/sub")).await.unwrap();
    dest.create_file(Path::new("/album/sub/clash.txt"), b"DEST-is-much-bigger")
        .await
        .unwrap();
    (source, dest)
}

/// A deep-merge clash must report the destination's REAL size in its
/// `write-conflict`, not a fabricated `0`.
///
/// The merge walker already holds the dest `FileEntry` (it listed the level to
/// build `dest_by_name`), so the size costs nothing. Reporting `0` makes the
/// dialog claim "Existing: 0 bytes" about a file that has content — and feeds
/// that `0` into the conditional reduction the sibling test pins.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deep_merge_clash_reports_the_real_destination_size() {
    let (source, dest) = make_deep_clash_with_bigger_dest().await;
    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Skip, false));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-deep-dest-size",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    let (dest_size, source_size) = {
        let conflicts = events.inner.conflicts.lock().unwrap();
        let c = conflicts.first().expect("one deep file conflict");
        (c.destination_size, c.source_size)
    };
    assert_eq!(
        dest_size,
        Some("DEST-is-much-bigger".len() as u64),
        "the deep clash must report the destination's real size, not 0"
    );
    assert_eq!(
        source_size,
        Some(3),
        "the deep clash must report the source's real size"
    );
}

/// "Overwrite all smaller" answered on the FIRST clash inside a merge must
/// still compare against the destination's real size. A fabricated `0` makes
/// every destination look smaller than the incoming file, so the answer
/// silently degrades to an unconditional overwrite — on exactly the file the
/// user was looking at when they clicked.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn overwrite_all_smaller_keeps_a_larger_destination_on_the_first_deep_clash() {
    let (source, dest) = make_deep_clash_with_bigger_dest().await;
    let state = make_state();
    let events = Arc::new(ConflictResponderSink::new(
        &state,
        ConflictResolution::OverwriteSmaller,
        true,
    ));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-deep-overwrite-smaller",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    assert_eq!(
        read_all(&dest, "/album/sub/clash.txt").await,
        b"DEST-is-much-bigger",
        "a LARGER destination must survive OverwriteSmaller, even on the first prompted clash"
    );
}
